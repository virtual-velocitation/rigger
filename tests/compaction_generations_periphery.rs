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

use common::cli::keyed;
use common::cli::nanos;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_rigger_envs;
use common::cli::run_stream_identity;
use common::cli::temp_store_project;
use common::fixtures::meta_replay_key;
use rigger::contextgraph::sqlite::RebuildSink;
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
    use rigger::contextgraph::Projection;
    let open = || Projector::open(graph_db.to_str().unwrap(), project).unwrap();
    for batch in batches {
        open().apply_batch(batch).unwrap();
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
    use rigger::contextgraph::Projection;
    Projector::open(db.to_str().unwrap(), PROJECT)
        .unwrap()
        .apply_batch(events)
        .unwrap();
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
        let head = log.last().map_or(0, |e| e.position);
        let gained: Vec<Event> = log.iter().filter(|e| e.position > after).cloned().collect();
        gained.chunks(batch).try_for_each(|b| sink(b, head))
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

/// Rebuild the `graph.db` at `db` from `source`, reporting nothing.
fn rebuild(
    db: &Path,
    source: &mut rigger::contextgraph::sqlite::RebuildSource,
) -> Result<bool, rigger::contextgraph::Error> {
    rigger::contextgraph::sqlite::Projector::rebuild(
        db.to_str().unwrap(),
        PROJECT,
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
        graph.apply_batch(&log[3..]).unwrap_err().0,
        rigger::contextgraph::REBUILD_OWED,
        "nothing folds incrementally into a pre-rule graph.db"
    );
    drop(graph);
    let afters = std::cell::RefCell::new(Vec::new());
    let mut reported = Vec::new();
    let ran = Projector::rebuild(
        old_db.to_str().unwrap(),
        PROJECT,
        &mut source_over(&log, 2, &afters),
        &mut |at| reported.push(at),
    )
    .unwrap();
    assert!(ran, "the owed rebuild runs");
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
    assert!(!again, "a paid rebuild does not run again");
    Projector::open(old_db.to_str().unwrap(), PROJECT)
        .unwrap()
        .apply_batch(&later_events())
        .unwrap();
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
    assert!(ran, "the owed rebuild runs");
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
        (true, vec![0], vec![log[2].position]),
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
            true,
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
        (true, vec![log[2].position]),
        "the next rebuild folds only the tail"
    );
    let again = rebuild(&db, &mut |_, _| panic!("a finished rebuild reads nothing")).unwrap();
    assert!(!again, "and the one after it has nothing to do");
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
    assert!(rebuild(&selected_db, &mut live_selection_of(&backend, PROJECT, 2)).unwrap());

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
        use rigger::contextgraph::Projection;
        Projector::open(graph_db.to_str().unwrap(), &run_stream_identity(root))
            .unwrap()
            .apply_batch(&log[..3])
            .unwrap();
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
    use rigger::contextgraph::Projection;
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
    graph.apply_batch(&later_events()).unwrap();
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
            "emitted DecisionMade (position {}); not folded into the context graph: {}\n",
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
/// `rigger setup` rather than failing on a lock, and an emit appends without folding; when the
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
                graph_db.to_str().unwrap(),
                &project,
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

    resume.send(()).unwrap();
    assert!(rebuilder.join().unwrap(), "the rebuild ran");
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
    assert!(
        emitted.get("error").is_none(),
        "the emit succeeds; got: {emitted}"
    );
    let log = store.log();
    assert_eq!(
        (log.len(), log.last().unwrap().type_.as_str()),
        (7, "DecisionMade"),
        "the emit appended to the log"
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
    assert!(
        emitted.get("error").is_none(),
        "the emit succeeds; got: {emitted}"
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
