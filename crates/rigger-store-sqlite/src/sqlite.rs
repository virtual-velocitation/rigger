//! THE SQLITE OPENER: the one place a store connection is opened.
//!
//! Every SQLite store this crate keeps (the event log, the graph projection) opens its file
//! here, so the connection settings they all rely on are applied once: write-ahead logging, so
//! readers never block the one writer, and a busy timeout, so a writer that meets a lock queues
//! instead of failing at once. Each store then applies its own schema to the connection. The
//! truncating checkpoint every store ends a whole-file rewrite with is here too.

use rusqlite::Connection;

/// Open (creating if needed) the SQLite file at `path` (`":memory:"` in tests) with the settings
/// every store connection shares.
pub fn open_connection(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}

/// Fold the write-ahead log of `conn`'s file back into the file and truncate the log to zero
/// bytes, reporting whether it did: false when a reader still held a snapshot of the log after
/// every ask, so its frames stay in the log until a later truncation.
///
/// `PRAGMA wal_checkpoint(TRUNCATE)` waits on the connection's busy handler and still declines
/// while a reader holds a snapshot, its first column then 1; a blocked checkpoint is transient,
/// so it is asked a bounded number of times ([`TRUNCATE_ATTEMPTS`]), never waited out.
pub fn truncate_log(conn: &Connection) -> rusqlite::Result<bool> {
    for attempt in 0..TRUNCATE_ATTEMPTS {
        let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
        if busy == 0 {
            return Ok(true);
        }
        if attempt + 1 < TRUNCATE_ATTEMPTS {
            std::thread::sleep(TRUNCATE_BACKOFF);
        }
    }
    Ok(false)
}

/// How many times [`truncate_log`] asks a blocked checkpoint again, and how long it waits between
/// asks: bounded and short, since what is at stake is only when the frames leave the log.
const TRUNCATE_ATTEMPTS: u32 = 5;
const TRUNCATE_BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);

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

    /// The truncation empties the write-ahead log beside an idle connection, and declines,
    /// saying so, while a reader holds a snapshot of the log.
    #[test]
    fn the_log_is_truncated_unless_a_reader_holds_a_snapshot_of_it() {
        for reading in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("store.db");
            let path = path.to_str().unwrap();
            let conn = open_connection(path).unwrap();
            conn.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);")
                .unwrap();
            let reader = open_connection(path).unwrap();
            if reading {
                reader
                    .execute_batch("BEGIN; SELECT count(*) FROM t;")
                    .unwrap();
            }
            conn.execute_batch("INSERT INTO t VALUES (2); PRAGMA busy_timeout = 0;")
                .unwrap();
            let truncated = truncate_log(&conn).unwrap();
            let wal = std::fs::metadata(format!("{path}-wal")).map_or(0, |m| m.len());
            assert_eq!(
                (truncated, wal == 0),
                (!reading, !reading),
                "reading {reading}: truncated, and the log empty, exactly when no reader holds it"
            );
        }
    }
}
