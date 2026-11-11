//! The record refold the migration runs: a rebuild of graph.db folds the record events of the log
//! and skips every derived type by its type, so a log still holding derived rows refolds to the
//! graph its records alone fold to, with no derived row passed over and no cause for another
//! rebuild named. A graph.db the rule before it folded is migrated by `rigger setup`: refolded
//! from the records alone and swapped in only when it keeps every record fact the old file holds
//! and no record names a grouping id - refused otherwise, the old file kept and what it would lose
//! printed.

use std::time::SystemTime;

use rigger::contextgraph::sqlite::{causes_once_seeded, stream_past, Projector};
use rigger::eventstore::Event;

use crate::common;
use common::cli::{
    graph_identity, no_progress, pin_identity, read_run_events, rigger_file, run_rigger_envs,
    run_stream_identity, seed_pre_ledger_events, setup_ok, temp_store_project, user_version,
};
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

/// One row of every derived type, each as `(type, payload)`, naming the file the records govern.
fn derived_rows() -> Vec<(&'static str, serde_json::Value)> {
    vec![
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
}

#[test]
fn record_refold_skips_derived_types() {
    let mismatches = derived_rows()
        .into_iter()
        .filter_map(|(type_, payload)| mismatch(type_, payload))
        .collect::<Vec<_>>();
    assert!(
        mismatches.is_empty(),
        "a derived row changed the refold:\n{}",
        mismatches.join("\n")
    );
}

/// The rows a file the rule before the records-only fold folded holds beside its records, `{P}`
/// standing for the project: the tree file the records govern recorded as a file with a definition
/// under it, a grouping it belongs to with the record identity's assertion of that grouping, and
/// the edge assertion ledger that rule kept, holding the definition's edge.
const RULE_6_DERIVED_ROWS: &str = "
CREATE TABLE IF NOT EXISTS edge_assertions (
  project TEXT NOT NULL, identity TEXT NOT NULL, generation TEXT NOT NULL, edge_id INTEGER NOT NULL,
  since INTEGER NOT NULL, source INTEGER NOT NULL,
  PRIMARY KEY (project, identity, edge_id)
);
UPDATE nodes SET kind = 'file' WHERE id = 'src/lib.rs';
INSERT OR REPLACE INTO nodes (id, kind, attrs, project) VALUES
  ('src/lib.rs::parse', 'function', '{\"name\":\"parse\",\"line\":\"1\"}', '{P}'),
  ('community/1/0', 'community', '{}', '{P}');
INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier) VALUES
  ('src/lib.rs', 'src/lib.rs::parse', 'CONTAINS', 2, NULL, 2, '{P}', 'extracted');
INSERT INTO edge_assertions VALUES ('{P}', 'gc/src/lib.rs', 'g1', last_insert_rowid(), 2, 2);
INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier) VALUES
  ('src/lib.rs', 'community/1/0', 'IN_COMMUNITY', 3, NULL, 3, '{P}', 'extracted');
INSERT OR REPLACE INTO node_assertions
  VALUES ('{P}', '', '', 'community/1/0', 'community', '{}');
";

/// A decision, kept in the log, that the rule-6 file folded and the records-only fold folds again.
fn a_decision_governing_the_tree_file() -> Event {
    ev_at(
        1,
        "DecisionMade",
        decision_json("d-keep", "keep the parser", &["src/lib.rs"], ""),
    )
}

/// An operator store whose graph.db the rule before the records-only fold folded: its log holds
/// `records` then one row of every derived type ([`derived_rows`]), and its graph.db holds what the
/// records fold to, the derived facts that rule kept beside them ([`RULE_6_DERIVED_ROWS`]), and
/// then `plant` (`{P}` standing for the project), recorded at rule 6.
fn a_rule_6_store(records: &[Event], plant: &str) -> tempfile::TempDir {
    use rigger::contextgraph::sqlite::Projector;
    let dir = temp_store_project();
    let root = dir.path();
    pin_identity(root, "proj-migrate");
    let mut log = records.to_vec();
    log.extend(
        derived_rows()
            .into_iter()
            .map(|(type_, payload)| ev_at(0, type_, payload)),
    );
    seed_pre_ledger_events(root, &log);
    let graph_db = rigger_file(root, "graph.db");
    let project = run_stream_identity(root);
    let graph = Projector::open(graph_db.to_str().unwrap(), &project).unwrap();
    let folded: Vec<Event> = read_run_events(root)
        .into_iter()
        .take(records.len())
        .collect();
    common::fixtures::folds(&graph, &folded);
    drop(graph);
    rusqlite::Connection::open(&graph_db)
        .unwrap()
        .execute_batch(&format!(
            "{}{}PRAGMA user_version = 6;",
            RULE_6_DERIVED_ROWS.replace("{P}", &project),
            plant.replace("{P}", &project)
        ))
        .unwrap();
    dir
}

/// The graph.db of the store at `root`, its whole live projection rendered sorted.
fn rendered_graph(root: &std::path::Path) -> Vec<String> {
    use rigger::contextgraph::sqlite::Projector;
    let graph_db = rigger_file(root, "graph.db");
    let whole = Projector::open(graph_db.to_str().unwrap(), &run_stream_identity(root))
        .unwrap()
        .whole()
        .unwrap();
    let mut rendered: Vec<String> = whole
        .nodes
        .iter()
        .map(|n| format!("node {} {}", n.id, n.kind))
        .chain(
            whole
                .edges
                .iter()
                .map(|e| format!("edge {} -{}-> {}", e.from, e.rel, e.to)),
        )
        .collect();
    rendered.sort();
    rendered
}

