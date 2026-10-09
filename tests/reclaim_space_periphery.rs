//! PERIPHERY (contract / API / integration) tests for spec 107, criterion 14 - RECLAMATION
//! STAGES IN MEMORY - driven from OUTSIDE the store crate, through the surface a caller of
//! `rigger::eventstore::sqlite` is handed: `Store::bytes_on_disk`, `Store::reclaim_space` and the
//! `Reclamation` it answers with.
//!
//! The store's own tests pin each arm of the reclamation from inside, with the connection in
//! hand and the compacting step injected. This file covers what that view cannot reach:
//!
//!   1. **The caller's protocol.** A caller measures the log, does its own write, and only then
//!      asks for the reclamation; the figure it is handed back spans the whole of that, so it is
//!      the difference of the two sizes the caller itself can read either side of the command.
//!   2. **The report as a value a caller outside the crate builds.** Every arm is compared
//!      against a `Reclamation` spelled here, field by field, which is what a consumer rendering
//!      one in a test has to be able to do.
//!   3. **A REAL failing step, and its rerun.** A second connection holding the write lock makes
//!      the engine itself refuse the rewrite: the failure arrives as a named field beside an
//!      unmeasured figure, the free pages stay in the file, and the same call made again once the
//!      lock is gone reclaims them.
//!   4. **Where the rewrite stages its copy, seen from the filesystem.** The engine is pointed at
//!      a temporary directory of this test's own; a rewrite staged in a file writes there and a
//!      rewrite staged in memory does not. The same log rewritten through a connection left on
//!      the engine's default is the positive control that the directory is being watched at all.
//!      The same watch covers the reclamation `prune_derived_index` calls; that block goes with
//!      the function when criterion 16 deletes it, and the rest of this file stands.
//!   5. **What the rewrite takes on the log's own partition.** The shipped guidance tells an
//!      operator the rewritten file passes through the write-ahead log beside the log, so that
//!      partition needs about the compacted size free. A reader parked on the log until the
//!      rewrite has committed holds that write-ahead log where it can be measured.
//!
//! Every test here takes [`serial`]: item 4 points the whole process's engine at one directory,
//! so nothing else in this binary may stage a temporary file while it watches.

mod common;

use common::fixtures::{file_len, plant_free_pages, pragma_i64};
use rigger::eventstore::sqlite::{Reclamation, Store};
use rigger::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());

/// One test of this binary at a time. A test that failed while holding the guard must not fail
/// the ones after it for that reason, so a poisoned lock is taken all the same.
fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the store carries into [`Reclamation::compaction_error`] when the write lock is still
/// held at the end of the engine's busy timeout.
fn write_lock_held() -> String {
    Error::Backend("database is locked".into()).to_string()
}

// ---------------------------------------------------------------------------------------
// 1. The caller's protocol: measure, write, reclaim
// ---------------------------------------------------------------------------------------

#[test]
fn a_caller_is_told_what_the_log_lost_since_the_size_it_measured_before_its_own_write() {
    let _serial = serial();
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let wal = dir.path().join("events.db-wal");
    let store = Store::open(db.to_str().unwrap()).unwrap();
    plant_free_pages(&db, 3_000);
    // The free pages are folded into the main file and the write-ahead log emptied, so the size
    // the caller measures is the main file alone and every byte written after it is visible.
    assert_eq!(pragma_i64(&db, "wal_checkpoint(TRUNCATE)"), 0);
    let pages_before = pragma_i64(&db, "page_count");
    let main_before = file_len(&db);
    assert_eq!(file_len(&wal), 0);

    let before = store.bytes_on_disk();
    assert_eq!(
        before,
        Some(main_before),
        "with an empty write-ahead log the log occupies exactly its main file"
    );
    let before = before.unwrap();

    // THE CALLER'S OWN WRITE, after its measurement and before the reclamation.
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("RunStarted", vec![b'x'; 64 * 1024])],
        )
        .expect("the caller's own write");
    let wal_at_the_call = file_len(&wal);
    assert!(
        wal_at_the_call >= 64 * 1024,
        "the write must have landed in the write-ahead log, or the size at the call equals the \
         size handed in and the two cannot be told apart; it holds {wal_at_the_call} byte(s)"
    );
    assert_eq!(
        store.bytes_on_disk(),
        Some(main_before + wal_at_the_call),
        "the log's size is its main file PLUS its write-ahead log"
    );

    let reclaimed = store.reclaim_space(Some(before));

    let main_after = file_len(&db);
    assert_eq!(
        file_len(&wal),
        0,
        "a reclamation that landed folded the write-ahead log back and truncated it"
    );
    assert_eq!(store.bytes_on_disk(), Some(main_after));
    assert!(
        main_after < main_before,
        "the main file must be rewritten smaller: {main_before} before, {main_after} after"
    );
    assert_eq!(
        reclaimed,
        Reclamation {
            reclaimed_bytes: Some(main_before - main_after),
            compaction_ran: true,
            on_disk_measured: true,
            compaction_error: None,
        },
        "the figure is the size the caller measured BEFORE its write less the size it reads \
         now, never a difference taken from the size at the call ({} byte(s) larger)",
        wal_at_the_call
    );
    assert_eq!(pragma_i64(&db, "freelist_count"), 0);
    assert!(pragma_i64(&db, "page_count") < pages_before);
    assert_eq!(
        store
            .read_stream("run", 0, Direction::Forward)
            .expect("read the caller's write back")
            .len(),
        1,
        "and the caller's write survives the rewrite"
    );
}

