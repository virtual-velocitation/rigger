//! PERIPHERY (contract / API / CLI) tests for spec 101, criterion 4 - a derived-index compaction
//! sheds every SUPERSEDED GENERATION of a batch identity, not only the earlier recordings of one
//! exact key.
//!
//! `tests/reset_derived_compaction.rs` drives the binary over generations recorded in ascending
//! order on one stream and pins the rebuilt graph. This file closes the boundaries that fixture
//! cannot reach:
//!
//!   1. **Latest is by position, not by name.** A file that RETURNS to an earlier content (a
//!      revert) keeps the returned generation; the one it departed from is the superseded one, and
//!      the returned generation's earlier recordings count as exact-key duplicates, never as
//!      superseded.
//!   2. **Identity is per stream and per prefix.** One file's code (`gc`) and design (`gd`) batches
//!      are independent identities, and a later generation on another stream never supersedes
//!      this stream's.
//!   3. **The `ContentIdentity::with_key_parts` / `key_parts` contract.** A policy that declares
//!      no parser keeps exact-key semantics; a key the declared parser rejects is its own identity
//!      and is never shed as superseded; and the store honors WHATEVER parser the policy declares,
//!      never a key shape of its own.
//!   4. **Preview parity.** `Store::count_derived_duplicates` and the bare `rigger reset` menu
//!      count exactly the rows a real prune removes, superseded generations included.
//!   5. **The fold's generation rule at its seams.** `ingest::derived_generation` names a
//!      generation only for a keyed derived-index event; the live `graph.db` a run folds event by
//!      event, reopened between batches, is the projection a whole-log rebuild and a compacted
//!      rebuild fold; and one project's generations never supersede another's in a shared file.

mod common;

use common::cli::escalate_solo_unit;
use common::cli::keyed;
use common::cli::nanos;
use common::cli::read_run_events;
use common::cli::rigger_command;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_rigger_envs;
use common::cli::run_stream_identity;
use common::cli::temp_store_project;
use common::cli::with_graph_locked;
use common::cli::write_workflow_fixture;
use common::cli::REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW;
use common::fixtures::cleanup;
use common::fixtures::dir_snapshot;
use common::fixtures::files_open_by;
use common::fixtures::meta_replay_key;
use common::fixtures::wait_until_for;
use common::git::temp_git_project_with_commit;
use rigger::contextgraph::sqlite::{PruneStats, RebuildSink, Rebuilt};
use rigger::contextgraph::Fold;
use rigger::contextgraph::{
    TYPE_CODE_ENTITY_EXTRACTED, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED,
    TYPE_EDGE_INFERRED,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{ContentIdentity, Event, EventStore, ExpectedRevision};
use std::path::Path;

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

const PROJECT: &str = "proj-gen";

/// A `CodeEntityExtracted` payload naming `name` at `line`.
fn entity(name: &str, line: u32) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/f.rs", "name": name, "kind": "function", "line": line, "lang": "rust",
    }))
    .unwrap()
}

/// A `DocLinkExtracted` payload: one `SPECIFIES` design fact from `docs/f.md` to `to`.
fn link(to: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "from": "docs/f.md", "to": to, "rel": rigger::contextgraph::REL_SPECIFIES,
    }))
    .unwrap()
}

/// A file-backed store in `dir` with `events` appended, in order, to `stream` of [`PROJECT`].
fn store_with(dir: &Path, batches: &[(&str, Vec<Event>)]) -> (Store, String) {
    let db = dir.join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    {
        let store = Namespaced::new(&backend, PROJECT);
        for (stream, events) in batches {
            store.append(stream, ExpectedRevision::Any, events).unwrap();
        }
    }
    (backend, db.to_str().unwrap().to_string())
}

/// `(position, stream, type, replay key, valid_from nanos)` of every keyed row, by position.
type Kept = (i64, String, String, String, i64);

fn keyed_rows(db: &str) -> Vec<Kept> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare("SELECT position, stream, type, meta, valid_from FROM events ORDER BY position")
        .unwrap();
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .filter_map(|(p, s, t, m, v)| meta_replay_key(&m).map(|k| (p, s, t, k, v)))
        .collect();
    out
}

/// The four derived types, in the shipped policy's declared order, with the given counts.
fn per_type(code: usize, edge: usize, concept: usize, link: usize) -> Vec<(String, usize)> {
    vec![
        (TYPE_CODE_ENTITY_EXTRACTED.to_string(), code),
        (TYPE_EDGE_INFERRED.to_string(), edge),
        (TYPE_DOC_CONCEPT_EXTRACTED.to_string(), concept),
        (TYPE_DOC_LINK_EXTRACTED.to_string(), link),
    ]
}

/// The shipped policy's meta key, covered types and valid-time partition, with NO key parser.
fn identity_without_key_parts() -> ContentIdentity {
    let shipped = rigger::ingest::derived_index_identity();
    ContentIdentity::new(shipped.meta_key().to_string(), shipped.types().to_vec())
        .with_reasserting_types(shipped.reasserting().unwrap().to_vec())
}

// ---------------------------------------------------------------------------------------
// 1. Latest by position: a revert keeps the returned generation
// ---------------------------------------------------------------------------------------

#[test]
fn a_file_that_returns_to_an_earlier_content_keeps_that_generation_and_sheds_the_one_it_left() {
    let dir = tempfile::tempdir().unwrap();
    let stream = rigger::conductor::STREAM;
    // h1, then h2 (twice), then back to h1: the LATEST recorded generation is h1 although h2 is
    // the lexically later hash and was the newer content for a while.
    let events = vec![
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            entity("alpha", 1),
            "gc/src/f.rs@h1#0",
            10,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/f.rs"),
            "gd/docs/f.md@h1#0",
            10,
        ),
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            entity("alpha", 2),
            "gc/src/f.rs@h2#0",
            20,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/f.rs"),
            "gd/docs/f.md@h2#0",
            20,
        ),
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            entity("alpha", 2),
            "gc/src/f.rs@h2#0",
            21,
        ),
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            entity("alpha", 1),
            "gc/src/f.rs@h1#0",
            30,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/f.rs"),
            "gd/docs/f.md@h1#0",
            30,
        ),
    ];
    let (backend, db) = store_with(dir.path(), &[(stream, events)]);
    let before = keyed_rows(&db);
    let identity = rigger::ingest::derived_index_identity();
    let prefix = Namespaced::prefix_for(PROJECT);

    // Code: h1@10 is an exact-key duplicate of h1@30, h2@20 and h2@21 are superseded (3).
    // Design: h1@10 is an exact-key duplicate, h2@20 is superseded (2).
    let preview = backend
        .count_derived_duplicates(&prefix, &identity)
        .unwrap()
        .removed;
    assert_eq!(
        preview,
        per_type(3, 0, 0, 2),
        "the preview must count the revert's shed rows"
    );

    let pruned = backend.prune_derived_index(&prefix, &identity).unwrap();
    assert_eq!(
        pruned.removed, preview,
        "the prune must remove exactly what its preview counted"
    );
    assert_eq!(
        pruned.superseded_generations, 3,
        "only the departed generation's rows (code h2 twice, design h2 once) are superseded; the \
         returned generation's earlier recordings are exact-key duplicates"
    );

    let after = keyed_rows(&db);
    let last = |key: &str| {
        before
            .iter()
            .filter(|r| r.3 == key)
            .map(|r| r.0)
            .max()
            .unwrap()
    };
    let kept: Vec<(i64, String, i64)> = after.iter().map(|r| (r.0, r.3.clone(), r.4)).collect();
    assert_eq!(
        kept,
        vec![
            (
                last("gc/src/f.rs@h1#0"),
                "gc/src/f.rs@h1#0".to_string(),
                nanos(30)
            ),
            // The design fact h1 first asserted at 10s keeps that date: it was asserted by every
            // generation, so its earliest recording anywhere in the identity is its date.
            (
                last("gd/docs/f.md@h1#0"),
                "gd/docs/f.md@h1#0".to_string(),
                nanos(10)
            ),
        ],
        "the returned generation's LATEST recordings survive; the code row keeps its own \
         valid-time (it supersedes) and the design row carries its fact's earliest one"
    );
}

/// The valid-time a surviving design fact carries is the earliest recording of THAT fact - the
/// byte-identical payload - within THAT batch identity, never the identity's earliest date and
/// never another identity's. `docs/f.md` asserts `a` at h1 and re-asserts it at h2, where it also
/// asserts a new fact `b` (recorded twice); `docs/g.md` records the byte-identical `a` payload
/// later, under its own identity.
#[test]
fn a_carried_valid_time_is_the_earliest_recording_of_the_same_fact_in_the_same_identity() {
    let dir = tempfile::tempdir().unwrap();
    let events = vec![
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/a.rs"),
            "gd/docs/f.md@h1#0",
            10,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/a.rs"),
            "gd/docs/f.md@h2#0",
            20,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/b.rs"),
            "gd/docs/f.md@h2#1",
            20,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/b.rs"),
            "gd/docs/f.md@h2#1",
            25,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/a.rs"),
            "gd/docs/g.md@k1#0",
            30,
        ),
    ];
    let (backend, db) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
    let identity = rigger::ingest::derived_index_identity();
    let prefix = Namespaced::prefix_for(PROJECT);

    let pruned = backend.prune_derived_index(&prefix, &identity).unwrap();
    assert_eq!(
        pruned.removed,
        per_type(0, 0, 0, 2),
        "h1's recording is superseded and h2#1's first recording is an exact-key duplicate"
    );
    assert_eq!(pruned.superseded_generations, 1);

    let kept: Vec<(String, i64)> = keyed_rows(&db).into_iter().map(|r| (r.3, r.4)).collect();
    assert_eq!(
        kept,
        vec![
            // `a` was first asserted by the shed h1 generation: its date comes back from it.
            ("gd/docs/f.md@h2#0".to_string(), nanos(10)),
            // `b` is a different fact: it keeps the date IT was first recorded at, not h1's.
            ("gd/docs/f.md@h2#1".to_string(), nanos(20)),
            // The same bytes under another identity are another file's fact: never carried.
            ("gd/docs/g.md@k1#0".to_string(), nanos(30)),
        ],
        "each surviving design fact carries the earliest date of its own payload in its own \
         identity"
    );
}

/// The whole live projection - every node, every live edge, every column - of a graph folded from
/// `store`'s run stream, in its public wire form.
fn rebuilt_whole(store: &Store, graph_db: &Path) -> String {
    fold_in_batches(graph_db, PROJECT, &[run_events(store, PROJECT)])
}

/// `project`'s run stream in `store`, as recorded (positions included).
fn run_events(store: &Store, project: &str) -> Vec<Event> {
    Namespaced::new(store, project)
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
}

/// Fold `batches` into `graph_db` for `project`, REOPENING the projector for every batch (the way
/// successive runs fold one live `graph.db`), and return the whole live projection's wire form.
fn fold_in_batches(graph_db: &Path, project: &str, batches: &[Vec<Event>]) -> String {
    use rigger::contextgraph::sqlite::Projector;

    let open = || Projector::open(graph_db.to_str().unwrap(), project).unwrap();
    for batch in batches {
        common::fixtures::folds(&open(), batch);
    }
    serde_json::to_string(&open().whole().unwrap()).unwrap()
}

/// A fact a generation DROPS and a later generation asserts again is live again from its RETURN,
/// never from the date it first held: the generation between retired it. `docs/f.md` asserts `a`
/// and `b` at h1, drops `b` at h2, and returns to h1; `src/f.rs` defines `gone` at h1, drops it at
/// h2, and defines it again on the return. The compaction carries `a` (asserted by every
/// generation) back to 10s but `b` only to its return at 30s, and the graph rebuilt from the
/// compacted log is the whole log's, node for node and edge for edge.
#[test]
fn a_fact_that_returns_after_a_generation_dropped_it_is_dated_from_its_return() {
    let dir = tempfile::tempdir().unwrap();
    let mut events = Vec::new();
    for (generation, secs) in [("h1", 10), ("h2", 20), ("h1", 30)] {
        let code = format!("gc/src/f.rs@{generation}");
        let design = format!("gd/docs/f.md@{generation}");
        let mut def = serde_json::from_slice::<serde_json::Value>(&entity("alpha", 1)).unwrap();
        def["fresh"] = serde_json::Value::Bool(true);
        events.push(keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            serde_json::to_vec(&def).unwrap(),
            &format!("{code}#0"),
            secs,
        ));
        events.push(keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/a.rs"),
            &format!("{design}#0"),
            secs,
        ));
        if generation == "h1" {
            events.push(keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("gone", 9),
                &format!("{code}#1"),
                secs,
            ));
            events.push(keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("src/b.rs"),
                &format!("{design}#1"),
                secs,
            ));
        }
    }
    let (backend, db) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
    let before = rebuilt_whole(&backend, &dir.path().join("before.db"));

    let identity = rigger::ingest::derived_index_identity();
    let pruned = backend
        .prune_derived_index(&Namespaced::prefix_for(PROJECT), &identity)
        .unwrap();
    assert_eq!(
        pruned.removed,
        per_type(3, 0, 0, 3),
        "h2's two rows are superseded and h1's first four recordings are exact-key duplicates"
    );

    let kept: Vec<(String, i64)> = keyed_rows(&db).into_iter().map(|r| (r.3, r.4)).collect();
    assert_eq!(
        kept,
        vec![
            ("gc/src/f.rs@h1#0".to_string(), nanos(30)),
            ("gd/docs/f.md@h1#0".to_string(), nanos(10)),
            ("gc/src/f.rs@h1#1".to_string(), nanos(30)),
            ("gd/docs/f.md@h1#1".to_string(), nanos(30)),
        ],
        "`a` held through every generation and keeps 10s; `b` was dropped at h2 and holds only \
         since its return at 30s"
    );

    let after = rebuilt_whole(&backend, &dir.path().join("after.db"));
    assert_eq!(
        after, before,
        "the graph rebuilt from the compacted log must be the whole log's"
    );
    let graph: rigger::contextgraph::Graph = serde_json::from_str(&before).unwrap();
    let edges: Vec<(&str, &str, i64)> = graph
        .edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str(), e.valid_from))
        .collect();
    assert_eq!(
        edges,
        vec![
            ("docs/f.md", "src/a.rs", nanos(10)),
            ("docs/f.md", "src/b.rs", nanos(30)),
            ("src/f.rs", "src/f.rs::alpha", nanos(30)),
            ("src/f.rs", "src/f.rs::gone", nanos(30)),
        ],
        "the whole log dates each live fact from the start of its unbroken run of generations"
    );
}

/// An event outside the generation rule: a `DecisionMade` governing `path`, valid from `secs`.
fn governs(path: &str, secs: u64) -> Event {
    Event::new(
        "DecisionMade",
        serde_json::to_vec(&serde_json::json!({
            "id": "d1", "summary": "s", "governs": [path], "supersedes": "",
        }))
        .unwrap(),
    )
    .with_valid_from(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

/// A code batch head: `CodeEntityExtracted` for `name` at `line`, carrying the `fresh` boundary
/// and, when `partial`, the degraded-parse marker.
fn head(name: &str, line: u32, partial: bool) -> Vec<u8> {
    let mut v: serde_json::Value = serde_json::from_slice(&entity(name, line)).unwrap();
    v["fresh"] = serde_json::Value::Bool(true);
    if partial {
        v["partial"] = serde_json::Value::Bool(true);
    }
    serde_json::to_vec(&v).unwrap()
}

/// The fold state of the graph at `graph_db` that decides how FUTURE events fold (spec 101, "the
/// identity"): every recorded test reference and where it resolved (pending ones included), the restorable attrs a retired node keeps, each
/// identity's current generation, the node and edge assertions of live generations (edges by their
/// columns, never their row ids), and the detached attachments a returning node revives. Everything
/// else in the file is history a compacted log no longer replays.
fn fold_state(graph_db: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(graph_db).unwrap();
    [
        "SELECT project, name, file, evidence, source, target FROM proofs
          ORDER BY project, name, file, source, evidence",
        "SELECT project, id, attrs FROM retired_nodes WHERE attrs IS NOT NULL ORDER BY project, id",
        "SELECT project, identity, generation FROM generations ORDER BY project, identity",
        "SELECT project, identity, generation, node_id, kind, attrs FROM live_node_assertions
          ORDER BY project, identity, node_id",
        "SELECT a.project, a.identity, a.generation, e.from_id, e.to_id, e.rel, e.tier,
                e.valid_from, e.valid_to, e.source
           FROM live_edge_assertions a JOIN edges e ON e.id = a.edge_id
          ORDER BY 1, 2, 3, 4, 5, 6, 7",
        "SELECT d.project, d.node_id, e.to_id, e.rel, e.tier, e.valid_from, e.source
           FROM detached_attachments d JOIN edges e ON e.id = d.edge_id
          ORDER BY 1, 2, 3, 4",
    ]
    .into_iter()
    .flat_map(|sql| {
        let mut stmt = conn.prepare(sql).unwrap();
        let width = stmt.column_count();
        stmt.query_map([], |r| {
            Ok((0..width)
                .map(|i| match r.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join(" | "))
        })
        .unwrap()
        .map(Result::unwrap)
        .map(|row| format!("{sql}: {row}"))
        .collect::<Vec<_>>()
    })
    .collect()
}

/// The events folded into both rebuilds AFTER the comparison, so a divergence latent in the fold
/// state surfaces: a later file defining every name the fixtures drop, and a decision naming the
/// dropped entity, which brings a retired node - and whatever detached from it - back.
fn later_events() -> Vec<Event> {
    let mut events: Vec<Event> = ["gone", "alpha", "mid"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                def_in("src/late.rs", name, 1, i == 0),
                &format!("gc/src/late.rs@z#{i}"),
                90,
            )
        })
        .collect();
    events.push(
        Event::new(
            "DecisionMade",
            serde_json::to_vec(&serde_json::json!({
                "id": "d-late", "summary": "s", "governs": ["src/f.rs::gone"], "supersedes": "",
            }))
            .unwrap(),
        )
        .with_valid_from(std::time::UNIX_EPOCH + std::time::Duration::from_secs(91)),
    );
    events
        .into_iter()
        .zip(1_000_000u64..)
        .map(|(mut e, position)| {
            e.position = position;
            e
        })
        .collect()
}

/// The whole live projection and the [`fold_state`] of `graph_db`, as one comparable value.
fn identity_of(graph_db: &Path) -> (String, Vec<String>) {
    use rigger::contextgraph::sqlite::Projector;
    let whole = Projector::open(graph_db.to_str().unwrap(), PROJECT)
        .unwrap()
        .whole()
        .unwrap();
    (serde_json::to_string(&whole).unwrap(), fold_state(graph_db))
}

/// Fold `events` whole, compact the log with the shipped policy, fold it again, and require the
/// two rebuilds to be identical - the live projection AND the fold state that decides future folds
/// ([`fold_state`]) - both as compared and again after [`later_events`] fold into each, and to hold
/// exactly `nodes` as `(id, kind)`. The live `graph.db` a run keeps is folded incrementally - one
/// event per batch, the projector reopened between them - and must be that same state before the
/// log is compacted. Returns the whole log's projection so a case can pin attrs and tiers too.
fn compaction_rebuilds_the_whole_logs_graph(
    events: Vec<Event>,
    nodes: &[(&str, &str)],
) -> rigger::contextgraph::Graph {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
    let (before_db, live_db, after_db) = (
        dir.path().join("before.db"),
        dir.path().join("live.db"),
        dir.path().join("after.db"),
    );
    let before = rebuilt_whole(&backend, &before_db);
    let one_by_one: Vec<Vec<Event>> = run_events(&backend, PROJECT)
        .into_iter()
        .map(|e| vec![e])
        .collect();
    fold_in_batches(&live_db, PROJECT, &one_by_one);
    assert_eq!(
        identity_of(&live_db),
        identity_of(&before_db),
        "the graph folded event by event across reopens must be the whole log's"
    );
    backend
        .prune_derived_index(
            &Namespaced::prefix_for(PROJECT),
            &rigger::ingest::derived_index_identity(),
        )
        .unwrap();
    rebuilt_whole(&backend, &after_db);
    assert_eq!(
        identity_of(&after_db),
        identity_of(&before_db),
        "the graph rebuilt from the compacted log must be the whole log's, fold state included"
    );
    for db in [&before_db, &after_db] {
        fold_in_batches(db, PROJECT, &[later_events()]);
    }
    assert_eq!(
        identity_of(&after_db),
        identity_of(&before_db),
        "both rebuilds must fold a later event to the same state"
    );
    let graph: rigger::contextgraph::Graph = serde_json::from_str(&before).unwrap();
    let held: Vec<(&str, &str)> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.kind.as_str()))
        .collect();
    assert_eq!(held, nodes, "the whole log's live nodes; graph: {before}");
    graph
}

/// A `CodeEntityExtracted` payload defining `name` at `line` in `file`, heading its batch when
/// `fresh`.
fn def_in(file: &str, name: &str, line: u32, fresh: bool) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
        "fresh": fresh,
    }))
    .unwrap()
}

/// An `EdgeInferred` payload: `caller` in `file` references `name`.
fn call_in(file: &str, caller: &str, name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": file, "name": name, "lang": "rust", "caller": caller,
    }))
    .unwrap()
}

/// `src/g.rs` defines `user`, which calls `gone`: a cross-file reference and its `CALLS` twin.
fn g_calls_gone(secs: u64) -> Vec<Event> {
    vec![
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            def_in("src/g.rs", "user", 1, true),
            "gc/src/g.rs@k1#0",
            secs,
        ),
        keyed(
            TYPE_EDGE_INFERRED,
            call_in("src/g.rs", "user", "gone"),
            "gc/src/g.rs@k1#1",
            secs,
        ),
    ]
}

/// The live `(from, to, rel, tier)` of every edge `graph` holds into `to`.
fn tiers_into<'g>(
    graph: &'g rigger::contextgraph::Graph,
    to: &str,
) -> Vec<(&'g str, &'g str, &'g str)> {
    graph
        .edges
        .iter()
        .filter(|e| e.to == to)
        .map(|e| (e.from.as_str(), e.rel.as_str(), e.tier.as_str()))
        .collect()
}

/// A graph-derived attachment payload: `node` in `community` (`CommunityAssigned`), heading the
/// pass.
fn community_of(node: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "node": node, "community": "community/1/c0", "resolution": 1.0, "hash": "h",
        "fresh": true,
    }))
    .unwrap()
}

/// A concept pass: the concept `concept/1/k0` (`ConceptDerived`) and `node` realizing it
/// (`ConceptRealized`), valid from `secs`.
fn concept_of(node: &str, secs: u64) -> Vec<Event> {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    vec![
        Event::new(
            rigger::contextgraph::TYPE_CONCEPT_DERIVED,
            serde_json::to_vec(&serde_json::json!({
                "concept": "concept/1/k0", "label": "K", "resolution": 1.0, "hash": "h",
                "fresh": true,
            }))
            .unwrap(),
        )
        .with_valid_from(at),
        Event::new(
            rigger::contextgraph::TYPE_CONCEPT_REALIZED,
            serde_json::to_vec(&serde_json::json!({
                "node": node, "concept": "concept/1/k0", "resolution": 1.0, "hash": "h",
            }))
            .unwrap(),
        )
        .with_valid_from(at),
    ]
}

rigger::test_cases! {
    /// A definition a later generation drops while a decision still governs it stays live as the
    /// artifact the decision names: the definition's own kind and attrs went with its generation.
    a_dropped_entity_a_decision_governs_is_the_decisions_artifact: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("gone", 9), "gc/src/f.rs@h1#1", 10),
            governs("src/f.rs::gone", 15),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 2, false), "gc/src/f.rs@h2#0", 20),
        ],
        &[
            ("d1", "decision"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::gone", "artifact"),
        ],
    );
    /// A file whose extraction empties (the structural sentinel) while a decision governs it is
    /// the decision's artifact, no longer a parsed source file.
    an_emptied_file_a_decision_governs_is_the_decisions_artifact: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 10),
            governs("src/f.rs", 15),
            keyed(
                TYPE_EDGE_INFERRED,
                serde_json::to_vec(&serde_json::json!({
                    "file": "src/f.rs", "name": "", "lang": "rust", "fresh": true,
                }))
                .unwrap(),
                "gc/src/f.rs@h2#0",
                20,
            ),
        ],
        &[("d1", "decision"), ("src/f.rs", "artifact")],
    );
    /// A design link a later generation drops to a file the code half still holds leaves the
    /// file as the code half says it is.
    a_dropped_link_to_a_parsed_file_leaves_the_file: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 10),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/f.rs"), "gd/docs/f.md@h1#0", 10),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/g.rs"), "gd/docs/f.md@h1#1", 10),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/g.rs"), "gd/docs/f.md@h2#0", 20),
        ],
        &[
            ("docs/f.md", "artifact"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/g.rs", "artifact"),
        ],
    );
    /// A degraded-parse marker the newer generation no longer carries is gone from the file.
    a_partial_marker_a_later_generation_drops_is_retracted: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, true), "gc/src/f.rs@h1#0", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 2, false), "gc/src/f.rs@h2#0", 20),
        ],
        &[("src/f.rs", "file"), ("src/f.rs::alpha", "code-entity")],
    );
    /// A decision naming a definition between two generations of its file: the definition is a
    /// code entity whether the decision folded after the first generation or before the latest.
    a_decision_between_generations_names_the_code_entity: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 10),
            governs("src/f.rs::alpha", 15),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 2, false), "gc/src/f.rs@h2#0", 20),
        ],
        &[
            ("d1", "decision"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
        ],
    );
    /// A document that is both a design concept and the source of a link in one generation stays
    /// the design doc it was extracted as, with its title, in every generation.
    a_design_doc_that_links_in_the_same_generation_stays_a_design_doc: compaction_rebuilds_the_whole_logs_graph(
        ["h1", "h2"]
            .into_iter()
            .zip([10, 20])
            .flat_map(|(generation, secs)| {
                [
                    keyed(
                        TYPE_DOC_CONCEPT_EXTRACTED,
                        serde_json::to_vec(&serde_json::json!({
                            "kind": rigger::contextgraph::KIND_DESIGN_DOC, "id": "docs/f.md",
                            "title": "F", "doc": "docs/f.md",
                        }))
                        .unwrap(),
                        &format!("gd/docs/f.md@{generation}#0"),
                        secs,
                    ),
                    keyed(
                        TYPE_DOC_LINK_EXTRACTED,
                        link("src/a.rs"),
                        &format!("gd/docs/f.md@{generation}#1"),
                        secs,
                    ),
                ]
            })
            .collect(),
        &[("docs/f.md", "design-doc"), ("src/a.rs", "artifact")],
    );
    /// A design concept a later generation drops is retired with it.
    a_dropped_design_concept_is_retired: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(
                TYPE_DOC_CONCEPT_EXTRACTED,
                serde_json::to_vec(&serde_json::json!({
                    "kind": rigger::contextgraph::KIND_DESIGN_DOC, "id": "docs/f.md#old",
                    "title": "Old", "doc": "docs/f.md",
                }))
                .unwrap(),
                "gd/docs/f.md@h1#0",
                10,
            ),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/a.rs"), "gd/docs/f.md@h1#1", 10),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/a.rs"), "gd/docs/f.md@h2#0", 20),
        ],
        &[("docs/f.md", "artifact"), ("src/a.rs", "artifact")],
    );
    /// A decision that names a definition only AFTER the generation that dropped it: the node
    /// comes back as the decision's artifact, never as the definition the shed generation made.
    a_decision_naming_an_already_dropped_entity_names_an_artifact: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("gone", 9), "gc/src/f.rs@h1#1", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 2, false), "gc/src/f.rs@h2#0", 20),
            governs("src/f.rs::gone", 25),
        ],
        &[
            ("d1", "decision"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::gone", "artifact"),
        ],
    );
    /// A design link that names a definition only AFTER the generation that dropped it: the node
    /// comes back as the link's artifact, never as the definition the shed generation made - the
    /// restore takes the kind of whatever re-asserts it, a derived asserter as much as a decision.
    a_link_naming_an_already_dropped_entity_names_an_artifact: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("gone", 9), "gc/src/f.rs@h1#1", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 2, false), "gc/src/f.rs@h2#0", 20),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/f.rs::gone"), "gd/docs/f.md@h1#0", 25),
        ],
        &[
            ("docs/f.md", "artifact"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::gone", "artifact"),
        ],
    );
    /// Three generations of one file's code and design batches, each dropping or adding a fact:
    /// only what the third asserts (and what a decision holds) is live.
    three_generations_fold_to_the_latest_ones_facts: compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("gone", 9), "gc/src/f.rs@h1#1", 10),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/a.rs"), "gd/docs/f.md@h1#0", 10),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/old.rs"), "gd/docs/f.md@h1#1", 10),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 2, false), "gc/src/f.rs@h2#0", 20),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("mid", 5), "gc/src/f.rs@h2#1", 20),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/a.rs"), "gd/docs/f.md@h2#0", 20),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 3, false), "gc/src/f.rs@h3#0", 30),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/a.rs"), "gd/docs/f.md@h3#0", 30),
            keyed(TYPE_DOC_LINK_EXTRACTED, link("src/g.rs"), "gd/docs/f.md@h3#1", 30),
        ],
        &[
            ("docs/f.md", "artifact"),
            ("src/a.rs", "artifact"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/g.rs", "artifact"),
        ],
    );
}

