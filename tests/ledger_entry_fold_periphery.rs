//! Periphery (contract / API / integration) tests for spec 107 criterion 2, THE ENTRY AND ITS
//! BATCH FOLD AS ONE. These run OUTSIDE the crates, over the root crate's public surface and the
//! built binary, and guard the boundaries the in-crate fold tests are structurally blind to.
//!
//! The in-crate tests fold into a concrete projector they never reopen and read its private
//! tables. Here every entry folds through the `Projection` port as a trait object, into a
//! `graph.db` file a later open reads, so the generation an entry installs, and each of the three
//! outcomes a later entry meets, are what the file holds and not what one connection remembers.
//!
//! "A failed or refused fold writes no `applied` row" is stated here through the port alone: the
//! position that failed, or that the generic fold refused, folds afterwards and never answers
//! that it was already applied.
//!
//! A fold that fails or is refused writes no owed mark beside `graph.db` (spec 124, AN APPENDER
//! WAITS UNDER THE CEILING AND NEVER MARKS), so a second projector on the same file owes nothing
//! and folds the position the first left unrecorded.
//!
//! Two projectors folding entries of one generation at once is a seam no single-connection test
//! reaches: the second waits for the first's transaction and then meets a re-recording.
//!
//! The entry's payload is an on-log form a rebuild replays: its exact bytes, the refusal of a
//! payload missing any field, and the acceptance of a key a later writer adds are pinned over the
//! raw JSON. And the one operator surface that could hand an entry to the generic fold,
//! `rigger emit`, is driven through the built binary: it refuses the type and appends nothing.
//!
//! Criterion 10's LEDGER FORM of the folding store is the one caller of that port method outside
//! a rebuild. It is driven here over a sqlite log and a `graph.db` file: one append and one
//! `applied` row per entry, the fold's outcome answered beside the store's report for each of the
//! three ways a fold ends, and a fold it could not make said only when its caller asks.

mod common;

use std::cell::Cell;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use common::cli::{
    applied_positions, read_run_events, rigger_file, run_rigger, temp_store_project,
};
use common::fixtures::{
    decision_json, event_of, generation_ingested, live_edges, live_node_ids, nanos, placed_at,
    whole_json,
};
use rigger::contextgraph::sqlite::{Projector, RebuildSink};
use rigger::contextgraph::{
    wired, EntryFold, Error, Fold, Projection, KIND_DECISION, REBUILD_OWED, REL_CONTAINS,
    TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE,
};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::ingest::{folding_into, EntryAppendedAndFolded, FoldingStore};
use rigger::retention::GenerationIngested;

/// The file every entry of these tests names unless a test says otherwise.
const F: &str = "src/f.rs";

/// The identity of [`F`]'s code half.
const IDENTITY: &str = "gc/src/f.rs";

/// What [`fold_through_port`] answers for an entry whose fold asked its batch once and folded it.
const BATCH_ASKED: (Result<EntryFold, String>, u32) = (Ok(EntryFold::BatchAsked), 1);

/// What [`fold_through_port`] answers for a re-recording: folded without asking its batch.
const RE_RECORDED: (Result<EntryFold, String>, u32) = (Ok(EntryFold::ReRecording), 0);

/// What the emit surface answers a type no agent may emit.
const EMIT_REFUSED: &str = "refusing to emit \"GenerationIngested\": only agent context events \
                            (DecisionMade, ReviewFinding, LessonLearned, UnitProposed) may be \
                            emitted this way.";

/// The on-log payload of the entry `gc/src/a.rs` at `h1`, extracted from blob `b10b` under the
/// walk's flag.
const RAW_PAYLOAD: &str =
    r#"{"prefix":"gc","file":"src/a.rs","generation":"h1","blob":"b10b","excluded":true}"#;

/// What an entry of `gc/<file>` at `generation` records, its batch extracted from `blob` under
/// the walk's flag `excluded`.
fn named(file: &str, generation: &str, blob: &str, excluded: bool) -> GenerationIngested {
    generation_ingested("gc", file, generation, blob, excluded)
}

/// The constructor's entry of `gc/<file>` at `generation`, at position `pos`, valid from `secs`.
fn entry(file: &str, generation: &str, pos: u64, secs: u64) -> Event {
    common::fixtures::entry_at("gc", file, generation, pos, secs)
}

/// One unkeyed definition of `name` in `file`, as a batch event.
fn defines(file: &str, name: &str, line: u64, fresh: bool) -> Event {
    common::fixtures::definition_event(file, name, "function", line, fresh)
}

/// Fold `entry` through the port with a batch function answering `batch`: the outcome, or the
/// error's text, and how many times the batch was asked for.
fn fold_through_port(
    p: &dyn Projection,
    entry: &Event,
    batch: Result<Option<Vec<Event>>, Error>,
) -> (Result<EntryFold, String>, u32) {
    let asked = Cell::new(0);
    let count = &asked;
    let outcome = p
        .apply_generation(
            entry,
            Box::new(move || {
                count.set(count.get() + 1);
                batch
            }),
        )
        .map_err(|e| e.0);
    (outcome, asked.get())
}

