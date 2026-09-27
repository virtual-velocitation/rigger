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