/// `src/f.rs` defines `alpha` and `gone` at h1 and only `alpha` at h2.
fn f_drops_gone() -> (Vec<Event>, Event) {
    (
        vec![
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                head("alpha", 1, false),
                "gc/src/f.rs@h1#0",
                10,
            ),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("gone", 9),
                "gc/src/f.rs@h1#1",
                10,
            ),
        ],
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            head("alpha", 2, false),
            "gc/src/f.rs@h2#0",
            20,
        ),
    )
}

/// A cross-file reference (and its `CALLS` twin) to a name only a later-dropped definition defined
/// is INFERRED while that definition lives and AMBIGUOUS again once it retires - whether the
/// reference folded before the definition or after it - exactly what the compacted log folds,
/// where the definition never existed.
fn a_dropped_definitions_cross_file_callers_fall_back_to_ambiguous(reference_first: bool) {
    let (h1, h2) = f_drops_gone();
    let events: Vec<Event> = if reference_first {
        [g_calls_gone(5), h1, vec![h2]].concat()
    } else {
        [h1, g_calls_gone(12), vec![h2]].concat()
    };
    let graph = compaction_rebuilds_the_whole_logs_graph(
        events,
        &[
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/g.rs", "file"),
            ("src/g.rs::gone", "code-entity"),
            ("src/g.rs::user", "code-entity"),
        ],
    );
    assert_eq!(
        tiers_into(&graph, "src/g.rs::gone"),
        vec![
            ("src/g.rs", "REFERENCES", "ambiguous"),
            ("src/g.rs::user", "CALLS", "ambiguous"),
        ],
        "no definition of `gone` is left, so neither edge may claim one"
    );
}

rigger::test_cases! {
    /// The reference folds after the definition it resolved against.
    a_reference_after_its_definition_is_ambiguous_once_the_definition_drops:
        a_dropped_definitions_cross_file_callers_fall_back_to_ambiguous(false);
    /// The reference folds before the definition that promoted it.
    a_reference_before_its_definition_is_ambiguous_once_the_definition_drops:
        a_dropped_definitions_cross_file_callers_fall_back_to_ambiguous(true);
}

/// A cross-file reference to a name that ANOTHER live definition still defines stays INFERRED
/// when one of its definitions drops.
#[test]
fn a_reference_keeps_its_inferred_tier_while_another_definition_of_its_name_lives() {
    let (h1, h2) = f_drops_gone();
    let graph = compaction_rebuilds_the_whole_logs_graph(
        [
            h1,
            vec![keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                def_in("src/h.rs", "gone", 4, true),
                "gc/src/h.rs@j1#0",
                11,
            )],
            g_calls_gone(12),
            vec![h2],
        ]
        .concat(),
        &[
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/g.rs", "file"),
            ("src/g.rs::gone", "code-entity"),
            ("src/g.rs::user", "code-entity"),
            ("src/h.rs", "file"),
            ("src/h.rs::gone", "code-entity"),
        ],
    );
    assert_eq!(
        tiers_into(&graph, "src/g.rs::gone"),
        vec![
            ("src/g.rs", "REFERENCES", "inferred"),
            ("src/g.rs::user", "CALLS", "inferred"),
        ],
        "`src/h.rs` still defines `gone`"
    );
}

/// A test's proof that landed on a definition a later generation drops goes back to waiting, so
/// the next definition of the name receives it - as it does in the compacted log, where the
/// dropped definition never existed and the proof waited from the start.
#[test]
fn a_proof_on_a_dropped_definition_lands_on_the_next_definition_of_its_name() {
    let (h1, h2) = f_drops_gone();
    let graph = compaction_rebuilds_the_whole_logs_graph(
        [
            h1,
            vec![
                keyed(
                    TYPE_EDGE_INFERRED,
                    proof("gone", 3),
                    "gc/tests/t.rs@k1#0",
                    12,
                ),
                h2,
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    def_in("src/h.rs", "gone", 4, true),
                    "gc/src/h.rs@j1#0",
                    30,
                ),
            ],
        ]
        .concat(),
        &[
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/h.rs", "file"),
            ("src/h.rs::gone", "code-entity"),
        ],
    );
    let gone = graph
        .nodes
        .iter()
        .find(|n| n.id == "src/h.rs::gone")
        .unwrap();
    let proof: Vec<(&str, &str)> = gone
        .attrs
        .iter()
        .filter(|(k, _)| k.starts_with("pro"))
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        proof,
        vec![
            ("proof_evidence", r#"["tests/t.rs:3"]"#),
            ("proven_by", "1")
        ],
        "the proof the dropped definition held belongs to the definition that replaced it"
    );
}

/// Where a proof lands is a function of the definitions that hold NOW, never of the order they
/// folded in: a cross-file proof on the only definition of its name waits again the moment a second
/// definition of the name makes it ambiguous, whether that definition folded before the test's
/// reference or after it - so neither same-named definition claims it, and both rebuilds agree.
fn a_second_definition_makes_a_resolved_proof_wait(second_first: bool) {
    let second = keyed(
        TYPE_CODE_ENTITY_EXTRACTED,
        def_in("src/h.rs", "gone", 4, true),
        "gc/src/h.rs@j1#0",
        11,
    );
    let reference = keyed(
        TYPE_EDGE_INFERRED,
        proof("gone", 3),
        "gc/tests/t.rs@k1#0",
        12,
    );
    let (h1, _) = f_drops_gone();
    let tail = if second_first {
        vec![second, reference]
    } else {
        vec![reference, second]
    };
    let graph = compaction_rebuilds_the_whole_logs_graph(
        [h1, tail].concat(),
        &[
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::gone", "code-entity"),
            ("src/h.rs", "file"),
            ("src/h.rs::gone", "code-entity"),
        ],
    );
    let proven: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.attrs.contains_key("proven_by") || n.attrs.contains_key("proof_evidence"))
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(
        proven,
        Vec::<&str>::new(),
        "an ambiguous name's proof waits on neither definition"
    );
}

rigger::test_cases! {
    /// The second definition folds before the test's reference.
    a_reference_to_an_already_ambiguous_name_waits:
        a_second_definition_makes_a_resolved_proof_wait(true);
    /// The second definition folds after the proof landed on the first.
    a_landed_proof_waits_again_once_its_name_turns_ambiguous:
        a_second_definition_makes_a_resolved_proof_wait(false);
}

/// Several proofs on one definition list their evidence in the order the log recorded it, and a
/// test file's re-extraction that drops its reference leaves the definition without any proof
/// attrs at all - the same state a log that never recorded the reference folds to.
#[test]
fn proofs_list_in_recorded_order_and_a_dropped_last_proof_leaves_no_proof_attrs() {
    let reference = |file: &str, line: u32| {
        serde_json::to_vec(&serde_json::json!({
            "file": file, "name": "gone", "lang": "rust", "fresh": true, "line": line,
            "is_test": true,
        }))
        .unwrap()
    };
    let (h1, _) = f_drops_gone();
    let graph = compaction_rebuilds_the_whole_logs_graph(
        [
            h1,
            vec![
                keyed(
                    TYPE_EDGE_INFERRED,
                    reference("tests/z.rs", 9),
                    "gc/tests/z.rs@a#0",
                    12,
                ),
                keyed(
                    TYPE_EDGE_INFERRED,
                    reference("tests/a.rs", 5),
                    "gc/tests/a.rs@a#0",
                    13,
                ),
            ],
        ]
        .concat(),
        &[
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::gone", "code-entity"),
        ],
    );
    let gone = graph
        .nodes
        .iter()
        .find(|n| n.id == "src/f.rs::gone")
        .unwrap();
    assert_eq!(
        (
            gone.attrs["proven_by"].as_str(),
            gone.attrs["proof_evidence"].as_str()
        ),
        ("2", r#"["tests/z.rs:9","tests/a.rs:5"]"#),
        "recorded order, not name order"
    );
    let (h1, _) = f_drops_gone();
    let dropped = compaction_rebuilds_the_whole_logs_graph(
        [
            h1,
            vec![
                keyed(
                    TYPE_EDGE_INFERRED,
                    reference("tests/z.rs", 9),
                    "gc/tests/z.rs@a#0",
                    12,
                ),
                keyed(
                    TYPE_EDGE_INFERRED,
                    serde_json::to_vec(&serde_json::json!({
                        "file": "tests/z.rs", "name": "", "lang": "rust", "fresh": true,
                        "is_test": true,
                    }))
                    .unwrap(),
                    "gc/tests/z.rs@b#0",
                    14,
                ),
            ],
        ]
        .concat(),
        &[
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::gone", "code-entity"),
        ],
    );
    let gone = dropped
        .nodes
        .iter()
        .find(|n| n.id == "src/f.rs::gone")
        .unwrap();
    assert_eq!(
        gone.attrs.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["kind", "lang", "line", "name"],
        "no proof attrs remain once the last proof is gone"
    );
}

/// A design link an UNKEYED recording asserts (one written before replay keys, which the
/// compaction never selects) outlives a keyed generation that drops it: the unkeyed recording is
/// an asserter of its own, so both rebuilds hold it live from its first date.
#[test]
fn a_link_an_unkeyed_recording_asserts_outlives_the_keyed_generation_that_drops_it() {
    let graph = compaction_rebuilds_the_whole_logs_graph(
        vec![
            Event::new(TYPE_DOC_LINK_EXTRACTED, link("src/old.rs"))
                .with_valid_from(std::time::UNIX_EPOCH + std::time::Duration::from_secs(5)),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("src/old.rs"),
                "gd/docs/f.md@h1#0",
                10,
            ),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("src/a.rs"),
                "gd/docs/f.md@h2#0",
                20,
            ),
        ],
        &[
            ("docs/f.md", "artifact"),
            ("src/a.rs", "artifact"),
            ("src/old.rs", "artifact"),
        ],
    );
    let edges: Vec<(&str, i64)> = graph
        .edges
        .iter()
        .map(|e| (e.to.as_str(), e.valid_from))
        .collect();
    assert_eq!(
        edges,
        vec![("src/a.rs", nanos(20)), ("src/old.rs", nanos(5))],
        "the unkeyed link holds from its own date; the keyed h2 link from h2's"
    );
}

/// Graph-derived attachments never hold a node: a community assignment and a concept realization
/// on an entity a later generation drops retire with it, while a decision still names another
/// dropped entity, which stays as the decision's artifact. No live edge dangles in either rebuild.
#[test]
fn attachments_on_a_dropped_entity_retire_with_it_while_knowledge_holds_its_sibling() {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(11);
    let graph = compaction_rebuilds_the_whole_logs_graph(
        [
            vec![
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("alpha", 1, false),
                    "gc/src/f.rs@h1#0",
                    10,
                ),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("gone", 9),
                    "gc/src/f.rs@h1#1",
                    10,
                ),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("other", 5),
                    "gc/src/f.rs@h1#2",
                    10,
                ),
                Event::new(
                    rigger::contextgraph::TYPE_COMMUNITY_ASSIGNED,
                    community_of("src/f.rs::gone"),
                )
                .with_valid_from(at),
            ],
            concept_of("src/f.rs::gone", 11),
            vec![
                governs("src/f.rs::other", 12),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("alpha", 2, false),
                    "gc/src/f.rs@h2#0",
                    20,
                ),
            ],
        ]
        .concat(),
        &[
            ("community/1/c0", "community"),
            ("concept/1/k0", "concept"),
            ("d1", "decision"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::other", "artifact"),
        ],
    );
    let ids: std::collections::BTreeSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    let dangling: Vec<(&str, &str)> = graph
        .edges
        .iter()
        .filter(|e| !ids.contains(e.from.as_str()) || !ids.contains(e.to.as_str()))
        .map(|e| (e.from.as_str(), e.rel.as_str()))
        .collect();
    assert_eq!(
        dangling,
        Vec::<(&str, &str)>::new(),
        "no live edge may dangle"
    );
}

/// An attachment on an entity every generation keeps is live in both rebuilds - whether it folded
/// before the entity's latest generation (the compacted log replays it onto a node not held yet)
/// or after it.
#[test]
fn an_attachment_on_an_entity_every_generation_keeps_stays_live() {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(11);
    let graph = compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                head("alpha", 1, false),
                "gc/src/f.rs@h1#0",
                10,
            ),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("keep", 9),
                "gc/src/f.rs@h1#1",
                10,
            ),
            Event::new(
                rigger::contextgraph::TYPE_COMMUNITY_ASSIGNED,
                community_of("src/f.rs::keep"),
            )
            .with_valid_from(at),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                head("alpha", 2, false),
                "gc/src/f.rs@h2#0",
                20,
            ),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("keep", 8),
                "gc/src/f.rs@h2#1",
                20,
            ),
        ],
        &[
            ("community/1/c0", "community"),
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::keep", "code-entity"),
        ],
    );
    assert_eq!(
        tiers_into(&graph, "community/1/c0"),
        vec![("src/f.rs::keep", "IN_COMMUNITY", "inferred")],
        "the kept entity's membership is live"
    );
}

/// A `SPECIFIES` link from `docs/f.md` to `to`, spelled with its keys in `rel`, `to`, `from` order
/// - the order the other ingest sink writes - and, when `extra`, carrying a field the fold ignores.
fn link_respelled(to: &str, extra: bool) -> Vec<u8> {
    let tail = if extra { r#","note":"x""# } else { "" };
    format!(r#"{{"rel":"SPECIFIES","to":"{to}","from":"docs/f.md"{tail}}}"#).into_bytes()
}

/// One design fact recorded under both sinks' spellings keeps its earliest date: across
/// generations the fact is the parsed payload, never its bytes, and within one key every earlier
/// recording counts however its payload is spelled - and the compacted log rebuilds the whole
/// log's graph.
#[test]
fn a_fact_recorded_in_both_sinks_spellings_keeps_its_earliest_date() {
    let events = || {
        vec![
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("src/a.rs"),
                "gd/docs/f.md@h1#0",
                10,
            ),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link_respelled("src/a.rs", false),
                "gd/docs/f.md@h2#0",
                20,
            ),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link_respelled("src/b.rs", true),
                "gd/docs/f.md@h2#1",
                25,
            ),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("src/a.rs"),
                "gd/docs/f.md@h2#0",
                30,
            ),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("src/b.rs"),
                "gd/docs/f.md@h2#1",
                35,
            ),
        ]
    };
    let dir = tempfile::tempdir().unwrap();
    let (backend, db) = store_with(dir.path(), &[(rigger::conductor::STREAM, events())]);
    backend
        .prune_derived_index(
            &Namespaced::prefix_for(PROJECT),
            &rigger::ingest::derived_index_identity(),
        )
        .unwrap();
    let kept: Vec<(String, i64)> = keyed_rows(&db).into_iter().map(|r| (r.3, r.4)).collect();
    assert_eq!(
        kept,
        vec![
            ("gd/docs/f.md@h2#0".to_string(), nanos(10)),
            ("gd/docs/f.md@h2#1".to_string(), nanos(25)),
        ],
        "`a` holds since h1 in either spelling; `b`'s key holds since its first recording"
    );
    compaction_rebuilds_the_whole_logs_graph(
        events(),
        &[
            ("docs/f.md", "artifact"),
            ("src/a.rs", "artifact"),
            ("src/b.rs", "artifact"),
        ],
    );
}

/// A policy that declares no [`rigger::eventstore::FactIdentity`] has no way to tell two spellings
/// of one fact apart from two facts: it carries a valid-time only between byte-identical payloads
/// (the fail-safe direction), where the shipped policy, which declares the fold's own fact key,
/// carries across spellings too.
#[test]
fn a_policy_without_a_fact_identity_carries_only_between_byte_identical_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, db) = store_with(
        dir.path(),
        &[(
            rigger::conductor::STREAM,
            vec![
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/a.rs"),
                    "gd/docs/f.md@h1#0",
                    10,
                ),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/a.rs"),
                    "gd/docs/f.md@h2#0",
                    20,
                ),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/b.rs"),
                    "gd/docs/g.md@h1#0",
                    10,
                ),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link_respelled("src/b.rs", false),
                    "gd/docs/g.md@h2#0",
                    20,
                ),
            ],
        )],
    );
    let identity = identity_without_key_parts().with_key_parts(rigger::ingest::derived_key_parts);
    assert!(
        identity.facts().is_none(),
        "the policy declares no fact identity"
    );
    backend
        .prune_derived_index(&Namespaced::prefix_for(PROJECT), &identity)
        .unwrap();
    let kept: Vec<(String, i64)> = keyed_rows(&db).into_iter().map(|r| (r.3, r.4)).collect();
    assert_eq!(
        kept,
        vec![
            ("gd/docs/f.md@h2#0".to_string(), nanos(10)),
            ("gd/docs/g.md@h2#0".to_string(), nanos(20)),
        ],
        "identical bytes carry the earliest date; a respelling does not"
    );
}

/// An `AliasDefined` recording naming `alias` as `canonical`, at `secs`.
fn alias_defined(alias: &str, canonical: &str, secs: u64) -> Event {
    Event::new(
        rigger::contextgraph::TYPE_ALIAS_DEFINED,
        serde_json::to_vec(&serde_json::json!({"alias": alias, "canonical": canonical})).unwrap(),
    )
    .with_valid_from(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

/// A link a later generation spells through a name alias is the fact the fold resolves it to, as
/// of that recording: `h2` links `docs/f.md` to `a-alias`. Defined as `src/a.rs` BEFORE `h1` linked
/// `src/a.rs`, both generations assert one link and the survivor carries `h1`'s date; defined only
/// BETWEEN the generations, `h1` linked the bare name and `h2` is a new link from its own date. The
/// compaction keys facts as the fold does either way, so the compacted log rebuilds the whole
/// log's graph.
fn an_alias_spelled_link_is_the_fact_the_fold_resolves_it_to(defined_first: bool) {
    let events = || {
        let (h1_to, alias_at) = if defined_first {
            ("src/a.rs", 0)
        } else {
            ("a-alias", 1)
        };
        let mut events = vec![
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link(h1_to),
                "gd/docs/f.md@h1#0",
                10,
            ),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("a-alias"),
                "gd/docs/f.md@h2#0",
                20,
            ),
        ];
        events.insert(alias_at, alias_defined("a-alias", "src/a.rs", 5));
        events
    };
    let dir = tempfile::tempdir().unwrap();
    let (backend, db) = store_with(dir.path(), &[(rigger::conductor::STREAM, events())]);
    backend
        .prune_derived_index(
            &Namespaced::prefix_for(PROJECT),
            &rigger::ingest::derived_index_identity(),
        )
        .unwrap();
    let kept: Vec<(String, i64)> = keyed_rows(&db).into_iter().map(|r| (r.3, r.4)).collect();
    let since = if defined_first { 10 } else { 20 };
    assert_eq!(
        kept,
        vec![("gd/docs/f.md@h2#0".to_string(), nanos(since))],
        "the survivor is dated as the fold dates the link it resolves to"
    );
    let graph = compaction_rebuilds_the_whole_logs_graph(
        events(),
        &[("docs/f.md", "artifact"), ("src/a.rs", "artifact")],
    );
    assert_eq!(
        graph
            .edges
            .iter()
            .map(|e| (e.to.as_str(), e.valid_from))
            .collect::<Vec<_>>(),
        vec![("src/a.rs", nanos(since))],
        "the whole log's link holds from that same date"
    );
}

rigger::test_cases! {
    /// The alias is defined before either generation links.
    an_alias_defined_first_joins_both_generations_into_one_link:
        an_alias_spelled_link_is_the_fact_the_fold_resolves_it_to(true);
    /// The alias is defined only between the generations.
    an_alias_defined_between_generations_starts_a_new_link:
        an_alias_spelled_link_is_the_fact_the_fold_resolves_it_to(false);
}

/// An alias REDEFINED between the generations resolves each recording through the definition that
/// preceded it: `h1` links `a-alias` while it names `src/a.rs`, `h2` links the same spelling after
/// it was redefined as `src/b.rs`. They are two links, never one fact: the survivor keeps `h2`'s own
/// date, the whole log retires `h1`'s link, and the compacted log rebuilds the whole log's graph.
#[test]
fn an_alias_redefined_between_generations_names_a_new_link() {
    let events = || {
        vec![
            alias_defined("a-alias", "src/a.rs", 5),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("a-alias"),
                "gd/docs/f.md@h1#0",
                10,
            ),
            alias_defined("a-alias", "src/b.rs", 15),
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("a-alias"),
                "gd/docs/f.md@h2#0",
                20,
            ),
        ]
    };
    let dir = tempfile::tempdir().unwrap();
    let (backend, db) = store_with(dir.path(), &[(rigger::conductor::STREAM, events())]);
    backend
        .prune_derived_index(
            &Namespaced::prefix_for(PROJECT),
            &rigger::ingest::derived_index_identity(),
        )
        .unwrap();
    let kept: Vec<(String, i64)> = keyed_rows(&db).into_iter().map(|r| (r.3, r.4)).collect();
    assert_eq!(
        kept,
        vec![("gd/docs/f.md@h2#0".to_string(), nanos(20))],
        "the survivor links what the alias named when it was recorded, from its own date"
    );
    let graph = compaction_rebuilds_the_whole_logs_graph(
        events(),
        &[("docs/f.md", "artifact"), ("src/b.rs", "artifact")],
    );
    assert_eq!(
        graph
            .edges
            .iter()
            .map(|e| (e.to.as_str(), e.valid_from))
            .collect::<Vec<_>>(),
        vec![("src/b.rs", nanos(20))],
        "the whole log holds only the redefined link, from its own date"
    );
}

/// Where an alias is defined that the recording's own stream never folds - another stream of this
/// project, or another project's run stream - it names nothing for this stream's recordings, as it
/// names nothing for the fold that rebuilds this project's graph from its run stream alone: `h2`
/// linking `a-alias` is a new link, never `h1`'s link to `src/a.rs`, and keeps its own date.
fn an_alias_defined_outside_the_recordings_stream_joins_no_facts(project: &str, stream: &str) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    Namespaced::new(&backend, project)
        .append(
            stream,
            ExpectedRevision::Any,
            &[alias_defined("a-alias", "src/a.rs", 5)],
        )
        .unwrap();
    Namespaced::new(&backend, PROJECT)
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/a.rs"),
                    "gd/docs/f.md@h1#0",
                    10,
                ),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("a-alias"),
                    "gd/docs/f.md@h2#0",
                    20,
                ),
            ],
        )
        .unwrap();
    let pruned = backend
        .prune_derived_index(
            &Namespaced::prefix_for(PROJECT),
            &rigger::ingest::derived_index_identity(),
        )
        .unwrap();
    assert_eq!(
        (pruned.removed, pruned.superseded_generations),
        (per_type(0, 0, 0, 1), 1),
        "h1 is shed as a superseded generation"
    );
    let kept: Vec<(String, i64)> = keyed_rows(db.to_str().unwrap())
        .into_iter()
        .map(|r| (r.3, r.4))
        .collect();
    assert_eq!(
        kept,
        vec![("gd/docs/f.md@h2#0".to_string(), nanos(20))],
        "an alias another stream defines never carries h1's date onto h2"
    );
}

rigger::test_cases! {
    /// The alias is defined on another stream of this project.
    an_alias_on_another_stream_of_the_project_joins_no_facts:
        an_alias_defined_outside_the_recordings_stream_joins_no_facts(PROJECT, "side-stream");
    /// The alias is defined on another project's run stream in the same store.
    an_alias_another_project_defines_joins_no_facts:
        an_alias_defined_outside_the_recordings_stream_joins_no_facts(
            "proj-other",
            rigger::conductor::STREAM,
        );
}

/// A test file's proof reference to `name` at `line`: test-origin evidence heading its file's batch.
fn proof(name: &str, line: u32) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "tests/t.rs", "name": name, "lang": "rust", "fresh": true, "line": line,
        "is_test": true,
    }))
    .unwrap()
}

/// Proof a test recorded on a definition is another fold's word, not the generation's: when a
/// later generation drops the definition the node retires WITH that proof and WITHOUT the
/// definition's own attrs, and when a still later generation defines it again it comes back as
/// that generation says it is, still proven by the test - exactly what the compacted log folds,
/// where the reference waits for the definition and lands on it the moment it folds.
#[test]
fn a_tests_proof_survives_its_entitys_retirement_and_returns_with_it() {
    let graph = compaction_rebuilds_the_whole_logs_graph(
        vec![
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                head("alpha", 1, false),
                "gc/src/f.rs@h1#0",
                10,
            ),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("gone", 9),
                "gc/src/f.rs@h1#1",
                10,
            ),
            keyed(
                TYPE_EDGE_INFERRED,
                proof("gone", 3),
                "gc/tests/t.rs@h1#0",
                12,
            ),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                head("alpha", 2, false),
                "gc/src/f.rs@h2#0",
                20,
            ),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                head("alpha", 3, false),
                "gc/src/f.rs@h3#0",
                30,
            ),
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("gone", 7),
                "gc/src/f.rs@h3#1",
                30,
            ),
        ],
        &[
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "code-entity"),
            ("src/f.rs::gone", "code-entity"),
        ],
    );
    let gone = graph
        .nodes
        .iter()
        .find(|n| n.id == "src/f.rs::gone")
        .unwrap();
    let attrs: Vec<(&str, &str)> = gone
        .attrs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        attrs,
        vec![
            ("kind", "function"),
            ("lang", "rust"),
            ("line", "7"),
            ("name", "gone"),
            ("proof_evidence", r#"["tests/t.rs:3"]"#),
            ("proven_by", "1"),
        ],
        "the returned definition carries h3's attrs and the test's proof, nothing of h1's"
    );
}