// ---------------------------------------------------------------------------------------
// 2. The arms a caller reads off the report: no before-size, and nothing to reclaim
// ---------------------------------------------------------------------------------------

#[test]
fn a_caller_with_no_size_to_hand_in_is_told_unmeasured_and_a_settled_file_is_left_as_found() {
    let _serial = serial();
    assert_eq!(
        Reclamation::default(),
        Reclamation {
            reclaimed_bytes: None,
            compaction_ran: false,
            on_disk_measured: false,
            compaction_error: None,
        },
        "the report of a reclamation that never ran claims nothing"
    );
    assert_eq!(
        Store::open(":memory:").unwrap().bytes_on_disk(),
        None,
        "a store with no file behind it has no size on disk to hand in"
    );

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let wal = dir.path().join("events.db-wal");
    let store = Store::open(db.to_str().unwrap()).unwrap();

    // A SETTLED FILE: nothing free in it, so nothing is rewritten, whatever was handed in.
    assert_eq!(pragma_i64(&db, "freelist_count"), 0);
    let settled = store
        .bytes_on_disk()
        .expect("a file-backed store has a size");
    let main_bytes = std::fs::read(&db).unwrap();
    let wal_bytes = std::fs::read(&wal).unwrap();
    assert_eq!(
        store.reclaim_space(Some(settled + 4_096)),
        Reclamation {
            reclaimed_bytes: Some(0),
            compaction_ran: false,
            on_disk_measured: true,
            compaction_error: None,
        },
        "zero is the measurement over a file that was not rewritten, even where the size handed \
         in is above the size the file has"
    );
    assert_eq!(
        store.reclaim_space(None),
        Reclamation {
            reclaimed_bytes: None,
            compaction_ran: false,
            on_disk_measured: false,
            compaction_error: None,
        }
    );
    assert_eq!(std::fs::read(&db).unwrap(), main_bytes);
    assert_eq!(std::fs::read(&wal).unwrap(), wal_bytes);

    // THE SAME FILE HOLDING FREE PAGES, and still no size handed in: it is rewritten, and the
    // bytes are unmeasured rather than zero.
    plant_free_pages(&db, 400);
    let pages_before = pragma_i64(&db, "page_count");
    assert_eq!(
        store.reclaim_space(None),
        Reclamation {
            reclaimed_bytes: None,
            compaction_ran: true,
            on_disk_measured: false,
            compaction_error: None,
        }
    );
    assert_eq!(pragma_i64(&db, "freelist_count"), 0);
    assert!(pragma_i64(&db, "page_count") < pages_before);
}

// ---------------------------------------------------------------------------------------
// 3. A real failing step is reported, and the rerun reclaims what it left
// ---------------------------------------------------------------------------------------

