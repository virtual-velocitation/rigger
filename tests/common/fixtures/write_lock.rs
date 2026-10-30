//! The write-lock fixture: another writer holding a sqlite file's write lock while a test acts,
//! shared by the root suites and the graph crate's inline tests.

/// What sqlite answers a write that meets another writer's lock once its busy timeout has passed.
pub const DATABASE_IS_LOCKED: &str = "database is locked";

/// Hold the sqlite file `graph_db` under another writer's write lock, taken through a connection
/// of its own, while `run` runs - past every busy timeout a connection waiting on it spends - and
/// hand back what `run` returned once the lock is released.
pub fn with_graph_locked<T>(graph_db: &std::path::Path, run: impl FnOnce() -> T) -> T {
    let holder = rusqlite::Connection::open(graph_db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let out = run();
    holder.execute_batch("ROLLBACK").unwrap();
    out
}

/// How long [`with_graph_locked_for`] holds the write lock: past the busy timeout every store
/// connection waits on a lock (`open_connection`), and well inside the appender's ceiling.
pub const HELD_PAST_THE_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(7);

/// How long a test holds a write lock that a writer meeting it waits out inside one busy timeout
/// of its store connection (`open_connection`): shorter than that timeout, so the writer's open,
/// made after the hold began, takes the lock once it is released instead of failing contended.
pub const HELD_WITHIN_THE_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Run `run` on a thread of its own while another writer holds the write lock of the sqlite file
/// `graph_db` ([`with_graph_locked`]) for [`HELD_PAST_THE_BUSY_TIMEOUT`] from before `run` starts,
/// and hand back what `run` returned once it ends: a writer whose wait outlasts one busy timeout
/// meets the lock released.
pub fn with_graph_locked_for<T: Send>(
    graph_db: &std::path::Path,
    run: impl FnOnce() -> T + Send,
) -> T {
    std::thread::scope(|scope| {
        let running = with_graph_locked(graph_db, || {
            let running = scope.spawn(run);
            std::thread::sleep(HELD_PAST_THE_BUSY_TIMEOUT);
            running
        });
        running.join().unwrap()
    })
}