/// The generation the port answers for `identity`.
fn generation(p: &dyn Projection, identity: &str) -> Option<String> {
    p.current_generation(identity).unwrap()
}

/// One live `CONTAINS` edge from [`F`] to its definition `name`, valid from `secs` and sourced at
/// `pos`, as [`live_edges`] lists it.
fn contains(name: &str, secs: u64, pos: u64) -> (String, String, String, i64, u64) {
    (
        F.to_string(),
        REL_CONTAINS.to_string(),
        format!("{F}::{name}"),
        nanos(secs),
        pos,
    )
}

/// The id of every node `p` serves, sorted.
fn node_ids(p: &Projector) -> Vec<String> {
    let mut ids: Vec<String> = p.whole().unwrap().nodes.into_iter().map(|n| n.id).collect();
    ids.sort();
    ids
}

/// The path of a `graph.db` inside `dir`.
fn graph_db(dir: &tempfile::TempDir) -> String {
    dir.path().join("graph.db").to_str().unwrap().to_string()
}

#[test]
fn an_entry_folded_through_the_port_is_held_by_the_file_and_each_outcome_is_named_after_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = graph_db(&dir);
    // The entry as the log carries it: the type string and the payload's raw bytes.
    let raw = placed_at(
        Event::new(
            "GenerationIngested",
            br#"{"prefix":"gc","file":"src/f.rs","generation":"h1","blob":"","excluded":false}"#
                .to_vec(),
        ),
        7,
        10,
    );
    // The second batch event carries a replay key naming another identity, and its own position
    // and valid-time.
    let stray = placed_at(
        defines(F, "gone", 9, false)
            .with_meta(rigger::ingest::META_REPLAY_KEY, "gc/src/other.rs@zz#0"),
        42,
        99,
    );
    {
        let p = Projector::open(&path, "test").unwrap();
        let port: &dyn Projection = &p;
        assert_eq!(generation(port, IDENTITY), None);
        assert_eq!(
            fold_through_port(
                port,
                &raw,
                Ok(Some(vec![defines(F, "alpha", 1, true), stray]))
            ),
            BATCH_ASKED
        );
    }

    // A later open reads the generation and the batch's facts from the file.
    let p = Projector::open(&path, "test").unwrap();
    let port: &dyn Projection = &p;
    assert_eq!(generation(port, IDENTITY), Some("h1".to_string()));
    assert_eq!(
        live_edges(&p),
        vec![contains("alpha", 10, 7), contains("gone", 10, 7)],
        "every batch event folded at the entry's position and valid-time"
    );
    let held = whole_json(&p);

    // The applied position, whatever generation the entry at it names.
    assert_eq!(
        fold_through_port(
            port,
            &entry(F, "h2", 7, 20),
            Ok(Some(vec![defines(F, "other", 5, true)]))
        ),
        (Ok(EntryFold::AlreadyApplied), 0)
    );
    // The held generation, recorded again at a later position.
    assert_eq!(
        fold_through_port(
            port,
            &entry(F, "h1", 8, 50),
            Ok(Some(vec![defines(F, "other", 5, true)]))
        ),
        RE_RECORDED
    );
    // Another generation no source resolves.
    assert_eq!(
        fold_through_port(port, &entry(F, "h2", 9, 20), Ok(None)),
        BATCH_ASKED
    );
    assert_eq!(
        whole_json(&p),
        held,
        "none of the three changed a served fact"
    );
    assert_eq!(generation(port, IDENTITY), Some("h1".to_string()));

    // The generation that unresolved entry named is still not the held one, so a later entry of
    // it folds, and it retires the definition the stray-keyed event asserted: that event was
    // asserted under the entry's identity, never under the key it carried.
    assert_eq!(
        fold_through_port(
            port,
            &entry(F, "h2", 10, 30),
            Ok(Some(vec![defines(F, "alpha", 2, true)]))
        ),
        BATCH_ASKED
    );
    assert_eq!(live_edges(&p), vec![contains("alpha", 30, 10)]);
    assert_eq!(node_ids(&p), vec![F.to_string(), format!("{F}::alpha")]);
    drop(p);
    let p = Projector::open(&path, "test").unwrap();
    assert_eq!(generation(&p, IDENTITY), Some("h2".to_string()));
    assert_eq!(generation(&p, "gc/src/other.rs"), None);
}