#[test]
fn a_rewrite_the_engine_refuses_is_named_in_the_report_and_the_same_call_again_reclaims() {
    let _serial = serial();
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let store = Store::open(db.to_str().unwrap()).unwrap();
    plant_free_pages(&db, 3_000);
    let free_before = pragma_i64(&db, "freelist_count");
    let pages_before = pragma_i64(&db, "page_count");
    assert!(
        free_before > 100,
        "the fixture must leave real free pages; the freelist holds {free_before} page(s)"
    );
    let before = store
        .bytes_on_disk()
        .expect("a file-backed store has a size");

    // ANOTHER WRITER HOLDS THE FILE past the store's busy timeout: the rewrite cannot take the
    // write lock, so the engine itself refuses it.
    let writer = rusqlite::Connection::open(&db).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();

    let refused = store.reclaim_space(Some(before));

    assert_eq!(
        refused,
        Reclamation {
            reclaimed_bytes: None,
            compaction_ran: true,
            on_disk_measured: true,
            compaction_error: Some(write_lock_held()),
        },
        "a step that failed is a named field beside an unmeasured figure, never a zero"
    );
    assert_eq!(
        pragma_i64(&db, "freelist_count"),
        free_before,
        "the refused rewrite reclaimed nothing: the free pages are still in the file"
    );
    assert_eq!(pragma_i64(&db, "page_count"), pages_before);

    writer.execute_batch("ROLLBACK").unwrap();
    drop(writer);

    let rerun = store.reclaim_space(Some(before));

    let after = store
        .bytes_on_disk()
        .expect("a file-backed store has a size");
    assert!(
        after < before,
        "the rerun must shrink the log: {before} before, {after} after"
    );
    assert_eq!(
        rerun,
        Reclamation {
            reclaimed_bytes: Some(before - after),
            compaction_ran: true,
            on_disk_measured: true,
            compaction_error: None,
        }
    );
    assert_eq!(pragma_i64(&db, "freelist_count"), 0);
    assert!(pragma_i64(&db, "page_count") < pages_before);
}

// ---------------------------------------------------------------------------------------
// 4. Where the rewrite stages its copy, seen from the filesystem
// ---------------------------------------------------------------------------------------

/// A log at `<dir>/<name>.db` whose LIVE rows outweigh the engine's page cache several times
/// over, so a rewrite staged in a file has to spill its copy to that file, with free pages
/// planted beside them so there is something for a rewrite to reclaim.
fn log_too_large_to_stage_in_the_page_cache(
    dir: &std::path::Path,
    name: &str,
) -> (Store, std::path::PathBuf) {
    let db = dir.join(format!("{name}.db"));
    let store = Store::open(db.to_str().unwrap()).unwrap();
    let events: Vec<Event> = (0..48)
        .map(|_| Event::new("RunStarted", vec![b'x'; 256 * 1024]))
        .collect();
    store
        .append("run", ExpectedRevision::Any, &events)
        .expect("seed the live rows");
    plant_free_pages(&db, 400);
    (store, db)
}

/// The whole process's engine pointed at one temporary directory for as long as this value
/// lives. The setting is process-global, so it is cleared when the value drops, which a test
/// that fails part way reaches as surely as one that passes.
struct EngineStagingIn(rusqlite::Connection);

impl EngineStagingIn {
    fn point_at(dir: &std::path::Path) -> Self {
        let engine = rusqlite::Connection::open_in_memory().unwrap();
        engine
            .execute_batch(&format!(
                "PRAGMA temp_store_directory = '{}'",
                dir.display()
            ))
            .unwrap();
        Self(engine)
    }
}

impl Drop for EngineStagingIn {
    fn drop(&mut self) {
        // A failure to clear is not raised from a drop that may already be unwinding.
        let _ = self.0.execute_batch("PRAGMA temp_store_directory = ''");
    }
}

