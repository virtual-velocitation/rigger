//! SQLite file fixtures: raw reads and writes on a store file, through a connection of their
//! own, never the store's.

/// Leave roughly `rows` blobs' worth of reclaimable free pages in the sqlite file `db`: a table
/// filled and dropped releases its pages to the freelist, where they stay until something
/// vacuums the file.
pub fn plant_free_pages(db: &std::path::Path, rows: u64) {
    let conn = rusqlite::Connection::open(db).expect("open the log to plant free pages");
    conn.execute_batch(&format!(
        "CREATE TABLE junk(x BLOB);
         INSERT INTO junk(x)
           WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < {rows})
           SELECT randomblob(600) FROM c;
         DROP TABLE junk;"
    ))
    .expect("plant reclaimable free pages");
}

/// A whole-number `PRAGMA` of the sqlite file `db`, read through a connection of its own so the
/// measurement never depends on the state of the connection the store is using.
pub fn pragma_i64(db: &std::path::Path, pragma: &str) -> i64 {
    rusqlite::Connection::open(db)
        .expect("open the log to read a pragma")
        .query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("read PRAGMA {pragma}: {e}"))
}

/// Bytes the file at `path` occupies on disk, or 0 when it is not there: a `-wal` does not exist
/// before the first write and is deleted on a clean close, and neither absence is an error about
/// the space a store occupies.
pub fn file_len(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// THE ONE PRE-LEDGER ROW INSERTER (spec 107): `events` written at the tail of `stream` in the
/// sqlite store file `db` with raw SQL, as a store recorded them before it refused a derived
/// append. Each row holds its event's type, id, payload, metadata and valid-time, the stream's
/// next revision and one recording time for the batch; the positions sqlite issued are answered
/// in the order given. A store has opened `db` before, so the file holds the schema, and the
/// rows land in one transaction through a connection of the fixture's own, never the store's.
pub fn insert_pre_ledger_rows(
    db: &std::path::Path,
    stream: &str,
    events: &[rigger::eventstore::Event],
) -> Vec<u64> {
    let nanos = |t: std::time::SystemTime| {
        t.duration_since(std::time::UNIX_EPOCH)
            .expect("a time at or after the epoch")
            .as_nanos() as i64
    };
    let mut conn = rusqlite::Connection::open(db).expect("open the log to insert pre-ledger rows");
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .expect("wait for a writer rather than fail on its lock");
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .expect("take the write lock for the pre-ledger rows");
    let last: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(revision), -1) FROM events WHERE stream = ?1",
            [stream],
            |r| r.get(0),
        )
        .expect("read the stream's last revision");
    let recorded_at = nanos(std::time::SystemTime::now());
    let positions = (last + 1..)
        .zip(events)
        .map(|(revision, e)| {
            tx.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    stream,
                    e.type_,
                    e.id,
                    e.data,
                    serde_json::to_string(&e.meta).expect("metadata is a map of strings"),
                    nanos(e.valid_from),
                    recorded_at,
                    revision
                ],
            )
            .expect("insert a pre-ledger row");
            tx.last_insert_rowid() as u64
        })
        .collect();
    tx.commit().expect("commit the pre-ledger rows");
    positions
}

/// A STORE AS A BINARY BEFORE THE LEDGER WROTE IT (spec 107): the sqlite store `inner` over the
/// file `db`, whose every append lands through [`insert_pre_ledger_rows`] and so records a
/// derived event the store itself refuses; every read is `inner`'s own. It appends
/// unconditionally, so it refuses any expectation but [`ExpectedRevision::Any`]. A seeder that
/// records through a store - a namespace, the append-and-fold authority - records the pre-ledger
/// rows of an older store through this one.
///
/// [`ExpectedRevision::Any`]: rigger::eventstore::ExpectedRevision::Any
pub struct PreLedgerStore<'a> {
    pub db: &'a std::path::Path,
    pub inner: &'a dyn rigger::eventstore::EventStore,
}

impl rigger::eventstore::EventStore for PreLedgerStore<'_> {
    fn append(
        &self,
        stream: &str,
        expected: rigger::eventstore::ExpectedRevision,
        events: &[rigger::eventstore::Event],
    ) -> Result<rigger::eventstore::Appended, rigger::eventstore::Error> {
        assert!(
            matches!(expected, rigger::eventstore::ExpectedRevision::Any),
            "pre-ledger rows are inserted unconditionally, never under {expected:?}"
        );
        Ok(rigger::eventstore::Appended::all(insert_pre_ledger_rows(
            self.db, stream, events,
        )))
    }
    crate::delegate_event_store_reads!();
}
