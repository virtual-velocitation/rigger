//! Periphery (CLI / store / fold) tests for spec 60, criterion 5 - SUPPORTED COMPACTION.
//!
//! `rigger reset --derived` prunes what edits and the pre-dedup ingest accreted in the log's
//! derived index. For each of the four derived index types it keeps only each file's LATEST
//! generation, at the LATEST event per distinct replay key, deletes every superseded generation
//! and re-recording, and vacuums, so the file shrinks on disk. Every non-derived event survives
//! byte-for-byte, the graph projection stays consistent, and the compacted store is still a
//! working store.
//!
//! These tests drive the COMPILED binary against a real `.rigger/events.db`, because the criterion
//! is an operator-facing command whose observable effects are on-disk: which rows survive, what the
//! file weighs, what the command printed, and whether the store still reads and appends afterwards.
//!
//! What this file OWNS (criterion 5) and what it deliberately does not:
//!
//!   - OWNS: the `--derived` prune's selection rule (each file's latest generation, then the
//!     latest recording per distinct replay key, per derived type; spec 101, criterion 4 owns the
//!     latest-generation rule), the non-derived preservation, the on-disk shrink, the printed
//!     report, the loud refusal on a non-embedded backend, the composition with `--runs`, and the
//!     usability of the compacted store (`rigger validate` reads it, and it still accepts appends).
//!   - NOT OWNED: the run-path ingest dedup (criterion 1), the run-scoping boundary (criterion 2),
//!     the revert proof (criterion 3), and the storage guard (criterion 4). Those are pinned by
//!     their own units' tests and are not re-litigated here.

mod common;

use common::cli::keyed;
use common::cli::read_run_events;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_rigger_envs;
use common::cli::run_stream_identity;
use common::cli::temp_rigger_project;
use common::fixtures::edge_inferred;
use common::fixtures::file_len;
use common::fixtures::meta_replay_key;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

// ---------------------------------------------------------------------------------------
// The seeded log: an ALREADY-BLOATED store, exactly the shape this command exists to prune
// ---------------------------------------------------------------------------------------

/// How many times each derived batch was re-recorded before the project-scoped dedup existed.
/// Large enough that the pruned rows span many database pages, so the VACUUM's shrink is a real
/// on-disk effect and not a rounding artifact.
const ROUNDS: usize = 500;

/// The replay keys the seed records, in the `<prefix>/<file>@<hash>#<i>` form `ingest::key_batch`
/// builds. `src/b.rs` is recorded at TWO content generations, so the prune is proven to shed the
/// superseded one whole and keep only the latest generation's recording.
const KEY_A_DEF: &str = "gc/src/a.rs@h1#0";
const KEY_A_REF: &str = "gc/src/a.rs@h1#1";
const KEY_B_GEN1: &str = "gc/src/b.rs@h1#0";
const KEY_B_GEN2: &str = "gc/src/b.rs@h2#0";
/// The DESIGN-INTENT half of the derived index, keyed under the design prefix `gd`. It is seeded
/// deliberately rather than left at zero: a `DocLinkExtracted` folds into a bitemporal `SPECIFIES`
/// edge whose `valid_from` is the EARLIEST recording of the fact, so it is the one derived class a
/// prune that merely dropped rows would silently re-date. A proof that pinned only the code half
/// would pass while the design-intent layer diverged.
const KEY_D_SPEC: &str = "gd/docs/design.md@h1#0";

/// One row of the event log, as the table holds it. Comparing these tuples across the compaction
/// is what "survives byte-for-byte" MEANS: same global position, stream, type, id, payload bytes,
/// metadata, valid-from, recorded-at, and per-stream revision.
type Row = (i64, String, String, String, Vec<u8>, String, i64, i64, i64);

fn rows(db: &Path) -> Vec<Row> {
    let conn = rusqlite::Connection::open(db).expect("open the event log");
    let mut stmt = conn
        .prepare(
            "SELECT position, stream, type, id, data, meta, valid_from, recorded_at, revision \
             FROM events ORDER BY position",
        )
        .unwrap();
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    out
}

fn derived(row: &Row) -> bool {
    rigger::ingest::is_derived_index_type(&row.2)
}

/// A `CodeEntityExtracted` payload in the on-log JSON form, built raw so the test pins the wire
/// contract rather than an in-crate struct it cannot name.
fn code_entity(file: &str, name: &str, line: u32, fresh: bool) -> Vec<u8> {
    let mut v = serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
    });
    if fresh {
        v["fresh"] = serde_json::Value::Bool(true);
    }
    serde_json::to_vec(&v).unwrap()
}