// ---------------------------------------------------------------------------------------
// 2. Identity is per stream and per prefix
// ---------------------------------------------------------------------------------------

#[test]
fn a_generation_is_superseded_only_by_a_later_one_of_the_same_prefix_file_and_stream() {
    let dir = tempfile::tempdir().unwrap();
    let main = rigger::conductor::STREAM;
    let side = "side-stream";
    let (backend, db) = store_with(
        dir.path(),
        &[
            (
                main,
                vec![
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("a", 1),
                        "gc/src/f.rs@h1#0",
                        1,
                    ),
                    // The DESIGN batch of the same file path, at h1: a separate identity.
                    keyed(
                        TYPE_DOC_LINK_EXTRACTED,
                        link("src/f.rs"),
                        "gd/src/f.rs@h1#0",
                        2,
                    ),
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("a", 2),
                        "gc/src/f.rs@h2#0",
                        3,
                    ),
                    // A file whose path EXTENDS src/f.rs is its own identity too.
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("b", 1),
                        "gc/src/f.rs2@h1#0",
                        4,
                    ),
                ],
            ),
            (
                side,
                vec![keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("a", 9),
                    "gc/src/f.rs@h9#0",
                    5,
                )],
            ),
        ],
    );
    let identity = rigger::ingest::derived_index_identity();
    let prefix = Namespaced::prefix_for(PROJECT);

    let pruned = backend.prune_derived_index(&prefix, &identity).unwrap();
    assert_eq!(
        pruned.removed,
        per_type(1, 0, 0, 0),
        "only main's gc h1 is shed"
    );
    assert_eq!(pruned.superseded_generations, 1);

    let kept: Vec<(String, String)> = keyed_rows(&db)
        .into_iter()
        .map(|r| (r.1.strip_prefix(prefix.as_str()).unwrap().to_string(), r.3))
        .collect();
    let main_tail = main.to_string();
    let side_tail = side.to_string();
    assert_eq!(
        kept,
        vec![
            (main_tail.clone(), "gd/src/f.rs@h1#0".to_string()),
            (main_tail.clone(), "gc/src/f.rs@h2#0".to_string()),
            (main_tail, "gc/src/f.rs2@h1#0".to_string()),
            (side_tail, "gc/src/f.rs@h9#0".to_string()),
        ],
        "a later gc generation never supersedes the gd batch, a longer path, or another stream's \
         generation, and another stream's later generation never supersedes this stream's"
    );
}

// ---------------------------------------------------------------------------------------
// 3. The key-parser contract
// ---------------------------------------------------------------------------------------

#[test]
fn key_parts_answers_only_through_the_parser_the_policy_declared() {
    let shipped = rigger::ingest::derived_index_identity();
    assert_eq!(
        shipped.key_parts("gc/src/a.rs@h1#0"),
        Some(("gc/src/a.rs", "h1")),
        "the shipped policy cuts a derived key into its batch identity and generation"
    );
    assert_eq!(
        shipped.key_parts("gc/src/a.rs@h1"),
        None,
        "a key the shipped parser rejects has no parts"
    );
    assert_eq!(
        identity_without_key_parts().key_parts("gc/src/a.rs@h1#0"),
        None,
        "a policy that declared no parser answers None even for a well-formed derived key"
    );
}

/// Where no generation is known - the policy declares no parser, or the declared parser rejects
/// the key - nothing is shed as superseded: exact-key semantics keep each key's latest recording.
/// An unparseable key therefore survives beside a later well-formed generation of what LOOKS
/// like the same file (the fail-safe direction).
#[test]
fn a_key_with_no_parsed_generation_is_only_deduplicated_and_never_shed_as_superseded() {
    let no_parser = (
        identity_without_key_parts(),
        vec![
            ("gc/src/f.rs@h1#0", 1, 1),
            ("gc/src/f.rs@h1#0", 1, 2),
            ("gc/src/f.rs@h2#0", 2, 3),
            ("gc/src/f.rs@h2#0", 2, 4),
        ],
        2,
        vec![("gc/src/f.rs@h1#0", 2), ("gc/src/f.rs@h2#0", 4)],
    );
    // No `#<index>` tail on h1: the shipped parser rejects that key.
    let rejected_key = (
        rigger::ingest::derived_index_identity(),
        vec![
            ("gc/src/f.rs@h1", 1, 1),
            ("gc/src/f.rs@h1", 1, 2),
            ("gc/src/f.rs@h2#0", 2, 3),
        ],
        1,
        vec![("gc/src/f.rs@h1", 2), ("gc/src/f.rs@h2#0", 3)],
    );
    for (identity, recorded, removed, kept) in [no_parser, rejected_key] {
        let dir = tempfile::tempdir().unwrap();
        let events = recorded
            .iter()
            .map(|&(key, line, secs)| {
                keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("a", line), key, secs)
            })
            .collect();
        let (backend, db) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
        let prefix = Namespaced::prefix_for(PROJECT);

        assert_eq!(
            backend
                .count_derived_duplicates(&prefix, &identity)
                .unwrap()
                .removed,
            per_type(removed, 0, 0, 0),
            "the preview must count exactly what the prune removes"
        );
        let pruned = backend.prune_derived_index(&prefix, &identity).unwrap();
        assert_eq!(pruned.removed, per_type(removed, 0, 0, 0));
        assert_eq!(pruned.superseded_generations, 0);
        let actual: Vec<(String, i64)> = keyed_rows(&db).into_iter().map(|r| (r.3, r.4)).collect();
        let expected: Vec<(String, i64)> = kept
            .iter()
            .map(|&(key, secs)| (key.to_string(), nanos(secs)))
            .collect();
        assert_eq!(actual, expected, "each key keeps only its latest recording");
    }
}

/// A parser of a key shape the store has never seen: `<identity>:<generation>`.
fn colon_parts(key: &str) -> Option<(&str, &str)> {
    key.split_once(':')
}

#[test]
fn the_store_sheds_by_whatever_key_parser_the_policy_declares() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, db) = store_with(
        dir.path(),
        &[(
            rigger::conductor::STREAM,
            vec![
                keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("a", 1), "fileA:one", 1),
                keyed(TYPE_EDGE_INFERRED, entity("a", 1), "fileA:one", 1),
                keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("b", 1), "fileB:one", 2),
                keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("a", 2), "fileA:two", 3),
            ],
        )],
    );
    let identity = identity_without_key_parts().with_key_parts(colon_parts);
    assert_eq!(identity.key_parts("fileA:one"), Some(("fileA", "one")));
    let prefix = Namespaced::prefix_for(PROJECT);

    assert_eq!(
        backend
            .count_derived_duplicates(&prefix, &identity)
            .unwrap()
            .removed,
        per_type(1, 1, 0, 0)
    );
    let pruned = backend.prune_derived_index(&prefix, &identity).unwrap();
    assert_eq!(pruned.removed, per_type(1, 1, 0, 0));
    assert_eq!(
        pruned.superseded_generations, 2,
        "fileA:one is superseded by fileA:two in EVERY type its batch recorded"
    );
    let kept: Vec<String> = keyed_rows(&db).into_iter().map(|r| r.3).collect();
    assert_eq!(kept, vec!["fileB:one".to_string(), "fileA:two".to_string()]);
}

// ---------------------------------------------------------------------------------------
// 4. The operator's preview through the binary
// ---------------------------------------------------------------------------------------

#[test]
fn bare_reset_previews_superseded_generations_and_the_real_prune_removes_exactly_that() {
    let dir = temp_store_project();
    let root = dir.path();
    {
        let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("a", 1),
                        "gc/src/f.rs@h1#0",
                        1,
                    ),
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("a", 1),
                        "gc/src/f.rs@h1#0",
                        2,
                    ),
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("a", 2),
                        "gc/src/f.rs@h2#0",
                        3,
                    ),
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("a", 2),
                        "gc/src/f.rs@h2#0",
                        4,
                    ),
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        entity("a", 3),
                        "gc/src/f.rs@h3#0",
                        5,
                    ),
                ],
            )
            .unwrap();
    }

    let (menu, err, ok) = run_rigger(root, &["reset"]);
    assert!(ok, "a bare `rigger reset` must exit 0; stderr: {err}");
    assert!(
        menu.contains(
            "--derived: 4 redundant derived-index event(s) prunable from the event log across 4 \
             derived type(s), 4 of them recordings of a superseded generation"
        ),
        "the menu must count both superseded generations' four recordings, as superseded; got: \
         {menu:?}"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    assert!(
        out.contains("pruned 4 redundant derived-index event(s)")
            && out.contains("(CodeEntityExtracted 4, EdgeInferred 0, DocConceptExtracted 0, DocLinkExtracted 0)")
            && out.contains("4 of them recordings of a superseded generation"),
        "the real prune must remove exactly what the menu previewed, all of it superseded; got: \
         {out:?}"
    );
}

// ---------------------------------------------------------------------------------------
// 5. The fold's generation rule at its seams
// ---------------------------------------------------------------------------------------

/// A `(batch identity, generation)` pair as the key parser answers it.
type Parts<'k> = (&'k str, &'k str);

/// `ingest::derived_generation` is the one place the fold learns who asserted a fact: the
/// `(<prefix>/<file>, generation)` of a derived-index event's replay key, and nothing for any other
/// event, for a derived event with no key, or for a key that is not the derived shape.
#[test]
fn derived_generation_names_only_a_keyed_derived_events_identity_and_generation() {
    let cases: Vec<(Event, Option<Parts>, &str)> = vec![
        (
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("a", 1),
                "gc/src/f.rs@h1#0",
                1,
            ),
            Some(("gc/src/f.rs", "h1")),
            "a code entity",
        ),
        (
            keyed(TYPE_EDGE_INFERRED, entity("a", 1), "gc/src/f.rs@h2#3", 1),
            Some(("gc/src/f.rs", "h2")),
            "an inferred edge",
        ),
        (
            keyed(
                TYPE_DOC_CONCEPT_EXTRACTED,
                link("x"),
                "gd/docs/f.md@h3#0",
                1,
            ),
            Some(("gd/docs/f.md", "h3")),
            "a doc concept",
        ),
        (
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("x"),
                "gd/docs/a@b#c.md@h4#12",
                1,
            ),
            Some(("gd/docs/a@b#c.md", "h4")),
            "a doc link whose file holds `@` and `#`",
        ),
        (
            keyed("DecisionMade", link("x"), "gc/src/f.rs@h1#0", 1),
            None,
            "a non-derived event carrying a derived-shaped key",
        ),
        (
            Event::new(TYPE_CODE_ENTITY_EXTRACTED, entity("a", 1)),
            None,
            "a derived event with no replay key",
        ),
        (
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("a", 1),
                "gc/src/f.rs@h1",
                1,
            ),
            None,
            "a derived event whose key has no `#<index>` tail",
        ),
    ];
    for (event, expected, what) in &cases {
        assert_eq!(
            rigger::ingest::derived_generation(event),
            *expected,
            "derived_generation of {what}"
        );
    }
}

/// Two projects share one `graph.db`: each one's generations supersede only its own. Folded
/// interleaved - A's h1, B's h1, B's h9, A's h2 - each project's projection is exactly the one it
/// folds alone, so A's h2 retires A's `gone` and B's h9 never touched A.
#[test]
fn one_projects_generations_never_supersede_anothers_in_a_shared_graph() {
    const OTHER: &str = "proj-gen-other";
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(
            rigger::conductor::STREAM,
            vec![
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("alpha", 1, false),
                    "gc/src/f.rs@h1#0",
                    10,
                ),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("gone", 9),
                    "gc/src/f.rs@h1#1",
                    10,
                ),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("alpha", 2, false),
                    "gc/src/f.rs@h2#0",
                    20,
                ),
            ],
        )],
    );
    Namespaced::new(&backend, OTHER)
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("beta", 1, false),
                    "gc/src/f.rs@h1#0",
                    11,
                ),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("kept", 3),
                    "gc/src/f.rs@h9#1",
                    12,
                ),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("beta", 2, false),
                    "gc/src/f.rs@h9#0",
                    12,
                ),
            ],
        )
        .unwrap();
    let a = run_events(&backend, PROJECT);
    let b = run_events(&backend, OTHER);
    let a_alone = fold_in_batches(&dir.path().join("a.db"), PROJECT, std::slice::from_ref(&a));
    let b_alone = fold_in_batches(&dir.path().join("b.db"), OTHER, std::slice::from_ref(&b));

    let shared = dir.path().join("shared.db");
    fold_in_batches(&shared, PROJECT, &[a[..2].to_vec()]);
    fold_in_batches(&shared, OTHER, &[b[..1].to_vec(), b[1..].to_vec()]);
    let a_shared = fold_in_batches(&shared, PROJECT, &[a[2..].to_vec()]);
    let b_shared = fold_in_batches(&shared, OTHER, &[]);

    assert_eq!(a_shared, a_alone, "A's projection is the one A folds alone");
    assert_eq!(b_shared, b_alone, "B's projection is the one B folds alone");
    let ids = |json: &str| -> Vec<String> {
        let graph: rigger::contextgraph::Graph = serde_json::from_str(json).unwrap();
        graph.nodes.into_iter().map(|n| n.id).collect()
    };
    assert_eq!(
        ids(&a_alone),
        vec!["src/f.rs", "src/f.rs::alpha"],
        "A's h2 retired the `gone` its h1 defined"
    );
    assert_eq!(
        ids(&b_alone),
        vec!["src/f.rs", "src/f.rs::beta", "src/f.rs::kept"],
        "B's h9 holds `beta` and `kept`"
    );
}

// ---------------------------------------------------------------------------------------
// 6. A graph.db folded before the generation rule is rebuilt once, by `rigger setup`
// ---------------------------------------------------------------------------------------

/// `src/f.rs` drops `gone` and `docs/f.md` drops its link to `src/old.rs` between h1 and h2: the
/// facts a graph folded before the generation rule keeps and a rebuild does not.
fn two_generations_dropping_facts() -> Vec<Event> {
    vec![
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            head("alpha", 1, false),
            "gc/src/f.rs@h1#0",
            10,
        ),
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            entity("gone", 9),
            "gc/src/f.rs@h1#1",
            10,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/old.rs"),
            "gd/docs/f.md@h1#0",
            10,
        ),
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            head("alpha", 2, false),
            "gc/src/f.rs@h2#0",
            20,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/a.rs"),
            "gd/docs/f.md@h2#0",
            20,
        ),
    ]
}

/// A `graph.db` at `db` holding `events` folded and then put in the shape a file folded before the
/// generation rule has: `ledgers` (SQL) rewrites its generation ledgers and its projection version
/// is cleared.
fn a_pre_rule_graph_db(db: &Path, events: &[Event], ledgers: &str) {
    use rigger::contextgraph::sqlite::Projector;

    common::fixtures::folds(
        &Projector::open(db.to_str().unwrap(), PROJECT).unwrap(),
        events,
    );
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute_batch(&format!(
        "DROP VIEW live_node_assertions; DROP VIEW live_edge_assertions;
         DROP TABLE generations; DROP TABLE node_assertions; DROP TABLE edge_assertions;
         DROP TABLE retired_nodes; DROP TABLE detached_attachments; DROP TABLE relabel_owed;
         {ledgers}
         PRAGMA user_version = 0;"
    ))
    .unwrap();
}

/// What a rebuild that reports nothing hands its progress callback.
fn no_progress(_: rigger::contextgraph::sqlite::RebuildProgress) {}

/// A rebuild source over `log`: every call records the position it was asked to read after, and
/// hands the events past it in batches of `batch`, each with the log's last position.
fn source_over<'a>(
    log: &'a [Event],
    batch: usize,
    afters: &'a std::cell::RefCell<Vec<u64>>,
) -> impl FnMut(u64, &mut RebuildSink) -> Result<(), rigger::contextgraph::Error> + 'a {
    move |after, sink| {
        afters.borrow_mut().push(after);
        rigger::contextgraph::sqlite::stream_past(log, after, batch, sink)
    }
}

/// A rebuild source over `backend`'s live selection of `project`'s run stream, in batches of
/// `batch` - the source `rigger setup` hands a rebuild.
fn live_selection_of<'a>(
    backend: &'a Store,
    project: &'a str,
    batch: usize,
) -> impl FnMut(u64, &mut RebuildSink) -> Result<(), rigger::contextgraph::Error> + 'a {
    let identity = rigger::ingest::derived_index_identity();
    move |after, sink| {
        backend
            .read_live_selection(
                &Namespaced::prefix_for(project),
                rigger::conductor::STREAM,
                &identity,
                after,
                batch,
                &mut |events, head| {
                    sink(events, head).map_err(|e| rigger::eventstore::Error::Backend(e.0))
                },
            )
            .map_err(|e| rigger::contextgraph::Error(e.to_string()))
    }
}

/// Rebuild the `graph.db` at `db` from `source`, reporting nothing: whether there was one to pay,
/// and what it did.
fn rebuild(
    db: &Path,
    source: &mut rigger::contextgraph::sqlite::RebuildSource,
) -> Result<Option<Rebuilt>, rigger::contextgraph::Error> {
    use rigger::contextgraph::sqlite::Projector;
    Projector::rebuild(
        &Projector::lock_rebuild(db.to_str().unwrap()).unwrap(),
        PROJECT,
        false,
        source,
        &mut no_progress,
    )
}

/// A `graph.db` folded before the generation rule - with no ledgers at all, or with them in an
/// older shape - owes one rebuild: nothing folds into it until then, the rebuild reaches exactly
/// what a fresh fold of the log reaches, reporting each batch it folds, and it happens once - a
/// paid rebuild never reads the log again.
fn a_pre_rule_graph_db_is_rebuilt_from_the_log_once(ledgers: &str) {
    use rigger::contextgraph::sqlite::{Projector, RebuildProgress};
    use rigger::contextgraph::Projection;
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (old_db, fresh_db) = (dir.path().join("old.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&old_db, &log[..3], ledgers);

    let graph = Projector::open(old_db.to_str().unwrap(), PROJECT).unwrap();
    assert!(
        graph.rebuild_owed().unwrap(),
        "a pre-rule graph.db owes a rebuild"
    );
    assert_eq!(
        Fold::of_batch(|| rigger::contextgraph::wired(Some(&graph)), &log[3..]),
        Fold::NotFolded(format!("graph: {}", rigger::contextgraph::REBUILD_OWED)),
        "nothing folds incrementally into a pre-rule graph.db"
    );
    drop(graph);
    let afters = std::cell::RefCell::new(Vec::new());
    let mut reported = Vec::new();
    let ran = Projector::rebuild(
        &Projector::lock_rebuild(old_db.to_str().unwrap()).unwrap(),
        PROJECT,
        false,
        &mut source_over(&log, 2, &afters),
        &mut |at| reported.push(at),
    )
    .unwrap();
    assert_eq!(ran, Some(Rebuilt::default()), "the owed rebuild runs");
    let (p, head) = (|i: usize| log[i].position, log[4].position);
    let at = |through, folded| RebuildProgress {
        start: 0,
        through,
        head,
        folded,
    };
    assert_eq!(
        reported,
        vec![at(p(1), 2), at(p(3), 4), at(p(4), 5)],
        "the rebuild reports each batch it folds"
    );
    assert_eq!(
        *afters.borrow(),
        vec![0, p(4)],
        "the log is read whole into the shadow, then past it for the tail"
    );
    assert_eq!(
        (
            dir.path().join("old.db.rebuild").exists(),
            dir.path().join("old.db.rebuild-journal").exists()
        ),
        (false, false),
        "the shadow and its journal are removed once copied in"
    );

    fold_in_batches(&fresh_db, PROJECT, std::slice::from_ref(&log));
    assert_eq!(
        identity_of(&old_db),
        identity_of(&fresh_db),
        "the rebuilt graph.db is the log's, fold state included"
    );
    let reopened = Projector::open(old_db.to_str().unwrap(), PROJECT).unwrap();
    assert!(
        !reopened.rebuild_owed().unwrap(),
        "the rebuild happens once"
    );
    drop(reopened);
    let again = rebuild(&old_db, &mut |_, _| {
        panic!("a paid rebuild never reads the log")
    })
    .unwrap();
    assert_eq!(again, None, "a paid rebuild does not run again");
    common::fixtures::folds(
        &Projector::open(old_db.to_str().unwrap(), PROJECT).unwrap(),
        &later_events(),
    );
}

/// Given a log with no live selection - a server-backed log, whose live selection is its run stream
/// as it stands - and a `graph.db` that misses one of its events, when setup reads the graph's
/// ledger against it and pays the rebuild, then the owed check reads the stream's positions alone
/// in one ordered pass, materializing no event, and the rebuild reads each event once: its fold
/// reads the stream, and its tail reads only the event the stream gained meanwhile.
#[test]
fn a_log_with_no_live_selection_is_read_for_positions_alone_and_rebuilt_reading_each_event_once() {
    use common::fixtures::{CountedRead, ReadCountingStore};
    use rigger::contextgraph::sqlite::{stream_positions, stream_source, Projector};
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let n = log.len();
    let graph_db = dir.path().join("graph.db");
    fold_in_batches(&graph_db, PROJECT, &[log[..n - 1].to_vec()]);
    let late = Event::new(
        rigger::contextgraph::TYPE_DECISION_MADE,
        br#"{"id":"d-late","summary":"s","governs":["src/f.rs"],"supersedes":""}"#.to_vec(),
    );
    let store = Namespaced::new(&backend, PROJECT);
    let counting =
        ReadCountingStore::new(&store).interleaving(1, rigger::conductor::STREAM, vec![late]);
    let run = || rigger::conductor::STREAM.to_string();

    let owed = Projector::open(graph_db.to_str().unwrap(), PROJECT)
        .unwrap()
        .owed_against(&mut stream_positions(
            &counting,
            rigger::conductor::STREAM,
            2,
        ))
        .unwrap();
    let rebuilt = Projector::rebuild(
        &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
        PROJECT,
        !owed.is_empty(),
        &mut stream_source(&counting, rigger::conductor::STREAM, 2),
        &mut no_progress,
    )
    .unwrap();
    assert_eq!(
        (owed, rebuilt, counting.reads()),
        (
            vec![rigger::contextgraph::OWED_LOST_FOLD],
            Some(Rebuilt::default()),
            vec![
                CountedRead::StreamPositions {
                    stream: run(),
                    handed: n,
                },
                CountedRead::StreamBatched {
                    stream: run(),
                    from: 0,
                    batches: [vec![2; n / 2], vec![1; n % 2]].concat(),
                },
                CountedRead::StreamBatched {
                    stream: run(),
                    from: n as i64,
                    batches: vec![1],
                },
            ]
        ),
        "positions alone, then each event read once across the fold and its tail, no read \
         handing more than the batch bound of two"
    );
    assert_eq!(
        serde_json::to_string(
            &Projector::open(graph_db.to_str().unwrap(), PROJECT)
                .unwrap()
                .whole()
                .unwrap()
        )
        .unwrap(),
        fold_in_batches(
            &dir.path().join("clean.db"),
            PROJECT,
            &[run_events(&backend, PROJECT)]
        ),
        "the rebuilt graph is a fresh fold of the whole log, the late event included"
    );
}

rigger::test_cases! {
    /// Folded before the ledgers existed at all.
    a_graph_db_without_ledgers_is_rebuilt_from_the_log_once:
        a_pre_rule_graph_db_is_rebuilt_from_the_log_once("");
    /// Folded with the ledgers in an older shape.
    a_graph_db_with_older_ledgers_is_rebuilt_from_the_log_once:
        a_pre_rule_graph_db_is_rebuilt_from_the_log_once(
            "CREATE TABLE generations (project TEXT, identity TEXT, generation TEXT, prior TEXT);
             CREATE TABLE edge_assertions (
               project TEXT, identity TEXT, generation TEXT, edge_id INTEGER);",
        );
}

/// A rebuild folds the log one event at a time, as the live fold does: an event whose fold fails
/// (a malformed payload the log holds) is skipped, never failing the rebuild, and leaves the rest
/// of the log folded exactly as without it.
#[test]
fn a_rebuild_skips_an_event_whose_fold_fails() {
    let malformed = Event::new(
        "DecisionMade",
        br#"{"id":"bad","summary":"s","governs":"src/f.rs","supersedes":""}"#.to_vec(),
    );
    let dir = tempfile::tempdir().unwrap();
    let mut events = two_generations_dropping_facts();
    events.insert(3, malformed);
    let (backend, _) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
    let log = run_events(&backend, PROJECT);
    let (rebuilt, clean) = (dir.path().join("rebuilt.db"), dir.path().join("clean.db"));
    a_pre_rule_graph_db(&rebuilt, &log[..1], "");
    let afters = std::cell::RefCell::new(Vec::new());
    let ran = rebuild(&rebuilt, &mut source_over(&log, 10, &afters)).unwrap();
    assert_eq!(
        ran,
        Some(Rebuilt {
            passed_over: 1,
            pruned: PruneStats::default()
        }),
        "the owed rebuild runs, passing over the malformed event"
    );
    let without: Vec<Event> = log
        .iter()
        .filter(|e| e.position != log[3].position)
        .cloned()
        .collect();
    fold_in_batches(&clean, PROJECT, &[without]);
    assert_eq!(
        identity_of(&rebuilt),
        identity_of(&clean),
        "the malformed event is skipped and every other event folds"
    );
}

/// An event appended while the rebuild ran - by an emit that found the old file still owing and so
/// did not fold - is on the log when the shadow is swapped in: the rebuild then reads the log past
/// the last position it folded, and only past it, and folds what it gained into the new file, so
/// the rebuilt graph is the whole log's.
#[test]
fn a_rebuild_folds_what_the_log_gained_while_it_ran() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (rebuilt, fresh) = (dir.path().join("rebuilt.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&rebuilt, &log[..3], "");
    let (afters, before) = (
        std::cell::RefCell::new(Vec::new()),
        std::cell::RefCell::new(Vec::new()),
    );
    let (mut before_run, mut whole) = (
        source_over(&log[..3], 2, &before),
        source_over(&log, 2, &afters),
    );
    let mut reads = 0;
    let ran = rebuild(&rebuilt, &mut |after, sink| {
        reads += 1;
        if reads == 1 {
            before_run(after, sink)
        } else {
            whole(after, sink)
        }
    })
    .unwrap();
    assert_eq!(
        (ran, before.borrow().clone(), afters.borrow().clone()),
        (Some(Rebuilt::default()), vec![0], vec![log[2].position]),
        "the tail is read once, past the last position the shadow folded"
    );
    fold_in_batches(&fresh, PROJECT, std::slice::from_ref(&log));
    assert_eq!(
        identity_of(&rebuilt),
        identity_of(&fresh),
        "the events appended during the rebuild are folded after it"
    );
}

/// A rebuild interrupted part way leaves `graph.db` byte for byte as it was, still owing, with its
/// committed batches in the shadow; the next rebuild resumes past the last of them, folding only
/// what they did not, and reaches exactly the whole log's graph.
#[test]
fn an_interrupted_rebuild_leaves_graph_db_untouched_and_resumes_from_its_last_committed_batch() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (db, fresh) = (dir.path().join("graph.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&db, &log[..3], "");
    let before = std::fs::read(&db).unwrap();
    let interrupted = rebuild(&db, &mut |_, sink| {
        sink(&log[..2], log[4].position)?;
        Err(rigger::contextgraph::Error("interrupted".to_string()))
    });
    assert_eq!(
        interrupted.unwrap_err().0,
        "interrupted",
        "the interruption fails the rebuild"
    );
    assert!(
        std::fs::read(&db).unwrap() == before
            && Projector::open(db.to_str().unwrap(), PROJECT)
                .unwrap()
                .rebuild_owed()
                .unwrap(),
        "graph.db is untouched and still owes the rebuild"
    );
    assert!(
        dir.path().join("graph.db.rebuild").exists(),
        "the committed batch is kept in the shadow"
    );

    let (afters, handed) = (
        std::cell::RefCell::new(Vec::new()),
        std::cell::RefCell::new(Vec::new()),
    );
    let mut source = source_over(&log, 2, &afters);
    let ran = rebuild(&db, &mut |after, sink| {
        source(after, &mut |events, head| {
            handed
                .borrow_mut()
                .extend(events.iter().map(|e| e.position));
            sink(events, head)
        })
    })
    .unwrap();
    assert_eq!(
        (ran, afters.borrow().clone(), handed.borrow().clone()),
        (
            Some(Rebuilt::default()),
            vec![log[1].position, log[4].position],
            vec![log[2].position, log[3].position, log[4].position]
        ),
        "the rerun resumes past the committed batch and never folds it again"
    );
    fold_in_batches(&fresh, PROJECT, std::slice::from_ref(&log));
    assert_eq!(
        identity_of(&db),
        identity_of(&fresh),
        "the resumed rebuild is the whole log's"
    );
}

/// A rebuild interrupted after its swap, while folding the tail the log gained, leaves a `graph.db`
/// that no longer owes the rebuild but still owes that tail: the next rebuild folds only the tail,
/// past the last position folded, and the one after it has nothing to do.
#[test]
fn a_rebuild_interrupted_in_its_tail_finishes_the_tail_on_the_next_call() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (db, fresh) = (dir.path().join("graph.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&db, &log[..3], "");
    let mut reads = 0;
    let interrupted = rebuild(&db, &mut |_, sink| {
        reads += 1;
        match reads {
            1 => sink(&log[..3], log[2].position),
            _ => Err(rigger::contextgraph::Error("interrupted".to_string())),
        }
    });
    assert_eq!(interrupted.unwrap_err().0, "interrupted");
    assert_eq!(
        version_and_owed(&db),
        (1, false),
        "the rebuilt file was swapped in"
    );

    let afters = std::cell::RefCell::new(Vec::new());
    let ran = rebuild(&db, &mut source_over(&log, 2, &afters)).unwrap();
    assert_eq!(
        (ran, afters.borrow().clone()),
        (Some(Rebuilt::default()), vec![log[2].position]),
        "the next rebuild folds only the tail"
    );
    let again = rebuild(&db, &mut |_, _| panic!("a finished rebuild reads nothing")).unwrap();
    assert_eq!(again, None, "and the one after it has nothing to do");
    fold_in_batches(&fresh, PROJECT, &[log]);
    assert_eq!(identity_of(&db), identity_of(&fresh));
}

/// The live selection a rebuild folds is exactly what `rigger reset --derived` keeps - every
/// non-derived event, each identity's latest generation, the carried valid-times - streamed once
/// in position order, in batches, each with the stream's last position; read past a position it
/// hands only what follows. The graph folded from it is the whole log's, and the compacted log's.
#[test]
fn the_live_selection_is_exactly_what_the_compaction_keeps_and_rebuilds_the_whole_logs_graph() {
    let dir = tempfile::tempdir().unwrap();
    let mut events = two_generations_dropping_facts();
    events.insert(2, governs("src/f.rs::alpha", 15));
    events.push(keyed(
        TYPE_DOC_LINK_EXTRACTED,
        link("src/a.rs"),
        "gd/docs/f.md@h2#0",
        25,
    ));
    let (backend, _) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
    let log = run_events(&backend, PROJECT);
    let head = log.last().unwrap().position;
    let identity = rigger::ingest::derived_index_identity();
    let select = |after, batch| {
        let mut batches: Vec<(Vec<(u64, std::time::SystemTime)>, u64)> = Vec::new();
        backend
            .read_live_selection(
                &Namespaced::prefix_for(PROJECT),
                rigger::conductor::STREAM,
                &identity,
                after,
                batch,
                &mut |events, head| {
                    batches.push((
                        events.iter().map(|e| (e.position, e.valid_from)).collect(),
                        head,
                    ));
                    Ok(())
                },
            )
            .unwrap();
        batches
    };
    let selected = select(0, 2);
    let (whole_db, selected_db, compacted_db) = (
        dir.path().join("whole.db"),
        dir.path().join("selected.db"),
        dir.path().join("compacted.db"),
    );
    rebuilt_whole(&backend, &whole_db);
    a_pre_rule_graph_db(&selected_db, &log[..1], "");
    assert_eq!(
        rebuild(&selected_db, &mut live_selection_of(&backend, PROJECT, 2)).unwrap(),
        Some(Rebuilt::default())
    );

    backend
        .prune_derived_index(&Namespaced::prefix_for(PROJECT), &identity)
        .unwrap();
    let kept: Vec<(u64, std::time::SystemTime)> = run_events(&backend, PROJECT)
        .iter()
        .map(|e| (e.position, e.valid_from))
        .collect();
    assert_eq!(
        selected,
        kept.chunks(2)
            .map(|batch| (batch.to_vec(), head))
            .collect::<Vec<_>>(),
        "the selection is the compacted log, in batches of two, each with the stream's head"
    );
    assert_eq!(
        select(kept[1].0, 10),
        vec![(kept[2..].to_vec(), head)],
        "read past a position it hands only what follows"
    );
    rebuilt_whole(&backend, &compacted_db);
    assert_eq!(
        identity_of(&selected_db),
        identity_of(&whole_db),
        "the graph folded from the live selection is the whole log's"
    );
    assert_eq!(
        identity_of(&compacted_db),
        identity_of(&whole_db),
        "and the compacted log's"
    );
}

/// `rigger reset --derived` refuses to compact while the project's `graph.db` still owes its
/// rebuild, and says how to pay it; `rigger setup` rebuilds it from the log, after which the
/// compaction runs.
#[test]
fn reset_derived_refuses_until_a_pre_rule_graph_db_is_rebuilt() {
    let dir = temp_store_project();
    let root = dir.path();
    {
        let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &two_generations_dropping_facts(),
            )
            .unwrap();
        let graph_db = rigger_file(root, "graph.db");
        let log = store
            .read_stream(
                rigger::conductor::STREAM,
                0,
                rigger::eventstore::Direction::Forward,
            )
            .unwrap();
        use rigger::contextgraph::sqlite::Projector;

        common::fixtures::folds(
            &Projector::open(graph_db.to_str().unwrap(), &run_stream_identity(root)).unwrap(),
            &log[..3],
        );
        rusqlite::Connection::open(&graph_db)
            .unwrap()
            .execute_batch("PRAGMA user_version = 0;")
            .unwrap();
    }

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(!ok, "reset --derived must refuse; stdout: {out}");
    assert!(
        err.contains(&format!(
            "reset --derived: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the refusal is the one owed refusal, naming the command that rebuilds it; stderr: {err}"
    );
    let (_, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must rebuild the graph; stderr: {err}");
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok,
        "reset --derived must run once the graph is rebuilt; stderr: {err}"
    );
    assert!(
        out.contains("pruned 3 redundant derived-index event(s)")
            && out.contains("3 of them recordings of a superseded generation"),
        "h1's three recordings are shed as superseded; got: {out}"
    );
}

/// Pruning superseded edges (`rigger reset --runs`) keeps what the fold may still bring back: an
/// attachment detached from a retired node and a design link its identity's prior generation
/// asserted. Only history nothing will revive is reclaimed, and the preview counts exactly that.
#[test]
fn pruning_superseded_edges_keeps_what_the_fold_may_revive() {
    use rigger::contextgraph::sqlite::Projector;

    let dir = tempfile::tempdir().unwrap();
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(11);
    let (backend, _) = store_with(
        dir.path(),
        &[(
            rigger::conductor::STREAM,
            vec![
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("alpha", 1, false),
                    "gc/src/f.rs@h1#0",
                    10,
                ),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("gone", 9),
                    "gc/src/f.rs@h1#1",
                    10,
                ),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/old.rs"),
                    "gd/docs/f.md@h1#0",
                    10,
                ),
                Event::new(
                    rigger::contextgraph::TYPE_COMMUNITY_ASSIGNED,
                    community_of("src/f.rs::gone"),
                )
                .with_valid_from(at),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    head("alpha", 2, false),
                    "gc/src/f.rs@h2#0",
                    20,
                ),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/a.rs"),
                    "gd/docs/f.md@h2#0",
                    20,
                ),
            ],
        )],
    );
    let db = dir.path().join("graph.db");
    fold_in_batches(&db, PROJECT, &[run_events(&backend, PROJECT)]);
    let graph = Projector::open(db.to_str().unwrap(), PROJECT).unwrap();
    let boundary = Some(nanos(100));
    let preview = graph.count_prunable(&[], boundary).unwrap();
    let pruned = graph.prune(&[], boundary).unwrap();
    assert_eq!(
        (preview.superseded_edges, pruned.superseded_edges),
        (2, 2),
        "only the retired CONTAINS edges of h1 (alpha's and gone's) are reclaimable"
    );
    common::fixtures::folds(&graph, &later_events());
    assert_eq!(
        tiers_into(&graph.whole().unwrap(), "community/1/c0"),
        vec![("src/f.rs::gone", "IN_COMMUNITY", "inferred")],
        "the detached membership survives the prune and revives when its node returns"
    );
    let edge_rows: i64 = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE to_id = 'src/old.rs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        edge_rows, 1,
        "the prior generation's retired link stays revivable"
    );
}

