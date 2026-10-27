//! The catch-up cursor fixtures (spec 124, THE CURSOR AND ITS SEEDING and HORIZON IN TESTS): where
//! a graph file's `caught_up` cursor stands, a file without it as one folded before the cursor
//! existed, the seeding `rigger setup` runs before it reads why a file owes its rebuild, and events
//! older than the catch-up horizon - shared by the root suites and the graph crate's inline tests.

use rigger::contextgraph::sqlite::{PositionSource, Projector, StampLookup, CATCH_UP_HORIZON};
use rigger::eventstore::Event;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// How long before its own stamp a backdated event is recorded: past the catch-up horizon, so a
/// seeding reads every hole among backdated events as old.
pub fn past_the_horizon() -> Duration {
    CATCH_UP_HORIZON + Duration::from_secs(1)
}

/// `log`, each event's store stamp replaced by what `stamp` answers for it.
pub fn restamped(log: &[Event], stamp: impl Fn(SystemTime) -> SystemTime) -> Vec<Event> {
    log.iter()
        .cloned()
        .map(|mut e| {
            e.recorded_at = stamp(e.recorded_at);
            e
        })
        .collect()
}

/// `log`, each event recorded [`past_the_horizon`] earlier than it was ([`restamped`]): the events
/// an in-crate source hands when a hole among them must read as old.
pub fn backdated(log: &[Event]) -> Vec<Event> {
    restamped(log, |at| at - past_the_horizon())
}

/// The store stamp of the event of `log` at `position`, or None when `log` holds none there: the
/// point lookup a seeding reads a missing position's age through, over a log read into memory.
pub fn stamp_in(
    log: &[Event],
    position: u64,
) -> Result<Option<SystemTime>, rigger::contextgraph::Error> {
    Ok(log
        .iter()
        .find(|e| e.position == position)
        .map(|e| e.recorded_at))
}

/// Why the graph file behind `p` owes its rebuild as `rigger setup` reads it: its cursor seeded
/// now against the positions `live` streams, each missing position's stamp read through
/// `stamp_of`, handed `head` as the log's head, then the causes the file carries joined by the
/// seeding's verdict.
pub fn owed_once_seeded(
    p: &Projector,
    live: &mut PositionSource,
    stamp_of: &mut StampLookup,
    head: Option<u64>,
) -> Vec<&'static str> {
    let seeding = p
        .seed_cursor(live, stamp_of, head, SystemTime::now())
        .unwrap();
    p.owed_against(&seeding).unwrap()
}

/// Where the `caught_up` cursor of the graph file at `graph_db` stands, read through a connection
/// of its own: None on a file without the table.
pub fn caught_up_at(graph_db: &Path) -> Option<u64> {
    let conn = rusqlite::Connection::open(graph_db).unwrap();
    match conn.query_row("SELECT position FROM caught_up", [], |r| r.get::<_, i64>(0)) {
        Ok(position) => Some(position as u64),
        Err(e) if e.to_string() == "no such table: caught_up" => None,
        Err(e) => panic!("read the caught_up cursor: {e}"),
    }
}

/// Move the `caught_up` cursor of the graph file at `graph_db` to `to`, through a connection of its
/// own and around the cursor's setter: another writer's catch-up, or a rebuild's swap, placing it
/// there.
pub fn move_the_cursor(graph_db: &Path, to: u64) {
    rusqlite::Connection::open(graph_db)
        .unwrap()
        .execute("UPDATE caught_up SET position = ?1", [to as i64])
        .unwrap();
}

/// Drop the `caught_up` cursor of the graph file at `graph_db`, through a connection of its own:
/// the file as a fold made it before the cursor existed, or as an older binary's swap left it.
pub fn drop_the_cursor(graph_db: &Path) {
    rusqlite::Connection::open(graph_db)
        .unwrap()
        .execute_batch("DROP TABLE caught_up")
        .unwrap();
}