/// A `DocLinkExtracted` payload in the on-log JSON form: one design-intent link, folded into a
/// `<doc> --SPECIFIES--> <code>` edge whose valid-time is when the design fact FIRST became true.
fn doc_link(from: &str, to: &str, rel: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "from": from, "to": to, "rel": rel })).unwrap()
}

/// Seed `root` with a bloated event log: `ROUNDS` re-recordings of every derived batch (the
/// duplication the pre-dedup ingest accumulated), alongside the events the prune must never touch.
///
/// The events the prune MUST leave alone are seeded deliberately, each one closing a different way
/// the rule could be got wrong:
///
///   - two IDENTICAL `ReviewFinding`s: domain events legitimately repeat, and both must survive.
///   - two `DecisionMade`s carrying a DERIVED-LOOKING replay key: the partition is decided BY TYPE
///     FIRST, so a non-derived event is ineligible however its key is spelled.
///   - two derived events carrying NO replay key: a key is what names a content generation, so an
///     event without one is never provably redundant and appends through (the fail-safe direction).
///   - `src/b.rs` at two content generations: every recording of the superseded one is shed.
fn seed_bloated_log(root: &Path) {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));

    let mut events: Vec<Event> = Vec::with_capacity(ROUNDS * 4 + 8);

    events.push(
        Event::new("RunStarted", br#"{"run":"r1","criteria":["c"]}"#.to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(10)),
    );
    for _ in 0..2 {
        events.push(
            Event::new(
                "ReviewFinding",
                br#"{"id":"f1","summary":"raised twice"}"#.to_vec(),
            )
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(11)),
        );
    }
    for _ in 0..2 {
        events.push(
            Event::new(
                "DecisionMade",
                br#"{"id":"d1","summary":"s","governs":["src/a.rs"],"supersedes":""}"#.to_vec(),
            )
            // A NON-derived event whose replay key is spelled exactly like a derived one.
            .with_meta(rigger::ingest::META_REPLAY_KEY, KEY_A_DEF)
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(12)),
        );
    }
    for _ in 0..2 {
        events.push(
            Event::new(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity("src/keyless.rs", "unkeyed", 1, false),
            )
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(13)),
        );
    }

    for r in 0..ROUNDS {
        let secs = 1_000 + r as u64;
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity("src/a.rs", "alpha", 1, true),
            KEY_A_DEF,
            secs,
        ));
        events.push(keyed(
            rigger::contextgraph::TYPE_EDGE_INFERRED,
            edge_inferred("src/a.rs", "beta"),
            KEY_A_REF,
            secs,
        ));
    }
    for r in 0..ROUNDS {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity("src/b.rs", "gen_one", 1, true),
            KEY_B_GEN1,
            2_000 + r as u64,
        ));
    }
    for r in 0..ROUNDS {
        events.push(keyed(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            code_entity("src/b.rs", "gen_two", 2, true),
            KEY_B_GEN2,
            3_000 + r as u64,
        ));
    }
    // The DESIGN-INTENT half, re-recorded exactly like the code half. Its valid-time RISES with
    // every re-recording, so the fold's earliest-assertion date is the FIRST round's - the fact
    // this prune must not re-date when it keeps the last round's row.
    for r in 0..ROUNDS {
        events.push(keyed(
            rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED,
            doc_link(
                "docs/design.md",
                "src/a.rs",
                rigger::contextgraph::REL_SPECIFIES,
            ),
            KEY_D_SPEC,
            4_000 + r as u64,
        ));
    }

    for chunk in events.chunks(500) {
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, chunk)
            .unwrap();
    }

    // Fold the WAL into the main file so a size measured right after seeding is the real one.
    let conn = rusqlite::Connection::open(rigger_file(root, "events.db")).unwrap();
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
}

/// How many rows the prune must remove per derived type, given the seed above: every recording of
/// a superseded generation (`src/b.rs`'s first, all `ROUNDS` of it), and every recording of a
/// surviving key except its latest.
const REMOVED_CODE_ENTITIES: usize = 3 * ROUNDS - 2;
const REMOVED_EDGES: usize = ROUNDS - 1;
const REMOVED_DOC_LINKS: usize = ROUNDS - 1;

// ---------------------------------------------------------------------------------------
// The selection rule
// ---------------------------------------------------------------------------------------

