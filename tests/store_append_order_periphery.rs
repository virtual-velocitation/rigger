//! Periphery (contract / API / integration) test for spec 71 criterion 1: APPEND REFUSES
//! DISORDER, the concurrency face this criterion names by its own text ("two writers racing
//! one stream serialize through the transaction - the loser retries-or-refuses through the
//! SAME named error, never a bare unique-constraint failure") and the design section's own
//! constraints walk ("CONCURRENT writers - seek, assertion, and insert share one transaction;
//! a racing sibling serializes and re-seeks; a race surfaces as retry-or-refusal through the
//! NAMED error, never a bare `UNIQUE(stream, revision)` failure").
//!
//! This runs OUTSIDE the crate, over the sqlite backend's public surface
//! (`rigger::eventstore::sqlite::Store`), because the coverage this criterion needs sits at a
//! boundary the inside-out unit tests and the existing backend-agnostic contract test are both
//! structurally blind to:
//!
//!  - `contract::append_races_one_stream_serialize_to_the_named_conflict` drives every racing
//!    thread through the SAME `&dyn EventStore` reference. For the sqlite backend that
//!    reference is one `Store` wrapping one `Arc<Mutex<Connection>>`, so every racer queues on
//!    that in-process `Mutex` before a single statement reaches SQLite - the contract test
//!    proves the LOGICAL property (exactly one winner, every loser gets `Error::Conflict`) but
//!    can never exercise genuine cross-connection contention at the SQLite engine level (the
//!    `BEGIN IMMEDIATE` write-lock queueing and `busy_timeout` mechanics `Store::append`'s own
//!    comment calls out by name as what makes this safe ACROSS connections, not just within one
//!    `Store`);
//!  - the pre-existing cross-connection test in `sqlite.rs`
//!    (`concurrent_cross_connection_appends_serialize_without_spurious_lock_errors`) opens two
//!    real separate connections on one file and races them - but only under
//!    `ExpectedRevision::Any`, which never conflicts, so it proves the queueing is spurious-lock-
//!    free without ever producing a single `Error::Conflict` to check the naming of. Nothing in
//!    the tree drives `ExpectedRevision::Exact` under real, separate-connection contention.
//!
//! So the one shape this criterion's own words require - many real connections racing one
//! stream under one shared stale `Exact` expectation, serializing through SQLite's own write
//! lock rather than an in-process `Mutex`, with every loser naming the SAME `Error::Conflict`
//! and never a raw `database is locked` or bare `UNIQUE(stream, revision)` leak - is pinned
//! here. `Store::open` and `Store::append` are the only surface this test touches; it never
//! reaches into a private field or a raw connection, so it exercises exactly what an external
//! consumer of the port can exercise.

use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision, Filter};
use std::sync::{Arc, Barrier};

/// Opens a fresh file-backed store at `path`, seeds `stream` with five conforming
/// events (revisions 0..4), then reproduces the recorded incident directly on the raw
/// file: deletes revision 1 (the compaction's hole) and inserts a row AT revision 1 as
/// the stream's newest row BY POSITION - the stale writer that computes its next
/// revision by some means other than this store's own position-order tail read. This is
/// the only way to reach the disordered state at all: every conforming backend computes
/// the revision it writes itself, so no sequence of calls through `EventStore::append`
/// alone can ever produce it.
fn seed_disordered_stream(path: &str, stream: &str) {
    let store = Store::open(path).expect("a file-backed store opens");
    let seed: Vec<Event> = (0..5u8).map(|i| Event::new("S", vec![i])).collect();
    store
        .append(stream, ExpectedRevision::Any, &seed)
        .expect("seeding the healthy prefix must succeed");
    drop(store);

    let raw = rusqlite::Connection::open(path).expect("an independent writer opens");
    raw.execute(
        "DELETE FROM events WHERE stream = ?1 AND revision = 1",
        [stream],
    )
    .expect("the supported compaction deletes the hole");
    raw.execute(
        "INSERT INTO events \
         (stream, type, id, data, meta, valid_from, recorded_at, revision) \
         VALUES (?1, 'STALE', 'periphery-stale-writer', X'00', '{}', 0, 0, 1)",
        [stream],
    )
    .expect("the stale writer reissues the revision the compaction freed");
    // Position order for `stream` is now: rev 0, rev 2, rev 3, rev 4, rev 1 (newest).
    // The revision-order tail is 4; the position-order tail is 1.
}