#[test]
fn a_position_whose_ledger_fold_failed_folds_afterwards_so_the_failure_wrote_no_applied_row() {
    // A failed fold writes no owed mark (spec 124, AN APPENDER WAITS UNDER THE CEILING AND NEVER
    // MARKS), so the projection owes nothing after a failure and the same position can be folded
    // again. The constructor's entries name no stream, so each error is its bare cause.
    let p = Projector::open(":memory:", "test").unwrap();
    let port: &dyn Projection = &p;

    // The batch function fails.
    let first = entry(F, "h1", 7, 10);
    assert_eq!(
        fold_through_port(port, &first, common::fixtures::blob_unread()),
        (Err(common::fixtures::BLOB_UNREAD.to_string()), 1)
    );
    assert!(
        !port.rebuild_owed().unwrap(),
        "the failed fold leaves the projection owing nothing"
    );
    assert_eq!(generation(port, IDENTITY), None);
    assert_eq!(node_ids(&p), Vec::<String>::new());
    assert_eq!(
        fold_through_port(port, &first, alpha_answer()),
        BATCH_ASKED,
        "position 7 was not recorded by the fold that failed"
    );
    assert_eq!(generation(port, IDENTITY), Some("h1".to_string()));

    // The entry's payload does not parse.
    let unparsable = placed_at(Event::new("GenerationIngested", b"{}".to_vec()), 8, 20);
    assert_eq!(
        fold_through_port(
            port,
            &unparsable,
            Ok(Some(vec![defines(F, "other", 5, true)]))
        ),
        (
            Err(
                "GenerationIngested payload: missing field `prefix` at line 1 column 2".to_string()
            ),
            0
        )
    );
    assert_eq!(
        fold_through_port(
            port,
            &entry(F, "h1", 8, 20),
            Ok(Some(vec![defines(F, "other", 5, true)]))
        ),
        RE_RECORDED,
        "position 8 was not recorded by the entry that did not parse"
    );

    // A batch event's fold fails after an earlier one folded.
    let poison = common::fixtures::POISON;
    let rejected = serde_json::from_slice::<serde_json::Value>(poison)
        .unwrap_err()
        .to_string();
    let third = entry("src/g.rs", "k1", 9, 30);
    assert_eq!(
        fold_through_port(
            port,
            &third,
            Ok(Some(vec![
                defines("src/g.rs", "beta", 1, true),
                Event::new(TYPE_CODE_ENTITY_EXTRACTED, poison.to_vec()),
            ]))
        ),
        (Err(rejected), 1)
    );
    assert_eq!(
        generation(port, "gc/src/g.rs"),
        None,
        "the generation the first batch event installed rolled back with it"
    );
    assert_eq!(node_ids(&p), vec![F.to_string(), format!("{F}::alpha")]);
    assert_eq!(
        fold_through_port(port, &third, Ok(None)),
        BATCH_ASKED,
        "position 9 was not recorded by the batch that failed"
    );
    assert_eq!(generation(port, "gc/src/g.rs"), None);
}

#[test]
fn a_ledger_entry_the_public_fold_refused_folds_afterwards_through_apply_generation() {
    let refused = common::fixtures::entry_refusal();
    let p = Projector::open(":memory:", "test").unwrap();
    let port: &dyn Projection = &p;

    // One entry alone.
    let alone = entry(F, "h1", 10, 40);
    assert_eq!(Fold::of(wired(Some(port)), &alone), refused);
    assert_eq!(generation(port, IDENTITY), None);
    assert_eq!(
        fold_through_port(port, &alone, Ok(Some(vec![defines(F, "alpha", 2, true)]))),
        BATCH_ASKED,
        "position 10 was not recorded by the refusal"
    );
    assert_eq!(generation(port, IDENTITY), Some("h1".to_string()));
    assert_eq!(live_edges(&p), vec![contains("alpha", 40, 10)]);

    // A batch holding an entry is refused whole: the decision before the entry did not land.
    let batch = [decision("d1", 11, 50), entry(F, "h2", 12, 50)];
    assert_eq!(common::fixtures::fold_of(port, &batch), refused);
    assert_eq!(
        live_node_ids(&p.whole().unwrap(), KIND_DECISION),
        Vec::<String>::new()
    );
    assert_eq!(Fold::of(wired(Some(port)), &batch[0]), Fold::Folded);
    assert_eq!(
        live_node_ids(&p.whole().unwrap(), KIND_DECISION),
        vec!["d1".to_string()],
        "position 11 was not recorded by the refused batch, so the decision folds now"
    );
    assert_eq!(
        fold_through_port(port, &batch[1], Ok(None)),
        BATCH_ASKED,
        "position 12 was not recorded by the refused batch"
    );
    assert_eq!(generation(port, IDENTITY), Some("h1".to_string()));
}

/// A second projector opened on the `graph.db` of `dir`, after a fold into it failed or was
/// refused: it owes nothing and no owed mark stands beside the file (spec 124, AN APPENDER WAITS
/// UNDER THE CEILING AND NEVER MARKS).
fn a_second_projector_owing_nothing(dir: &tempfile::TempDir) -> Projector {
    let second = Projector::open(&graph_db(dir), "test").unwrap();
    assert_eq!(
        (
            second.owed_because().unwrap(),
            common::fixtures::mark_stands_beside(graph_db(dir).as_ref())
        ),
        (Vec::<&str>::new(), false),
        "the fold that failed or was refused wrote no mark"
    );
    second
}

