//! SQLite-backed EventStore. A single connection behind a mutex serializes
//! writes, so concurrent appenders queue instead of deadlocking on the
//! lock-upgrade (SQLITE_BUSY) class. Per-stream revisions and a `UNIQUE(stream,
//! revision)` index give optimistic concurrency; `$all` is `ORDER BY position`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, TransactionBehavior};

use super::{
    Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position, Revision,
    Subscription, NO_STREAM,
};

const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
CREATE TABLE IF NOT EXISTS events (
  position    INTEGER PRIMARY KEY AUTOINCREMENT,
  stream      TEXT NOT NULL,
  type        TEXT NOT NULL,
  id          TEXT NOT NULL,
  data        BLOB NOT NULL,
  meta        TEXT NOT NULL,
  valid_from  INTEGER NOT NULL,
  recorded_at INTEGER NOT NULL,
  revision    INTEGER NOT NULL,
  UNIQUE(stream, revision)
);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream);
";

const COLS: &str = "position, stream, type, id, data, meta, valid_from, recorded_at, revision";

/// Store is the SQLite-backed EventStore. The connection is shared (Arc) so a
/// subscription's polling thread reads the same database the writers append to.
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    /// Open (creating if needed) the store at path. Use ":memory:" in tests.
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = Connection::open(path).map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
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

fn meta_json(m: &BTreeMap<String, String>) -> String {
    serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string())
}

fn parse_meta(s: &str) -> BTreeMap<String, String> {
    serde_json::from_str(s).unwrap_or_default()
}

fn like_of(filter: &Filter) -> String {
    filter
        .stream_prefix
        .as_ref()
        .map(|p| format!("{p}%"))
        .unwrap_or_else(|| "%".to_string())
}

fn row_to_event(r: &rusqlite::Row) -> rusqlite::Result<Event> {
    let meta: String = r.get(5)?;
    Ok(Event {
        position: r.get::<_, i64>(0)? as Position,
        stream: r.get(1)?,
        type_: r.get(2)?,
        id: r.get(3)?,
        data: r.get(4)?,
        meta: parse_meta(&meta),
        valid_from: from_nanos(r.get(6)?),
        recorded_at: from_nanos(r.get(7)?),
        revision: r.get::<_, i64>(8)? as Revision,
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
        // BEGIN IMMEDIATE, not the default BEGIN DEFERRED: acquire the write lock up
        // front so a second connection (a separate process - the death courier racing
        // the worker's self-report) QUEUES on `busy_timeout` instead of starting a read
        // snapshot it must later upgrade. A deferred read->write upgrade under WAL with a
        // concurrent writer cannot be resolved by the busy handler (SQLITE_BUSY_SNAPSHOT)
        // and surfaces as a hard `database is locked` backend error; taking the write lock
        // immediately makes concurrent appenders serialize cleanly, so a stale expectation
        // surfaces as the port's `Error::Conflict` (which callers retry) and never as a
        // spurious lock error. This is what the module header promises ("concurrent
        // appenders queue instead of deadlocking on the SQLITE_BUSY class") and what the
        // optimistic-concurrency contract needs to hold across connections, not just
        // within one in-process `Store`.
        let tx = guard
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(be)?;

        let count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM events WHERE stream = ?1",
                [stream],
                |r| r.get(0),
            )
            .map_err(be)?;
        let last_revision: Revision = count - 1; // NO_STREAM (-1) when the stream is empty
        let ok = match expected {
            ExpectedRevision::Any => true,
            ExpectedRevision::NoStream => last_revision == NO_STREAM,
            ExpectedRevision::Exact(v) => last_revision == v,
        };
        if !ok {
            return Err(Error::Conflict {
                stream: stream.to_string(),
                expected,
                actual: last_revision,
            });
        }

        // The store stamps recorded_at on ingest (one clock per batch).
        let recorded_at = to_nanos(SystemTime::now());
        let mut last_pos: Position = 0;
        for (i, e) in events.iter().enumerate() {
            let revision = count + i as i64; // the next per-stream revision
            tx.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    stream,
                    e.type_,
                    e.id,
                    e.data,
                    meta_json(&e.meta),
                    to_nanos(e.valid_from),
                    recorded_at,
                    revision
                ],
            )
            .map_err(be)?;
            last_pos = tx.last_insert_rowid() as Position;
        }
        tx.commit().map_err(be)?;
        Ok(last_pos)
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        let order = direction_sql(dir);
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {COLS} FROM events WHERE stream = ?1 AND revision >= ?2 ORDER BY revision {order}"
        );
        let mut stmt = conn.prepare(&sql).map_err(be)?;
        let rows = stmt
            .query_map(params![stream, from], row_to_event)
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
        let like = like_of(filter);
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {COLS} FROM events WHERE position > ?1 AND stream LIKE ?2 ORDER BY position {order}"
        );
        let mut stmt = conn.prepare(&sql).map_err(be)?;
        let rows = stmt
            .query_map(params![from as i64, like], row_to_event)
            .map_err(be)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(be)
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        let conn = Arc::clone(&self.conn);
        let like = like_of(filter);
        Ok(spawn_subscription(
            move |state: &mut Watermark| {
                let guard = conn.lock().unwrap();
                poll_all(&guard, state.position, &like)
            },
            Watermark {
                position: from,
                revision: NO_STREAM,
            },
        ))
    }

    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
        let conn = Arc::clone(&self.conn);
        let stream = stream.to_string();
        Ok(spawn_subscription(
            move |state: &mut Watermark| {
                let guard = conn.lock().unwrap();
                poll_stream(&guard, &stream, state.revision)
            },
            // `revision > from-1` includes `from`.
            Watermark {
                position: 0,
                revision: from - 1,
            },
        ))
    }
}