#[test]
fn reset_derived_keeps_only_the_latest_recording_of_the_latest_generation_of_each_file() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_bloated_log(root);

    let before = rows(&rigger_file(root, "events.db"));
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    let after = rows(&rigger_file(root, "events.db"));

    // For every replay key of each file's LATEST generation, exactly ONE row survives, and it is
    // the row the log recorded LAST - the file's current recording.
    for key in [KEY_A_DEF, KEY_A_REF, KEY_B_GEN2, KEY_D_SPEC] {
        let kept: Vec<&Row> = after
            .iter()
            .filter(|r| derived(r) && meta_replay_key(&r.5).as_deref() == Some(key))
            .collect();
        assert_eq!(
            kept.len(),
            1,
            "exactly one derived event must survive for {key}; got {}",
            kept.len()
        );
        let latest = before
            .iter()
            .filter(|r| derived(r) && meta_replay_key(&r.5).as_deref() == Some(key))
            .map(|r| r.0)
            .max()
            .expect("the seed recorded this key");
        assert_eq!(
            kept[0].0, latest,
            "the surviving recording of {key} must be the LATEST one the log held"
        );
    }

    // A superseded content generation is shed whole (spec 101, criterion 4): `src/b.rs` keeps
    // only its latest generation's recording.
    let b_rows: Vec<String> = after
        .iter()
        .filter(|r| derived(r))
        .filter_map(|r| meta_replay_key(&r.5))
        .filter(|k| k.starts_with("gc/src/b.rs@"))
        .collect();
    assert_eq!(
        b_rows,
        vec![KEY_B_GEN2.to_string()],
        "only src/b.rs's latest generation may keep a recording; got {b_rows:?}"
    );
}

// ---------------------------------------------------------------------------------------
// The fail-safe direction: nothing else may be dropped, reordered, or rewritten
// ---------------------------------------------------------------------------------------

#[test]
fn reset_derived_preserves_every_non_derived_event_and_every_keyless_derived_event_byte_for_byte() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_bloated_log(root);

    let before = rows(&rigger_file(root, "events.db"));
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    let after = rows(&rigger_file(root, "events.db"));

    // Byte-for-byte, in order: every column of every non-derived row is unchanged, INCLUDING its
    // global position and its per-stream revision. The compaction moves the store's revision
    // CURSOR, it never rewrites a row.
    let survivors_expected: Vec<&Row> = before.iter().filter(|r| !derived(r)).collect();
    let survivors_actual: Vec<&Row> = after.iter().filter(|r| !derived(r)).collect();
    assert_eq!(
        survivors_actual, survivors_expected,
        "every non-derived event must survive the compaction byte-for-byte and in order"
    );

    // Two IDENTICAL domain events mean the fact was recorded twice: both rows stay.
    assert_eq!(
        after.iter().filter(|r| r.2 == "ReviewFinding").count(),
        2,
        "two identical ReviewFindings must both survive - only the derived index is content-keyed"
    );
    // TYPE FIRST: a non-derived event is ineligible however its replay key is spelled.
    assert_eq!(
        after.iter().filter(|r| r.2 == "DecisionMade").count(),
        2,
        "a non-derived event carrying a derived-LOOKING replay key must never be pruned"
    );
    // A derived event with NO replay key names no content generation, so it is never provably
    // redundant: it appends through, which is the fail-safe direction.
    assert_eq!(
        after
            .iter()
            .filter(|r| derived(r) && meta_replay_key(&r.5).is_none())
            .count(),
        2,
        "a derived event carrying no replay key must survive - it names no content generation"
    );
}

// ---------------------------------------------------------------------------------------
// The log this command exists for: one whose history predates the durable project identity
// ---------------------------------------------------------------------------------------