/// `rigger setup` over the store at `root`, which must fail: its stderr, after insisting the
/// graph.db stands as it was - still at rule 6, owing the migration - and that a second setup
/// refuses the same way.
fn setup_refuses(root: &std::path::Path) -> String {
    let graph_db = rigger_file(root, "graph.db");
    let before = rendered_graph_raw(&graph_db);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[]);
    assert!(
        !ok,
        "setup must refuse the migration; stdout: {out} stderr: {err}"
    );
    assert_eq!(
        (user_version(&graph_db), rendered_graph_raw(&graph_db)),
        (6, before),
        "a refused migration keeps the old file as it stands; stderr: {err}"
    );
    let (_, again, ok) = run_rigger_envs(root, &["setup"], &[]);
    assert_eq!(
        (ok, again.contains(err.lines().last().unwrap_or_default())),
        (false, true),
        "a second setup refuses the same way; stderr: {again}"
    );
    err
}

/// Every node and edge row of the graph.db at `db`, read as the file holds them, sorted.
fn rendered_graph_raw(db: &std::path::Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut rows: Vec<String> = conn
        .prepare(
            "SELECT 'node ' || id || ' ' || kind FROM nodes
             UNION ALL SELECT 'edge ' || from_id || ' -' || rel || '-> ' || to_id || ' '
                              || valid_from || ' ' || ifnull(valid_to, '-') FROM edges",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.sort();
    rows
}

/// Given a rule-6 graph.db holding a decision, its governing edge and its assertion that no event
/// of the log folds, when the operator runs `rigger setup`, then the migration is refused, naming
/// each lost fact, and the old file is kept as it stands.
#[test]
fn migration_refuses_when_a_record_would_be_lost() {
    let store = a_rule_6_store(
        &[a_decision_governing_the_tree_file()],
        "INSERT INTO nodes (id, kind, attrs, project)
           VALUES ('d-gone', 'decision', '{\"summary\":\"gone\"}', '{P}');
         INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
           VALUES ('d-gone', 'src/lib.rs', 'GOVERNS', 4, NULL, 4, '{P}', 'extracted');
         INSERT INTO node_assertions
           VALUES ('{P}', '', '', 'd-gone', 'decision', '{\"summary\":\"gone\"}');",
    );
    let err = setup_refuses(store.path());
    let named: Vec<&str> = err
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("d-gone"))
        .collect();
    assert_eq!(
        named,
        vec![
            "node d-gone (decision)",
            "edge d-gone -GOVERNS-> src/lib.rs",
            "assertion of d-gone (decision)",
        ],
        "the refusal names every record fact the refold would lose; stderr: {err}"
    );
}

/// Given a rule-6 graph.db holding a decision governing a file of the tree, recorded as a file
/// with a definition and a grouping beside it, when the operator runs `rigger setup`, then the
/// migration swaps in the records-only graph: the decision and its governing edge onto the file,
/// now a bare artifact, and nothing the derived rows folded.
#[test]
fn migration_keeps_a_decision_governing_a_tree_file() {
    use rigger::contextgraph::sqlite::PROJECTION_VERSION;
    let store = a_rule_6_store(&[a_decision_governing_the_tree_file()], "");
    let out = setup_ok(store.path(), &[]);
    assert_eq!(
        (
            user_version(&rigger_file(store.path(), "graph.db")),
            rendered_graph(store.path())
        ),
        (
            PROJECTION_VERSION,
            vec![
                "edge d-keep -GOVERNS-> src/lib.rs".to_string(),
                "node d-keep decision".to_string(),
                "node src/lib.rs artifact".to_string(),
            ]
        ),
        "the migrated file records the current rule and holds the records' graph alone; \
         stdout: {out}"
    );
}

/// Given a log holding a decision that governs a grouping id, folded into a rule-6 graph.db, when
/// the operator runs `rigger setup`, then the migration is refused, naming the record's edge onto
/// the grouping, and the old file is kept as it stands.
#[test]
fn migration_refuses_when_a_record_names_a_grouping() {
    let store = a_rule_6_store(
        &[ev_at(
            1,
            "DecisionMade",
            decision_json("d-group", "group the parser", &["community/1/0"], ""),
        )],
        "",
    );
    let err = setup_refuses(store.path());
    assert_eq!(
        err.lines()
            .map(str::trim)
            .filter(|l| l.contains("d-group"))
            .collect::<Vec<_>>(),
        vec!["edge d-group -GOVERNS-> community/1/0"],
        "the refusal names the record's edge onto the grouping; stderr: {err}"
    );
}

/// Given a rule-6 graph.db `rigger setup` migrated, when the operator runs `rigger setup` again,
/// then no rebuild is owed or reported and the graph stands as the migration left it.
#[test]
fn migration_is_idempotent() {
    let store = a_rule_6_store(&[a_decision_governing_the_tree_file()], "");
    let graph_db = rigger_file(store.path(), "graph.db");
    let project = run_stream_identity(store.path());
    let first = setup_ok(store.path(), &[]);
    assert!(
        first.contains("rebuilding graph.db"),
        "the first setup migrates; stdout: {first}"
    );
    let migrated = graph_identity(&graph_db, &project);
    let again = setup_ok(store.path(), &[]);
    assert_eq!(
        (
            again.contains("graph.db"),
            graph_identity(&graph_db, &project)
        ),
        (false, migrated),
        "a migrated store owes no migration and keeps its graph; stdout: {again}"
    );
}
