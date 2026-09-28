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

mod common;

use common::cli::keyed;
use common::cli::nanos;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_stream_identity;
use common::cli::temp_store_project;
use common::fixtures::meta_replay_key;
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
        .unwrap();
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
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    let events = Namespaced::new(store, PROJECT)
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap();
    let p = Projector::open(graph_db.to_str().unwrap(), PROJECT).unwrap();
    p.apply_batch(&events).unwrap();
    serde_json::to_string(&p.whole().unwrap()).unwrap()
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
                .unwrap(),
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
            .unwrap(),
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
        menu.contains("--derived: 4 duplicate event(s)"),
        "the menu must count both superseded generations' four recordings; got: {menu:?}"
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