/// A bloated log is, by construction, an OLD log - and an old log's history is filed under the
/// pre-identity namespace `proj-<repo-basename>-`. The moment `rigger init` mints
/// `.rigger/project.id`, every command resolves the project to the MINTED id, so a prune that
/// addressed streams by the current identity alone would match no stream at all and report a
/// perfectly successful removal of zero rows - silently no-opping on exactly the store it exists
/// to compact, and reporting a prune that did not happen is the one outcome an operator cannot
/// detect.
///
/// So `reset` runs the same one-time identity migration every other maintenance command runs
/// before it opens the store, and the prune then addresses the project's real history. The fixture
/// seeds BEFORE the mint on purpose: seeding after it would file the history under the minted
/// namespace and the assertion would hold whether or not the migration ran.
#[test]
fn reset_derived_compacts_a_log_whose_history_predates_the_minted_project_identity() {
    let dir = temp_rigger_project();
    let root = dir.path();
    // Seeded under the LEGACY basename namespace: no `.rigger/project.id` exists yet.
    seed_bloated_log(root);
    let legacy = run_stream_identity(root);

    // `rigger init` mints the durable identity. The history stays where it was written.
    let (_, ierr, iok) = run_rigger(root, &["init"]);
    assert!(iok, "rigger init must scaffold the project; stderr: {ierr}");
    let minted = run_stream_identity(root);
    assert_ne!(
        minted, legacy,
        "rigger init must mint an identity distinct from the basename, or this fixture does not \
         reproduce the shape it exists for"
    );

    let before = rows(&rigger_file(root, "events.db"));
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    let after = rows(&rigger_file(root, "events.db"));

    assert!(
        after.len() < before.len(),
        "reset --derived must compact a log whose history predates the minted identity: \
         {} rows before, {} after (a silent no-op here is the shape this command exists for)",
        before.len(),
        after.len()
    );
    assert!(
        out.contains(&format!("CodeEntityExtracted {REMOVED_CODE_ENTITIES}"))
            && out.contains(&format!("EdgeInferred {REMOVED_EDGES}"))
            && out.contains(&format!("DocLinkExtracted {REMOVED_DOC_LINKS}")),
        "the report must name the same per-type removals a migrated log yields; got: {out:?}"
    );
    // And the migration moved the history rather than duplicating it: every surviving row now
    // lives under the MINTED namespace, none under the legacy one.
    let minted_prefix = format!("proj-{minted}-");
    let legacy_prefix = format!("proj-{legacy}-");
    assert!(
        after.iter().all(|r| r.1.starts_with(&minted_prefix)),
        "every surviving row must live under the minted namespace {minted_prefix:?}"
    );
    assert!(
        !after.iter().any(|r| r.1.starts_with(&legacy_prefix)),
        "no row may be left behind under the legacy namespace {legacy_prefix:?}"
    );
}

// ---------------------------------------------------------------------------------------
// The observable operator effects: the file shrinks, and the command says what it did
// ---------------------------------------------------------------------------------------

#[test]
fn reset_derived_shrinks_the_log_on_disk_and_reports_the_rows_per_type_and_the_bytes_reclaimed() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_bloated_log(root);

    let db = rigger_file(root, "events.db");
    let before_bytes = file_len(&db);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");

    // ROWS PER TYPE. An operator has to be able to read what was removed, per derived type - a
    // prune that reports only a total cannot be checked against what was expected.
    assert!(
        out.contains(&format!("CodeEntityExtracted {REMOVED_CODE_ENTITIES}")),
        "reset --derived must report the CodeEntityExtracted rows it removed; got: {out:?}"
    );
    assert!(
        out.contains(&format!("EdgeInferred {REMOVED_EDGES}")),
        "reset --derived must report the EdgeInferred rows it removed; got: {out:?}"
    );
    assert!(
        out.contains(&format!("DocLinkExtracted {REMOVED_DOC_LINKS}")),
        "reset --derived must report the DocLinkExtracted rows it removed - the design-intent half \
         of the index is pruned exactly like the code half; got: {out:?}"
    );
    assert!(
        out.contains("DocConceptExtracted 0"),
        "reset --derived must report every derived type, including the ones it removed nothing \
         from; got: {out:?}"
    );

    // BYTES RECLAIMED, and the shrink they describe. Without the VACUUM the DELETE only frees
    // pages inside the file and the log stays exactly as large on disk as before.
    let reclaimed = reported_reclaimed(&out).unwrap_or_else(|| {
        panic!("reset --derived must report a reclaimed-byte count; got {out:?}")
    });
    assert!(
        reclaimed > 0,
        "a real prune must report a non-zero reclaimed-byte count; got {reclaimed} from {out:?}"
    );
    let after_bytes = file_len(&db);
    assert!(
        after_bytes < before_bytes,
        "reset --derived must COMPACT the log on disk: {before_bytes} -> {after_bytes} bytes"
    );

    // A SECOND pass finds nothing to prune: the compaction is idempotent, and it says so honestly
    // rather than reporting a prune that did not happen.
    let (again, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "a second reset --derived must succeed; stderr: {err}");
    assert!(
        again.contains("CodeEntityExtracted 0") && again.contains("EdgeInferred 0"),
        "a second pass over an already-compacted log must report removing nothing; got: {again:?}"
    );
}