/// The fold rule a `graph.db` records (its `user_version`).
fn user_version(db: &Path) -> i64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}

/// The fold rule a `graph.db` records after an open of it, and whether that open owes a rebuild.
fn version_and_owed(db: &Path) -> (i64, bool) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    let owed = Projector::open(db.to_str().unwrap(), PROJECT)
        .unwrap()
        .rebuild_owed()
        .unwrap();
    (user_version(db), owed)
}

/// Opening a pre-rule `graph.db` never pays its rebuild and never writes to it: every open until
/// [`Projector::rebuild`] owes it again and leaves the file byte for byte as it was, so a command
/// that opens and refuses cannot let the next one fold onto it or undo a rebuild. A file at the
/// old rule that never folded anything holds nothing a rebuild could change: its open owes
/// nothing and records the current rule.
#[test]
fn an_open_owes_the_rebuild_until_it_is_paid_writes_nothing_and_an_unfolded_file_owes_none() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let folded = dir.path().join("folded.db");
    a_pre_rule_graph_db(&folded, &log[..3], "");
    let before = std::fs::read(&folded).unwrap();
    assert_eq!(
        version_and_owed(&folded),
        (0, true),
        "the first open owes it"
    );
    assert_eq!(
        version_and_owed(&folded),
        (0, true),
        "an open that did not rebuild leaves the rebuild owed"
    );
    assert!(
        std::fs::read(&folded).unwrap() == before,
        "opening a file that owes its rebuild writes nothing to it"
    );

    let unfolded = dir.path().join("unfolded.db");
    a_pre_rule_graph_db(&unfolded, &[], "");
    assert_eq!(
        version_and_owed(&unfolded),
        (1, false),
        "a file that folded nothing owes nothing and records the current rule"
    );
    assert_eq!(version_and_owed(&unfolded), (1, false), "and stays current");
}

/// The `graph.db` schema of the release before the generation rule: no ledgers, no `proofs`, and
/// the `pending_proof` table that rule replaced.
const RELEASE_ERA_SCHEMA: &str = "
PRAGMA journal_mode = WAL;
CREATE TABLE nodes (
  id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
  project TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (id, project)
);
CREATE TABLE edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
  valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL,
  project TEXT NOT NULL DEFAULT '',
  tier TEXT NOT NULL DEFAULT 'extracted'
);
CREATE INDEX idx_edges_from ON edges(from_id);
CREATE INDEX idx_edges_to ON edges(to_id);
CREATE TABLE aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
CREATE TABLE applied (position INTEGER PRIMARY KEY);
CREATE TABLE pending_proof (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project TEXT NOT NULL DEFAULT '',
  name TEXT NOT NULL,
  evidence TEXT NOT NULL
);
CREATE INDEX idx_pending_proof_name ON pending_proof(project, name);
";

/// An operator store whose log holds [`two_generations_dropping_facts`] plus a test's proof, and
/// whose `graph.db` the release before the generation rule folded: it still holds the definition
/// and the design link a superseded generation asserted, and the proof waiting in `pending_proof`.
struct ReleaseEraStore {
    dir: tempfile::TempDir,
    events_db: std::path::PathBuf,
    graph_db: std::path::PathBuf,
}

impl ReleaseEraStore {
    /// The store with its durable identity already minted, so `rigger setup` keeps its namespace.
    fn new() -> Self {
        Self::with_identity(true)
    }

    /// The store, with its durable identity `minted` already or still named after its directory
    /// (which `rigger setup` then mints, moving the log to it).
    fn with_identity(minted: bool) -> Self {
        let dir = temp_store_project();
        let root = dir.path();
        if minted {
            std::fs::write(root.join(".rigger").join("project.id"), "proj-era\n").unwrap();
        }
        let project = run_stream_identity(root);
        let mut log = two_generations_dropping_facts();
        log.push(keyed(
            TYPE_EDGE_INFERRED,
            proof("alpha", 3),
            "gc/tests/t.rs@k1#0",
            30,
        ));
        let events_db = rigger_file(root, "events.db");
        let graph_db = rigger_file(root, "graph.db");
        let appended = {
            let backend = Store::open(events_db.to_str().unwrap()).unwrap();
            Namespaced::new(&backend, &project)
                .append(rigger::conductor::STREAM, ExpectedRevision::Any, &log)
                .unwrap();
            run_events(&backend, &project)
        };
        let _ = std::fs::remove_file(&graph_db);
        let conn = rusqlite::Connection::open(&graph_db).unwrap();
        conn.execute_batch(RELEASE_ERA_SCHEMA).unwrap();
        for e in &appended {
            conn.execute("INSERT INTO applied (position) VALUES (?1)", [e.position])
                .unwrap();
        }
        for (id, kind) in [
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "function"),
            ("src/f.rs::gone", "function"),
            ("docs/f.md", "artifact"),
            ("src/old.rs", "artifact"),
        ] {
            conn.execute(
                "INSERT INTO nodes (id, kind, attrs, project) VALUES (?1, ?2, NULL, ?3)",
                [id, kind, project.as_str()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
             VALUES ('docs/f.md', 'src/old.rs', ?1, 10000000000, NULL, 3, ?2, 'extracted')",
            [rigger::contextgraph::REL_SPECIFIES, project.as_str()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO pending_proof (project, name, evidence) VALUES (?1, 'gone', 'tests/t.rs:3')",
            [project.as_str()],
        )
        .unwrap();
        drop(conn);
        ReleaseEraStore {
            dir,
            events_db,
            graph_db,
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// The project's identity as the binary resolves it now.
    fn project(&self) -> String {
        run_stream_identity(self.root())
    }

    /// The project's run stream as the log now records it.
    fn log(&self) -> Vec<Event> {
        run_events(
            &Store::open(self.events_db.to_str().unwrap()).unwrap(),
            &self.project(),
        )
    }

    /// The `graph.db` file's bytes: equal before and after a command that wrote nothing to it.
    fn graph_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.graph_db).unwrap()
    }

    /// The live projection and fold state of the project's `graph.db`, beside those of a fresh
    /// fold of the whole log.
    fn graph_and_a_fresh_fold_of_the_log(&self) -> ((String, Vec<String>), (String, Vec<String>)) {
        use rigger::contextgraph::sqlite::Projector;
        let whole = Projector::open(self.graph_db.to_str().unwrap(), &self.project())
            .unwrap()
            .whole()
            .unwrap();
        let fresh = self.root().join("fresh.db");
        (
            (
                serde_json::to_string(&whole).unwrap(),
                fold_state(&self.graph_db),
            ),
            (
                fold_in_batches(&fresh, &self.project(), &[self.log()]),
                fold_state(&fresh),
            ),
        )
    }
}

/// Given an operator store whose `graph.db` the release before the generation rule folded, when
/// the operator runs `rigger setup`, then that `graph.db` is rebuilt cold from the log - setup says
/// it is rebuilding and reports how far along it is - the superseded facts are gone, the old table
/// is dropped, the file records the current rule, and it holds exactly the graph a fresh fold of
/// the whole log reaches. A second `rigger setup` owes and reports no rebuild.
#[test]
fn rigger_setup_rebuilds_a_release_era_graph_db_from_the_log_and_stamps_the_rule() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    let store = ReleaseEraStore::new();
    let (out, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    let rebuild: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("graph.db") || l.starts_with("rebuilt "))
        .collect();
    let head = store.log().last().unwrap().position;
    assert_eq!(
        rebuild,
        vec![
            "rebuilding graph.db from the event log: it was folded under an older fold rule, so \
             the log's live selection is refolded once"
                .to_string(),
            format!("rebuilt 3 events, through position {head} of {head} (100%)"),
            "rebuilt graph.db from the event log".to_string(),
        ],
        "setup says it is rebuilding and how far along it is, folding only the live selection - \
         h1's three superseded recordings are never folded; stdout: {out}"
    );
    assert!(
        !store
            .root()
            .join(".rigger")
            .join("graph.db.rebuild")
            .exists(),
        "the shadow is removed once copied in"
    );

    let pending_proof_tables: i64 = rusqlite::Connection::open(&store.graph_db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'pending_proof'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        (user_version(&store.graph_db), pending_proof_tables),
        (1, 0),
        "the rebuilt file records the current rule and no longer holds the old table"
    );
    let rebuilt = Projector::open(store.graph_db.to_str().unwrap(), &store.project()).unwrap();
    assert!(!rebuilt.rebuild_owed().unwrap(), "the rebuild was paid");
    let whole = rebuilt.whole().unwrap();
    drop(rebuilt);
    assert_eq!(
        (
            whole.nodes.iter().any(|n| n.id == "src/f.rs::gone"),
            whole.edges.iter().any(|e| e.to == "src/old.rs"),
        ),
        (false, false),
        "the superseded generation's definition and design link are gone"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuilt graph.db is the whole log's, fold state included"
    );

    let (out, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "a second setup must succeed; stderr: {err}");
    assert!(
        !out.contains("graph.db"),
        "a paid rebuild is not owed or reported again; stdout: {out}"
    );
}

/// Given a `graph.db` that owes its rebuild for both causes at once - the release before the
/// generation rule folded it, and it misses an event the log holds - when the operator runs `rigger setup`,
/// then setup names both causes in order, pays both with its one rebuild (the lost fold's mark is
/// gone and the file records the current rule), and a second `rigger setup` owes nothing.
#[test]
fn rigger_setup_names_both_causes_a_graph_db_owes_its_rebuild_for_and_pays_both() {
    let store = ReleaseEraStore::new();
    let mark = rigger_file(store.root(), "graph.db.owed");
    std::fs::write(&mark, b"").unwrap();
    let (out, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("rebuilding graph.db"))
            .collect::<Vec<_>>(),
        vec![
            "rebuilding graph.db from the event log: it was folded under an older fold rule, and \
             it misses an event the log holds, so the log's live selection is refolded once"
        ],
        "setup names the older rule first and the lost fold second; stdout: {out}"
    );
    assert_eq!(
        (mark.exists(), user_version(&store.graph_db)),
        (false, 1),
        "the one rebuild pays both: the mark is dropped and the file records the current rule"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(graph, fresh, "the rebuilt graph.db is the whole log's");

    let (out, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "a second setup must succeed; stderr: {err}");
    assert!(
        !out.contains("graph.db"),
        "a paid rebuild is not owed or reported again; stdout: {out}"
    );
}

/// Given a release-era store whose durable identity `rigger setup` has yet to mint, when the
/// operator runs `rigger setup`, then the log moves to the minted identity before the rebuild reads
/// it, and the rebuilt `graph.db` is the whole log's under that identity - never an empty graph
/// folded from a namespace the history has not reached yet.
#[test]
fn rigger_setup_rebuilds_from_the_log_under_the_identity_it_mints() {
    let store = ReleaseEraStore::with_identity(false);
    let legacy = store.project();
    let (out, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_ne!(store.project(), legacy, "setup minted the durable identity");
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert!(
        graph.0.contains("src/f.rs::alpha"),
        "the rebuild read the moved history; graph: {}",
        graph.0
    );
    assert_eq!(graph, fresh, "the rebuilt graph.db is the whole log's");
}

/// Given a project with no `graph.db`, `rigger setup` owes and reports no rebuild and creates none.
#[test]
fn rigger_setup_on_a_project_without_a_graph_db_creates_none() {
    let dir = common::cli::temp_project();
    let (out, err, ok) = run_rigger_envs(dir.path(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stderr: {err}");
    assert!(
        !rigger_file(dir.path(), "graph.db").exists() && !out.contains("graph.db"),
        "no graph.db is created or rebuilt; stdout: {out}"
    );
}

/// Given a `graph.db` still at the old fold rule, when an agent runs `rigger emit`, then the event
/// is appended to the log and the emit says it was not folded, naming `rigger setup`, and
/// `graph.db` is left byte for byte as it was; `rigger setup` then folds the event with the rest.
#[test]
fn an_emit_at_the_old_rule_appends_says_it_skipped_the_fold_and_leaves_graph_db_unchanged() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let (out, err, ok) = run_rigger(
        store.root(),
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-up","summary":"s","governs":["src/f.rs::alpha"],"supersedes":""}"#,
        ],
    );
    assert!(ok, "the emit must succeed; stdout: {out} stderr: {err}");
    let log = store.log();
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (7, "DecisionMade"),
        "the decision is appended to the log"
    );
    assert_eq!(
        out,
        format!(
            "emitted DecisionMade (position {}); not folded into the context graph: graph: {}\n",
            last.position,
            rigger::contextgraph::REBUILD_OWED
        ),
        "the emit says it skipped the fold and names the rebuild"
    );
    assert!(
        store.graph_bytes() == before,
        "an emit at the old rule writes nothing to graph.db"
    );

    let (_, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stderr: {err}");
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuild folds the emitted decision with the rest of the log"
    );
}

/// Given a `graph.db` still at the old fold rule, a command whose answer depends on the fold
/// (`rigger graph build`) refuses at once naming `rigger setup` and writes nothing, while a
/// read-only inspection (`rigger graph --around`) answers from the graph as it stands, says the
/// rebuild is owed, and writes nothing either.
#[test]
fn a_fold_dependent_command_refuses_at_the_old_rule_and_an_inspection_answers_as_it_stands() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let (out, err, ok) = run_rigger(store.root(), &["graph", "build"]);
    assert!(!ok, "graph build must refuse; stdout: {out}");
    assert!(
        err.contains(&format!(
            "graph build: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the refusal names `rigger setup`; stderr: {err}"
    );
    assert!(store.graph_bytes() == before, "the refusal writes nothing");

    let (out, err, ok) = run_rigger(store.root(), &["graph", "--around", "docs/f.md"]);
    assert!(ok, "an inspection still answers; stderr: {err}");
    assert!(
        err.contains(&rebuild_owed_note()),
        "the inspection says the rebuild is owed; stderr: {err}"
    );
    assert!(
        out.contains("src/old.rs"),
        "it answers from the graph as it stands; stdout: {out}"
    );
    assert!(
        store.graph_bytes() == before,
        "the inspection writes nothing"
    );
}

/// While `rigger setup`'s rebuild folds its shadow, a read-only open answers from the old file
/// without waiting and changes nothing, a command that depends on the fold refuses at once naming
/// `rigger setup` rather than failing on a lock, an emit appends without folding, and a second
/// rebuild is refused rather than folding into the same shadow; when the
/// rebuild has swapped the shadow in, its ledgers are intact and it has folded the emitted event
/// too - the graph is exactly the whole log's.
#[test]
fn opens_racing_the_rebuild_neither_wait_nor_undo_it() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};
    let store = ReleaseEraStore::new();
    let (paused_tx, paused) = channel();
    let (resume, resume_rx) = channel::<()>();
    let rebuilder = {
        let (events_db, graph_db) = (store.events_db.clone(), store.graph_db.clone());
        let project = store.project();
        std::thread::spawn(move || {
            let backend = Store::open(events_db.to_str().unwrap()).unwrap();
            let mut batches = 0;
            let mut source = live_selection_of(&backend, &project, 1);
            let ran = Projector::rebuild(
                &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
                &project,
                false,
                &mut source,
                &mut |_| {
                    batches += 1;
                    if batches == 1 {
                        paused_tx.send(()).unwrap();
                        resume_rx.recv().unwrap();
                    }
                },
            )
            .unwrap();
            ran
        })
    };
    paused.recv().unwrap();