#[test]
fn a_refused_public_fold_writes_no_mark_and_a_second_projector_folds_the_entry() {
    let dir = tempfile::tempdir().unwrap();
    let first = Projector::open(&graph_db(&dir), "test").unwrap();
    assert_eq!(
        fold_through_port(&first, &entry(F, "h1", 7, 10), alpha_answer()),
        BATCH_ASKED
    );
    let refused = entry(F, "h2", 8, 20);
    assert_eq!(
        Fold::of(wired(Some(&first as &dyn Projection)), &refused),
        common::fixtures::entry_refusal()
    );

    let second = a_second_projector_owing_nothing(&dir);
    assert_eq!(
        fold_through_port(
            &second,
            &refused,
            Ok(Some(vec![defines(F, "alpha", 2, true)]))
        ),
        BATCH_ASKED,
        "position 8 was not recorded by the refusal, so the second projector folds it"
    );
    assert_eq!(generation(&second, IDENTITY), Some("h2".to_string()));
}

#[test]
fn a_failed_ledger_fold_writes_no_mark_and_a_second_projector_folds_the_position() {
    let dir = tempfile::tempdir().unwrap();
    let first = Projector::open(&graph_db(&dir), "test").unwrap();
    let failed = entry(F, "h1", 7, 10);
    assert_eq!(
        fold_through_port(&first, &failed, common::fixtures::blob_unread()),
        (Err(common::fixtures::BLOB_UNREAD.to_string()), 1)
    );

    let second = a_second_projector_owing_nothing(&dir);
    assert_eq!(
        fold_through_port(&second, &failed, alpha_answer()),
        BATCH_ASKED,
        "position 7 was not recorded by the failure, so the second projector folds it"
    );
    assert_eq!(generation(&second, IDENTITY), Some("h1".to_string()));
}

#[test]
fn a_second_projector_folding_the_same_generation_meanwhile_waits_and_meets_a_re_recording() {
    let dir = tempfile::tempdir().unwrap();
    let path = graph_db(&dir);
    let first = Projector::open(&path, "test").unwrap();
    let second = Projector::open(&path, "test").unwrap();
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();

    let (first_outcome, second_outcome) = std::thread::scope(|s| {
        let first = &first;
        let second = &second;
        // The first fold stops inside its batch function, its transaction open.
        let holding = s.spawn(move || {
            first
                .apply_generation(
                    &entry(F, "h1", 7, 10),
                    Box::new(move || {
                        entered_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        alpha_answer()
                    }),
                )
                .map_err(|e| e.0)
        });
        entered_rx.recv().unwrap();
        // The second folds an entry of the same generation at the next position while the first
        // still holds its transaction.
        let waiting = s.spawn(move || {
            fold_through_port(
                second,
                &entry(F, "h1", 8, 20),
                Ok(Some(vec![defines(F, "other", 5, true)])),
            )
        });
        std::thread::sleep(Duration::from_millis(300));
        release_tx.send(()).unwrap();
        (holding.join().unwrap(), waiting.join().unwrap())
    });

    assert_eq!(first_outcome, Ok(EntryFold::BatchAsked));
    assert_eq!(
        second_outcome, RE_RECORDED,
        "the second read the generation only after the first installed it"
    );
    let third = Projector::open(&path, "test").unwrap();
    assert_eq!(generation(&third, IDENTITY), Some("h1".to_string()));
    assert_eq!(live_edges(&third), vec![contains("alpha", 10, 7)]);
    assert!(!third.rebuild_owed().unwrap());
}

/// Spec 122, EXTRACTION OUTSIDE THE WRITE TRANSACTION, through the public rebuild: given a
/// `graph.db` whose rebuild's tail meets a ledger entry appended while its shadow folded, when a
/// writer holding a connection of its own to that file folds a decision while the tail's
/// `reextract` blocks on a barrier for the entry, then the writer's fold lands, and the rebuilt
/// file holds what folding the entry with its batch in hand, after the writer's decision, gives.
#[test]
fn a_writer_folds_into_the_file_while_its_rebuild_tail_extracts_an_entry_and_the_entry_folds_as_in_hand(
) {
    let dir = tempfile::tempdir().unwrap();
    let path = graph_db(&dir);
    let log = vec![
        decision("d6", 6, 6),
        entry(F, "h1", 7, 10),
        decision("d8", 8, 12),
    ];
    let writer = Projector::open(&path, "test").unwrap();
    let reads = Cell::new(0);
    let written = common::fixtures::rebuild_meeting_a_writer(
        &path,
        true,
        &mut grown_while_the_shadow_folded(&log, &reads, || {}),
        &writer,
        &log[2..],
        &alpha_batch(),
    );
    assert_eq!(
        (written, reads.get()),
        (Fold::Folded, 2),
        "the writer's fold lands while the tail extracts the entry's batch"
    );
    assert_eq!(applied_positions(Path::new(&path)), vec![6, 7, 8]);
    // The same events folded in the order they committed: the writer's decision before the entry.
    let (in_hand, oracle) = oracle_file(&dir);
    common::fixtures::folds(&oracle, &log[..1]);
    common::fixtures::folds(&oracle, &log[2..]);
    assert_eq!(
        fold_through_port(&oracle, &log[1], alpha_answer()),
        BATCH_ASKED
    );
    assert_same_graph(&path, &in_hand);
}