/// The reclaimed byte count out of the `reset --derived` report line (`reclaimed <N> byte(s)`).
fn reported_reclaimed(out: &str) -> Option<u64> {
    let needle = "reclaimed ";
    let at = out.rfind(needle)? + needle.len();
    let rest = &out[at..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

// ---------------------------------------------------------------------------------------
// The compacted store is still a store: it reads clean, it folds the same graph, it appends
// ---------------------------------------------------------------------------------------

/// Fold `events` into a FRESH projection at `path` and return its live rows: the nodes, and the
/// live edges with the provenance the fold assigned them.
fn fold_snapshot(events: &[Event], project: &str, path: &Path) -> (Vec<String>, Vec<String>) {
    use rigger::contextgraph::sqlite::Projector;

    {
        let p = Projector::open(path.to_str().unwrap(), project).unwrap();
        common::fixtures::folds(&p, events);
    }
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut nodes: Vec<String> = conn
        .prepare("SELECT id, kind, COALESCE(attrs,''), project FROM nodes")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    nodes.sort();
    // EVERY column of a live edge, `valid_from` INCLUDED. It is the one column a prune that merely
    // deleted rows would move: the fold keeps the EARLIEST assertion of a fact, so dropping a
    // fact's first recording re-dates it to whichever recording survives. That is invisible to a
    // comparison of ids and provenance alone, and it is the whole value of the design-intent
    // layer, so the snapshot compares the bitemporal interval rather than excluding it.
    let mut edges: Vec<String> = conn
        .prepare(
            "SELECT from_id, to_id, rel, valid_from, source, project, tier FROM edges \
             WHERE valid_to IS NULL",
        )
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    edges.sort();
    (nodes, edges)
}

#[test]
fn a_compacted_log_folds_to_the_same_live_graph_reads_clean_and_still_accepts_appends() {
    let dir = temp_rigger_project();
    let root = dir.path();
    // Scaffold FIRST, then seed. `rigger init` is what mints `.rigger/project.id`, and the minted
    // id is what every later command namespaces its streams under - so a fixture that scaffolds
    // AFTER the prune would seed and prune under one identity and prove nothing about the
    // identity a real project actually runs under. Ordering it first also leaves `rigger validate`
    // below with nothing to judge except the store this test compacted.
    let (_, ierr, iok) = run_rigger(root, &["init"]);
    assert!(iok, "rigger init must scaffold the project; stderr: {ierr}");
    seed_bloated_log(root);
    let id = run_stream_identity(root);

    let scratch = tempfile::tempdir().unwrap();
    let before_graph = fold_snapshot(
        &read_run_events(root),
        &id,
        &scratch.path().join("before.db"),
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");

    // THE PROJECTION CONTRACT. The graph is an upsert projection, so every recording of a key
    // folds to the same rows: keeping the latest and dropping the rest leaves a log that folds to
    // the identical live graph - the same nodes, the same live edges, the same provenance AND the
    // same bitemporal valid-from, because the prune carries a pruned key's earliest valid-time
    // onto the recording it keeps.
    let after_graph = fold_snapshot(
        &read_run_events(root),
        &id,
        &scratch.path().join("after.db"),
    );
    assert_eq!(
        after_graph.0, before_graph.0,
        "the compacted log must fold to the same nodes"
    );
    assert_eq!(
        after_graph.1, before_graph.1,
        "the compacted log must fold to the same live edges, with the same provenance and the \
         same valid-from"
    );
    assert!(
        !before_graph.0.is_empty() && !before_graph.1.is_empty(),
        "the seeded log must actually fold to something, or the equality above proves nothing"
    );

    // The equality above must not pass vacuously on the class that can actually move: the
    // DESIGN-INTENT edge, whose whole value is the date the design fact FIRST became true. Its
    // valid-from is pinned to the seed's EARLIEST recording (4_000s), not to the surviving row's
    // own (4_000 + ROUNDS - 1) - the exact re-dating a delete-only prune would ship.
    let earliest_specifies_ns = Duration::from_secs(4_000).as_nanos() as i64;
    let specifies: Vec<&String> = after_graph
        .1
        .iter()
        .filter(|row| row.contains("|SPECIFIES|"))
        .collect();
    assert_eq!(
        specifies.len(),
        1,
        "the seed must fold exactly one live SPECIFIES edge, or the valid-from pin below proves \
         nothing; got {specifies:?}"
    );
    assert!(
        specifies[0].contains(&format!("|SPECIFIES|{earliest_specifies_ns}|")),
        "the compacted log must keep the design fact's EARLIEST assertion date ({earliest_specifies_ns}); \
         got {specifies:?}"
    );

    // `rigger validate` reads the compacted store cleanly.
    let (_, verr, vok) = run_rigger(root, &["validate"]);
    assert!(
        vok,
        "rigger validate must read the compacted store cleanly; stderr: {verr}"
    );

    // AND IT IS STILL A STORE. The prune leaves gaps in the stream's revisions, so a store that
    // derived its revision cursor from the surviving ROW COUNT would reissue a revision the stream
    // still holds and fail the `UNIQUE(stream, revision)` index on the very next append.
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &id);
    store
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                "RunStarted",
                br#"{"run":"r2","criteria":["c"]}"#.to_vec(),
            )],
        )
        .expect("a compacted store must still accept appends");
    let log = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert_eq!(
        log.last().map(|e| e.type_.as_str()),
        Some("RunStarted"),
        "the appended event must be readable back off the compacted store"
    );
}