#[test]
fn the_rewrite_writes_nothing_into_the_temporary_directory_the_engine_resolves() {
    let _serial = serial();
    let logs = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let (subject, subject_db) = log_too_large_to_stage_in_the_page_cache(logs.path(), "subject");
    let (pruned, pruned_db) = log_too_large_to_stage_in_the_page_cache(logs.path(), "pruned");
    let (_control, control_db) = log_too_large_to_stage_in_the_page_cache(logs.path(), "control");

    // THE WHOLE PROCESS'S ENGINE IS POINTED AT `staging`, the first place it looks for somewhere
    // to put a temporary file. A file created there and unlinked again still moves the
    // directory's modification time, which is what is read below.
    let _engine = EngineStagingIn::point_at(staging.path());
    let touched = || {
        std::fs::metadata(staging.path())
            .unwrap()
            .modified()
            .unwrap()
    };
    let untouched = touched();
    // Longer than the coarsest clock a filesystem stamps a directory with, so a write that
    // happens from here on cannot land on the stamp just read.
    std::thread::sleep(Duration::from_millis(100));

    // THE STORE'S OWN RECLAMATION.
    let subject_before = subject
        .bytes_on_disk()
        .expect("a file-backed store has a size");
    let reclaimed = subject.reclaim_space(Some(subject_before));
    let subject_after = subject
        .bytes_on_disk()
        .expect("a file-backed store has a size");
    assert!(subject_after < subject_before);
    assert_eq!(
        reclaimed,
        Reclamation {
            reclaimed_bytes: Some(subject_before - subject_after),
            compaction_ran: true,
            on_disk_measured: true,
            compaction_error: None,
        },
        "the rewrite must really have run, or an untouched directory proves nothing"
    );
    assert_eq!(pragma_i64(&subject_db, "freelist_count"), 0);
    assert_eq!(
        touched(),
        untouched,
        "the reclamation staged its copy of the log in a file under the temporary directory"
    );

    // THE RECLAMATION THE PRUNE CALLS, over a log it has nothing to delete from.
    let report = pruned
        .prune_derived_index("", &rigger::ingest::derived_index_identity())
        .expect("prune a log holding no derived row");
    assert_eq!(report.total_removed(), 0);
    assert!(
        report.reclamation.compaction_ran && report.reclamation.compaction_error.is_none(),
        "the prune's reclamation must really have rewritten the file; got {report:?}"
    );
    assert_eq!(pragma_i64(&pruned_db, "freelist_count"), 0);
    assert_eq!(
        touched(),
        untouched,
        "the prune's reclamation staged its copy of the log in a file under the temporary \
         directory"
    );
    assert_eq!(std::fs::read_dir(staging.path()).unwrap().count(), 0);

    // THE POSITIVE CONTROL: the same log rewritten through a connection left on the engine's
    // default stages its copy in a file, and the directory shows it.
    let file_staged = rusqlite::Connection::open(&control_db).unwrap();
    assert_eq!(
        file_staged
            .query_row("PRAGMA temp_store", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0,
        "the control connection is on the engine's default"
    );
    file_staged.execute_batch("VACUUM").unwrap();
    assert_eq!(pragma_i64(&control_db, "freelist_count"), 0);
    assert_ne!(
        touched(),
        untouched,
        "a rewrite staged in a file must move the watched directory's modification time, or \
         the two assertions above watched a directory nothing would ever have written to"
    );
}

// ---------------------------------------------------------------------------------------
// 5. What the rewrite takes on the log's own partition
// ---------------------------------------------------------------------------------------

#[test]
fn the_rewritten_file_passes_through_the_write_ahead_log_beside_the_log() {
    let _serial = serial();
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let wal = dir.path().join("events.db-wal");
    let store = Store::open(db.to_str().unwrap()).unwrap();
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("RunStarted", vec![b'x'; 256 * 1024])],
        )
        .expect("seed a live row");
    plant_free_pages(&db, 3_000);
    // The write-ahead log starts EMPTY, so every byte it holds below was put there by the
    // rewrite.
    assert_eq!(pragma_i64(&db, "wal_checkpoint(TRUNCATE)"), 0);
    assert_eq!(file_len(&wal), 0);
    assert!(pragma_i64(&db, "freelist_count") > 100);
    let before = store
        .bytes_on_disk()
        .expect("a file-backed store has a size");

    // A READER PARKED ON THE LOG keeps the write-ahead log from being folded back, and lets go
    // the moment the rewrite has committed: a fresh connection then reads a file with no free
    // page. What the write-ahead log holds at that moment is what the rewrite put through it.
    let reader = rusqlite::Connection::open(&db).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    reader
        .query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
        .expect("park a reader on the log");
    let (watched_db, watched_wal) = (db.clone(), wal.clone());
    let watcher = std::thread::spawn(move || {
        let give_up = Instant::now() + Duration::from_secs(20);
        while pragma_i64(&watched_db, "freelist_count") != 0 && Instant::now() < give_up {
            std::thread::sleep(Duration::from_millis(5));
        }
        let held = file_len(&watched_wal);
        drop(reader);
        held
    });

    let reclaimed = store.reclaim_space(Some(before));
    let held_in_the_wal = watcher.join().expect("the watcher must not panic");

    let compacted = file_len(&db);
    assert!(
        held_in_the_wal >= compacted,
        "the rewritten file must pass through the write-ahead log beside the log, which is why \
         the log's own partition needs about the compacted size free: the write-ahead log held \
         {held_in_the_wal} byte(s) for a compacted file of {compacted}"
    );
    assert_eq!(file_len(&wal), 0);
    assert_eq!(
        reclaimed,
        Reclamation {
            reclaimed_bytes: Some(before - compacted),
            compaction_ran: true,
            on_disk_measured: true,
            compaction_error: None,
        },
        "once the reader lets go the reclamation lands and is measured"
    );
    assert!(
        compacted >= 256 * 1024,
        "the compacted file still holds the live row; it is {compacted} byte(s)"
    );
}