    let started = Instant::now();
    let reader = Projector::open(store.graph_db.to_str().unwrap(), &store.project()).unwrap();
    let (owed, whole) = (reader.rebuild_owed().unwrap(), reader.whole().unwrap());
    drop(reader);
    assert_eq!(
        (owed, whole.edges.iter().any(|e| e.to == "src/old.rs")),
        (true, true),
        "a read during the rebuild sees the old file as it stands"
    );
    let (_, err, ok) = run_rigger(store.root(), &["graph", "build"]);
    assert!(
        !ok && err.contains(&format!(
            "graph build: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "a folding command during the rebuild refuses naming `rigger setup`; stderr: {err}"
    );
    let (out, err, ok) = run_rigger(
        store.root(),
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-race","summary":"s","governs":["src/f.rs::alpha"],"supersedes":""}"#,
        ],
    );
    assert!(
        ok && out.contains("not folded into the context graph"),
        "an emit during the rebuild appends without folding; stdout: {out} stderr: {err}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "none of them waited on the rebuild's lock (the busy timeout is 5s)"
    );
    let second = Projector::lock_rebuild(store.graph_db.to_str().unwrap())
        .map(drop)
        .map_err(|e| e.0);
    assert_eq!(
        second,
        Err(rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS.to_string()),
        "a second rebuild racing this one is refused at its lock, never interleaved in its shadow"
    );

    resume.send(()).unwrap();
    assert_eq!(
        rebuilder.join().unwrap(),
        Some(Rebuilt::default()),
        "the rebuild ran"
    );
    assert_eq!(
        user_version(&store.graph_db),
        1,
        "and recorded the current rule"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuilt ledgers are intact and the raced emit is folded"
    );
}

/// The one note every read-only surface prints while `graph.db` owes its rebuild.
fn rebuild_owed_note() -> String {
    format!(
        "note: {} - until then the context graph answers as it stands",
        rigger::contextgraph::REBUILD_OWED
    )
}

/// Given a `graph.db` still at the old fold rule, every read-only surface - `rigger graph --show`,
/// `rigger validate` and `rigger dash --export` - says once that the rebuild is owed, naming
/// `rigger setup`, and writes nothing to the file; once `rigger setup` has paid it, none of them
/// says it again.
#[test]
fn the_read_only_surfaces_say_the_rebuild_is_owed_write_nothing_and_stop_once_setup_pays_it() {
    let store = ReleaseEraStore::new();
    let (_, err, ok) = run_rigger(store.root(), &["init"]);
    assert!(
        ok,
        "init scaffolds the config validate reads; stderr: {err}"
    );
    let surfaces: [&[&str]; 3] = [
        &["graph", "--show", "src/f.rs::gone"],
        &["validate"],
        &["dash", "--export", "snapshot.html"],
    ];
    let note = rebuild_owed_note();
    let before = store.graph_bytes();
    for args in surfaces {
        let (out, err, ok) = run_rigger(store.root(), args);
        assert!(
            ok,
            "rigger {} answers as it stands; stdout: {out} stderr: {err}",
            args.join(" ")
        );
        assert_eq!(
            err.matches(note.as_str()).count(),
            1,
            "rigger {} says once that the rebuild is owed; stdout: {out} stderr: {err}",
            args.join(" ")
        );
        assert!(
            store.graph_bytes() == before,
            "rigger {} writes nothing to a graph.db owing its rebuild",
            args.join(" ")
        );
    }
    let (out, err, ok) = run_rigger(store.root(), surfaces[0]);
    assert!(
        ok && out.contains("src/f.rs::gone"),
        "graph --show answers from the graph as it stands; stdout: {out} stderr: {err}"
    );

    let (_, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stderr: {err}");
    for args in surfaces {
        let (_, err, _) = run_rigger(store.root(), args);
        assert!(
            !err.contains(rigger::contextgraph::REBUILD_OWED),
            "rigger {} says nothing once the rebuild is paid; stderr: {err}",
            args.join(" ")
        );
    }
}

/// One `tools/call` of `name` with `arguments` over `mcp`.
fn tool_call(
    mcp: &mut common::mcp::McpSession,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    mcp.call(
        "tools/call",
        serde_json::json!({"name": name, "arguments": arguments}),
    )
}

/// Given a `graph.db` still at the old fold rule, an agent's `rigger mcp` session starts and
/// serves it as it stands: `rigger_graph` (both selectors) and `rigger_ground` refuse at once
/// naming `rigger setup`, while `rigger_emit` appends and writes nothing to `graph.db`. When
/// `rigger setup` pays the rebuild while that session runs, its next `rigger_graph` answers from
/// the rebuilt graph - the superseded link gone - and its next emit folds live, so the graph stays
/// exactly the whole log's.
#[test]
fn an_mcp_session_refuses_the_fold_dependent_tools_until_setup_pays_the_rebuild_it_serves() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let mut mcp =
        common::mcp::McpSession::start_with(store.root(), &["mcp", "--spawn", "u/implementer#0"]);
    for (tool, arguments) in [
        ("rigger_graph", serde_json::json!({"around": "docs/f.md"})),
        ("rigger_graph", serde_json::json!({"show": "alpha"})),
        ("rigger_ground", serde_json::json!({"query": "alpha"})),
    ] {
        let answer = tool_call(&mut mcp, tool, arguments.clone());
        assert_eq!(
            answer["error"]["message"],
            format!("{tool}: {}", rigger::contextgraph::REBUILD_OWED),
            "{tool} {arguments} refuses naming the rebuild; got: {answer}"
        );
    }
    let decision = |id: &str| {
        serde_json::json!({
            "type": "DecisionMade",
            "data": {"id": id, "summary": "s", "governs": ["src/f.rs::alpha"], "supersedes": ""},
        })
    };
    let emitted = tool_call(&mut mcp, "rigger_emit", decision("d-owed"));
    let log = store.log();
    assert_eq!(
        (log.len(), log.last().unwrap().type_.as_str()),
        (7, "DecisionMade"),
        "the emit appended to the log"
    );
    assert_eq!(
        emitted["result"]["structuredContent"],
        serde_json::json!({
            "position": log.last().unwrap().position,
            "folded": false,
            "reason": format!("graph: {}", rigger::contextgraph::REBUILD_OWED),
        }),
        "the emit succeeds and says it was not folded, naming the rebuild; got: {emitted}"
    );
    assert!(
        store.graph_bytes() == before,
        "neither the session nor its emit wrote to a graph.db owing its rebuild"
    );

    let (_, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stderr: {err}");
    let around = tool_call(
        &mut mcp,
        "rigger_graph",
        serde_json::json!({"around": "docs/f.md"}),
    );
    let answer = around["result"].to_string();
    assert!(
        around.get("error").is_none()
            && answer.contains("src/a.rs")
            && !answer.contains("src/old.rs"),
        "the same session answers from the rebuilt graph; got: {around}"
    );
    let emitted = tool_call(&mut mcp, "rigger_emit", decision("d-paid"));
    assert_eq!(
        emitted["result"]["structuredContent"],
        serde_json::json!({"position": store.log().last().unwrap().position, "folded": true}),
        "the emit succeeds and says it folded; got: {emitted}"
    );
    let finished = mcp.finish();
    assert!(
        finished.status.success(),
        "the session exits cleanly on EOF; stderr: {}",
        String::from_utf8_lossy(&finished.stderr)
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuild folded the first emit and the session folded the second live"
    );
}

// ---------------------------------------------------------------------------------------
// 7. The shadow rebuild's seams: its sources, its tail, and an emit that meets it
// ---------------------------------------------------------------------------------------

/// The positions of each batch `stream_past` hands for `log` past `after`, in batches of `batch`,
/// with the head handed beside each.
fn streamed_past(log: &[Event], after: u64, batch: usize) -> Vec<(Vec<u64>, u64)> {
    let mut handed = Vec::new();
    rigger::contextgraph::sqlite::stream_past(log, after, batch, &mut |events, head| {
        handed.push((events.iter().map(|e| e.position).collect(), head));
        Ok(())
    })
    .unwrap();
    handed
}

/// `stream_past` hands exactly the events past `after` - an event AT `after` is already folded -
/// in batches of at most `batch`, each with the log's last position, and hands nothing (never an
/// empty batch) when nothing lies past `after` or the log is empty.
#[test]
fn stream_past_hands_only_what_lies_past_the_position_in_batches_with_the_head() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let p: Vec<u64> = log.iter().map(|e| e.position).collect();
    assert_eq!(
        streamed_past(&log, 0, 2),
        vec![
            (vec![p[0], p[1]], p[4]),
            (vec![p[2], p[3]], p[4]),
            (vec![p[4]], p[4])
        ],
        "the whole log, in batches of two, each with the head"
    );
    assert_eq!(
        streamed_past(&log, p[1], 3),
        vec![(vec![p[2], p[3], p[4]], p[4])],
        "an event at the position is passed over; a batch that fits exactly is handed once"
    );
    assert_eq!(
        streamed_past(&log, p[4], 2),
        Vec::<(Vec<u64>, u64)>::new(),
        "nothing past the head hands nothing"
    );
    assert_eq!(
        streamed_past(&[], 7, 2),
        Vec::<(Vec<u64>, u64)>::new(),
        "an empty log hands nothing"
    );
    let failed = rigger::contextgraph::sqlite::stream_past(&log, 0, 2, &mut |_, _| {
        Err(rigger::contextgraph::Error("sink refused".to_string()))
    });
    assert_eq!(
        failed.unwrap_err().0,
        "sink refused",
        "a sink's failure stops the stream and is the stream's"
    );
}

/// The batches `backend`'s live selection of `project` hands past `after` in batches of `batch`,
/// as positions, with the head handed beside each.
fn selected_past(backend: &Store, project: &str, after: u64, batch: usize) -> Vec<(Vec<u64>, u64)> {
    let mut handed = Vec::new();
    backend
        .read_live_selection(
            &Namespaced::prefix_for(project),
            rigger::conductor::STREAM,
            &rigger::ingest::derived_index_identity(),
            after,
            batch,
            &mut |events, head| {
                handed.push((events.iter().map(|e| e.position).collect(), head));
                Ok(())
            },
        )
        .unwrap();
    handed
}

/// The live selection is one project's run stream alone: another project's events interleaved in
/// the same file are never handed, the head is this stream's last position rather than the file's,
/// a selection that fills its last batch exactly is never followed by an empty one, and a project
/// with no run stream hands nothing.
#[test]
fn the_live_selection_is_one_projects_stream_with_that_streams_head() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(dir.path(), &[]);
    let other = "proj-other";
    let generation = |key: &str, line| {
        vec![keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            head("alpha", line, false),
            key,
            10,
        )]
    };
    for (project, key, line) in [
        (PROJECT, "gc/src/f.rs@h1#0", 1),
        (other, "gc/src/f.rs@h1#0", 1),
        (PROJECT, "gc/src/f.rs@h2#0", 2),
        (other, "gc/src/f.rs@h2#0", 2),
        (PROJECT, "gc/src/g.rs@h1#0", 3),
        (other, "gc/src/g.rs@h1#0", 3),
    ] {
        Namespaced::new(&backend, project)
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &generation(key, line),
            )
            .unwrap();
    }
    let mine = run_events(&backend, PROJECT);
    let theirs = run_events(&backend, other);
    let (m, t) = (
        mine.iter().map(|e| e.position).collect::<Vec<_>>(),
        theirs.iter().map(|e| e.position).collect::<Vec<_>>(),
    );
    assert!(
        t[2] > m[2],
        "the other project's last event lies past this one's"
    );
    assert_eq!(
        selected_past(&backend, PROJECT, 0, 2),
        vec![(vec![m[1], m[2]], m[2])],
        "h1 is shed, the two latest generations fill one batch exactly, and the head is this \
         stream's"
    );
    assert_eq!(
        selected_past(&backend, other, 0, 1),
        vec![(vec![t[1]], t[2]), (vec![t[2]], t[2])],
        "the other project's selection is its own"
    );
    assert_eq!(
        selected_past(&backend, "proj-absent", 0, 2),
        Vec::<(Vec<u64>, u64)>::new(),
        "a project with no run stream hands nothing"
    );
    assert_eq!(
        (
            live_positions(&backend, PROJECT, 2),
            live_positions(&backend, other, 1),
            live_positions(&backend, "proj-absent", 2)
        ),
        (vec![vec![m[1], m[2]]], vec![vec![t[1]], vec![t[2]]], vec![]),
        "the positions alone are the same selection, batch for batch"
    );
}

/// The batches of positions `backend`'s live selection of `project` hands in batches of `batch`.
fn live_positions(backend: &Store, project: &str, batch: usize) -> Vec<Vec<u64>> {
    let mut handed = Vec::new();
    backend
        .read_live_positions(
            &Namespaced::prefix_for(project),
            rigger::conductor::STREAM,
            &rigger::ingest::derived_index_identity(),
            batch,
            &mut |positions| {
                handed.push(positions.to_vec());
                Ok(())
            },
        )
        .unwrap();
    handed
}

/// The live selection is refused, before a single event is handed, under a policy that has not
/// declared which of its types re-assert a fact: a selection that cannot say which valid-time a
/// kept row carries would silently re-date facts in the graph it rebuilds.
#[test]
fn the_live_selection_refuses_a_policy_without_a_reasserting_partition_and_hands_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let shipped = rigger::ingest::derived_index_identity();
    let undeclared = ContentIdentity::new(shipped.meta_key().to_string(), shipped.types().to_vec());
    let mut handed = 0;
    let refused = backend.read_live_selection(
        &Namespaced::prefix_for(PROJECT),
        rigger::conductor::STREAM,
        &undeclared,
        0,
        2,
        &mut |events, _| {
            handed += events.len();
            Ok(())
        },
    );
    let message = refused.unwrap_err().to_string();
    assert!(
        message.contains(&format!(
            "the content-identity policy for {:?} has not declared which of its types re-assert \
             a fact in place",
            shipped.types()
        )),
        "the refusal names the undeclared partition; got: {message}"
    );
    assert_eq!(handed, 0, "nothing is handed before the refusal");
}

/// A rebuild interrupted part way, then resumed after the log gained a NEW generation of a file
/// whose superseded generation the shadow had already folded, reaches exactly the whole log's
/// graph: the resumed selection is the log's live selection NOW, and the generation fold retires
/// what the newer generation drops, as it does live.
fn a_resumed_rebuild_after_the_log_gained_a_generation_is_the_whole_logs(gained: Vec<Event>) {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (db, fresh) = (dir.path().join("graph.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&db, &log[..1], "");
    let mut first = live_selection_of(&backend, PROJECT, 1);
    let mut batches = 0;
    let interrupted = rebuild(&db, &mut |after, sink| {
        first(after, &mut |events, head| {
            batches += 1;
            if batches == 2 {
                return Err(rigger::contextgraph::Error("interrupted".to_string()));
            }
            sink(events, head)
        })
    });
    assert!(interrupted.is_err(), "the rebuild is interrupted");
    assert!(
        dir.path().join("graph.db.rebuild").exists(),
        "its first batch - h2's `alpha` - is committed in the shadow"
    );

    Namespaced::new(&backend, PROJECT)
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &gained)
        .unwrap();
    assert_eq!(
        rebuild(&db, &mut live_selection_of(&backend, PROJECT, 2)).unwrap(),
        Some(Rebuilt::default())
    );
    assert!(
        !dir.path().join("graph.db.rebuild").exists(),
        "the shadow is removed once copied in"
    );
    fold_in_batches(
        &fresh,
        PROJECT,
        std::slice::from_ref(&run_events(&backend, PROJECT)),
    );
    assert_eq!(
        identity_of(&db),
        identity_of(&fresh),
        "the resumed rebuild is the whole log's graph"
    );
}

rigger::test_cases! {
    /// `src/f.rs` moves on to a third content that adds `delta`.
    a_resumed_rebuild_folds_a_generation_the_log_gained_as_the_whole_log_does:
        a_resumed_rebuild_after_the_log_gained_a_generation_is_the_whole_logs(vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 3, false), "gc/src/f.rs@h3#0", 30),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("delta", 5), "gc/src/f.rs@h3#1", 30),
        ]);
    /// `src/f.rs` returns to h1, bringing `gone` back.
    a_resumed_rebuild_folds_a_revert_the_log_gained_as_the_whole_log_does:
        a_resumed_rebuild_after_the_log_gained_a_generation_is_the_whole_logs(vec![
            keyed(TYPE_CODE_ENTITY_EXTRACTED, head("alpha", 1, false), "gc/src/f.rs@h1#0", 30),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("gone", 9), "gc/src/f.rs@h1#1", 30),
        ]);
}

/// Whether `db` holds a table named `table`: `rebuild_cursor` while a rebuild's tail is not yet
/// folded; `lost_fold`, a record of a lost fold beside the ledger, never.
fn holds_table(db: &Path, table: &str) -> bool {
    exists(
        db,
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        table,
    )
}

/// Whether `db`'s applied ledger records `position`.
fn applied(db: &Path, position: u64) -> bool {
    exists(
        db,
        "SELECT EXISTS (SELECT 1 FROM applied WHERE position = ?1)",
        position,
    )
}

/// What the `SELECT EXISTS` query `sql` answers over `db` with `param` bound to `?1`.
fn exists(db: &Path, sql: &str, param: impl rusqlite::ToSql) -> bool {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row(sql, [param], |r| r.get(0))
        .unwrap()
}

/// Given `rigger setup`'s rebuild has swapped its shadow in and is about to fold the tail the log
/// gained, when an agent runs `rigger emit`, then the emit neither waits on the rebuild nor skips
/// the fold: the file is current, so its event is folded at once; the tail then meets it exactly
/// once, drops its cursor, and the graph is exactly the whole log's.
#[test]
fn an_emit_while_the_rebuild_folds_its_tail_is_folded_at_once_and_met_exactly_once() {
    use rigger::contextgraph::sqlite::Projector;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};
    let store = ReleaseEraStore::new();
    let head_before = store.log().last().unwrap().position;
    let (paused_tx, paused) = channel();
    let (resume, resume_rx) = channel::<()>();
    let rebuilder = {
        let (events_db, graph_db) = (store.events_db.clone(), store.graph_db.clone());
        let project = store.project();
        std::thread::spawn(move || {
            let backend = Store::open(events_db.to_str().unwrap()).unwrap();
            let mut source = live_selection_of(&backend, &project, 1);
            let mut reads = 0;
            Projector::rebuild(
                &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
                &project,
                false,
                &mut |after, sink| {
                    reads += 1;
                    if reads == 2 {
                        paused_tx.send(after).unwrap();
                        resume_rx.recv().unwrap();
                    }
                    source(after, sink)
                },
                &mut |_| {},
            )
            .unwrap()
        })
    };
    let tail_after = paused.recv().unwrap();
    assert_eq!(
        tail_after, head_before,
        "the tail reads past the last position the shadow folded"
    );
    assert_eq!(
        (
            user_version(&store.graph_db),
            holds_table(&store.graph_db, "rebuild_cursor")
        ),
        (1, true),
        "the rebuilt file is swapped in and still owes its tail"
    );

    let started = Instant::now();
    let (out, err, ok) = run_rigger(
        store.root(),
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-tail","summary":"s","governs":["src/f.rs::alpha"],"supersedes":""}"#,
        ],
    );
    let emitted = store.log().last().unwrap().position;
    assert_eq!(
        (ok, out.as_str()),
        (
            true,
            format!(
                "emitted DecisionMade (position {emitted}) and folded it into the context graph\n"
            )
            .as_str()
        ),
        "an emit during the tail folds its event; stderr: {err}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the emit did not wait on the rebuild (the busy timeout is 5s)"
    );
    assert!(
        applied(&store.graph_db, emitted),
        "the emitted event is folded before the tail runs"
    );

    resume.send(()).unwrap();
    assert_eq!(
        rebuilder.join().unwrap(),
        Some(Rebuilt::default()),
        "the rebuild ran"
    );
    assert!(
        !holds_table(&store.graph_db, "rebuild_cursor"),
        "the tail is folded and its cursor dropped"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the tail met the emitted event exactly once: the graph is the whole log's"
    );
}

/// Run `rigger emit DecisionMade` in `root` for a decision `id` governing `src/f.rs`, returning
/// its stdout, its stderr and whether it exited successfully.
fn emit_decision(root: &Path, id: &str) -> (String, String, bool) {
    let args = format!(r#"{{"id":"{id}","summary":"s","governs":["src/f.rs"],"supersedes":""}}"#);
    run_rigger(root, &["emit", "DecisionMade", &args])
}

/// Given a current `graph.db` another writer holds locked past the busy timeout, when an agent
/// runs `rigger emit`, then the event is on the log and the emit reports its position, but it does
/// not claim to have folded an event its fold failed to write: the graph is current, so no
/// rebuild will ever re-derive it, and a claimed fold would hide a permanent divergence.
#[test]
fn an_emit_whose_fold_fails_does_not_claim_it_folded() {
    let dir = temp_store_project();
    let root = dir.path();
    let (out, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok && out.ends_with("and folded it into the context graph\n"),
        "an emit into an unlocked graph folds; stdout: {out} stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let holder = rusqlite::Connection::open(&graph_db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    let (out, err, ok) = emit_decision(root, "d-locked");
    holder.execute_batch("ROLLBACK").unwrap();
    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (2, "DecisionMade"),
        "the event is on the log"
    );
    assert!(
        !applied(&graph_db, last.position),
        "the locked fold wrote nothing"
    );
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "emitted DecisionMade (position {}); not folded into the context graph: \
                 graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it\n",
                last.position
            )
        ),
        "the emit reports the position and the fold it could not make, with the reason; \
         stderr: {err}"
    );
}

/// Given a current `graph.db` another writer holds locked past the busy timeout, when an agent's
/// `rigger mcp` session calls `rigger_emit`, then the event is on the log and the tool answers its
/// position and that it was not folded, with the reason: the graph is current, so no rebuild will
/// ever re-derive the event, and an answer that looked like a fold would hide the divergence.
#[test]
fn an_mcp_emit_whose_fold_fails_answers_not_folded_with_the_reason() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let decision = |id: &str| {
        serde_json::json!({
            "type": "DecisionMade",
            "data": {"id": id, "summary": "s", "governs": ["src/f.rs"], "supersedes": ""},
        })
    };
    let mut mcp = common::mcp::McpSession::start_with(root, &["mcp", "--spawn", "u/implementer#0"]);
    let free = tool_call(&mut mcp, "rigger_emit", decision("d-free"));
    let free_at = read_run_events(root).last().unwrap().position;
    assert_eq!(
        free["result"]["structuredContent"],
        serde_json::json!({"position": free_at, "folded": true}),
        "an emit into an unlocked graph folds; got: {free}"
    );

    let graph_db = rigger_file(root, "graph.db");
    let holder = rusqlite::Connection::open(&graph_db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let locked = tool_call(&mut mcp, "rigger_emit", decision("d-locked"));
    holder.execute_batch("ROLLBACK").unwrap();
    let finished = mcp.finish();
    assert!(
        finished.status.success(),
        "the session exits cleanly on EOF; stderr: {}",
        String::from_utf8_lossy(&finished.stderr)
    );

    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (3, "DecisionMade"),
        "the locked emit's event is on the log"
    );
    assert_eq!(
        locked["result"]["structuredContent"],
        serde_json::json!({
            "position": last.position,
            "folded": false,
            "reason": "graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it",
        }),
        "the tool answers the position and the fold it could not make, with the reason; got: {locked}"
    );
    assert_eq!(
        (
            applied(&graph_db, free_at),
            applied(&graph_db, last.position)
        ),
        (true, false),
        "the free emit folded and the locked fold wrote nothing"
    );
}

/// Given a `graph.db` that cannot be opened as a graph (its bytes are not a database), when an
/// agent runs `rigger emit`, then the emit still succeeds with its event on the log and reports
/// the fold it could not make, with the reason, and leaves the unreadable file exactly as it was:
/// an event already durably appended is never reported as a failed emit.
#[test]
fn an_emit_into_a_graph_it_cannot_open_is_on_the_log_and_reported_not_folded() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let garbage = b"this file is not a sqlite database, only text standing in for one".repeat(64);
    std::fs::write(&graph_db, &garbage).unwrap();

    let (out, err, ok) = emit_decision(root, "d-unopenable");
    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (2, "DecisionMade"),
        "the event is on the log"
    );
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "emitted DecisionMade (position {}); not folded into the context graph: \
                 graph: file is not a database\n",
                last.position
            )
        ),
        "the emit succeeds and reports the fold it could not make, with the reason; stderr: {err}"
    );
    assert_eq!(
        std::fs::read(&graph_db).unwrap(),
        garbage,
        "the unopenable graph.db is left exactly as it was"
    );
}

/// Given history recorded before the project minted its durable identity, when a command that
/// opens the store (a bare `rigger reset`) runs the one-time identity migration, then the
/// migration's own `DecisionMade` goes through the one emit core into the graph it was wired with:
/// it is on the log under the minted identity AND folded into the current `graph.db`, where it
/// resolves - no rebuild will ever re-derive an event a current graph missed.
#[test]
fn the_identity_migrations_decision_is_folded_into_the_graph_it_migrates() {
    use rigger::contextgraph::Projection;

    let dir = temp_store_project();
    let root = dir.path();
    let (legacy, minted) = legacy_history_then_minted_identity(root);

    let (_, err, ok) = run_rigger(root, &["reset"]);
    let log = read_run_events(root);
    let migration = format!("identity-migration-{minted}");
    let last = log.last().unwrap();
    assert!(
        ok && err.contains(&migration_report(
            &legacy,
            &minted,
            last.position,
            " and folded it into the context graph"
        )),
        "the bare reset migrates the one legacy stream and reports its decision's fold; \
         stderr: {err}"
    );
    assert_eq!(
        (
            log.len(),
            last.type_.as_str(),
            serde_json::from_slice::<serde_json::Value>(&last.data).unwrap()["id"].clone()
        ),
        (2, "DecisionMade", serde_json::json!(migration)),
        "the migration's decision follows the legacy one on the minted run stream"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        (
            applied(&graph_db, log[0].position),
            applied(&graph_db, last.position)
        ),
        (true, true),
        "both the legacy emit and the migration's decision are folded"
    );
    assert_eq!(
        common::cli::open_graph(root).resolve(&migration).unwrap(),
        Some(migration.clone()),
        "the migration's decision resolves in the graph under the minted identity"
    );
}

/// Record a decision under the legacy basename namespace of the project at `root`, creating its
/// store and graph, then mint its durable identity with `rigger init`, leaving that history for
/// the one-time identity migration: the legacy and the minted identity.
fn legacy_history_then_minted_identity(root: &Path) -> (String, String) {
    let (_, err, ok) = emit_decision(root, "d-legacy");
    assert!(
        ok,
        "the legacy emit creates the store and graph; stderr: {err}"
    );
    let legacy = run_stream_identity(root);
    let (_, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init mints the identity; stderr: {err}");
    let minted = run_stream_identity(root);
    assert_ne!(minted, legacy, "init minted a distinct identity");
    (legacy, minted)
}

/// The stderr line a bare `rigger reset` prints when it migrates one legacy stream `legacy` ->
/// `minted` and records its decision at `position`, ending in what became of that decision's fold.
fn migration_report(legacy: &str, minted: &str, position: u64, fold: &str) -> String {
    format!(
        "rigger: migrated project identity - renamed 1 stream(s) from the legacy namespace \
         {legacy:?} to the minted identity {minted:?} (.rigger/project.id); recorded its \
         decision (position {position}){fold}\n"
    )
}

/// Given a `graph.db` still at the old fold rule, when a courier runs `rigger result`, then the
/// result is on the log and the courier exits successfully, but it says the result was not folded,
/// naming `rigger setup`, and `graph.db` is left byte for byte as it was: a command whose job is
/// to append skips the fold while the rebuild is owed and says so.
#[test]
fn a_result_at_the_old_rule_is_on_the_log_and_says_it_skipped_the_fold() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let (out, err, ok) = run_rigger(store.root(), &["result", "u/implementer#0", "did the work"]);
    let log = store.log();
    let last = log.last().unwrap();
    assert_eq!(
        (ok, log.len(), last.type_.as_str()),
        (true, 7, "SpawnResult"),
        "the result is appended to the log and the courier succeeds; stderr: {err}"
    );
    assert_eq!(
        out,
        format!(
            "recorded result for u/implementer#0 (position {})\n\
             not folded into the context graph: graph: {}\n",
            last.position,
            rigger::contextgraph::REBUILD_OWED
        ),
        "the result says it skipped the fold and names the rebuild"
    );
    assert!(
        store.graph_bytes() == before,
        "a result at the old rule writes nothing to graph.db"
    );
}

/// Given a current `graph.db` another writer holds locked past the busy timeout, when a courier
/// runs `rigger result`, then the result is on the log and the courier exits successfully, but it
/// says the result was not folded, with the reason: the graph is current, so no rebuild will ever
/// re-derive it, and a silent miss would keep an adjudicator's discarded findings in grounding.
#[test]
fn a_result_whose_fold_fails_says_it_was_not_folded() {
    let dir = temp_store_project();
    let root = dir.path();
    let (out, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok && out.ends_with("and folded it into the context graph\n"),
        "an emit into an unlocked graph folds; stdout: {out} stderr: {err}"
    );
    let (out, err, ok) = run_rigger(root, &["result", "u/implementer#0", "first"]);
    let free = read_run_events(root).last().unwrap().position;
    assert_eq!(
        (ok, out),
        (
            true,
            format!("recorded result for u/implementer#0 (position {free})\n")
        ),
        "a result into an unlocked graph folds silently; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let holder = rusqlite::Connection::open(&graph_db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    let (out, err, ok) = run_rigger(root, &["result", "v/implementer#0", "second"]);
    holder.execute_batch("ROLLBACK").unwrap();
    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (3, "SpawnResult"),
        "the locked result is on the log"
    );
    assert_eq!(
        (applied(&graph_db, free), applied(&graph_db, last.position)),
        (true, false),
        "the free result folded and the locked fold wrote nothing"
    );
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "recorded result for v/implementer#0 (position {})\n\
                 not folded into the context graph: graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it\n",
                last.position
            )
        ),
        "the result reports the fold it could not make, with the reason; stderr: {err}"
    );
}