/// A corrupted stream stays refused on every subsequent attempt, not only the first -
/// closing the class of regression where a guard fires once and then goes quiet because
/// a future change let a failed attempt advance a cached cursor. Every attempt must
/// land zero rows, so the on-disk row count never moves past what the corruption left.
#[test]
fn the_refusal_repeats_on_every_subsequent_attempt() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("run.db");
    let path = path.to_str().expect("a utf-8 path").to_string();
    seed_disordered_stream(&path, "run");

    let store = Store::open(&path).expect("reopen the store");
    for attempt in 0..3u8 {
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new("N", vec![attempt])],
            )
            .expect_err("a still-broken stream must refuse every attempt, not only the first");
    }

    let after = store
        .read_stream("run", 0, Direction::Forward)
        .expect("reads stay untouched by a refused write");
    assert_eq!(
        after.len(),
        5,
        "three refused attempts must land zero rows: the seeded 5, unmoved by the \
         delete-and-reissue (still 5 rows total: 0,2,3,4 plus the reissued 1)"
    );
}

/// "Fail-safe directions only: the assertion may only refuse a write ... no path gains
/// repair-by-side-effect." Reads are a path: both `read_stream` (revision order) and
/// `read_all` (position order) must keep answering a broken stream's rows verbatim,
/// never erroring and never silently resorting them into agreement.
#[test]
fn a_broken_stream_still_reads_back_by_both_orders_without_erroring_or_repairing() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("run.db");
    let path = path.to_str().expect("a utf-8 path").to_string();
    seed_disordered_stream(&path, "run");

    let store = Store::open(&path).expect("reopen the store");

    let by_revision = store
        .read_stream("run", 0, Direction::Forward)
        .expect("a broken stream's revision-order read must not error");
    let revisions: Vec<i64> = by_revision.iter().map(|e| e.revision).collect();
    assert_eq!(
        revisions,
        vec![0, 1, 2, 3, 4],
        "revision order answers exactly what the revision column holds, sorted"
    );

    let by_position = store
        .read_all(0, Direction::Forward, &Filter::default())
        .expect("a broken stream's position-order read must not error");
    let positions_revisions: Vec<i64> = by_position
        .iter()
        .filter(|e| e.stream == "run")
        .map(|e| e.revision)
        .collect();
    assert_eq!(
        positions_revisions,
        vec![0, 2, 3, 4, 1],
        "position order answers the true insertion order, unrepaired: the stale \
         writer's row (revision 1) is the newest by POSITION even though it holds a \
         lower revision than every row inserted before it"
    );
}

/// The companion of the `exact_revision_race_across_real_connections...` test above,
/// covering the OTHER half of the same risk: the new
/// monotonicity assertion added by this criterion runs one extra indexed seek inside the
/// same transaction as every append. Several real, separate connections racing the SAME
/// stream under `ExpectedRevision::Any` (which never conflicts on the ExpectedRevision
/// check itself) must still land gap-free, duplicate-free revisions in an order where
/// position and revision fully agree - proving the new seek's own read never observes a
/// transiently inconsistent snapshot under genuine cross-connection contention that would
/// spuriously trip `Error::OutOfOrder` against a perfectly legitimate racing append.
#[test]
fn any_revision_race_across_real_connections_lands_gap_free_with_no_false_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.db");
    let path = path.to_str().unwrap().to_string();

    const CONNECTIONS: usize = 16;

    let stores: Vec<Arc<Store>> = (0..CONNECTIONS)
        .map(|_| Arc::new(Store::open(&path).unwrap()))
        .collect();

    let stream = "any-race";
    let barrier = Arc::new(Barrier::new(CONNECTIONS));
    let handles: Vec<_> = stores
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, store)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.append(
                    stream,
                    ExpectedRevision::Any,
                    &[Event::new("R", vec![i as u8])],
                )
            })
        })
        .collect();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for outcome in &outcomes {
        assert!(
            outcome.is_ok(),
            "an `Any`-expectation append can never conflict, so the new monotonicity \
             seek must never spuriously refuse one under real contention: {outcome:?}"
        );
    }

    let by_revision = stores[0]
        .read_stream(stream, 0, Direction::Forward)
        .unwrap();
    let mut revisions: Vec<i64> = by_revision.iter().map(|e| e.revision).collect();
    revisions.sort_unstable();
    assert_eq!(
        revisions,
        (0..CONNECTIONS as i64).collect::<Vec<_>>(),
        "no gap and no duplicate across {CONNECTIONS} real connections racing one stream"
    );

    let by_position: Vec<i64> = stores[0]
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap()
        .into_iter()
        .filter(|e| e.stream == stream)
        .map(|e| e.revision)
        .collect();
    assert_eq!(
        by_position,
        by_revision.iter().map(|e| e.revision).collect::<Vec<_>>(),
        "position order and revision order fully agree for a stream built only through \
         real concurrent Any appends - the new seek never disagrees with the tail it \
         itself just wrote"
    );
}

