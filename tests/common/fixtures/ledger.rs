//! Rebuild fixtures: a cold rebuild of a graph file from a log, and a graph file left owing its
//! rebuild - the one definition the root suites and the conductor's own tests share.

use rigger::eventstore::Event;

/// A COLD REBUILD OF `log`: rebuild the graph file `graph_db` under the project `test`, owed, from
/// `log` in batches of at most `batch` events, each one committed transaction. It insists the
/// rebuild ran.
pub fn rebuild_from_the_log(graph_db: &std::path::Path, log: &[Event], batch: usize) {
    use rigger::contextgraph::sqlite::{stream_past, Projector, RebuildSink};

    let rebuilt = Projector::open(graph_db.to_str().unwrap(), "test")
        .unwrap()
        .rebuild(
            &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
            true,
            &mut |after, sink: &mut RebuildSink| stream_past(log, after, batch, sink),
            &mut |_| {},
        )
        .unwrap();
    assert!(rebuilt.is_some(), "premise: the rebuild ran");
}

/// LEAVE `graph`, open on the graph file `graph_db`, OWING ITS REBUILD by an older fold rule: it
/// folds one decision, then the file records the rule before the current one, so every fold after
/// it is refused for that debt. The rule is stamped through the store's own opener, the one sqlite
/// connection every crate that mounts this fixture reaches.
pub fn owe_a_rebuild(graph: &dyn rigger::contextgraph::RecordFold, graph_db: &std::path::Path) {
    let decision = super::event_of("DecisionMade", super::decision_json("d-owed", "s", &[], ""));
    super::folds(graph, &[super::placed_at(decision, 1, 1)]);
    rigger_store_sqlite::sqlite::open_connection(graph_db.to_str().unwrap())
        .unwrap()
        .pragma_update(
            None,
            "user_version",
            rigger::contextgraph::sqlite::PROJECTION_VERSION - 1,
        )
        .unwrap();
    assert!(graph.rebuild_owed().unwrap());
}