/// Spec 122, EXTRACTION OUTSIDE THE WRITE TRANSACTION, through the public rebuild: given a
/// `graph.db` whose rebuild's tail meets a ledger entry that a writer holding a connection of its
/// own folded through the port after the swap and before the tail read the log - and then a newer
/// generation of its identity over it, so only the entry's `applied` row says it was folded - when
/// the tail reaches the entry, then it never asks the entry's batch, and the file holds the
/// writer's folds.
#[test]
fn a_rebuild_tail_never_extracts_an_entry_a_writer_folded_before_the_tail_read_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = graph_db(&dir);
    let log = vec![
        decision("d6", 6, 6),
        entry(F, "h1", 7, 10),
        entry(F, "h2", 8, 20),
    ];
    let writer = Projector::open(&path, "test").unwrap();
    let (written, reads, asks) = tail_after_a_writer(&path, &log, || {
        log[1..]
            .iter()
            .map(|e| fold_through_port(&writer, e, alpha_answer()))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        (written, reads, asks),
        (Some(vec![BATCH_ASKED; 2]), 2, 0),
        "the tail asks no batch for an entry the file already folded"
    );
    assert_eq!(applied_positions(Path::new(&path)), vec![6, 7, 8]);
    let (in_hand, oracle) = oracle_file(&dir);
    common::fixtures::folds(&oracle, &log[..1]);
    for e in &log[1..] {
        assert_eq!(fold_through_port(&oracle, e, alpha_answer()), BATCH_ASKED);
    }
    assert_same_graph(&path, &in_hand);
}

/// Spec 122, EXTRACTION OUTSIDE THE WRITE TRANSACTION, through the public rebuild: given a
/// `graph.db` whose rebuild's tail meets a ledger entry of the generation a writer holding a
/// connection of its own installed through the port, from a later entry, after the swap and
/// before the tail read the log, when the tail reaches the earlier entry, then it never asks its
/// batch - a re-recording - and records its position alone.
#[test]
fn a_rebuild_tail_never_extracts_a_re_recording_of_the_generation_a_writer_installed() {
    let dir = tempfile::tempdir().unwrap();
    let path = graph_db(&dir);
    let log = vec![
        decision("d6", 6, 6),
        entry(F, "h1", 7, 10),
        entry(F, "h1", 9, 20),
    ];
    let writer = Projector::open(&path, "test").unwrap();
    let (written, reads, asks) = tail_after_a_writer(&path, &log, || {
        fold_through_port(&writer, &log[2], alpha_answer())
    });
    assert_eq!(
        (written, reads, asks),
        (Some(BATCH_ASKED), 2, 0),
        "the tail asks no batch for a re-recording"
    );
    assert_eq!(applied_positions(Path::new(&path)), vec![6, 7, 9]);
    let (in_hand, oracle) = oracle_file(&dir);
    common::fixtures::folds(&oracle, &log[..1]);
    assert_eq!(
        fold_through_port(&oracle, &log[2], alpha_answer()),
        BATCH_ASKED
    );
    assert_eq!(
        fold_through_port(&oracle, &log[1], alpha_answer()),
        RE_RECORDED
    );
    assert_same_graph(&path, &in_hand);
}

/// The one definition `alpha` of [`F`], as an entry's batch.
fn alpha_batch() -> Vec<Event> {
    vec![defines(F, "alpha", 1, true)]
}

/// A batch function's answer that resolves the entry to [`alpha_batch`].
fn alpha_answer() -> Result<Option<Vec<Event>>, Error> {
    Ok(Some(alpha_batch()))
}

/// A rebuild source over `log` for a `graph.db` whose log grew while its rebuild's shadow folded:
/// the first read - the shadow's - finds `log[..1]`, and each later read - the tail's - finds it
/// whole, `before_tail` run once just before the first of them. `reads` counts the reads.
fn grown_while_the_shadow_folded<'a>(
    log: &'a [Event],
    reads: &'a Cell<u32>,
    mut before_tail: impl FnMut() + 'a,
) -> impl FnMut(u64, &mut RebuildSink) -> Result<(), Error> + 'a {
    move |after, sink| {
        reads.set(reads.get() + 1);
        if reads.get() == 2 {
            before_tail();
        }
        let read = if reads.get() == 1 { &log[..1] } else { log };
        common::fixtures::one_read(read, after, sink)
    }
}

/// Rebuild the `graph.db` at `path`, owed, from `log` grown while its shadow folded
/// ([`grown_while_the_shadow_folded`]), `write` run just before the tail reads it, answering
/// [`alpha_batch`] for the batch of each entry it is asked for: what `write` answered, how many
/// reads the rebuild made, and how many times it asked an entry's batch.
fn tail_after_a_writer<T>(
    path: &str,
    log: &[Event],
    write: impl FnOnce() -> T,
) -> (Option<T>, u32, u32) {
    let reads = Cell::new(0);
    let mut write = Some(write);
    let mut written = None;
    let mut asks = 0;
    common::fixtures::rebuild_clean(
        path,
        true,
        &mut grown_while_the_shadow_folded(log, &reads, || {
            written = write.take().map(|write| write());
        }),
        &mut |_| {
            asks += 1;
            alpha_answer()
        },
    );
    (written, reads.get(), asks)
}