/// Given a store still named after its directory whose `graph.db` is at the old fold rule, when
/// the operator mints the identity and runs a bare `rigger reset`, then the migration renames the
/// legacy stream and records its decision, but says that decision was not folded, naming the
/// rebuild it owes, and `graph.db` does not hold it until `rigger setup` rebuilds from the log.
#[test]
fn an_identity_migration_into_a_graph_that_owes_its_rebuild_says_its_decision_was_not_folded() {
    let store = ReleaseEraStore::with_identity(false);
    let legacy = store.project();
    let (_, err, ok) = run_rigger(store.root(), &["init"]);
    assert!(ok, "rigger init mints the identity; stderr: {err}");
    let minted = store.project();
    assert_ne!(minted, legacy, "init minted a distinct identity");

    let (out, err, ok) = run_rigger(store.root(), &["reset"]);
    let log = store.log();
    let last = log.last().unwrap();
    assert_eq!(
        out.lines().next(),
        Some(format!("--runs: {}", rigger::contextgraph::REBUILD_OWED).as_str()),
        "the menu counts nothing prunable from a graph that owes its rebuild, and says so; \
         stderr: {err}"
    );
    assert_eq!(
        (
            ok,
            log.len(),
            last.type_.as_str(),
            serde_json::from_slice::<serde_json::Value>(&last.data).unwrap()["id"].clone()
        ),
        (
            true,
            7,
            "DecisionMade",
            serde_json::json!(format!("identity-migration-{minted}"))
        ),
        "the migration's decision follows the six moved events on the minted run stream; \
         stderr: {err}"
    );
    assert!(
        err.contains(&migration_report(
            &legacy,
            &minted,
            last.position,
            &format!(
                "; not folded into the context graph: graph: {}",
                rigger::contextgraph::REBUILD_OWED
            )
        )),
        "the migration reports that its decision skipped the owed graph, with the reason; \
         stderr: {err}"
    );
    assert!(
        !applied(&store.graph_db, last.position),
        "the owed graph.db does not hold the migration's decision"
    );

    let (out, err, ok) = run_rigger_envs(store.root(), &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert!(
        applied(&store.graph_db, last.position),
        "setup's rebuild folds the migration's decision from the log"
    );
}

/// Given a current `graph.db` whose fold of an emit was lost to another writer's lock, when the
/// agent emits again, then the graph owes its rebuild: the next emit appends and says it was not
/// folded because the rebuild is owed, a read-only inspection says the rebuild is owed, and once
/// `rigger setup` has paid it every event of the log is folded - the lost one included - and an
/// emit folds again.
#[test]
fn a_fold_lost_to_a_lock_marks_the_graph_owed_until_setup_rebuilds_it() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let (out, err, _) = with_graph_locked(&graph_db, || emit_decision(root, "d-locked"));
    assert!(
        out.ends_with("; not folded into the context graph: graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it\n"),
        "the locked emit's fold is lost; stdout: {out} stderr: {err}"
    );
    let lost = read_run_events(root).last().unwrap().position;

    let (out, err, ok) = emit_decision(root, "d-after");
    let after = read_run_events(root).last().unwrap().position;
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "emitted DecisionMade (position {after}); not folded into the context graph: \
                 graph: {}\n",
                rigger::contextgraph::REBUILD_OWED
            )
        ),
        "the graph now owes its rebuild, so the next emit says so; stderr: {err}"
    );
    let (_, err, ok) = run_rigger(root, &["graph", "--around", "src/f.rs"]);
    assert!(
        ok && err.contains(&rebuild_owed_note()),
        "a read-only inspection says the rebuild is owed; stderr: {err}"
    );
    let owed_log = read_run_events(root).len();
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (ok, read_run_events(root).len()),
        (false, owed_log),
        "the compaction refuses a graph a lost fold left owed and prunes nothing; stdout: {out}"
    );
    assert!(
        err.contains(&rigger::contextgraph::rebuild_owed_refusal(
            "reset --derived"
        )),
        "the compaction's refusal names `rigger setup`; stderr: {err}"
    );

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("rebuilding graph.db"))
            .collect::<Vec<_>>(),
        vec![LOST_FOLD_REBUILD_LINE],
        "setup names the cause the graph owes its rebuild for - the lost fold, not an older \
         rule; stdout: {out}"
    );
    assert_eq!(
        (applied(&graph_db, lost), applied(&graph_db, after)),
        (true, true),
        "setup's rebuild folds the lost event and the one emitted while owed"
    );
    let (out, err, ok) = emit_decision(root, "d-rebuilt");
    assert!(
        ok && out.ends_with(" and folded it into the context graph\n"),
        "once rebuilt, an emit folds again; stdout: {out} stderr: {err}"
    );
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok && out.starts_with("reset --derived: pruned 0 redundant derived-index event(s)"),
        "once setup has paid the rebuild the compaction runs; stdout: {out} stderr: {err}"
    );
}

/// Given a `graph.db` a lost fold left owed, when the operator removes the graph file and runs
/// `rigger setup`, then setup drops the mark the removed file left behind, so the next emit makes a
/// fresh graph and folds into it rather than inheriting a debt that file no longer carries.
#[test]
fn an_owed_mark_whose_graph_is_removed_is_dropped_by_setup_and_the_next_emit_folds() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let mark = rigger_file(root, "graph.db.owed");
    with_graph_locked(&graph_db, || emit_decision(root, "d-locked"));
    assert!(mark.exists(), "the lost fold marks the graph owed");
    for file in ["graph.db", "graph.db-wal", "graph.db-shm"] {
        let path = rigger_file(root, file);
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert!(
        !mark.exists(),
        "setup drops the mark of the removed graph file"
    );
    let (out, err, ok) = emit_decision(root, "d-fresh");
    assert!(
        ok && out.ends_with(" and folded it into the context graph\n"),
        "the next emit folds into a fresh graph; stdout: {out} stderr: {err}"
    );
}

/// Append a `DecisionMade` whose payload is `data` to `root`'s run stream straight through the
/// store, behind every fold - the shape of an event whose process died before its fold, or one
/// that never met the emit surface's shape check - and answer its log position.
fn append_unfolded_decision(root: &Path, data: &[u8]) -> u64 {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    Namespaced::new(&backend, &run_stream_identity(root))
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new("DecisionMade", data.to_vec())],
        )
        .unwrap()
        .one("the unfolded decision")
        .unwrap()
}

/// What `rigger setup` prints before it rebuilds a `graph.db` that owes its rebuild only because
/// it misses an event the log holds.
const LOST_FOLD_REBUILD_LINE: &str = "rebuilding graph.db from the event log: it misses an event \
     the log holds, so the log's live selection is refolded once";

/// Given `root`'s `graph.db` misses the event at log position `lost`, when the operator runs
/// `rigger setup` in a fresh process, then setup names the missing event as the cause, its
/// rebuild folds it, the next emit folds, and a second `rigger setup` owes nothing - the rebuilt
/// ledger holds every position of the log's live selection.
fn the_next_setup_pays_the_missing_event(root: &Path, lost: u64) {
    let graph_db = rigger_file(root, "graph.db");
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(
        (
            out.lines()
                .filter(|l| l.starts_with("rebuilding graph.db"))
                .collect::<Vec<_>>(),
            applied(&graph_db, lost)
        ),
        (vec![LOST_FOLD_REBUILD_LINE], true),
        "setup finds the event missing from the ledger, says so, and its rebuild folds it; \
         stdout: {out}"
    );
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("passed over "))
            .collect::<Vec<_>>(),
        Vec::<&str>::new(),
        "a rebuild whose every payload folds passes over nothing; stdout: {out}"
    );
    let (out, err, ok) = emit_decision(root, "d-rebuilt");
    assert!(
        ok && out.ends_with(" and folded it into the context graph\n"),
        "once rebuilt, an emit folds again; stdout: {out} stderr: {err}"
    );
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(
        ok && !out.contains("graph.db"),
        "a paid rebuild is not owed or reported again; stdout: {out} stderr: {err}"
    );
}

/// Given a current `graph.db`, when an agent's `rigger emit` loses its fold to another writer's
/// lock while the directory the owed mark lives in is not writable, then the emit appends and
/// says it did not fold, naming the mark it could not write, and records nothing else - no mark,
/// no table in the file - yet the loss is not forgotten with the process: the next `rigger setup`
/// finds the event missing from the graph's ledger and pays it.
#[test]
fn a_fold_lost_where_its_mark_cannot_be_written_is_paid_by_the_next_setup_from_the_ledger() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let mark = rigger_file(root, "graph.db.owed");
    let rigger_dir = graph_db.parent().unwrap().to_path_buf();
    // Both files' write-ahead files stay in place while these are open, so the append and the
    // lock need no new file in the directory the mark cannot be written to.
    let keepers = [rigger_file(root, "events.db"), graph_db.clone()].map(|db| {
        let keeper = rusqlite::Connection::open(db).unwrap();
        keeper
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        keeper
    });
    std::fs::set_permissions(&rigger_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (out, err, ok) = with_graph_locked(&graph_db, || emit_decision(root, "d-lost"));
    std::fs::set_permissions(&rigger_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(keepers);
    let lost = read_run_events(root).last().unwrap().position;
    let prefix = format!(
        "emitted DecisionMade (position {lost}); not folded into the context graph: graph: \
         database is locked; the mark that graph.db owes its rebuild was not written ("
    );
    let suffix = "graph.db.owed: Permission denied (os error 13)) - the next `rigger setup` finds \
                  the event missing from graph.db and rebuilds it\n";
    assert_eq!(
        (
            ok,
            out.starts_with(&prefix) && out.ends_with(suffix),
            mark.exists(),
            holds_table(&graph_db, "lost_fold"),
            applied(&graph_db, lost)
        ),
        (true, true, false, false, false),
        "the emit appends, says it did not fold and names the unwritten mark, and records the \
         loss nowhere but in the ledger it misses; stdout: {out} stderr: {err}"
    );
    the_next_setup_pays_the_missing_event(root, lost);
}

/// Given an event on the log that no fold ever reached - its process died between the append and
/// the fold, so nothing ran to record the loss - when the operator runs `rigger setup`, then setup
/// finds it missing from the graph's ledger and pays it.
#[test]
fn an_event_appended_but_never_folded_is_paid_by_the_next_setup_from_the_ledger() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let lost = append_unfolded_decision(
        root,
        br#"{"id":"d-crashed","summary":"s","governs":["src/f.rs"],"supersedes":""}"#,
    );
    assert!(
        !applied(&rigger_file(root, "graph.db"), lost),
        "the event is on the log and not in the graph"
    );
    the_next_setup_pays_the_missing_event(root, lost);
}

/// Given a project whose event log is a KurrentDB server, and a `graph.db` on this machine that
/// misses an event another writer of that server appended - a second machine sharing the log -
/// when the operator runs `rigger setup`, then setup reads the server's positions against the
/// graph's ledger, names the missing event as the cause, rebuilds `graph.db` from the server's
/// whole run stream (a server-backed log has no compaction plan, so its live selection is the
/// stream as it stands), reports its progress and its prune, and leaves the graph a fold of that
/// stream. Before the other writer's append, the emit's own fold owes nothing; after the rebuild, a
/// second setup owes nothing. Gracefully skipped when no container runtime is reachable.
#[test]
fn setup_rebuilds_a_graph_db_missing_an_event_another_writer_appended_to_the_server_log() {
    common::fixtures::with_kurrentdb(|conn| {
        let dir = common::cli::temp_project();
        let root = dir.path();
        let server = [("KURRENTDB_CONN", conn), ("RIGGER_NPM", "true")];
        let setup = || run_rigger_envs(root, &["setup"], &server);
        let decision = |id: &str| {
            format!(r#"{{"id":"{id}","summary":"s","governs":["src/f.rs"],"supersedes":""}}"#)
        };
        let rebuild_lines = |out: &str| -> Vec<String> {
            out.lines()
                .filter(|l| {
                    ["rebuil", "pruned ", "passed over "]
                        .iter()
                        .any(|p| l.starts_with(p))
                })
                .map(str::to_string)
                .collect()
        };
        let (out, err, ok) = setup();
        assert_eq!(
            (ok, rebuild_lines(&out)),
            (true, Vec::<String>::new()),
            "setup mints the identity and, with no graph.db, rebuilds nothing; stdout: {out} \
             stderr: {err}"
        );
        let (out, err, ok) = run_rigger_envs(
            root,
            &["emit", "DecisionMade", &decision("d-first")],
            &server,
        );
        assert!(
            ok && out.ends_with(" and folded it into the context graph\n"),
            "the emit appends to the server and folds into the local graph.db; stdout: {out} \
             stderr: {err}"
        );
        let (out, err, ok) = setup();
        assert_eq!(
            (ok, out.contains("graph.db"), rebuild_lines(&out)),
            (true, false, Vec::<String>::new()),
            "the emit's fold recorded the position the server's positions read hands, so setup \
             owes nothing; stdout: {out} stderr: {err}"
        );

        let project = run_stream_identity(root);
        let backend = rigger::eventstore::kurrentdb::Store::open(conn).unwrap();
        let log = Namespaced::new(&backend, &project);
        let lost = log
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new("DecisionMade", decision("d-other").into_bytes())],
            )
            .unwrap()
            .one("the other writer's decision")
            .unwrap();
        let graph_db = rigger_file(root, "graph.db");
        assert!(
            !applied(&graph_db, lost),
            "the other writer's event is on the server and not in this machine's graph"
        );

        let (out, err, ok) = setup();
        let stream = log
            .read_stream(
                rigger::conductor::STREAM,
                0,
                rigger::eventstore::Direction::Forward,
            )
            .unwrap();
        let head = stream.last().unwrap().position;
        let scratch = tempfile::tempdir().unwrap();
        let folded = fold_in_batches(
            &scratch.path().join("graph.db"),
            &project,
            std::slice::from_ref(&stream),
        );
        assert_eq!(
            (
                ok,
                rebuild_lines(&out),
                stream.len(),
                applied(&graph_db, lost),
                whole_graph(root)
            ),
            (
                true,
                vec![
                    LOST_FOLD_REBUILD_LINE.to_string(),
                    format!("rebuilt 2 events, through position {head} of {head} (100%)"),
                    "rebuilt graph.db from the event log".to_string(),
                    "pruned 0 dead-run node(s) and reclaimed 0 superseded edge(s) from the \
                     rebuilt graph"
                        .to_string(),
                ],
                2,
                true,
                folded
            ),
            "setup finds the other writer's event missing from the ledger, rebuilds from the \
             server's whole run stream in one batch, prunes nothing before a run starts, and the \
             graph is the fold of that stream; stdout: {out} stderr: {err}"
        );
        let (out, err, ok) = setup();
        assert_eq!(
            (ok, out.contains("graph.db"), rebuild_lines(&out)),
            (true, false, Vec::<String>::new()),
            "the rebuilt ledger holds every position the server hands, so the rebuild is paid; \
             stdout: {out} stderr: {err}"
        );
    });
}

/// Given an event on the log whose payload the fold rejects - appended without the emit surface's
/// shape check - when the operator runs `rigger setup`, then setup finds it missing from the
/// graph's ledger, rebuilds, and says it passed that one event over, recording it as folded so the
/// next setup owes nothing for it.
#[test]
fn setup_says_how_many_events_its_rebuild_passed_over_for_a_payload_the_fold_rejects() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let rejected = [
        append_unfolded_decision(root, b"{ not valid json"),
        append_unfolded_decision(
            root,
            br#"{"id":"d-trailing","summary":"s","governs":["src/t.rs"],"supersedes":""} x"#,
        ),
    ];

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(
        (
            out.lines()
                .filter(|l| l.starts_with("rebuilding graph.db")
                    || l.starts_with("rebuilt graph.db")
                    || l.starts_with("passed over "))
                .collect::<Vec<_>>(),
            rejected.map(|at| applied(&rigger_file(root, "graph.db"), at)),
        ),
        (
            vec![
                LOST_FOLD_REBUILD_LINE,
                "rebuilt graph.db from the event log",
                "passed over 2 event(s) whose payload the fold rejects, recorded as folded",
            ],
            [true, true]
        ),
        "setup rebuilds, and says the two events it passed over - a malformed payload and one \
         with trailing bytes; stdout: {out}"
    );
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(
        ok && !out.contains("graph.db"),
        "the passed-over event is not owed again; stdout: {out} stderr: {err}"
    );
}

/// Given a `graph.db` that misses a well-formed event the log holds, when the operator runs
/// `rigger setup` and its rebuild meets a storage error folding that event - the store's failure,
/// not a payload the fold rejects - then setup fails naming the error, passes nothing over, and
/// leaves `graph.db` untouched with the event still missing from its ledger; once the storage
/// recovers, the next `rigger setup` resumes the rebuild and folds the event.
#[test]
fn setup_whose_rebuild_meets_a_storage_error_fails_and_the_next_setup_folds_the_event() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    let first = read_run_events(root).last().unwrap().position;
    let lost = append_unfolded_decision(
        root,
        br#"{"id":"d-boom","summary":"s","governs":["src/f.rs"],"supersedes":""}"#,
    );
    let graph_db = rigger_file(root, "graph.db");
    let shadow = rigger_file(root, "graph.db.rebuild");
    drop(
        rigger::contextgraph::sqlite::Projector::open(
            shadow.to_str().unwrap(),
            &run_stream_identity(root),
        )
        .unwrap(),
    );
    // Opened per statement and closed at once: a rebuild owns its shadow alone, so the test holds
    // no connection to it while `rigger setup` rebuilds.
    let on_shadow = |sql: &str| {
        rusqlite::Connection::open(&shadow)
            .unwrap()
            .execute_batch(sql)
            .unwrap()
    };
    on_shadow(
        "CREATE TRIGGER boom BEFORE INSERT ON nodes WHEN NEW.id = 'd-boom'
         BEGIN SELECT RAISE(ABORT, 'disk gave out'); END;",
    );

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            ok,
            err.lines().last(),
            out.lines()
                .filter(|l| l.starts_with("rebuilt graph.db") || l.starts_with("passed over "))
                .collect::<Vec<_>>(),
            applied(&graph_db, first),
            applied(&graph_db, lost),
        ),
        (
            false,
            Some("rigger: graph: event store: disk gave out"),
            Vec::<&str>::new(),
            true,
            false
        ),
        "setup fails naming the storage error, passes nothing over, and leaves graph.db as it \
         was, the event still missing; stdout: {out} stderr: {err}"
    );

    on_shadow("DROP TRIGGER boom;");
    the_next_setup_pays_the_missing_event(root, lost);
}

/// Given a project whose run stream grew only through rigger's own verbs - `rigger step` minting
/// the run and driving its unit to an escalation, and `rigger resume-unit` granting it another
/// attempt - when the operator runs `rigger setup`, then setup rebuilds nothing: every event a verb
/// appended is folded, so the graph's ledger holds every position the log does, and nothing was
/// lost for setup to find missing.
#[test]
fn every_event_rigger_s_own_verbs_append_is_folded_so_setup_owes_no_rebuild() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    escalate_solo_unit(root);
    let (_, err, ok) = run_rigger(root, &["resume-unit", "solo", "--attempts", "1"]);
    assert!(
        ok,
        "resume-unit on the escalated unit must succeed; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        read_run_events(root)
            .into_iter()
            .filter(|e| !applied(&graph_db, e.position))
            .map(|e| (e.position, e.type_))
            .collect::<Vec<_>>(),
        Vec::<(u64, String)>::new(),
        "every event the verbs appended is in the graph's ledger"
    );
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup must succeed; stdout: {out} stderr: {err}");
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("rebuilding graph.db") || l.starts_with("rebuilt "))
            .collect::<Vec<_>>(),
        Vec::<&str>::new(),
        "setup owes no rebuild for a log its verbs folded; stdout: {out}"
    );
}

/// Given an escalated unit and a `graph.db` whose bytes are not a database, when the operator runs
/// `rigger resume-unit`, then the resume succeeds with its `UnitResumed` on the log and says the fold
/// it could not make, with the reason: a verb whose job is to append never opens `graph.db` before
/// its append, so an unopenable graph never costs the operator's recovery verb its record.
#[test]
fn resume_unit_into_a_graph_it_cannot_open_is_on_the_log_and_reported_not_folded() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    escalate_solo_unit(root);
    let garbage = b"this file is not a sqlite database, only text standing in for one".repeat(64);
    std::fs::write(rigger_file(root, "graph.db"), &garbage).unwrap();

    let (out, err, ok) = run_rigger(root, &["resume-unit", "solo", "--attempts", "1"]);
    let log = run_log(root);
    assert_eq!(
        (ok, log.last().map(|(_, t)| t.as_str())),
        (true, Some("UnitResumed")),
        "the resume succeeds and is on the log; stdout: {out} stderr: {err}"
    );
    assert_eq!(
        err,
        "rigger: recorded 1 run event(s); not folded into the context graph: graph: file is not \
         a database\n",
        "the fold it could not make is said, with the reason"
    );
}

/// A git project scaffolded with the escalating one-unit workflow, its grounder one the binary
/// rejects: a `--fresh` step or run mints its boundary, re-pins the definition and then fails at
/// the grounder, before it drives anything or serves stdin.
fn fresh_run_project() -> tempfile::TempDir {
    let dir = temp_git_project_with_commit();
    write_workflow_fixture(dir.path(), &REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW);
    let body = REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW
        .body
        .replace("grounder: nop", "grounder: totally-bogus-grounder");
    std::fs::write(rigger_file(dir.path(), "workflow.yml"), body).unwrap();
    dir
}

/// `(position, type)` of every event on `root`'s run stream, oldest first.
fn run_log(root: &Path) -> Vec<(u64, String)> {
    read_run_events(root)
        .into_iter()
        .map(|e| (e.position, e.type_))
        .collect()
}

/// Given a project, when the operator begins a new run with `args` (`rigger step --fresh`, or
/// `rigger run --fresh` on either driver), then the boundary it mints and every other event it
/// appends before it drives are folded: the graph's ledger holds every position the log does.
fn a_fresh_runs_mint_is_folded(args: &[&str]) {
    let dir = fresh_run_project();
    let root = dir.path();
    let (out, err, ok) = run_rigger(root, args);
    assert!(
        !ok && err.contains("totally-bogus-grounder"),
        "the run fails at the grounder, after its mint; stdout: {out} stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let log = run_log(root);
    assert_eq!(
        (
            log.iter().filter(|(_, t)| t == "RunStarted").count(),
            log.iter()
                .filter(|(p, _)| !applied(&graph_db, *p))
                .cloned()
                .collect::<Vec<_>>(),
        ),
        (1, Vec::<(u64, String)>::new()),
        "the fresh boundary is on the log and every appended event is in the graph's ledger; \
         log: {log:?}"
    );
}

/// Given a project whose `graph.db` owes its rebuild (a fold lost to another writer's lock), when
/// the operator begins a new run with `args`, then it refuses naming `rigger setup` before it
/// appends anything: no boundary is minted that the graph would miss, and neither the log nor
/// `graph.db` changes.
fn a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
    args: &[&str],
    command: &str,
) {
    let dir = fresh_run_project();
    let root = dir.path();
    common::cli::seed_store(root);
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(ok, "the first emit creates the graph; stderr: {err}");
    let graph_db = rigger_file(root, "graph.db");
    let (out, err, _) = with_graph_locked(&graph_db, || emit_decision(root, "d-locked"));
    assert!(
        out.ends_with("; not folded into the context graph: graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it\n"),
        "the locked emit's fold is lost, so the graph owes its rebuild; stdout: {out} stderr: {err}"
    );
    let (log, graph) = (run_log(root), std::fs::read(&graph_db).unwrap());

    let (out, err, ok) = run_rigger(root, args);
    assert_eq!(
        (
            ok,
            err.trim_end()
                .ends_with(&rigger::contextgraph::rebuild_owed_refusal(command))
        ),
        (false, true),
        "the fresh run refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
    assert_eq!(
        (run_log(root), std::fs::read(&graph_db).unwrap() == graph),
        (log, true),
        "the refusal mints no boundary and leaves graph.db as it was"
    );
}

rigger::test_cases! {
    /// `rigger step --fresh` mints its boundary through the step's folding store.
    a_fresh_steps_mint_is_folded: a_fresh_runs_mint_is_folded(&["step", "--fresh"]);
    /// `rigger run --driver cli --fresh` mints its boundary through a folding store.
    a_fresh_cli_runs_mint_is_folded:
        a_fresh_runs_mint_is_folded(&["run", "--driver", "cli", "--base", "HEAD", "--fresh"]);
    /// `rigger run --driver workflow --fresh` mints its boundary through a folding store.
    a_fresh_workflow_runs_mint_is_folded:
        a_fresh_runs_mint_is_folded(&["run", "--driver", "workflow", "--base", "HEAD", "--fresh"]);
    /// `rigger step --fresh` opens the graph before it mints.
    a_fresh_step_on_an_owed_graph_refuses_before_it_mints:
        a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
            &["step", "--fresh"],
            "step",
        );
    /// `rigger run --driver cli --fresh` opens the graph before it mints.
    a_fresh_cli_run_on_an_owed_graph_refuses_before_it_mints:
        a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
            &["run", "--driver", "cli", "--base", "HEAD", "--fresh"],
            "run",
        );
    /// `rigger run --driver workflow --fresh` opens the graph before it mints.
    a_fresh_workflow_run_on_an_owed_graph_refuses_before_it_mints:
        a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
            &["run", "--driver", "workflow", "--base", "HEAD", "--fresh"],
            "run",
        );
}

