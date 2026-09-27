//! THE SQLITE OPENER: the one place a store connection is opened.
//!
//! Every SQLite store this crate keeps (the event log, the graph projection) opens its file
//! here, so the connection settings they all rely on are applied once: write-ahead logging, so
//! readers never block the one writer, and a busy timeout, so a writer that meets a lock queues
//! instead of failing at once. Each store then applies its own schema to the connection.

use rusqlite::Connection;

/// Open (creating if needed) the SQLite file at `path` (`":memory:"` in tests) with the settings
/// every store connection shares.
pub fn open_connection(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_connection_uses_write_ahead_logging_and_waits_on_a_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.db");
        let conn = open_connection(path.to_str().unwrap()).unwrap();
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        let timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!((journal.as_str(), timeout), ("wal", 5000));
    }
}
