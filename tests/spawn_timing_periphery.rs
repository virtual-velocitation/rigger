//! Periphery (public-API / contract) tests for spec 61 criterion 9, SPAWN TIMING:
//! `rigger::metrics::{Metrics::spawn_timing, Metrics::unpaired_spawns, SpawnTiming}` - the
//! per-agent wall-clock duration aggregate `metrics::project` folds by pairing recorded
//! `SpawnRequested`/`SpawnResult` events by spawn id (`src/metrics.rs`). The rendered
//! `rigger stats` boundary (`src/main.rs::append_spawn_timing`) is guarded separately by the
//! CLI-level tests in `tests/cli.rs`; this file guards the LIBRARY seam underneath it.
//!
//! These run OUTSIDE the crate, over the PUBLIC surface (`rigger::metrics::{project, Metrics,
//! SpawnTiming}`, `rigger::spawn::{SpawnRequest, SpawnResult}`,
//! `rigger::eventstore::sqlite::Store`), so they guard exactly what the implementer's
//! inside-out unit tests (`src/metrics.rs mod tests`, which reach `project`/`SpawnTiming` via
//! plain in-module calls and hand-build every `Event` in memory - their `spawn_requested`/
//! `spawn_result_at` helpers construct an `Event` via `.to_event()` and then overwrite its
//! `recorded_at` field directly, never through any store) are structurally blind to:
//!
//!  - PUBLIC REACHABILITY. `SpawnTiming` (both fields, `mean()`) and the two new `Metrics`
//!    fields are reachable as `rigger::metrics::...` from outside the crate. If any were
//!    accidentally left crate-private, only an external test fails to COMPILE - the inside-out
//!    tests, which reach everything through `super::`, would stay green regardless.
//!  - THE WRITER -> STORE -> READER WIRE FORM. The unit tests never persist a `SpawnRequest`/
//!    `SpawnResult` anywhere: they hand-set `Event.recorded_at` directly on an in-memory
//!    value. This test builds the SAME events through the real writer path
//!    (`SpawnRequest::new(..).to_event()` / `SpawnResult::ok(..).to_event()`), APPENDS them to
//!    a real `eventstore::sqlite::Store` (the real BLOB/INTEGER/TEXT columns, the real
//!    store-STAMPED `recorded_at` clock - not a caller-set value), reads them back, and folds
//!    the READ-BACK events - proving the SQLite round trip the pairing fold depends on in
//!    production actually holds, not just the in-memory shape the unit tests hand-build.
//!  - THE CROSS-MODULE FOLD-ARM SEAM. Before this criterion, `metrics::project` had NO arm
//!    for `TYPE_SPAWN_REQUESTED` at all, and its `TYPE_SPAWN_RESULT` arm returned early for
//!    every role outside the review tiers (the `review_tier` gate) before ever reaching this
//!    criterion's new recording. This test seeds one review-tier role (`adversary`, which
//!    passes the gate) and one NON-review-tier role (`implementer`, which does not) in the
//!    same real-store batch: both role buckets must be populated, so a regression that slips
//!    the new recording back below the tier gate reddens here, not just in a unit test that
//!    already assumes the ordering is correct.
//!
//! `metrics` and `eventstore::sqlite` are not feature-gated, so every test here runs
//! identically on both the default and the `--no-default-features` lane.

use std::time::Duration;

use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore, ExpectedRevision};
use rigger::metrics::{project, Metrics, SpawnTiming};
use rigger::spawn::{SpawnRequest, SpawnResult};

/// `SpawnTiming`'s two fields and `mean()` are constructible and computed exactly as
/// documented, entirely from OUTSIDE the crate - the public-API half of spec 61 c9's surface.
#[test]
fn spawn_timing_is_publicly_reachable_and_mean_divides_total_by_count() {
    let t = SpawnTiming {
        count: 3,
        total: Duration::from_secs(9),
    };
    assert_eq!(t.mean(), Duration::from_secs(3));

    let zero = SpawnTiming::default();
    assert_eq!(zero.count, 0);
    assert_eq!(zero.total, Duration::ZERO);
    assert_eq!(
        zero.mean(),
        Duration::ZERO,
        "mean of zero recorded spawns must not divide by zero"
    );
}

/// `Metrics::spawn_timing` / `Metrics::unpaired_spawns` are reachable and default-empty from
/// outside the crate, and an empty event slice folds to the same empty defaults `project`
/// documents for every other metric - the empty-accounting edge of this criterion's contract.
#[test]
fn metrics_spawn_timing_fields_are_publicly_reachable_and_default_empty() {
    let m = Metrics::default();
    assert!(m.spawn_timing.is_empty());
    assert_eq!(m.unpaired_spawns, 0);

    let projected = project(&[]);
    assert!(projected.spawn_timing.is_empty());
    assert_eq!(projected.unpaired_spawns, 0);
}

/// The full WRITER -> STORE -> READER round trip, across TWO roles - one review-tier
/// (`adversary`), one not (`implementer`) - plus one unanswered request. Real
/// `SpawnRequest`/`SpawnResult` events are appended to a real sqlite store, read back, and
/// only THEN folded; every assertion is over the READ-BACK events, never the ones built in
/// memory a moment earlier.
#[test]
fn spawn_timing_pairs_real_writer_events_through_a_real_store_by_role() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let store = Store::open(db.to_str().unwrap()).expect("open a real sqlite store");

    let implementer_req = SpawnRequest::new("u1", "impl", "implementer", 0, "do it");
    let adversary_req = SpawnRequest::new("u2", "review", "adversary", 0, "review it");
    let dead_req = SpawnRequest::new("u3", "impl", "implementer", 1, "never answered");

    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[
                implementer_req.to_event().unwrap(),
                dead_req.to_event().unwrap(),
            ],
        )
        .expect("append the two requests");
    // A short, real sleep so the paired duration is measurably nonzero through the store's
    // own wall-clock stamp - proving `recorded_at` (store-stamped on ingest) actually drives
    // the fold, rather than every path coincidentally producing an unmeasured zero.
    std::thread::sleep(Duration::from_millis(20));
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[SpawnResult::ok(&implementer_req.id, "done")
                .to_event()
                .unwrap()],
        )
        .expect("append the implementer result");

    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[adversary_req.to_event().unwrap()],
        )
        .expect("append the adversary request");
    std::thread::sleep(Duration::from_millis(20));
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[SpawnResult::ok(&adversary_req.id, "done")
                .to_event()
                .unwrap()],
        )
        .expect("append the adversary result");

    let events = store
        .read_stream("run", 0, Direction::Forward)
        .expect("read the stream back");
    assert_eq!(events.len(), 5, "all five appended events must read back");

    let m = project(&events);

    let implementer = m
        .spawn_timing
        .get("implementer")
        .expect("the NON-review-tier role must still fold - the tier gate must not suppress it");
    assert_eq!(implementer.count, 1);
    assert!(
        implementer.total > Duration::ZERO,
        "the real store's recorded_at must drive a genuinely nonzero duration"
    );

    let adversary = m
        .spawn_timing
        .get("adversary")
        .expect("the review-tier role must fold too, from the SAME batch");
    assert_eq!(adversary.count, 1);
    assert!(adversary.total > Duration::ZERO);

    assert_eq!(
        m.unpaired_spawns, 1,
        "the never-answered u3 request must be counted as unpaired, not silently dropped"
    );
}