/// Several REAL, separate connections (distinct `Store::open` handles on one on-disk file - the
/// multi-process shape production has, not several threads sharing one handle) race the SAME
/// stream under the SAME stale `ExpectedRevision::Exact`. They must serialize through SQLite's
/// own write-lock rather than interleave into a raw engine error: exactly one wins, and every
/// loser's stale expectation surfaces through the SAME named `Error::Conflict` - never a lock
/// error, never the bare `UNIQUE(stream, revision)` text a naive read of the schema would leak.
/// Repeated over several independent rounds (fresh stream each round, same live connection pool)
/// so the property is shown to hold under contention, not just once by luck.
#[test]
fn exact_revision_race_across_real_connections_serializes_through_the_named_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.db");
    let path = path.to_str().unwrap().to_string();

    const CONNECTIONS: usize = 8;
    const ROUNDS: usize = 5;

    // Open every connection up front, serialized, so only the appends race - exactly the
    // discipline the pre-existing cross-connection test in sqlite.rs already uses.
    let stores: Vec<Arc<Store>> = (0..CONNECTIONS)
        .map(|_| Arc::new(Store::open(&path).unwrap()))
        .collect();

    for round in 0..ROUNDS {
        let stream = format!("race-{round}");
        // Seed the stream at revision 0 through one connection, so every racer below shares
        // the SAME now-stale expectation.
        stores[0]
            .append(
                &stream,
                ExpectedRevision::NoStream,
                &[Event::new("Seed", b"0".to_vec())],
            )
            .expect("the seed append must succeed");

        let barrier = Arc::new(Barrier::new(CONNECTIONS));
        let handles: Vec<_> = stores
            .iter()
            .cloned()
            .map(|store| {
                let barrier = barrier.clone();
                let stream = stream.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.append(
                        &stream,
                        ExpectedRevision::Exact(0),
                        &[Event::new("R", b"racer".to_vec())],
                    )
                })
            })
            .collect();
        let outcomes: Vec<Result<_, Error>> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();

        let wins = outcomes.iter().filter(|o| o.is_ok()).count();
        assert_eq!(
            wins, 1,
            "round {round}: exactly one of {CONNECTIONS} real connections sharing one stale \
             expectation may win the race"
        );
        for outcome in &outcomes {
            if let Err(e) = outcome {
                assert!(
                    matches!(e, Error::Conflict { .. }),
                    "round {round}: a losing racer on a real separate connection must surface \
                     the named optimistic-concurrency error, never an unnamed one: {e}"
                );
                let message = e.to_string();
                assert!(
                    !message.to_uppercase().contains("UNIQUE"),
                    "round {round}: a losing racer must never see the bare \
                     UNIQUE(stream, revision) failure leak through: {message}"
                );
                assert!(
                    !message.to_lowercase().contains("database is locked")
                        && !message.to_lowercase().contains("locked"),
                    "round {round}: a losing racer must never see a raw SQLite lock error - \
                     the write-lock queueing this criterion's own design section names must \
                     resolve to the named Conflict, not a hard engine failure: {message}"
                );
            }
        }

        // A correct append afterward, at the revision the race actually left the stream on,
        // is untouched by any of it - through a DIFFERENT connection than the one that won,
        // proving the store's on-disk state (not just the winning connection's local view) is
        // what the next honest writer sees.
        stores[CONNECTIONS - 1]
            .append(
                &stream,
                ExpectedRevision::Exact(1),
                &[Event::new("Next", b"n".to_vec())],
            )
            .unwrap_or_else(|e| {
                panic!(
                    "round {round}: a correct append at the stream's true current revision, \
                     from a connection that lost the race, must proceed normally: {e}"
                )
            });
        let all = stores[0]
            .read_stream(&stream, 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            all.len(),
            3,
            "round {round}: the seed, the one race winner, and the follow-up append - nothing \
             more, nothing lost, read back through yet another connection"
        );
    }
}
