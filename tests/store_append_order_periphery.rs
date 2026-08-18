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
use rigger::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision};
use std::sync::{Arc, Barrier};

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