/// Given a `graph.db` still at the old fold rule, when the operator runs `rigger reset --runs`,
/// then it refuses at once naming `rigger setup` - it neither prunes nor compacts a graph that
/// owes its rebuild, nor folds into it - and leaves both the file and the log exactly as they were.
#[test]
fn reset_runs_on_a_graph_that_owes_its_rebuild_refuses_naming_setup_and_writes_nothing() {
    let store = ReleaseEraStore::new();
    let (graph, log) = (store.graph_bytes(), store.log().len());
    let (out, err, ok) = run_rigger(store.root(), &["reset", "--runs"]);
    assert!(!ok, "reset --runs must refuse; stdout: {out}");
    assert!(
        err.contains(&format!(
            "reset --runs: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the refusal names `rigger setup`; stderr: {err}"
    );
    assert_eq!(
        (store.graph_bytes() == graph, store.log().len()),
        (true, log),
        "the refusal writes nothing to graph.db or the log"
    );
}

/// Given a current `graph.db` another writer holds locked, when the operator runs `rigger graph
/// build` over a project with two source files, then the build appends both files' batches and
/// says they were not folded, naming the first fold it lost (the lock) rather than the owed
/// refusal the second batch met because of it; the graph now owes its rebuild, so the next build
/// refuses naming `rigger setup`.
#[cfg(feature = "symbols")]
#[test]
fn a_graph_build_whose_fold_is_lost_to_a_lock_says_so_and_the_next_build_refuses() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok,
        "the first emit creates the store and graph; stderr: {err}"
    );
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("lib.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(root.join("src").join("more.rs"), "pub fn beta() {}\n").unwrap();
    let graph_db = rigger_file(root, "graph.db");
    let before = read_run_events(root).len();

    let (out, err, ok) = with_graph_locked(&graph_db, || run_rigger(root, &["graph", "build"]));
    let ingested = read_run_events(root).len() - before;
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "graph build: ingested {ingested} code-ingest event(s) into .rigger/graph.db; \
                 not folded into the context graph: graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it\n"
            )
        ),
        "the build names the first fold it lost - the lock - never the owed refusal every later \
         batch then met; stderr: {err}"
    );
    let mut files: Vec<String> = read_run_events(root)[before..]
        .iter()
        .filter_map(|e| {
            serde_json::from_slice::<serde_json::Value>(&e.data).ok()?["file"]
                .as_str()
                .map(String::from)
        })
        .collect();
    files.dedup();
    assert_eq!(
        files,
        vec!["src/lib.rs".to_string(), "src/more.rs".to_string()],
        "both files' batches are on the log, one after the other"
    );

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        !ok && err.contains(&format!(
            "graph build: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the next build refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
}

/// Given a run whose `graph.db` another writer holds locked while `rigger step` records the run's
/// events, when the step runs, then it still advances the run and says on stderr that the events
/// it recorded were not folded, with the reason; the graph now owes its rebuild, so the next step
/// refuses naming `rigger setup` instead of advancing the run over a graph that lost them.
#[test]
fn a_step_whose_fold_is_lost_to_a_lock_says_so_and_the_next_step_refuses() {
    let dir = common::cli::temp_repoless_project();
    let root = dir.path();
    common::cli::seed_store(root);
    common::cli::write_workflow(root, "");
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && !err.contains("run event(s)"),
        "the first step parks the stage's spawn and, its events folded, says nothing of the \
         fold; stdout: {out} stderr: {err}"
    );
    let (_, err, ok) = run_rigger(root, &["result", "a/implementer#0", "done"]);
    assert!(ok, "the spawn's result is recorded; stderr: {err}");

    let graph_db = rigger_file(root, "graph.db");
    let (out, err, ok) = with_graph_locked(&graph_db, || run_rigger(root, &["step"]));
    assert_eq!(
        (ok, out.as_str()),
        (true, "{\"wave\":[],\"done\":true}\n"),
        "the step replays the result and finishes the run; stderr: {err}"
    );
    assert!(
        err.contains("; not folded into the context graph: graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it\n"),
        "the step says the events it recorded were not folded; stderr: {err}"
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        !ok && err.contains(&format!("step: {}", rigger::contextgraph::REBUILD_OWED)),
        "the next step refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
}

/// Cut `branch` from the run branch of the repository at `root`, commit on it, and fast-forward
/// the run branch onto it: a unit's work landed by hand, as `rigger reset --runs` closes it.
/// Returns the landed tip.
fn land_on_the_run_branch(root: &Path, branch: &str) -> String {
    use common::git::{git_ok, git_ok_with_identity, git_out};
    git_ok(root, &["branch", branch, "rigger-run"]);
    git_ok(root, &["checkout", "-q", branch]);
    git_ok_with_identity(root, &["commit", "-q", "--allow-empty", "-m", branch]);
    git_ok(root, &["checkout", "-q", "rigger-run"]);
    git_ok(root, &["merge", "-q", "--ff-only", branch]);
    git_out(root, &["rev-parse", branch])
}

/// The line `rigger reset --runs` prints for closing unit `unit` at landed tip `tip` of run `r1`,
/// before whatever it adds about the fold.
fn closed_unit_line(unit: &str, tip: &str) -> String {
    format!(
        "reset --runs: closed unit {unit:?} of run r1: no driver is alive and its branch tip {tip} \
         is landed on rigger-run, so its UnitIntegrated is recorded (by operator)"
    )
}

/// The position of the last `UnitIntegrated` on `root`'s run stream.
fn last_integration(root: &Path) -> u64 {
    read_run_events(root)
        .iter()
        .rev()
        .find(|e| e.type_ == "UnitIntegrated")
        .expect("a UnitIntegrated is on the log")
        .position
}

/// Given a run no driver drives whose unit's branch is landed on the run branch, when the operator
/// runs `rigger reset --runs`, then it records the unit's `UnitIntegrated`, folds it, and its
/// closing line claims nothing more; when another writer holds `graph.db` locked while it closes
/// the next landed unit, the closing line says that unit's integration was not folded, with the
/// reason, and the graph now owes its rebuild, so the next `rigger reset --runs` refuses naming
/// `rigger setup`.
#[test]
fn reset_runs_says_whether_the_integration_it_recorded_for_a_landed_unit_was_folded() {
    use common::git::{git_ok, init_repo};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    git_ok(root, &["checkout", "-q", "-b", "rigger-run"]);
    common::cli::seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","spec":"s.md"}"#),
            ("UnitStarted", r#"{"id":"u1","branch":"rigger/u1"}"#),
        ],
    );
    let tip = land_on_the_run_branch(root, "rigger/u1");

    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert_eq!(
        (ok, out.lines().next().unwrap_or_default().to_string()),
        (true, closed_unit_line("u1", &tip)),
        "the folded integration's line claims nothing more; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert!(
        applied(&graph_db, last_integration(root)),
        "the recorded integration is folded"
    );

    common::cli::seed_run_events(
        root,
        &[("UnitStarted", r#"{"id":"u2","branch":"rigger/u2"}"#)],
    );
    let tip = land_on_the_run_branch(root, "rigger/u2");
    let (out, err, _) = with_graph_locked(&graph_db, || run_rigger(root, &["reset", "--runs"]));
    assert_eq!(
        out.lines().next().unwrap_or_default(),
        format!(
            "{}; not folded into the context graph: graph: database is locked - the next `rigger setup` finds the event missing from graph.db and rebuilds it",
            closed_unit_line("u2", &tip)
        ),
        "the lost integration's line says it was not folded, with the reason; stderr: {err}"
    );
    assert!(
        !applied(&graph_db, last_integration(root)),
        "the integration is on the log but not in the graph"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(
        !ok && err.contains(&rigger::contextgraph::rebuild_owed_refusal("reset --runs")),
        "the next reset --runs refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
}

/// Given an in-memory graph, which no rebuild outlives, when a batch fails to fold into it, then
/// the failure is reported but marks nothing owed - no file beside it is written - and the next
/// batch folds.
#[test]
fn a_failed_fold_into_an_in_memory_graph_marks_nothing_owed_and_the_next_batch_folds() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    let mark = std::env::current_dir().unwrap().join(":memory:.owed");
    let mut poison = Event::new("DecisionMade", b"{ not valid json".to_vec());
    poison.position = 1;
    let graph = Projector::open(":memory:", PROJECT).unwrap();
    let failed = Fold::of_batch(|| rigger::contextgraph::wired(Some(&graph)), &[poison]);
    assert!(
        matches!(&failed, Fold::NotFolded(why) if why.starts_with("graph: ")),
        "the failed fold is reported: {failed:?}"
    );
    assert_eq!(
        (graph.rebuild_owed().unwrap(), mark.exists()),
        (false, false),
        "an in-memory graph owes nothing and writes no mark"
    );
    let mut good = Event::new(
        "DecisionMade",
        br#"{"id":"d","summary":"s","governs":["a.rs"],"supersedes":""}"#.to_vec(),
    );
    good.position = 2;
    common::fixtures::folds(&graph, &[good]);
}

/// The decision, finding and lesson node ids the `graph.db` of `root` holds, sorted.
fn provenance_nodes(root: &Path) -> Vec<String> {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    let mut ids: Vec<String> = graph
        .whole()
        .unwrap()
        .nodes
        .into_iter()
        .filter(|n| ["decision", "finding", "lesson"].contains(&n.kind.as_str()))
        .map(|n| n.id)
        .collect();
    ids.sort();
    ids
}

/// Given a log holding a closed run's decision and finding, a lesson, a closed-run decision that
/// supersedes one whose id the active run reuses (retiring its governing edge before the active
/// run began), and that active run, whose closed-run nodes and superseded edges `rigger reset
/// --runs` pruned from the live graph, when the operator's `graph.db` is replaced by an empty one
/// and `rigger setup` cold-rebuilds it from the log, then the rebuilt graph holds exactly the
/// decision, finding and lesson nodes the live graph held - never the pruned ones - and is the
/// live graph, every node and edge.
#[test]
fn a_cold_rebuild_of_a_log_whose_closed_runs_were_pruned_yields_the_live_graph() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let before = menus_runs_prune(root);
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(
        ok,
        "reset --runs prunes the closed run; stdout: {out} stderr: {err}"
    );
    let live = provenance_nodes(root);
    let live_whole = whole_graph(root);
    let pruned = menus_runs_prune(root);

    lose_graph(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(
        ok,
        "setup rebuilds the lost graph; stdout: {out} stderr: {err}"
    );
    assert_eq!(
        (live, provenance_nodes(root)),
        (
            vec!["d-live".to_string(), "l1".to_string(), "shared".to_string()],
            vec!["d-live".to_string(), "l1".to_string(), "shared".to_string()],
        ),
        "the rebuilt graph holds the live graph's decision, finding and lesson nodes, never the \
         pruned ones; stdout: {out}"
    );
    assert_eq!(
        (before, pruned, menus_runs_prune(root)),
        (
            runs_prunable(3, 1),
            runs_prunable(0, 0),
            runs_prunable(0, 0)
        ),
        "the closed run left nodes and a superseded edge to prune; once pruned, and once rebuilt, \
         nothing is left - the rebuild dropped the superseded edge reset --runs dropped"
    );
    assert_eq!(
        whole_graph(root),
        live_whole,
        "the rebuilt graph is the live pruned graph, every node and edge"
    );
}

/// Set up the project at `root` and record, as a run records it and folded into its live graph as
/// it lands, a lesson, a closed run `r1` (a decision, a finding, and a decision `d-sup` that
/// supersedes `shared`, retiring its governing edge before the active run began) and the active run
/// `r2`, which records `d-live` and reuses the id `shared`: `rigger reset --runs` prunes 3 dead-run
/// nodes (`d-dead`, `f-dead`, `d-sup`) and 1 superseded edge from it.
fn closed_run_store(root: &Path) {
    let decision = |id: &str| {
        format!(r#"{{"id":"{id}","summary":"s","governs":["src/f.rs"],"supersedes":""}}"#)
    };
    let (d_dead, shared, d_live) = (decision("d-dead"), decision("shared"), decision("d-live"));
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(
        ok,
        "setup mints the project identity; stdout: {out} stderr: {err}"
    );
    let (out, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "LessonLearned",
            r#"{"id":"l1","summary":"s","about":["src/f.rs"]}"#,
        ],
    );
    assert!(
        ok,
        "the first emit creates the store and graph; stdout: {out} stderr: {err}"
    );
    fold_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","spec":"s.md"}"#),
            ("DecisionMade", &d_dead),
            ("ReviewFinding", r#"{"id":"f-dead","about":["src/f.rs"]}"#),
            ("DecisionMade", &shared),
            (
                "DecisionMade",
                r#"{"id":"d-sup","summary":"s","governs":["src/f.rs"],"supersedes":"shared"}"#,
            ),
            ("RunStarted", r#"{"run":"r2","spec":"s.md"}"#),
            ("DecisionMade", &d_live),
            (
                "DecisionMade",
                r#"{"id":"shared","summary":"s","governs":["src/g.rs"],"supersedes":""}"#,
            ),
        ],
    );
}

/// Replace the `graph.db` of `root` with an empty one, which owes its rebuild from the log.
fn lose_graph(root: &Path) {
    let graph_db = rigger_file(root, "graph.db");
    std::fs::remove_file(&graph_db).unwrap();
    drop(
        rigger::contextgraph::sqlite::Projector::open(
            graph_db.to_str().unwrap(),
            &run_stream_identity(root),
        )
        .unwrap(),
    );
}

/// What `rigger setup` printed of `out` right after it said it rebuilt `graph.db`, if it did.
fn after_rebuilt(out: &str) -> Option<String> {
    out.lines()
        .skip_while(|l| *l != "rebuilt graph.db from the event log")
        .nth(1)
        .map(str::to_string)
}

/// The opening of the report `rigger reset --runs` prints in `out`: what it pruned, up to where it
/// names the graph it pruned.
fn reset_runs_pruned(out: &str) -> Option<String> {
    out.lines()
        .find(|l| l.starts_with("reset --runs: "))
        .and_then(|l| l.split(" from the context graph").next())
        .map(str::to_string)
}

/// Given a log whose closed run the live graph still holds, when `rigger reset --runs` prunes it
/// and then `graph.db` is lost and `rigger setup` rebuilds it from the same log, then setup reports,
/// right after its rebuilt line, the prune its rebuild made in the words `reset --runs` reports its
/// own - the same dead-run node and superseded edge counts.
#[test]
fn setup_reports_the_prune_its_rebuild_made_as_reset_runs_reports_it() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let (reset, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(ok, "reset --runs prunes; stdout: {reset} stderr: {err}");
    lose_graph(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (ok, reset_runs_pruned(&reset), after_rebuilt(&out)),
        (
            true,
            Some(
                "reset --runs: pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s)"
                    .to_string()
            ),
            Some(
                "pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) from the rebuilt graph"
                    .to_string()
            ),
        ),
        "setup reports the prune reset --runs reports, in its words; stdout: {out} stderr: {err}"
    );
}

/// Given a `rigger setup` whose rebuild stopped after its prune and its swap - the owed mark it
/// must then drop is a directory it cannot remove - when that is cleared and setup runs again,
/// then the rerun finishes the rebuild and reports the counts that prune made, never zero, and the
/// graph holds what `reset --runs` leaves.
#[test]
fn a_setup_rerun_after_its_rebuild_stopped_past_its_prune_reports_the_same_counts() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    let (stopped, stopped_ok) = stop_setup_past_its_prune(root, |_| {});
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            stopped_ok,
            after_rebuilt(&stopped),
            ok,
            after_rebuilt(&out),
            provenance_nodes(root),
        ),
        (
            false,
            None,
            true,
            Some(
                "pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) from the rebuilt graph"
                    .to_string()
            ),
            vec!["d-live".to_string(), "l1".to_string(), "shared".to_string()],
        ),
        "the stopped setup fails before its rebuilt line, and the rerun reports the prune it \
         made; stdout: {out} stderr: {err}"
    );
}

/// Run a `rigger setup` in `root` whose rebuild stops right after its prune and its swap - the owed
/// mark it must then drop is a directory it cannot remove, and its failure names that mark - then
/// `meanwhile`, while that mark still says `graph.db` owes its rebuild, and clear the mark: what
/// the stopped setup printed, and whether it exited successfully.
fn stop_setup_past_its_prune(root: &Path, meanwhile: fn(&Path)) -> (String, bool) {
    let mark = rigger_file(root, "graph.db.owed");
    std::fs::create_dir(&mark).unwrap();
    let (stopped, err, stopped_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        err.lines().last(),
        Some(
            format!(
                "rigger: graph: {}: Is a directory (os error 21)",
                Path::new(".rigger").join("graph.db.owed").display()
            )
            .as_str()
        ),
        "the stopped setup names the mark it could not remove; stdout: {stopped} stderr: {err}"
    );
    meanwhile(root);
    std::fs::remove_dir(&mark).unwrap();
    (stopped, stopped_ok)
}

/// Given a `rigger setup` whose rebuild stopped past its prune and its swap, when the log gains
/// events before setup runs again - a decision the active run records with `rigger emit`, which
/// the owed graph does not fold, a run another writer of the log starts, closing the run that was
/// active, or a closed run's decision id the active run records again about another file - or a
/// swap cut short left its private pruned copy behind, then the rerun names no cause - the shadow
/// is the rebuild's own unfinished work, which the swapped-in ledger owes nothing for - and resumes
/// its unpruned shadow, folding only what the log gained past it - its one progress line counts
/// exactly those events, and a rerun the log gained nothing for folds none - and prunes a fresh
/// copy of it from the whole run attribution it gathered, the first pass's included, never reading
/// a stale copy and leaving none behind: it keeps the active run's decision, prunes the run that
/// closed, keeps the id the active run re-recorded governing both files its two recordings name,
/// folds every event the log gained, keeps no rebuild state and leaves `rigger reset --runs`
/// nothing to prune; and a cold rebuild of the same log yields the same graph, every node and edge,
/// and the same report - the one prune of the whole log, never the sum of two: when the run that
/// closed is the one that was active, `shared` is dropped with the retired edge it owns, 5 nodes
/// and no edge left to reclaim.
#[test]
fn a_setup_resumed_past_its_prune_keeps_the_active_runs_gains_and_prunes_the_run_that_closed() {
    let active_run_decides: fn(&Path) = |root| {
        let (out, err, ok) = emit_decision(root, "d-window");
        assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    };
    let another_writer_starts_a_run: fn(&Path) = |root| {
        common::cli::with_run_store(root, |store| {
            store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        Event::new("RunStarted", br#"{"run":"r3","spec":"s.md"}"#.to_vec()),
                        Event::new(
                            "DecisionMade",
                            br#"{"id":"d-r3","summary":"s","governs":["src/f.rs"],"supersedes":""}"#
                                .to_vec(),
                        ),
                    ],
                )
                .unwrap();
        });
    };
    let active_run_re_records_a_closed_runs_decision: fn(&Path) = |root| {
        let (out, err, ok) = run_rigger(
            root,
            &[
                "emit",
                "DecisionMade",
                r#"{"id":"d-dead","summary":"s","governs":["src/h.rs"],"supersedes":""}"#,
            ],
        );
        assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    };
    let a_swap_cut_short_left_its_copy: fn(&Path) = |root| {
        std::fs::write(
            rigger_file(root, "graph.db.pruned"),
            b"left by a swap cut short",
        )
        .unwrap();
    };
    for (meanwhile, gains, kept, d_dead_governs, report) in [
        (
            active_run_decides,
            1,
            vec!["d-live", "d-window", "l1", "shared"],
            vec![],
            "pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) from the rebuilt graph",
        ),
        (
            another_writer_starts_a_run,
            2,
            vec!["d-r3", "l1"],
            vec![],
            "pruned 5 dead-run node(s) and reclaimed 0 superseded edge(s) from the rebuilt graph",
        ),
        (
            active_run_re_records_a_closed_runs_decision,
            1,
            vec!["d-dead", "d-live", "l1", "shared"],
            vec!["src/f.rs", "src/h.rs"],
            "pruned 2 dead-run node(s) and reclaimed 1 superseded edge(s) from the rebuilt graph",
        ),
        (
            a_swap_cut_short_left_its_copy,
            0,
            vec!["d-live", "l1", "shared"],
            vec![],
            "pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) from the rebuilt graph",
        ),
    ] {
        let dir = temp_store_project();
        let root = dir.path();
        closed_run_store(root);
        lose_graph(root);
        let held = read_run_events(root).len();
        let (stopped, stopped_ok) = stop_setup_past_its_prune(root, meanwhile);
        let graph_db = rigger_file(root, "graph.db");
        let log = read_run_events(root);
        let head = log.last().unwrap().position;
        let gained: Vec<u64> = log[held..].iter().map(|e| e.position).collect();
        // Whether the graph's applied ledger records each event the log gained.
        let folded = || -> Vec<bool> { gained.iter().map(|&p| applied(&graph_db, p)).collect() };
        let folded_before = folded();

        let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        let resumed = (
            ok,
            out.lines()
                .filter(|l| l.starts_with("rebuilding graph.db"))
                .map(str::to_string)
                .collect::<Vec<_>>(),
            rebuild_progress(&out),
            after_rebuilt(&out),
            provenance_nodes(root),
            live_governs(root, "d-dead"),
            folded(),
            [
                holds_table(&graph_db, "rebuild_cursor"),
                holds_table(&graph_db, "rebuild_run_closure"),
                rigger_file(root, "graph.db.rebuild").exists(),
                rigger_file(root, "graph.db.pruned").exists(),
            ],
            menus_runs_prune(root),
        );
        let resumed_graph = whole_graph(root);
        lose_graph(root);
        let (cold, cold_err, cold_ok) =
            run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        assert_eq!(
            (
                stopped_ok,
                after_rebuilt(&stopped),
                gained.len(),
                folded_before,
                resumed,
                (cold_ok, after_rebuilt(&cold), whole_graph(root)),
            ),
            (
                false,
                None,
                gains,
                vec![false; gains],
                (
                    true,
                    Vec::<String>::new(),
                    (gains > 0)
                        .then(|| {
                            format!("rebuilt {gains} events, through position {head} of {head} (100%)")
                        })
                        .into_iter()
                        .collect::<Vec<_>>(),
                    Some(report.to_string()),
                    kept.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    d_dead_governs
                        .iter()
                        .map(|file: &&str| file.to_string())
                        .collect::<Vec<_>>(),
                    vec![true; gains],
                    [false, false, false, false],
                    runs_prunable(0, 0),
                ),
                (true, Some(report.to_string()), resumed_graph),
            ),
            "the rerun folds only what the log gained, keeps what the active run gained and prunes \
             what closed, from the whole gathered attribution, with the report a cold rebuild \
             makes; rerun stdout: {out} stderr: {err}; cold stdout: {cold} stderr: {cold_err}"
        );
    }
}

/// The progress lines a `rigger setup` rebuild printed in `out`, in order: how many events each
/// batch carried it through.
fn rebuild_progress(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| l.starts_with("rebuilt ") && l.ends_with("%)"))
        .map(str::to_string)
        .collect()
}

/// The files the node `id` in `root`'s `graph.db` governs through a live edge, sorted.
fn live_governs(root: &Path, id: &str) -> Vec<String> {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    let mut files: Vec<String> = graph
        .whole()
        .unwrap()
        .edges
        .into_iter()
        .filter(|e| {
            e.from == id && e.rel == rigger::contextgraph::REL_GOVERNS && e.valid_to.is_none()
        })
        .map(|e| e.to)
        .collect();
    files.sort();
    files
}

/// Given a `rigger setup` whose rebuild swapped its pruned graph in and stopped before it folded the
/// tail - the state it leaves once the owed mark and the shadow are gone, the next two steps its
/// swap takes: `graph.db` holding the rebuild's cursor, no shadow beside it - when the log gains an
/// event no fold made (another writer's) and an agent's `rigger emit`, which folds at once into the
/// current file, then the next `rigger setup` names no cause, because the ledger owes the tail to
/// the swapped-in cursor rather than to a lost fold, folds exactly the tail - its one progress line
/// counts the two events past the cursor, through the log's last position - and meets the emitted
/// event once, reports the prune stamped with the swap, drops the rebuild state, and leaves the
/// graph a cold rebuild of the same log yields; a setup after it owes nothing.
#[test]
fn a_setup_stopped_in_its_tail_is_finished_by_the_next_setup_folding_exactly_the_tail() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    let (stopped, stopped_ok) = stop_setup_past_its_prune(root, |root| {
        std::fs::remove_file(rigger_file(root, "graph.db.rebuild")).unwrap();
    });
    let graph_db = rigger_file(root, "graph.db");
    let swapped_cursor = holds_table(&graph_db, "rebuild_cursor");
    let unfolded = append_unfolded_decision(
        root,
        br#"{"id":"d-tail","summary":"s","governs":["src/t.rs"],"supersedes":""}"#,
    );
    let (emitted, emit_err, emit_ok) = emit_decision(root, "d-emitted");
    let emitted_at = read_run_events(root).last().unwrap().position;
    let before = (applied(&graph_db, unfolded), applied(&graph_db, emitted_at));

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let finished = (
        ok,
        out.lines()
            .filter(|l| l.starts_with("rebuilding graph.db"))
            .collect::<Vec<_>>(),
        rebuild_progress(&out),
        after_rebuilt(&out),
        (applied(&graph_db, unfolded), applied(&graph_db, emitted_at)),
        provenance_nodes(root),
        [
            holds_table(&graph_db, "rebuild_cursor"),
            holds_table(&graph_db, "rebuild_run_closure"),
            rigger_file(root, "graph.db.rebuild").exists(),
        ],
    );
    let (again, again_err, again_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let finished_graph = whole_graph(root);
    lose_graph(root);
    let (cold, cold_err, cold_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            stopped_ok,
            after_rebuilt(&stopped),
            swapped_cursor,
            emit_ok && emitted.ends_with(" and folded it into the context graph\n"),
            before,
            finished,
            again_ok && !again.contains("graph.db"),
            (cold_ok, finished_graph),
        ),
        (
            false,
            None,
            true,
            true,
            (false, true),
            (
                true,
                Vec::<&str>::new(),
                vec![format!(
                    "rebuilt 2 events, through position {emitted_at} of {emitted_at} (100%)"
                )],
                Some(
                    "pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) from the rebuilt \
                     graph"
                        .to_string()
                ),
                (true, true),
                vec![
                    "d-emitted".to_string(),
                    "d-live".to_string(),
                    "d-tail".to_string(),
                    "l1".to_string(),
                    "shared".to_string(),
                ],
                [false, false, false],
            ),
            true,
            (true, whole_graph(root)),
        ),
        "the next setup folds exactly the tail and reaches the cold graph; emit stdout: {emitted} \
         stderr: {emit_err}; setup stdout: {out} stderr: {err}; second setup stdout: {again} \
         stderr: {again_err}; cold stdout: {cold} stderr: {cold_err}"
    );
}

/// Given a `graph.db` that owes its rebuild and a stale `graph.db.pruned` beside it with no shadow
/// - the copy a swap cut short left, whose rebuild is gone - when the operator runs `rigger reset
/// --runs`, then it removes the copy first and says so, and still refuses the prune naming `rigger
/// setup`, leaving `graph.db` and the log exactly as they were and creating no shadow.
#[test]
fn reset_runs_on_a_graph_that_owes_its_rebuild_removes_a_stale_pruned_copy_before_it_refuses() {
    let store = ReleaseEraStore::new();
    let root = store.root();
    let copy = rigger_file(root, "graph.db.pruned");
    std::fs::write(&copy, b"left by a swap cut short").unwrap();
    let (graph, log) = (store.graph_bytes(), store.log().len());
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert_eq!(
        (
            ok,
            out.lines().collect::<Vec<_>>(),
            err.lines().last(),
            copy.exists(),
            rigger_file(root, "graph.db.rebuild").exists(),
            store.graph_bytes() == graph,
            store.log().len(),
        ),
        (
            false,
            vec![
                "reset --runs: removed graph.db.pruned, the pruned copy a rebuild's stopped swap \
                 left beside graph.db"
            ],
            Some(
                format!(
                    "rigger: reset --runs: {}",
                    rigger::contextgraph::REBUILD_OWED
                )
                .as_str()
            ),
            false,
            false,
            true,
            log,
        ),
        "the stale copy is removed before the owed graph is refused; stdout: {out} stderr: {err}"
    );
}

