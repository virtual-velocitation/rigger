//! SQLite-backed EventStore. A single connection behind a mutex serializes
//! writes, so concurrent appenders queue instead of deadlocking on the
//! lock-upgrade (SQLITE_BUSY) class.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use super::{Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position};

const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
CREATE TABLE IF NOT EXISTS events (
  position    INTEGER PRIMARY KEY AUTOINCREMENT,
  stream      TEXT NOT NULL,
  type        TEXT NOT NULL,
  id          TEXT NOT NULL,
  data        BLOB NOT NULL,
  recorded_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream);
";

/// Store is the SQLite-backed EventStore.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open (creating if needed) the store at path. Use ":memory:" in tests.
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }
}

fn be<E: std::fmt::Display>(e: E) -> Error {
    Error::Backend(e.to_string())
}

fn to_nanos(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn from_nanos(n: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_nanos(n.max(0) as u64)
}

fn row_to_event(r: &rusqlite::Row) -> rusqlite::Result<Event> {
    Ok(Event {
        position: r.get::<_, i64>(0)? as Position,
        type_: r.get(1)?,
        id: r.get(2)?,
        data: r.get(3)?,
        recorded_at: from_nanos(r.get(4)?),
    })
}

impl EventStore for Store {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Position, Error> {
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction().map_err(be)?;

        let current: u64 = tx
            .query_row(
                "SELECT COUNT(*) FROM events WHERE stream = ?1",
                [stream],
                |r| r.get(0),
            )
            .map_err(be)?;
        let ok = match expected {
            ExpectedRevision::Any => true,
            ExpectedRevision::NoStream => current == 0,
            ExpectedRevision::Exact(v) => current == v,
        };
        if !ok {
            return Err(Error::Conflict {
                stream: stream.to_string(),
            });
        }

        let mut last: Position = 0;
        for e in events {
            tx.execute(
                "INSERT INTO events (stream, type, id, data, recorded_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![stream, e.type_, e.id, e.data, to_nanos(e.recorded_at)],
            )
            .map_err(be)?;
            last = tx.last_insert_rowid() as Position;
        }
        tx.commit().map_err(be)?;
        Ok(last)
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Position,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        let order = direction_sql(dir);
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT position, type, id, data, recorded_at FROM events
             WHERE stream = ?1 AND position > ?2 ORDER BY position {order}"
        );
        let mut stmt = conn.prepare(&sql).map_err(be)?;
        let rows = stmt
            .query_map(params![stream, from as i64], row_to_event)
            .map_err(be)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(be)
    }

    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        let order = direction_sql(dir);
        let like = filter
            .stream_prefix
            .as_ref()
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "%".to_string());
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT position, type, id, data, recorded_at FROM events
             WHERE position > ?1 AND stream LIKE ?2 ORDER BY position {order}"
        );
        let mut stmt = conn.prepare(&sql).map_err(be)?;
        let rows = stmt
            .query_map(params![from as i64, like], row_to_event)
            .map_err(be)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(be)
    }
}

fn direction_sql(dir: Direction) -> &'static str {
    match dir {
        Direction::Forward => "ASC",
        Direction::Backward => "DESC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_preserves_order() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("A", b"1".to_vec())],
        )
        .unwrap();
        s.append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("B", b"2".to_vec())],
        )
        .unwrap();
        let events = s.read_stream("run", 0, Direction::Forward).unwrap();
        assert_eq!(
            events.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
    }

    #[test]
    fn optimistic_concurrency_conflicts() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run",
            ExpectedRevision::NoStream,
            &[Event::new("A", b"1".to_vec())],
        )
        .unwrap();
        let err = s.append(
            "run",
            ExpectedRevision::NoStream,
            &[Event::new("B", b"2".to_vec())],
        );
        assert!(
            matches!(err, Err(Error::Conflict { .. })),
            "expected a conflict, got {err:?}"
        );
    }

    #[test]
    fn read_all_filters_by_prefix() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run-a",
            ExpectedRevision::Any,
            &[Event::new("X", b"1".to_vec())],
        )
        .unwrap();
        s.append(
            "other",
            ExpectedRevision::Any,
            &[Event::new("Y", b"2".to_vec())],
        )
        .unwrap();
        let filter = Filter {
            stream_prefix: Some("run-".to_string()),
        };
        let events = s.read_all(0, Direction::Forward, &filter).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].type_, "X");
    }
}