// ---------------------------------------------------------------------------------------
// The command surface: composition with --runs, and the loud refusal
// ---------------------------------------------------------------------------------------

#[test]
fn reset_composes_derived_with_runs_bare_reset_previews_it_and_an_unknown_mode_still_refuses() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_bloated_log(root);

    // A BARE `rigger reset` is a MENU, not an error (spec 68 supersedes the pre-spec-68 refusal
    // this test used to pin): it exits 0 and previews - read-only - the SAME total the real
    // `--derived` prune below actually removes.
    let (menu, menu_err, menu_ok) = run_rigger(root, &["reset"]);
    assert!(
        menu_ok,
        "a bare `rigger reset` must exit 0; stderr: {menu_err}"
    );
    assert!(
        menu.contains("--runs:") && menu.contains("--derived:"),
        "the bare menu must name both modes; got: {menu:?}"
    );
    let total_duplicates = REMOVED_CODE_ENTITIES + REMOVED_EDGES + REMOVED_DOC_LINKS;
    assert!(
        menu.contains(&format!(
            "{total_duplicates} redundant derived-index event(s)"
        )),
        "the bare menu's --derived line must preview the same total the real prune below \
         removes ({total_duplicates}); got: {menu:?}"
    );

    // Each flag prunes its own accumulation, and they compose in one invocation.
    let (out, err, ok) = run_rigger(root, &["reset", "--runs", "--derived"]);
    assert!(
        ok,
        "reset --runs --derived must succeed; stderr: {err}\n{out}"
    );
    assert!(
        out.contains("reset --runs:") && out.contains("reset --derived:"),
        "a composed reset must report BOTH prunes; got: {out:?}"
    );
    assert!(
        out.contains(&format!("CodeEntityExtracted {REMOVED_CODE_ENTITIES}")),
        "the composed --derived prune must do the same work it does alone; got: {out:?}"
    );

    // An unknown mode is refused rather than ignored - only a TRULY bare reset is the menu.
    let (_, err, ok) = run_rigger(root, &["reset", "--everything"]);
    assert!(!ok, "an unknown reset mode must be refused");
    assert!(
        err.contains("--everything"),
        "the refusal must name what it did not understand; got: {err:?}"
    );
}

