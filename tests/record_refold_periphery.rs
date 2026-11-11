//! The record refold the migration runs: a rebuild of graph.db folds the record events of the log
//! and skips every derived type by its type, so a log still holding derived rows refolds to the
//! graph its records alone fold to, with no derived row passed over and no cause for another
//! rebuild named.

use std::time::SystemTime;

use rigger::contextgraph::sqlite::{causes_once_seeded, stream_past, Projector};
use rigger::eventstore::Event;

use crate::common;
use common::cli::no_progress;
use common::fixtures::{decision_json, ev_at};

/// The record events of the log: a decision governing the file every derived row names.
fn records() -> Vec<Event> {
    vec![ev_at(
        1,
        "DecisionMade",
        decision_json("d-keep", "keep the parser", &["src/lib.rs"], ""),
    )]
}

/// The graph a rebuild of `log` folds into a fresh graph.db under `dir`, rendered sorted, with
/// the count of positions it passed over and the causes `rigger setup` would name for another
/// rebuild of it: the ones the file records, then a lost fold when its seeding against the log
/// finds one.
fn refolded(
    dir: &std::path::Path,
    name: &str,
    log: &[Event],
) -> (Vec<String>, usize, Vec<&'static str>) {
    let db = dir.join(name);
    let db = db.to_str().unwrap();
    let graph = Projector::open(db, "test").unwrap();
    graph
        .rebuild(
            &Projector::lock_rebuild(db).unwrap(),
            true,
            &mut |after, sink| stream_past(log, after, log.len().max(1), sink),
            &mut no_progress,
        )
        .unwrap();
    let whole = graph.whole().unwrap();
    let mut rendered: Vec<String> = whole
        .nodes
        .iter()
        .map(|n| format!("node {} {} {:?}", n.id, n.kind, n.attrs))
        .chain(
            whole
                .edges
                .iter()
                .map(|e| format!("edge {} -{}-> {}", e.from, e.rel, e.to)),
        )
        .collect();
    rendered.sort();
    let positions: Vec<_> = log.iter().map(|e| e.position).collect();
    let seeding = graph
        .seed_cursor(
            &mut |sink| sink(&positions),
            &mut |_| Ok(None),
            positions.last().copied(),
            SystemTime::now(),
        )
        .unwrap();
    (
        rendered,
        graph.passed_over().unwrap().total(),
        causes_once_seeded(graph.owed_because().unwrap(), &seeding),
    )
}

/// What a log holding the records and one `type_` row carrying `payload` refolds to, when it
/// differs from what the records alone fold to: the row must fold nothing, be passed over under
/// no cause, and leave setup naming no cause for another rebuild.
fn mismatch(type_: &str, payload: serde_json::Value) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    let alone = refolded(dir.path(), "records.db", &records());
    let mut log = records();
    log.push(ev_at(2, type_, payload));
    let with_derived = refolded(dir.path(), "derived.db", &log);
    (with_derived != alone)
        .then(|| format!("{type_}: {with_derived:?}\n  records alone: {alone:?}"))
}

#[test]
fn record_refold_skips_derived_types() {
    let mismatches = [
        (
            "CodeEntityExtracted",
            serde_json::json!({"file": "src/lib.rs", "name": "parse", "kind": "function", "line": 1, "lang": "rust"}),
        ),
        (
            "EdgeInferred",
            serde_json::json!({"file": "src/lib.rs", "name": "lex", "lang": "rust", "from": "src/lib.rs::parse", "kind": "calls"}),
        ),
        (
            "DocConceptExtracted",
            serde_json::json!({"kind": "design-doc", "id": "docs/design.md", "title": "Design", "doc": "docs/design.md"}),
        ),
        (
            "DocLinkExtracted",
            serde_json::json!({"from": "docs/design.md", "to": "src/lib.rs", "rel": "GOVERNS"}),
        ),
        (
            "GenerationIngested",
            serde_json::json!({"prefix": "gc", "file": "src/lib.rs", "generation": "g1", "blob": "", "excluded": false}),
        ),
        (
            "CommunityAssigned",
            serde_json::json!({"node": "src/lib.rs", "community": "community/1", "resolution": 1.0, "hash": "h"}),
        ),
        (
            "ConceptDerived",
            serde_json::json!({"concept": "concept/parsing", "label": "parsing", "resolution": 1.0, "hash": "h"}),
        ),
        (
            "ConceptRealized",
            serde_json::json!({"node": "src/lib.rs", "concept": "concept/parsing", "resolution": 1.0, "hash": "h"}),
        ),
    ]
    .into_iter()
    .filter_map(|(type_, payload)| mismatch(type_, payload))
    .collect::<Vec<_>>();
    assert!(
        mismatches.is_empty(),
        "a derived row changed the refold:\n{}",
        mismatches.join("\n")
    );
}