/// A fresh graph file `in_hand.db` in `dir` and a projector on it, the oracle a rebuilt file is
/// compared with ([`assert_same_graph`]).
fn oracle_file(dir: &tempfile::TempDir) -> (String, Projector) {
    let in_hand = dir.path().join("in_hand.db").to_str().unwrap().to_string();
    let oracle = Projector::open(&in_hand, "test").unwrap();
    (in_hand, oracle)
}

/// Insist the graph files at `rebuilt` and `in_hand` hold one graph ([`common::cli::graph_identity`]).
fn assert_same_graph(rebuilt: &str, in_hand: &str) {
    assert_eq!(
        common::cli::graph_identity(Path::new(rebuilt), "test"),
        common::cli::graph_identity(Path::new(in_hand), "test")
    );
}

/// The decision `id`, at position `pos`, valid from `secs`.
fn decision(id: &str, pos: u64, secs: u64) -> Event {
    placed_at(
        event_of(TYPE_DECISION_MADE, decision_json(id, "x", &["a.rs"], "")),
        pos,
        secs,
    )
}

/// The entry [`RAW_PAYLOAD`] records.
fn raw_named() -> GenerationIngested {
    named("src/a.rs", "h1", "b10b", true)
}

#[test]
fn an_entrys_payload_is_five_fields_on_the_log_and_reads_back_as_written() {
    assert_eq!(
        String::from_utf8(serde_json::to_vec(&raw_named()).unwrap()).unwrap(),
        RAW_PAYLOAD,
        "the on-log bytes of an entry"
    );
    assert_eq!(
        GenerationIngested::parse(RAW_PAYLOAD.as_bytes()),
        Ok(raw_named())
    );
    assert_eq!(raw_named().identity(), "gc/src/a.rs");
    // The constructor's event carries those bytes.
    assert_eq!(
        String::from_utf8(raw_named().event(1).data).unwrap(),
        RAW_PAYLOAD
    );
}

#[test]
fn a_payload_missing_any_of_the_five_fields_is_refused_naming_the_field() {
    for field in ["prefix", "file", "generation", "blob", "excluded"] {
        let mut payload: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(RAW_PAYLOAD).unwrap();
        assert!(payload.remove(field).is_some(), "{field} is a field");
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert_eq!(
            GenerationIngested::parse(&bytes),
            Err(format!(
                "GenerationIngested payload: missing field `{field}` at line 1 column {}",
                bytes.len()
            ))
        );
    }
}

#[test]
fn a_payload_carrying_a_key_a_later_writer_adds_still_reads_and_trailing_bytes_are_refused() {
    let later = RAW_PAYLOAD.replace("}", r#","later":1}"#);
    assert_eq!(GenerationIngested::parse(later.as_bytes()), Ok(raw_named()));
    let trailing = format!("{RAW_PAYLOAD} x");
    assert_eq!(
        GenerationIngested::parse(trailing.as_bytes()),
        Err(format!(
            "GenerationIngested payload: trailing characters at line 1 column {}",
            RAW_PAYLOAD.len() + 2
        ))
    );
}

/// The type of every event in `root`'s run stream, oldest first.
fn run_types(root: &std::path::Path) -> Vec<String> {
    read_run_events(root).into_iter().map(|e| e.type_).collect()
}

#[test]
fn rigger_emit_refuses_a_ledger_entry_appends_nothing_and_leaves_the_graph_owing_nothing() {
    // Given a project with a store.
    let dir = temp_store_project();
    let root = dir.path();

    // When the operator emits a ledger entry by hand.
    let (_, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "GenerationIngested",
            r#"{"prefix":"gc","file":"src/f.rs","generation":"h1","blob":"","excluded":false}"#,
        ],
    );

    // Then the emit is refused, nothing is appended and no fold was lost.
    assert!(!ok, "the emit must fail; stderr:\n{err}");
    assert!(err.contains(EMIT_REFUSED), "stderr:\n{err}");
    assert_eq!(run_types(root), Vec::<String>::new());
    assert!(!rigger_file(root, "graph.db.owed").exists());

    // And the same surface appends an event an agent may emit, so the empty stream above is a
    // stream this read would have seen an entry in.
    let (_, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d1","summary":"s","governs":["src/f.rs"],"supersedes":""}"#,
        ],
    );
    assert!(ok, "an agent context event is emitted; stderr:\n{err}");
    assert_eq!(run_types(root), vec!["DecisionMade".to_string()]);
    assert!(!rigger_file(root, "graph.db.owed").exists());
}

/// The stream the ledger form's tests append to.
const LEDGER_STREAM: &str = "run";

/// What the ledger form answered, as one comparable value: the position the store issued for the
/// entry, what became of its fold, and the outcome the fold named.
type Recorded = (Option<u64>, Fold, Option<EntryFold>);

/// [`Recorded`] of `done`.
fn recorded(done: EntryAppendedAndFolded) -> Recorded {
    (done.appended.last(), done.fold, done.outcome)
}

