//! Fixtures over the sqlite-backed event log.

use rigger::eventstore::Event;

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