/// Given a project that recorded a decision with `rigger emit` before any run started, when an
/// agent asks `rigger_peers` in a live `rigger mcp` session and the operator runs `rigger peers`,
/// then both label it LIVE - with no run there is no closed run for it to belong to; when another
/// writer of the log then starts a run and that run records a decision, then the same session's
/// next `rigger_peers` and `rigger peers` both label the earlier decision HISTORICAL and the run's
/// own LIVE: one rule, one answer on both surfaces.
#[test]
fn a_decision_recorded_before_any_run_is_live_on_both_peers_surfaces_until_a_run_starts() {
    let dir = temp_store_project();
    let root = dir.path();
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(
        ok,
        "setup mints the project identity; stdout: {out} stderr: {err}"
    );
    let (out, err, ok) = emit_decision(root, "d-early");
    assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    let mut mcp = common::mcp::McpSession::start(root);
    // `(id, live)` of each decision a `rigger_peers` reply holds, and the decision lines of a
    // `rigger peers` read, in order.
    let mut both_surfaces = || {
        let reply = mcp.peers();
        let tool: Vec<(String, bool)> = reply["result"]["structuredContent"]["decisions"]
            .as_array()
            .unwrap_or_else(|| panic!("a decisions array: {reply}"))
            .iter()
            .map(|d| {
                (
                    d["id"].as_str().unwrap().to_string(),
                    d["live"].as_bool().unwrap(),
                )
            })
            .collect();
        let (out, err, ok) = run_rigger(root, &["peers", "src/f.rs"]);
        assert!(ok, "rigger peers answers; stdout: {out} stderr: {err}");
        let cli: Vec<String> = out
            .lines()
            .filter(|l| l.starts_with("decision "))
            .map(str::to_string)
            .collect();
        (tool, cli)
    };
    let before_any_run = both_surfaces();
    common::cli::seed_run_events(root, &[("RunStarted", r#"{"run":"r1","spec":"s.md"}"#)]);
    let (out, err, ok) = emit_decision(root, "d-run");
    assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    let once_a_run_started = both_surfaces();
    let exit = mcp.finish();
    assert_eq!(
        (before_any_run, once_a_run_started, exit.status.success()),
        (
            (
                vec![("d-early".to_string(), true)],
                vec!["decision d-early | LIVE | s | governs: src/f.rs".to_string()],
            ),
            (
                vec![("d-early".to_string(), false), ("d-run".to_string(), true)],
                vec![
                    "decision d-early | HISTORICAL | s | governs: src/f.rs".to_string(),
                    "decision d-run | LIVE | s | governs: src/f.rs".to_string(),
                ],
            ),
            true,
        ),
        "both surfaces label a decision recorded before any run live, and historical once a run \
         started"
    );
}

/// Given a sqlite log with a stream subscription or an all-streams subscription live over it, each
/// having delivered the event the log held, when the log's events table is dropped under it by
/// another connection, then the subscription ends and reports the database's own error exactly as
/// the database spells it, never re-spelled as the port's error: the one forward read the stream
/// subscription polls through spells a failure as its caller does, so both kinds report the same
/// words.
#[test]
fn a_sqlite_subscription_whose_log_fails_reports_the_databases_own_error() {
    type Subscribe = fn(&Store) -> rigger::eventstore::Subscription;
    let to_the_stream: Subscribe = |store| store.subscribe_stream("s", 0).unwrap();
    let to_every_stream: Subscribe = |store| {
        store
            .subscribe_all(
                0,
                &rigger::eventstore::Filter {
                    stream_prefix: None,
                },
            )
            .unwrap()
    };
    let ended = [to_the_stream, to_every_stream].map(|subscribe| {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("events.db");
        let store = Store::open(db.to_str().unwrap()).unwrap();
        store
            .append(
                "s",
                ExpectedRevision::Any,
                &[Event::new("E", b"{}".to_vec())],
            )
            .unwrap();
        let subscription = subscribe(&store);
        let delivered = subscription
            .recv_timeout(std::time::Duration::from_secs(10))
            .map(|e| (e.stream, e.revision));
        let other = rusqlite::Connection::open(&db).unwrap();
        other
            .busy_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        other.execute_batch("DROP TABLE events").unwrap();
        let after = subscription
            .recv_timeout(std::time::Duration::from_secs(10))
            .map(|e| e.revision);
        (delivered, after, subscription.err())
    });
    assert_eq!(
        ended,
        [
            (
                Some(("s".to_string(), 0)),
                None,
                Some("no such table: events".to_string())
            ),
            (
                Some(("s".to_string(), 0)),
                None,
                Some("no such table: events".to_string())
            ),
        ],
        "each subscription delivers what the log held, then ends reporting the database's own error"
    );
}

/// Given a project that recorded decisions, a finding and a lesson with `rigger emit` before it
/// ever started a run, when its `graph.db` is lost and `rigger setup` rebuilds it, then setup
/// reports it pruned nothing and the rebuilt graph is the live one, every node and edge; and
/// `rigger reset --runs` over that log prunes nothing either, as the bare `rigger reset` menu
/// previews it over the live graph that holds every one of those nodes; and `rigger peers` labels
/// each of those decisions live, the one answer the prune gives.
#[test]
fn a_log_that_never_started_a_run_is_rebuilt_and_reset_without_pruning_anything() {
    let dir = temp_store_project();
    let root = dir.path();
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(
        ok,
        "setup mints the project identity; stdout: {out} stderr: {err}"
    );
    for (ty, body) in [
        (
            "LessonLearned",
            r#"{"id":"l1","summary":"s","about":["src/f.rs"]}"#,
        ),
        (
            "DecisionMade",
            r#"{"id":"d-old","summary":"s","governs":["src/f.rs"],"supersedes":""}"#,
        ),
        (
            "DecisionMade",
            r#"{"id":"d-new","summary":"s","governs":["src/f.rs"],"supersedes":"d-old"}"#,
        ),
        (
            "ReviewFinding",
            r#"{"id":"f1","by":"lens","summary":"s","about":["src/f.rs"]}"#,
        ),
    ] {
        let (out, err, ok) = run_rigger(root, &["emit", ty, body]);
        assert!(ok, "emit {ty}; stdout: {out} stderr: {err}");
    }
    let live = whole_graph(root);
    let menu = menus_runs_prune(root);
    lose_graph(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let rebuilt = whole_graph(root);
    let (reset, reset_err, reset_ok) = run_rigger(root, &["reset", "--runs"]);
    let (peers, peers_err, peers_ok) = run_rigger(root, &["peers", "src/f.rs"]);
    assert_eq!(
        (
            ok,
            after_rebuilt(&out),
            provenance_nodes(root),
            rebuilt,
            menu,
            reset_ok,
            reset_runs_pruned(&reset),
            whole_graph(root),
            peers_ok,
            peers
                .lines()
                .filter(|l| l.starts_with("decision "))
                .collect::<Vec<_>>(),
        ),
        (
            true,
            Some(
                "pruned 0 dead-run node(s) and reclaimed 0 superseded edge(s) from the rebuilt graph"
                    .to_string()
            ),
            vec![
                "d-new".to_string(),
                "d-old".to_string(),
                "f1".to_string(),
                "l1".to_string()
            ],
            live.clone(),
            runs_prunable(0, 0),
            true,
            Some(
                "reset --runs: pruned 0 dead-run node(s) and reclaimed 0 superseded edge(s)"
                    .to_string()
            ),
            live,
            true,
            vec![
                "decision d-old | LIVE | s | governs: src/f.rs",
                "decision d-new | LIVE | s | governs: src/f.rs",
            ],
        ),
        "nothing is dead before a run starts; setup stdout: {out} stderr: {err}; reset stderr: \
         {reset_err}; peers stderr: {peers_err}"
    );
}

/// Given a stale `graph.db.pruned` beside the graph, when `rigger reset --runs` runs while a
/// rebuild holds `graph.db.lock` in its swap - its shadow open, its private copy beside it - then
/// reset keeps the copy and names the rebuild in progress; once no rebuild holds the lock, `rigger
/// reset --runs` removes the copy and says so, and never touches the shadow a rebuild resumes from.
#[test]
fn reset_runs_removes_a_stale_pruned_copy_unless_a_rebuild_holds_graph_db_lock() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let copy = rigger_file(root, "graph.db.pruned");
    let shadow = rigger_file(root, "graph.db.rebuild");
    std::fs::write(&copy, b"the copy a live swap is using").unwrap();
    let removed = |out: &str| -> Vec<String> {
        out.lines()
            .filter(|l| l.contains("graph.db.pruned"))
            .map(str::to_string)
            .collect()
    };
    let rebuilding = hold_the_rebuild(root);
    let rebuilding_shadow = hold_shadow(root);
    let (held, held_err, held_ok) = run_rigger(root, &["reset", "--runs"]);
    let kept = std::fs::read(&copy).ok();
    drop((rebuilding, rebuilding_shadow));
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert_eq!(
        (
            held_ok,
            removed(&held),
            kept,
            ok,
            removed(&out),
            copy.exists(),
            shadow.exists(),
        ),
        (
            true,
            vec![format!(
                "reset --runs: kept graph.db.pruned: {}",
                rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS
            )],
            Some(b"the copy a live swap is using".to_vec()),
            true,
            vec![
                "reset --runs: removed graph.db.pruned, the pruned copy a rebuild's stopped swap \
                 left beside graph.db"
                    .to_string()
            ],
            false,
            true,
        ),
        "the copy is kept naming the rebuild in progress while it holds the lock, and removed once \
         none does; held stderr: {held_err}; stdout: {out} stderr: {err}"
    );
}

/// Where a rebuild in progress stands when a second `rigger setup` starts.
#[derive(Debug, Clone, Copy)]
enum RebuildPhase {
    /// It holds `graph.db.lock` and has written nothing yet.
    Locked,
    /// It folds its shadow, open mid-batch: the fixture's EXCLUSIVE connection ([`hold_shadow`])
    /// stands in for it, the test's detector for a setup that reads the shadow - not a lock a
    /// rebuild holds through its fold: a rebuild holds only `graph.db.lock`.
    Folding,
    /// It swaps: its shadow is open and its private pruned copy stands beside it.
    Swapping,
    /// Its swap has ended - no shadow, `graph.db` holding its cursor - and it folds the tail.
    FoldingTheTail,
}

/// Every `graph.db*` entry under `.rigger/` of `root` ([`dir_snapshot`]).
fn graph_files(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    dir_snapshot(&rigger_file(root, ""), "graph.db")
}

/// Given a rebuild in progress holding `graph.db.lock` - before its first write, in its fold, in its
/// swap and in its tail - when a second `rigger setup` starts, then it is refused at once with the
/// one refusal text naming the rebuild in progress, printing nothing else about `graph.db`, never
/// opening the shadow and leaving every `graph.db` file exactly as it found it - no shadow, copy or
/// mark it did not find; and once the rebuild lets go, the next `rigger setup` pays what it left
/// and reaches the graph a cold rebuild of the same log yields, with no rebuild state, shadow or
/// copy.
#[test]
fn a_second_setup_while_a_rebuild_holds_graph_db_lock_is_refused_at_once_and_touches_nothing() {
    use RebuildPhase::*;
    for phase in [Locked, Folding, Swapping, FoldingTheTail] {
        let dir = temp_store_project();
        let root = dir.path();
        closed_run_store(root);
        lose_graph(root);
        if let FoldingTheTail = phase {
            // The state a rebuild leaves once its swap has ended: the cursor it carried into
            // graph.db, and no shadow.
            stop_setup_past_its_prune(root, |root| {
                std::fs::remove_file(rigger_file(root, "graph.db.rebuild")).unwrap();
            });
        }
        let graph_db = rigger_file(root, "graph.db");
        let shadow = rigger_file(&root.canonicalize().unwrap(), "graph.db.rebuild");
        let holder = hold_the_rebuild(root);
        let held_shadow = matches!(phase, Folding | Swapping).then(|| hold_shadow(root));
        if let Swapping = phase {
            std::fs::write(
                rigger_file(root, "graph.db.pruned"),
                b"the copy the swap is using",
            )
            .unwrap();
        }
        let found = graph_files(root);
        let state = tempfile::tempdir().unwrap();
        let mut second = rigger_command(root, &["setup"], &[("RIGGER_NPM", "true")], state.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // How many descriptors the second setup ever held on the shadow while it ran: a setup
        // that read the held shadow would wait on the fixture's exclusive lock, long enough to be
        // seen.
        let pid = second.id();
        let mut shadow_opened = 0;
        let ended = wait_until_for(4800, || {
            shadow_opened = shadow_opened.max(files_open_by(pid, &shadow));
            !matches!(second.try_wait(), Ok(None))
        });
        if !ended {
            cleanup(&mut second);
            panic!("the second setup ({phase:?}) never ended in 120s");
        }
        let refused = second.wait_with_output().unwrap();
        let (out, err) = (
            String::from_utf8_lossy(&refused.stdout).into_owned(),
            String::from_utf8_lossy(&refused.stderr).into_owned(),
        );
        let refused = (
            refused.status.success(),
            out.lines()
                .filter(|l| l.contains("graph.db"))
                .map(str::to_string)
                .collect::<Vec<_>>(),
            err.lines().last().map(str::to_string),
            shadow_opened,
            graph_files(root) == found,
        );
        drop((holder, held_shadow));
        let (paid, paid_err, paid_ok) =
            run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        let paid_state = (
            paid_ok,
            after_rebuilt(&paid),
            holds_table(&graph_db, "rebuild_cursor"),
            shadow.exists(),
            rigger_file(root, "graph.db.pruned").exists(),
        );
        let paid_graph = whole_graph(root);
        lose_graph(root);
        let (cold, cold_err, cold_ok) =
            run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        assert_eq!(
            (refused, paid_state, (cold_ok, paid_graph)),
            (
                (
                    false,
                    Vec::<String>::new(),
                    Some(format!(
                        "rigger: graph: {}",
                        rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS
                    )),
                    0,
                    true,
                ),
                (
                    true,
                    Some(
                        "pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) from the \
                         rebuilt graph"
                            .to_string()
                    ),
                    false,
                    false,
                    false,
                ),
                (true, whole_graph(root)),
            ),
            "the second setup ({phase:?}) is refused at once and touches nothing, and the next \
             setup pays the rebuild; refused stdout: {out} stderr: {err}; next stdout: {paid} \
             stderr: {paid_err}; cold stdout: {cold} stderr: {cold_err}"
        );
    }
}

/// Given a `rigger setup` whose rebuild committed its first batches into its shadow and whose
/// process is then gone - killed while it still held `graph.db.lock` - when the next `rigger setup`
/// runs, then the lock is free: that setup takes it, resumes the shadow from its last committed
/// batch - its one progress line counts only the events past it - and reaches the graph a cold
/// rebuild of the same log yields; while the process lived, a setup was refused naming the
/// rebuild in progress.
#[test]
fn a_rebuild_whose_process_is_gone_leaves_no_lock_and_the_next_setup_resumes_its_shadow() {
    use rigger::contextgraph::sqlite::Projector;
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    let graph_db = rigger_file(root, "graph.db");
    let project = run_stream_identity(root);
    let log = read_run_events(root);
    let head = log.last().unwrap().position;
    // The rebuild's first two batches of two events are committed into its shadow; it stops.
    let mut batches = 0;
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let mut live = live_selection_of(&backend, &project, 2);
    let stopped = Projector::rebuild(
        &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
        &project,
        true,
        &mut |after, sink| {
            live(after, &mut |events, head| {
                batches += 1;
                if batches > 2 {
                    return Err(rigger::contextgraph::Error("stopped".to_string()));
                }
                sink(events, head)
            })
        },
        &mut |_| {},
    )
    .map_err(|e| e.to_string());
    drop(live);
    let cursor: u64 = rusqlite::Connection::open(rigger_file(root, "graph.db.rebuild"))
        .unwrap()
        .query_row("SELECT position FROM rebuild_cursor", [], |r| r.get(0))
        .unwrap();
    // Its process holds the lock until it is gone.
    let lock = rigger_file(root, "graph.db.lock").canonicalize().unwrap();
    let mut rebuilding = common::fixtures::lock_holder(&lock);
    let held = (
        files_open_by(rebuilding.id(), &lock),
        run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]),
    );
    cleanup(&mut rebuilding);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let past_the_cursor = log.iter().filter(|e| e.position > cursor).count();
    let resumed_graph = whole_graph(root);
    lose_graph(root);
    let (cold, cold_err, cold_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            stopped,
            (
                held.0,
                held.1 .2,
                held.1 .1.lines().last().map(str::to_string)
            ),
            ok,
            rebuild_progress(&out),
            after_rebuilt(&out),
            rigger_file(root, "graph.db.rebuild").exists(),
            (cold_ok, resumed_graph),
        ),
        (
            Err("graph: event store: stopped".to_string()),
            (
                1,
                false,
                Some(format!(
                    "rigger: graph: {}",
                    rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS
                ))
            ),
            true,
            vec![format!(
                "rebuilt {past_the_cursor} events, through position {head} of {head} (100%)"
            )],
            Some(
                "pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) from the rebuilt \
                 graph"
                    .to_string()
            ),
            false,
            (true, whole_graph(root)),
        ),
        "the lock of a gone process is free and the next setup resumes its shadow; stdout: {out} \
         stderr: {err}; cold stdout: {cold} stderr: {cold_err}"
    );
}

/// Given a project whose `graph.db` a `rigger setup` rebuilt, then `graph.db.lock` stands beside it,
/// zero bytes, and no `rigger reset` verb removes it: the bare menu, `--runs`, `--derived`,
/// `--build-cache` and `--scratch-orphans` each leave it where it is.
#[test]
fn graph_db_lock_stands_beside_graph_db_after_setup_and_no_reset_verb_removes_it() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "setup rebuilds graph.db; stdout: {out} stderr: {err}");
    let lock = rigger_file(root, "graph.db.lock");
    let stands = || std::fs::metadata(&lock).map(|m| m.len()).ok();
    let mut after = vec![("setup".to_string(), true, stands())];
    // The cache-home reclaim verbs act on a private cache home, never the operator's.
    let cache = tempfile::tempdir().unwrap();
    let cache_home = [("XDG_CACHE_HOME", cache.path().to_str().unwrap())];
    for verb in [
        vec!["reset"],
        vec!["reset", "--runs"],
        vec!["reset", "--derived"],
        vec!["reset", "--build-cache"],
        vec!["reset", "--scratch-orphans"],
    ] {
        let (out, err, ok) = run_rigger_envs(root, &verb, &cache_home);
        assert!(ok, "{verb:?} runs; stdout: {out} stderr: {err}");
        after.push((verb.join(" "), ok, stands()));
    }
    assert_eq!(
        after,
        [
            "setup",
            "reset",
            "reset --runs",
            "reset --derived",
            "reset --build-cache",
            "reset --scratch-orphans"
        ]
        .map(|verb| (verb.to_string(), true, Some(0)))
        .to_vec(),
        "graph.db.lock stands, zero bytes, through every reset verb"
    );
}

/// Given history recorded before the project minted its durable identity, and a rebuild in
/// progress holding `graph.db.lock`, when `rigger setup` runs, then it is refused before its
/// identity migration, in the operator's words naming the rebuild in progress: it renames no
/// stream and records no migration decision, prints nothing about `graph.db`, and leaves every
/// `graph.db` file as it found them; once the rebuild lets go, the next `rigger setup` migrates
/// the history and records the migration's decision on the minted stream.
#[test]
fn a_setup_refused_by_a_rebuild_in_progress_migrates_nothing_and_names_it_in_its_own_words() {
    let dir = temp_store_project();
    let root = dir.path();
    let (legacy, minted) = legacy_history_then_minted_identity(root);
    let streams = || (ids_under(root, &legacy), ids_under(root, &minted));
    let migrated = |err: &str| {
        err.lines()
            .filter(|l| l.starts_with("rigger: migrated project identity"))
            .count()
    };

    let holder = hold_the_rebuild(root);
    let found = graph_files(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let refused = (
        ok,
        out.lines()
            .filter(|l| l.contains("graph.db"))
            .map(str::to_string)
            .collect::<Vec<_>>(),
        migrated(&err),
        err.lines().last().map(str::to_string),
        streams(),
        graph_files(root) == found,
    );
    drop(holder);
    let (paid, paid_err, paid_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (refused, (paid_ok, migrated(&paid_err), streams())),
        (
            (
                false,
                Vec::<String>::new(),
                0,
                Some(
                    "rigger: graph: a rebuild of graph.db is in progress (a `rigger setup` holds \
                     graph.db.lock)"
                        .to_string()
                ),
                (vec!["d-legacy".to_string()], Vec::<String>::new()),
                true,
            ),
            (
                true,
                1,
                (
                    Vec::<String>::new(),
                    vec![
                        "d-legacy".to_string(),
                        format!("identity-migration-{minted}")
                    ]
                )
            ),
        ),
        "the refused setup migrates nothing and names the rebuild in progress, and the next setup \
         migrates; refused stdout: {out} stderr: {err}; next stdout: {paid} stderr: {paid_err}"
    );
}

/// The ids the run stream of `root` holds under the namespace `identity`, in log order.
fn ids_under(root: &Path, identity: &str) -> Vec<String> {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    Namespaced::new(&backend, identity)
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
        .iter()
        .map(|e| {
            serde_json::from_slice::<serde_json::Value>(&e.data).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// Given a `graph.db.lock` that cannot be opened - a directory stands at its path - when `rigger
/// setup` runs over the project's `graph.db`, then it fails naming `.rigger/graph.db.lock`, prints
/// nothing about `graph.db` and leaves every `graph.db` file as it found them; when `rigger reset
/// --runs` runs with no stale copy beside the graph, it never tries the lock and prunes as ever;
/// and with a stale `graph.db.pruned` beside it, `rigger reset --runs` fails naming the lock and
/// keeps the copy byte for byte. `rigger --help` says the copy is kept while a rebuild in progress
/// holds `graph.db.lock`.
#[test]
fn a_graph_db_lock_that_cannot_be_opened_is_named_by_setup_and_by_reset_runs_over_a_stale_copy() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let lock = rigger_file(root, "graph.db.lock");
    std::fs::create_dir(&lock).unwrap();
    let copy = rigger_file(root, "graph.db.pruned");
    let graph_db_lines = |out: &str| -> Vec<String> {
        out.lines()
            .filter(|l| l.contains("graph.db"))
            .map(str::to_string)
            .collect()
    };

    let found = graph_files(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let setup = (
        ok,
        graph_db_lines(&out),
        err.lines().last().map(str::to_string),
        graph_files(root) == found,
    );
    let (no_copy, no_copy_err, no_copy_ok) = run_rigger(root, &["reset", "--runs"]);
    let no_copy = (
        no_copy_ok,
        graph_db_lines(&no_copy),
        no_copy
            .lines()
            .find(|l| l.starts_with("reset --runs: pruned"))
            .and_then(|l| l.split(", then").next())
            .map(str::to_string),
    );
    std::fs::write(&copy, b"left by a swap that stopped").unwrap();
    let (over_copy, over_copy_err, over_copy_ok) = run_rigger(root, &["reset", "--runs"]);
    let over_copy = (
        over_copy_ok,
        graph_db_lines(&over_copy),
        over_copy_err.lines().last().map(str::to_string),
        std::fs::read(&copy).unwrap(),
        lock.is_dir(),
    );
    // The usage goes to stderr.
    let (_, help, help_ok) = run_rigger(root, &["--help"]);
    let help = (
        help_ok,
        help.lines()
            .skip_while(|l| !l.trim_start().starts_with("rigger reset --runs"))
            .take_while(|l| !l.trim_start().starts_with("rigger reset --derived"))
            .map(str::trim)
            .collect::<Vec<_>>()
            .split_last()
            .map(|(last, _)| last.to_string()),
    );
    let named = |lock: &Path| {
        format!(
            "rigger: graph: {}: Is a directory (os error 21)",
            lock.display()
        )
    };
    assert_eq!(
        (setup, no_copy, over_copy, help),
        (
            (
                false,
                Vec::<String>::new(),
                // Setup names the lock file relative to the project root it runs in.
                Some(named(&rigger_file(Path::new(""), "graph.db.lock"))),
                true
            ),
            (
                true,
                Vec::<String>::new(),
                Some(
                    "reset --runs: pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) \
                     from the context graph"
                        .to_string()
                )
            ),
            (
                false,
                Vec::<String>::new(),
                Some(named(&lock.canonicalize().unwrap())),
                b"left by a swap that stopped".to_vec(),
                true
            ),
            (
                true,
                Some("left, unless a rebuild in progress holds graph.db.lock".to_string())
            ),
        ),
        "setup and reset --runs over a stale copy name the lock file they cannot open, and reset \
         --runs with no copy never tries it; setup stdout: {out} stderr: {err}; no-copy stderr: \
         {no_copy_err}; over-copy stderr: {over_copy_err}"
    );
}

/// A rebuild in progress over the `graph.db` of `root`, holding `graph.db.lock` until it is
/// dropped.
fn hold_the_rebuild(root: &Path) -> rigger::contextgraph::sqlite::RebuildLock {
    rigger::contextgraph::sqlite::Projector::lock_rebuild(
        rigger_file(root, "graph.db").to_str().unwrap(),
    )
    .unwrap()
}

/// The shadow of the `graph.db` of `root`, held open mid-batch as a rebuild in its fold holds it,
/// until the connection is dropped. The connection is EXCLUSIVE as the test's detector for a setup
/// that reads the shadow - that setup would wait on it - not as a lock a rebuild holds through its
/// fold: a second setup is refused at `graph.db.lock` ([`hold_the_rebuild`]).
fn hold_shadow(root: &Path) -> rusqlite::Connection {
    let rebuilding = rusqlite::Connection::open(rigger_file(root, "graph.db.rebuild")).unwrap();
    rebuilding
        .execute_batch("PRAGMA locking_mode = EXCLUSIVE; CREATE TABLE folding (x);")
        .unwrap();
    rebuilding
}

/// Append each of `events` to the run stream of `root` and fold it into its `graph.db` as it lands,
/// as the conductor records a run: the live graph, never one a rebuild produced.
fn fold_run_events(root: &Path, events: &[(&str, &str)]) {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    common::cli::with_run_store(root, |store| {
        let folding = rigger::ingest::folding_into(store, Some(&graph), &|_| {});
        for &(ty, body) in events {
            let done = folding
                .append_and_fold(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[Event::new(ty, body.as_bytes().to_vec())],
                )
                .unwrap();
            assert_eq!(done.fold, Fold::Folded, "the {ty} folds as it lands");
        }
    });
}

/// The reset menu's `--runs` line counting `nodes` dead-run nodes and `edges` superseded edges.
fn runs_prunable(nodes: usize, edges: usize) -> String {
    format!(
        "--runs: {nodes} dead-run node(s) and {edges} superseded edge(s) prunable from the context \
         graph; rerun `rigger reset --runs` to reclaim them"
    )
}

/// The `--runs` line of the `rigger reset` menu of `root`: what the context graph holds to prune.
fn menus_runs_prune(root: &Path) -> String {
    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(ok, "the reset menu answers; stdout: {out} stderr: {err}");
    out.lines()
        .find(|l| l.starts_with("--runs:"))
        .unwrap_or_else(|| panic!("the menu names --runs; stdout: {out}"))
        .to_string()
}

/// The whole graph the `graph.db` of `root` holds - every node and edge - as JSON.
fn whole_graph(root: &Path) -> String {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    serde_json::to_string(&graph.whole().unwrap()).unwrap()
}