/// Every event of [`LEDGER_STREAM`] in `store` as `(position, type, payload text)`, in log order.
fn logged(store: &dyn EventStore) -> Vec<(u64, String, String)> {
    store
        .read_stream(LEDGER_STREAM, 0, Direction::Forward)
        .unwrap()
        .into_iter()
        .map(|e| (e.position, e.type_, String::from_utf8(e.data).unwrap()))
        .collect()
}

/// A line sink that keeps every line it is handed.
#[derive(Default)]
struct Said(std::sync::Mutex<Vec<String>>);

impl Said {
    fn say(&self, line: &str) {
        self.0.lock().unwrap().push(line.to_string());
    }

    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

#[test]
fn the_ledger_form_appends_each_entry_once_folds_it_with_its_batch_and_writes_one_applied_row() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("events.db").to_str().unwrap()).unwrap();
    let graph_path = dir.path().join("graph.db");
    let p = Projector::open(graph_path.to_str().unwrap(), "test").unwrap();
    let said = Said::default();
    let log = |line: &str| said.say(line);
    let folding = folding_into(&store, Some(&p as &dyn Projection), &log);
    let payload = |generation: &str, blob: &str| {
        format!(
            r#"{{"prefix":"gc","file":"src/f.rs","generation":"{generation}","blob":"{blob}","excluded":false}}"#
        )
    };

    // A first generation: its batch folds at the position the store issued for the entry.
    let first = recorded(
        folding
            .append_entry_and_fold(
                LEDGER_STREAM,
                &named(F, "h1", "b1", false).event(1),
                alpha_batch(),
            )
            .unwrap(),
    );
    let Some(first_at) = first.0 else {
        panic!("the store placed the first entry");
    };
    assert_eq!(
        first,
        (Some(first_at), Fold::Folded, Some(EntryFold::BatchAsked))
    );
    assert_eq!(generation(&p, IDENTITY), Some("h1".to_string()));
    assert_eq!(applied_positions(&graph_path), vec![first_at]);

    // The same generation again: a re-recording, whose batch is never folded.
    let again = recorded(
        folding
            .append_entry_and_fold(
                LEDGER_STREAM,
                &named(F, "h1", "b1", false).event(1),
                vec![defines(F, "never", 9, true)],
            )
            .unwrap(),
    );
    let Some(again_at) = again.0 else {
        panic!("the store placed the second entry");
    };
    assert_eq!(
        again,
        (Some(again_at), Fold::Folded, Some(EntryFold::ReRecording))
    );
    assert_eq!(applied_positions(&graph_path), vec![first_at, again_at]);
    assert_eq!(node_ids(&p), vec![F.to_string(), format!("{F}::alpha")]);

    // Another generation: its batch replaces the first's facts.
    let next = recorded(
        folding
            .append_entry_and_fold(
                LEDGER_STREAM,
                &named(F, "h2", "b2", false).event(1),
                vec![defines(F, "beta", 2, true)],
            )
            .unwrap(),
    );
    let Some(next_at) = next.0 else {
        panic!("the store placed the third entry");
    };
    assert_eq!(
        next,
        (Some(next_at), Fold::Folded, Some(EntryFold::BatchAsked))
    );
    assert_eq!(generation(&p, IDENTITY), Some("h2".to_string()));
    assert_eq!(
        applied_positions(&graph_path),
        vec![first_at, again_at, next_at],
        "one applied row per entry, at the entry's own position"
    );
    assert_eq!(
        live_edges(&p)
            .into_iter()
            .map(|(_, _, to, _, source)| (to, source))
            .collect::<Vec<_>>(),
        vec![(format!("{F}::beta"), next_at)]
    );

    // The log holds the three entries and no batch event.
    let entry_type = "GenerationIngested".to_string();
    assert_eq!(
        logged(&store),
        vec![
            (first_at, entry_type.clone(), payload("h1", "b1")),
            (again_at, entry_type.clone(), payload("h1", "b1")),
            (next_at, entry_type, payload("h2", "b2")),
        ]
    );
    assert_eq!(said.lines(), Vec::<String>::new());
}