#[test]
fn reset_derived_on_a_backend_that_cannot_compact_fails_loudly_naming_the_backend_it_needs() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_bloated_log(root);
    let db_before = rows(&rigger_file(root, "events.db")).len();

    // Deleting rows and reclaiming the file is a mechanic of the embedded log. Configured for the
    // server-backed store, `--derived` must FAIL - never silently report a prune that did not
    // happen, which is the one outcome an operator cannot detect.
    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert!(
        !ok,
        "reset --derived on a server-backed project must fail; stdout: {out}"
    );
    // WHAT THE REFUSAL HAS TO SAY, clause by clause, because "it exited non-zero" is satisfied by
    // any error at all - including one about something else entirely - and because a message an
    // operator cannot act on is how a loud refusal degrades back into a silent one. Spec 60 names
    // this arm, so its text is pinned here rather than living only in the binary: it must name the
    // MODE that was refused, the backend the compaction NEEDS, the backend this project is
    // CONFIGURED for, what to do instead, and that it is refusing rather than reporting a prune
    // that did not happen.
    let said = format!("{err}{out}");
    for (clause, needle) in [
        ("name the mode it refused", "reset --derived"),
        ("name the backend it needs", "events.db"),
        ("say WHY that backend is needed", "vacuums the file"),
        (
            "name the backend the project is configured for",
            "server-backed store",
        ),
        ("say what the operator can do instead", "Re-run it against"),
        (
            "say that it is refusing rather than pretending",
            "Refusing rather than reporting a prune that did not happen",
        ),
    ] {
        assert!(
            said.contains(needle),
            "the refusal must {clause} ({needle:?}); an operator reads this line and nothing else, \
             so the arm's message is a shipped surface, not an internal string; got: {said:?}"
        );
    }
    assert!(
        said.contains("sqlite"),
        "the refusal must name the embedded sqlite backend the compaction needs; got: {said:?}"
    );

    // And it must be a REFUSAL, not a half-done prune: the local log is untouched.
    assert_eq!(
        rows(&rigger_file(root, "events.db")).len(),
        db_before,
        "a refused compaction must leave the log exactly as it was"
    );

    // The refusal is decided BEFORE any pruning, so a composed invocation does not half-succeed.
    let (out, _, ok) = run_rigger_envs(
        root,
        &["reset", "--runs", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert!(
        !ok,
        "a composed reset naming an unsupported --derived must fail before pruning; stdout: {out}"
    );
    assert!(
        !out.contains("reset --runs:"),
        "the composed reset must refuse BEFORE running the --runs prune; got: {out:?}"
    );
}

// ---------------------------------------------------------------------------------------
// Spec 101, criterion 4: COMPACTION SHEDS SUPERSEDED GENERATIONS
// ---------------------------------------------------------------------------------------

/// How many times each generation's batch is recorded, so the latest generation also carries the
/// exact-key duplication the prune already sheds beside the superseded generations.
const GEN_RECORDINGS: u64 = 2;

/// Seed three content generations of ONE file - its code batch (`gc/src/f.rs`) and its design
/// batch (`gd/docs/f.md`) - each generation recorded [`GEN_RECORDINGS`] times, every recording at
/// a later valid-time than the one before. Every generation moves `alpha`'s line and references
/// `beta`, the shape an ordinary edit leaves, and the design batch asserts the SAME `SPECIFIES`
/// fact in every generation (so the fold dates it from generation 1). Generation 1 also defines
/// `gone` and links `src/old.rs`, and generation 2 DROPS both - the edit that removes a function
/// and a design link, whose facts only the shed generation ever asserted. Generation 3 adds a
/// design fact of its own.
fn seed_three_generations(root: &Path) {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let mut events = vec![
        Event::new("RunStarted", br#"{"run":"r1","criteria":["c"]}"#.to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(10)),
    ];
    let mut secs = 100;
    for generation in 1..=3u32 {
        for _ in 0..GEN_RECORDINGS {
            secs += 1;
            let code = format!("gc/src/f.rs@h{generation}");
            let mut code_batch = vec![(
                rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                code_entity("src/f.rs", "alpha", generation, true),
            )];
            if generation == 1 {
                code_batch.push((
                    rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                    code_entity("src/f.rs", "gone", 9, false),
                ));
            }
            code_batch.push((
                rigger::contextgraph::TYPE_EDGE_INFERRED,
                edge_inferred("src/f.rs", "beta"),
            ));
            let design = format!("gd/docs/f.md@h{generation}");
            let mut design_batch = vec![doc_link(
                "docs/f.md",
                "src/f.rs",
                rigger::contextgraph::REL_SPECIFIES,
            )];
            match generation {
                1 => design_batch.push(doc_link(
                    "docs/f.md",
                    "src/old.rs",
                    rigger::contextgraph::REL_SPECIFIES,
                )),
                3 => design_batch.push(doc_link(
                    "docs/f.md",
                    "src/g.rs",
                    rigger::contextgraph::REL_SPECIFIES,
                )),
                _ => {}
            }
            for (i, (type_, data)) in code_batch.into_iter().enumerate() {
                events.push(keyed(type_, data, &format!("{code}#{i}"), secs));
            }
            for (i, data) in design_batch.into_iter().enumerate() {
                events.push(keyed(
                    rigger::contextgraph::TYPE_DOC_LINK_EXTRACTED,
                    data,
                    &format!("{design}#{i}"),
                    secs,
                ));
            }
        }
    }
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

/// Rebuild `graph.db` from `events` into a fresh file at `path` and return the rebuilt
/// projection's whole content in its public wire form: every node and every live edge, every
/// column, as the bytes a consumer receives.
fn rebuilt_graph_bytes(events: &[Event], project: &str, path: &Path) -> Vec<u8> {
    use rigger::contextgraph::sqlite::Projector;

    let p = Projector::open(path.to_str().unwrap(), project).unwrap();
    common::fixtures::folds(&p, events);
    serde_json::to_vec(&p.whole().unwrap()).unwrap()
}

#[test]
fn reset_derived_sheds_every_superseded_generation_and_the_rebuilt_graph_is_byte_identical() {
    let dir = temp_rigger_project();
    let root = dir.path();
    let (_, ierr, iok) = run_rigger(root, &["init"]);
    assert!(iok, "rigger init must scaffold the project; stderr: {ierr}");
    seed_three_generations(root);
    let id = run_stream_identity(root);
    let scratch = tempfile::tempdir().unwrap();
    let before_graph = rebuilt_graph_bytes(
        &read_run_events(root),
        &id,
        &scratch.path().join("before.db"),
    );
    let before = rows(&rigger_file(root, "events.db"));

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    let after = rows(&rigger_file(root, "events.db"));

    // Only the LATEST generation's recordings survive, and of each of its keys only the last
    // recording (the exact-key dedup the prune already did).
    let latest_recording = |key: &str| {
        before
            .iter()
            .filter(|r| meta_replay_key(&r.5).as_deref() == Some(key))
            .map(|r| r.0)
            .max()
            .unwrap()
    };
    let kept: Vec<(i64, String)> = after
        .iter()
        .filter(|r| derived(r))
        .map(|r| (r.0, meta_replay_key(&r.5).unwrap()))
        .collect();
    let expected: Vec<(i64, String)> = [
        "gc/src/f.rs@h3#0",
        "gc/src/f.rs@h3#1",
        "gd/docs/f.md@h3#0",
        "gd/docs/f.md@h3#1",
    ]
    .iter()
    .map(|k| (latest_recording(k), k.to_string()))
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
    .collect();
    assert_eq!(
        kept, expected,
        "only the latest recording of each of the latest generation's keys may survive"
    );

    // The count shed, per type and in total, and how many of them were superseded generations:
    // generation 1 is 2 recordings x 5 events and generation 2 is 2 recordings x 3 events = 16
    // rows, the latest generation's exact-key duplicates are 4 more.
    assert!(
        out.contains(
            "(CodeEntityExtracted 7, EdgeInferred 5, DocConceptExtracted 0, DocLinkExtracted 8)"
        ),
        "the report must name the rows shed per type; got: {out:?}"
    );
    assert!(
        out.contains("pruned 20 redundant derived-index event(s)")
            && out.contains("16 of them recordings of a superseded generation"),
        "the report must name the total shed and the superseded-generation share; got: {out:?}"
    );

    // The design fact every generation asserted keeps the date it FIRST became true (generation
    // 1's first recording, 101s), carried onto the recording that survives.
    let spec_row = after
        .iter()
        .find(|r| meta_replay_key(&r.5).as_deref() == Some("gd/docs/f.md@h3#0"))
        .unwrap();
    assert_eq!(
        spec_row.6,
        Duration::from_secs(101).as_nanos() as i64,
        "the surviving recording of a re-asserted design fact must carry its earliest valid-time"
    );
    // The fact only generation 3 asserts keeps generation 3's own first date (105s).
    let new_row = after
        .iter()
        .find(|r| meta_replay_key(&r.5).as_deref() == Some("gd/docs/f.md@h3#1"))
        .unwrap();
    assert_eq!(
        new_row.6,
        Duration::from_secs(105).as_nanos() as i64,
        "a fact first asserted by the latest generation keeps its own earliest valid-time"
    );

    // graph.db rebuilt from the compacted log is byte-identical to one rebuilt from the original.
    let after_graph = rebuilt_graph_bytes(
        &read_run_events(root),
        &id,
        &scratch.path().join("after.db"),
    );
    assert_eq!(
        String::from_utf8(after_graph).unwrap(),
        String::from_utf8(before_graph.clone()).unwrap(),
        "the graph rebuilt from the compacted log must be byte-identical to the original's"
    );
    // Not vacuous: the whole log folds to exactly the latest generation's facts. The function
    // and the design link generation 2 dropped are gone from it, and every fact generation 3
    // makes is live.
    let graph: rigger::contextgraph::Graph = serde_json::from_slice(&before_graph).unwrap();
    let nodes: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        nodes,
        vec![
            "docs/f.md",
            "src/f.rs",
            "src/f.rs::alpha",
            "src/f.rs::beta",
            "src/g.rs"
        ],
        "the whole-log graph must hold exactly the latest generation's nodes"
    );
    let edges: Vec<(&str, &str, &str)> = graph
        .edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str(), e.rel.as_str()))
        .collect();
    assert_eq!(
        edges,
        vec![
            ("docs/f.md", "src/f.rs", rigger::contextgraph::REL_SPECIFIES),
            ("docs/f.md", "src/g.rs", rigger::contextgraph::REL_SPECIFIES),
            (
                "src/f.rs",
                "src/f.rs::alpha",
                rigger::contextgraph::REL_CONTAINS
            ),
            (
                "src/f.rs",
                "src/f.rs::beta",
                rigger::contextgraph::REL_REFERENCES
            ),
        ],
        "the whole-log graph must hold exactly the latest generation's live edges"
    );

    // A second pass has nothing left to shed.
    let (again, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "a second reset --derived must succeed; stderr: {err}");
    assert!(
        again.contains("pruned 0 redundant derived-index event(s)")
            && again.contains("0 of them recordings of a superseded generation"),
        "a second pass over a compacted log must shed nothing; got: {again:?}"
    );
}
