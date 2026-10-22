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