#[test]
fn the_ledger_form_answers_a_fold_it_could_not_make_beside_an_entry_that_is_on_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("events.db").to_str().unwrap()).unwrap();
    let graph_path = dir.path().join("graph.db");
    let p = Projector::open(graph_path.to_str().unwrap(), "test").unwrap();
    let said = Said::default();
    let log = |line: &str| said.say(line);
    let folding = folding_into(&store, Some(&p as &dyn Projection), &log);

    // A batch event the fold rejects: the fold fails, the entry stays on the log, and the graph
    // holds no applied row for it and writes no owed mark (spec 124, AN APPENDER WAITS UNDER THE
    // CEILING AND NEVER MARKS).
    let poison = common::fixtures::POISON;
    let rejected = serde_json::from_slice::<serde_json::Value>(poison)
        .unwrap_err()
        .to_string();
    let failed = recorded(
        folding
            .append_entry_and_fold(
                LEDGER_STREAM,
                &named(F, "h1", "b1", false).event(2),
                vec![
                    defines(F, "alpha", 1, true),
                    Event::new(TYPE_CODE_ENTITY_EXTRACTED, poison.to_vec()),
                ],
            )
            .unwrap(),
    );
    let Some(failed_at) = failed.0 else {
        panic!("the store placed the entry whose fold failed");
    };
    let lost = Fold::NotFolded(common::fixtures::lost_fold(
        &common::fixtures::graph_error(&rejected),
        common::fixtures::SETUP_PAYS,
    ));
    assert_eq!(failed, (Some(failed_at), lost.clone(), None));
    assert_eq!(p.owed_because().unwrap(), Vec::<&str>::new());
    assert_eq!(applied_positions(&graph_path), Vec::<u64>::new());
    assert_eq!(generation(&p, IDENTITY), None);

    // The graph owes nothing: the next entry is appended and folded with its batch.
    let next = recorded(
        folding
            .append_entry_and_fold(
                LEDGER_STREAM,
                &named(F, "h2", "b2", false).event(1),
                vec![defines(F, "beta", 2, true)],
            )
            .unwrap(),
    );
    let Some(next_at) = next.0 else {
        panic!("the store placed the entry folded after the failure");
    };
    assert_eq!(
        next,
        (Some(next_at), Fold::Folded, Some(EntryFold::BatchAsked))
    );
    assert_eq!(applied_positions(&graph_path), vec![next_at]);
    assert_eq!(
        logged(&store)
            .into_iter()
            .map(|(position, type_, _)| (position, type_))
            .collect::<Vec<_>>(),
        vec![
            (failed_at, "GenerationIngested".to_string()),
            (next_at, "GenerationIngested".to_string()),
        ]
    );

    // The form itself says nothing; its caller says a lost fold through the store's own log, in
    // the words every other lost fold is said in, and says nothing of a fold that was made.
    assert_eq!(said.lines(), Vec::<String>::new());
    let owed = common::fixtures::owed_refusal();
    folding.say_fold_lost(1, &Fold::Folded);
    folding.say_fold_lost(1, &lost);
    folding.say_fold_lost(3, &owed);
    assert_eq!(
        said.lines(),
        vec![
            common::fixtures::recorded_not_folded(
                1,
                &common::fixtures::lost_fold(&rejected, common::fixtures::SETUP_PAYS),
            ),
            common::fixtures::recorded_not_folded(3, REBUILD_OWED),
        ]
    );
}

#[test]
fn the_ledger_form_wired_to_no_graph_appends_the_entry_and_says_nothing_of_its_fold() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("events.db").to_str().unwrap()).unwrap();
    let said = Said::default();
    let log = |line: &str| said.say(line);
    let folding = folding_into(&store, None, &log);

    let done = recorded(
        folding
            .append_entry_and_fold(
                LEDGER_STREAM,
                &named(F, "h1", "b1", false).event(1),
                alpha_batch(),
            )
            .unwrap(),
    );
    let Some(placed_at) = done.0 else {
        panic!("the store placed the entry");
    };
    let unwired = Fold::NotFolded(common::fixtures::unwired());
    assert_eq!(done, (Some(placed_at), unwired.clone(), None));
    assert_eq!(logged(&store).len(), 1);
    folding.say_fold_lost(1, &unwired);
    assert_eq!(said.lines(), Vec::<String>::new());
}

#[test]
fn the_ledger_form_never_opens_the_graph_for_an_entry_the_store_did_not_place() {
    let said = Said::default();
    let log = |line: &str| said.say(line);
    let opened = std::sync::atomic::AtomicUsize::new(0);
    let p = Projector::open(":memory:", "test").unwrap();
    let folding = FoldingStore::new(
        &common::fixtures::SilentStore,
        Some(|| {
            opened.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            wired(Some(&p as &dyn Projection))
        }),
        &log,
    );

    let done = recorded(
        folding
            .append_entry_and_fold(
                LEDGER_STREAM,
                &named(F, "h1", "b1", false).event(1),
                alpha_batch(),
            )
            .unwrap(),
    );
    assert_eq!(done, (None, Fold::Folded, None));
    assert_eq!(opened.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(generation(&p, IDENTITY), None);

    // A placed entry opens it exactly once.
    let store = Store::open(":memory:").unwrap();
    let folding = FoldingStore::new(
        &store,
        Some(|| {
            opened.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            wired(Some(&p as &dyn Projection))
        }),
        &log,
    );
    let done = folding
        .append_entry_and_fold(
            LEDGER_STREAM,
            &named(F, "h1", "b1", false).event(1),
            alpha_batch(),
        )
        .unwrap();
    assert_eq!(
        (done.fold, done.outcome),
        (Fold::Folded, Some(EntryFold::BatchAsked))
    );
    assert_eq!(opened.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(generation(&p, IDENTITY), Some("h1".to_string()));
    assert_eq!(said.lines(), Vec::<String>::new());
}

#[test]
fn settle_names_a_fold_that_was_made_and_one_that_was_not_with_its_reason() {
    assert_eq!(
        [
            Fold::settle(Ok(())),
            Fold::settle(Err(Error("the write was refused".to_string())))
        ],
        [
            Fold::Folded,
            Fold::NotFolded(common::fixtures::graph_error("the write was refused"))
        ]
    );
}
