//! Fixtures over the sqlite-backed stores: the graph projector and the event log.

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Projection, TYPE_EDGE_INFERRED};
use rigger::eventstore::Event;

/// Fold into `p`, at log position `pos`, a caller-less reference from `file` to `name`.
pub fn apply_ref(p: &Projector, pos: u64, file: &str, name: &str) {
    let payload = serde_json::json!({ "file": file, "name": name, "lang": "rust" });
    let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Every event on `store`'s run stream, oldest first.
pub fn run_log(store: &rigger::eventstore::sqlite::Store) -> Vec<Event> {
    use rigger::eventstore::EventStore;
    store
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
}