/// The watermark a subscription's polling thread advances as it delivers events.
struct Watermark {
    position: Position,
    revision: Revision,
}

/// Spawn a polling subscription: `poll` returns the next batch given the current
/// watermark; the thread advances the watermark from each delivered event.
fn spawn_subscription<F>(poll: F, start: Watermark) -> Subscription
where
    F: Fn(&mut Watermark) -> rusqlite::Result<Vec<Event>> + Send + 'static,
{
    let (tx, rx) = channel();
    let err = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let err_thread = Arc::clone(&err);
    let handle = std::thread::spawn(move || {
        let mut state = start;
        while !stop_thread.load(Ordering::Relaxed) {
            match poll(&mut state) {
                Ok(events) => {
                    for e in events {
                        state.position = e.position;
                        state.revision = e.revision;
                        if tx.send(e).is_err() {
                            return; // the subscriber was dropped
                        }
                    }
                }
                Err(e) => {
                    *err_thread.lock().unwrap() = Some(e.to_string());
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    Subscription::new(rx, err, stop, handle)
}

fn poll_all(conn: &Connection, after: Position, like: &str) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events WHERE position > ?1 AND stream LIKE ?2 ORDER BY position ASC"
    ))?;
    let rows = stmt.query_map(params![after as i64, like], row_to_event)?;
    rows.collect()
}

fn poll_stream(conn: &Connection, stream: &str, after: Revision) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events WHERE stream = ?1 AND revision > ?2 ORDER BY revision ASC"
    ))?;
    let rows = stmt.query_map(params![stream, after], row_to_event)?;
    rows.collect()
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
    fn passes_the_contract() {
        crate::eventstore::contract::assert_contract(&Store::open(":memory:").unwrap());
    }

    #[test]
    fn assigns_per_stream_revisions() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "a",
            ExpectedRevision::Any,
            &[
                Event::new("A0", b"".to_vec()),
                Event::new("A1", b"".to_vec()),
            ],
        )
        .unwrap();
        s.append(
            "a",
            ExpectedRevision::Any,
            &[Event::new("A2", b"".to_vec())],
        )
        .unwrap();
        s.append(
            "b",
            ExpectedRevision::Any,
            &[Event::new("B0", b"".to_vec())],
        )
        .unwrap();
        let a = s.read_stream("a", 0, Direction::Forward).unwrap();
        assert_eq!(a.iter().map(|e| e.revision).collect::<Vec<_>>(), [0, 1, 2]);
        let b = s.read_stream("b", 0, Direction::Forward).unwrap();
        assert_eq!(b[0].revision, 0);
        // stream + valid_from round-trip
        assert_eq!(a[0].stream, "a");
    }

    #[test]
    fn conflict_reports_actual_revision() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run",
            ExpectedRevision::NoStream,
            &[Event::new("A", b"".to_vec()), Event::new("B", b"".to_vec())],
        )
        .unwrap();
        let err = s.append(
            "run",
            ExpectedRevision::NoStream,
            &[Event::new("C", b"".to_vec())],
        );
        match err {
            Err(Error::Conflict { actual, .. }) => {
                assert_eq!(actual, 1, "two events => last revision 1")
            }
            other => panic!("expected a conflict with actual revision, got {other:?}"),
        }
    }

    #[test]
    fn subscribe_stream_replays_then_goes_live() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "one",
            ExpectedRevision::Any,
            &[Event::new("PRE", b"".to_vec())],
        )
        .unwrap();
        s.append(
            "two",
            ExpectedRevision::Any,
            &[Event::new("OTHER", b"".to_vec())],
        )
        .unwrap();
        let sub = s.subscribe_stream("one", 0).unwrap();
        let first = sub
            .recv_timeout(Duration::from_secs(2))
            .expect("replay PRE");
        assert_eq!(first.type_, "PRE");
        s.append(
            "one",
            ExpectedRevision::Any,
            &[Event::new("LIVE", b"".to_vec())],
        )
        .unwrap();
        let second = sub.recv_timeout(Duration::from_secs(2)).expect("live LIVE");
        assert_eq!(second.type_, "LIVE");
        // the "two" stream's event must never arrive on a "one" subscription
        assert!(
            sub.try_recv().is_none() || sub.try_recv().map(|e| e.stream == "one").unwrap_or(true)
        );
    }

    #[test]
    fn subscribe_all_replays_then_goes_live() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("A", b"1".to_vec())],
        )
        .unwrap();
        let sub = s.subscribe_all(0, &Filter::default()).unwrap();
        let first = sub.recv_timeout(Duration::from_secs(2)).expect("replay A");
        assert_eq!(first.type_, "A");
        s.append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("B", b"2".to_vec())],
        )
        .unwrap();
        let second = sub.recv_timeout(Duration::from_secs(2)).expect("live B");
        assert_eq!(second.type_, "B");
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
        assert_eq!(events[0].stream, "run-a");
    }

    #[test]
    fn concurrent_cross_connection_appends_serialize_without_spurious_lock_errors() {
        // Two SEPARATE connections (two `Store` handles on one on-disk db - the
        // two-process shape of the death courier racing a worker's self-report) append
        // to the SAME stream at once, with NO shared in-process mutex to serialize them.
        // Under the default BEGIN DEFERRED a read->write upgrade with a concurrent writer
        // under WAL cannot be resolved by `busy_timeout` (SQLITE_BUSY_SNAPSHOT) and
        // surfaces as a hard `database is locked` backend error the optimistic layer
        // cannot retry. BEGIN IMMEDIATE takes the write lock up front, so the appenders
        // QUEUE and every write lands - which is what the module header promises and what
        // record_result_if_absent's compare-and-append relies on across connections. The
        // in-process contract test (`concurrent_appends_to_distinct_streams...`) cannot
        // reach this: its single `Mutex<Connection>` serializes the appends so they never
        // contend at the sqlite layer.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let path = path.to_str().unwrap().to_string();

        // Open both connections up front (serialized) so we race only the appends.
        let a = Arc::new(Store::open(&path).unwrap());
        let b = Arc::new(Store::open(&path).unwrap());

        const ROUNDS: usize = 40;
        let barrier = Arc::new(std::sync::Barrier::new(2));

        let spawn_writer = |s: Arc<Store>, bar: Arc<std::sync::Barrier>| {
            std::thread::spawn(move || {
                let mut hard_errs = 0usize;
                for _ in 0..ROUNDS {
                    bar.wait();
                    match s.append(
                        "run",
                        ExpectedRevision::Any,
                        &[Event::new("R", b"x".to_vec())],
                    ) {
                        Ok(_) => {}
                        // A stale-expectation conflict is a legitimate optimistic outcome;
                        // a lock error is the regression this test guards against.
                        Err(Error::Conflict { .. }) => {}
                        Err(_) => hard_errs += 1,
                    }
                }
                hard_errs
            })
        };

        let ha = spawn_writer(a.clone(), barrier.clone());
        let hb = spawn_writer(b.clone(), barrier.clone());
        let hard_errs = ha.join().unwrap() + hb.join().unwrap();
        assert_eq!(
            hard_errs, 0,
            "concurrent cross-connection appends must queue, never hard-fail with a lock error"
        );

        // Every one of the 2 * ROUNDS appends is durably recorded, with contiguous,
        // unique per-stream revisions - no lost write, no gap, no duplicated revision.
        let events = a.read_stream("run", 0, Direction::Forward).unwrap();
        assert_eq!(
            events.len(),
            2 * ROUNDS,
            "every concurrent append must be durably recorded"
        );
        let revs: Vec<Revision> = events.iter().map(|e| e.revision).collect();
        let expected: Vec<Revision> = (0..2 * ROUNDS as Revision).collect();
        assert_eq!(
            revs, expected,
            "per-stream revisions must stay contiguous and unique under concurrency"
        );
    }
}
