//! The catch-up cursor fixtures (spec 124, THE CURSOR AND ITS SEEDING and HORIZON IN TESTS): where
//! a graph file's `caught_up` cursor stands, a file without it as one folded before the cursor
//! existed, the seeding `rigger setup` runs before it reads why a file owes its rebuild, that
//! seeding holding the file's write lock while a test acts, and events older than the catch-up
//! horizon - shared by the root suites and the graph crate's inline tests.

use rigger::contextgraph::sqlite::{
    PositionSource, Projector, Seeding, StampLookup, CATCH_UP_HORIZON,
};
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

/// What `rigger setup` reads of why the graph file behind `p` owes its rebuild: its cursor seeded
/// now against the positions `live` streams, each missing position's stamp read through
/// `stamp_of`, handed `head` as the log's head, then the causes the file itself records
/// ([`Projector::owed_because`]) beside the seeding's verdict, whose lost fold setup names as a
/// cause of its own.
pub fn owed_once_seeded(
    p: &Projector,
    live: &mut PositionSource,
    stamp_of: &mut StampLookup,
    head: Option<u64>,
) -> (Vec<&'static str>, Seeding) {
    let seeding = p
        .seed_cursor(live, stamp_of, head, SystemTime::now())
        .unwrap();
    (p.owed_because().unwrap(), seeding)
}

/// Whether `rigger setup` rebuilds the graph file on what [`owed_once_seeded`] read of it: a cause
/// the file records, or the lost fold its seeding found.
pub fn setup_owes((causes, seeding): &(Vec<&'static str>, Seeding)) -> bool {
    !causes.is_empty() || *seeding == Seeding::Ran { lost: true }
}

/// Run `run` while the seeding `rigger setup` runs of the cursor of the graph file `open` opens
/// holds that file's write lock: the seeding's source streams the positions of `log`, then waits
/// `held` inside the seeding's transaction, `run` starting once they are streamed, each missing
/// position's stamp read from `log` ([`stamp_in`]) and its last position handed as the log's head.
/// The seeding must run and find nothing lost; hand back what `run` returned.
pub fn under_a_seedings_lock<T>(
    open: impl FnOnce() -> Projector + Send,
    log: &[Event],
    held: Duration,
    run: impl FnOnce() -> T,
) -> T {
    let (seeding, started) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let seeder = scope.spawn(move || {
            let positions: Vec<u64> = log.iter().map(|e| e.position).collect();
            open().seed_cursor(
                &mut |sink| {
                    sink(&positions)?;
                    seeding.send(()).unwrap();
                    std::thread::sleep(held);
                    Ok(())
                },
                &mut |position| stamp_in(log, position),
                positions.last().copied(),
                SystemTime::now(),
            )
        });
        started.recv().unwrap();
        let ran = run();
        assert_eq!(
            seeder.join().unwrap().unwrap(),
            Seeding::Ran { lost: false },
            "the seeding ran under its lock and found nothing lost"
        );
        ran
    })
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

/// The line `rigger setup` prints once its catch-up of `graph.db` reached the log's head with `n`
/// positions recorded by its calls (spec 124, CATCH-UP LINES).
pub fn caught_up_line(n: usize) -> String {
    format!("caught graph.db up with the event log: folded {n} event(s) it missed")
}

/// The line `rigger setup` prints, exiting zero, when another writer stopped its catch-up at the
/// position `at` its last call read from (spec 124, CATCH-UP LINES).
pub fn catch_up_skipped_line(at: u64) -> String {
    catch_up_stopped(
        at,
        "another writer holds the write lock or advanced the cursor past this call's read - the \
         next fold through the folding store or the next rigger setup continues it",
    )
}

/// The line `rigger setup` fails with when its catch-up stopped at the position `at` its last call
/// read, or would read, its page from, on `error` (spec 124, CATCH-UP LINES).
pub fn catch_up_failed_line(at: u64, error: &str) -> String {
    catch_up_stopped(
        at,
        &format!("{error} - rerun rigger setup once the cause is fixed"),
    )
}

/// The head both catch-up stop lines share: where the catch-up stopped, and why.
fn catch_up_stopped(at: u64, why: &str) -> String {
    format!("graph.db catch-up stopped at position {at}: {why}")
}
