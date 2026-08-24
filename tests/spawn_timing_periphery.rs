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
//! A second cross-module seam landed after a review reject on the first cut of this
//! criterion (adjudication `adj-u61c9-verdict-reject-untruthful-duration-aggregates`):
//! pairing now keys on `(run WINDOW, spawn id)`, where the window advances on every
//! `crate::run::TYPE_RUN_STARTED` this fold observes - a genuinely NEW cross-module seam
//! `metrics::project` did not consume before that fix. The implementer's own regression
//! tests for it (`src/metrics.rs mod tests::spawn_timing_never_pairs_a_cross_run_id_collision`
//! and neighbors) are pure in-memory folds with hand-set `recorded_at` and a hand-built
//! `RunStarted`, same blind spot as above; `spawn_timing_never_pairs_a_request_and_result_
//! from_different_run_windows` below re-proves it through a real store round trip instead.
//! The paired same-batch/negative-duration SUSPECT guard the same fix added is covered at
//! the compiled-binary boundary in `tests/cli.rs` (raw-SQL-controlled `recorded_at` is
//! needed to produce a genuine clock-skew negative duration, which a real store's
//! forward-only clock cannot); `spawn_timing_excludes_a_real_same_batch_pair_as_suspect_
//! not_a_silent_zero` below covers the same-batch half of it here too, since that half
//! (unlike clock skew) a real store CAN produce deterministically - one `store.append`
//! call stamps every event in the batch with the identical `recorded_at`.
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

/// The run-windowed pairing key (`adj-u61c9-verdict-reject-untruthful-duration-aggregates`'s
/// fix) through a REAL store: a spawn id reused across two `TYPE_RUN_STARTED` boundaries must
/// never let a later window's result pair with an earlier window's dangling request. Asserted
/// STRUCTURALLY (which bucket each event lands in), not by duration magnitude, so the test is
/// immune to timing flakiness - the pre-fix bug would have folded window 1's request into
/// `unpaired_spawns == 0` (silently absorbed by window 2's result) instead of `1`.
#[test]
fn spawn_timing_never_pairs_a_request_and_result_from_different_run_windows() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let store = Store::open(db.to_str().unwrap()).expect("open a real sqlite store");

    let run_started = || rigger::eventstore::Event::new(rigger::run::TYPE_RUN_STARTED, Vec::new());
    // The SAME textual spawn id is reused in both windows - a re-proposed/relaunched unit
    // reusing its auto-slugged id, the realistic collision this fix closes.
    let req = SpawnRequest::new("u1", "impl", "implementer", 0, "do it");

    // Window 1: parked, but never answered before window 2 begins.
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[run_started(), req.to_event().unwrap()],
        )
        .expect("append window 1's RunStarted + the never-answered-in-window request");

    std::thread::sleep(Duration::from_millis(20));

    // Window 2: the same id is re-requested and genuinely answered inside this window.
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[run_started(), req.to_event().unwrap()],
        )
        .expect("append window 2's RunStarted + the reused-id request");
    std::thread::sleep(Duration::from_millis(20));
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[SpawnResult::ok(&req.id, "done").to_event().unwrap()],
        )
        .expect("append window 2's result");

    let events = store
        .read_stream("run", 0, Direction::Forward)
        .expect("read the stream back");
    assert_eq!(
        events.len(),
        5,
        "both RunStarted markers and all three spawn events (2 requests + 1 result) must read \
         back"
    );

    let m = project(&events);

    let implementer = m
        .spawn_timing
        .get("implementer")
        .expect("window 2's genuine pair must fold");
    assert_eq!(
        implementer.count, 1,
        "exactly ONE pair may fold - window 1's dangling request must never absorb window 2's \
         result (that would either double the count or synthesize a bogus cross-window \
         duration spanning both windows)"
    );
    assert_eq!(
        m.unpaired_spawns, 1,
        "window 1's request, never answered WITHIN its own window, must surface as its own \
         unpaired spawn - not silently paired with a result from a later, unrelated window"
    );
}

/// The truthfulness guard the same fix added, through a REAL store: a request and its result
/// appended together in ONE `store.append` call share the store's single per-batch
/// `recorded_at` clock (`src/eventstore/sqlite.rs`: "the store stamps recorded_at on ingest,
/// one clock per batch"), so their real, store-stamped duration is an exact
/// `Duration::ZERO` - a genuine same-batch collision, not a hand-set one. It must fold into
/// `unpaired_spawns` as SUSPECT, never silently enter a role's mean as a fabricated zero.
/// A genuine, measurably-separate pair in the SAME role bucket rides alongside it, so a
/// regression that excludes the WHOLE role (rather than just the suspect pair) is visible
/// too.
#[test]
fn spawn_timing_excludes_a_real_same_batch_pair_as_suspect_not_a_silent_zero() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("events.db");
    let store = Store::open(db.to_str().unwrap()).expect("open a real sqlite store");

    let same_batch = SpawnRequest::new("u1", "impl", "implementer", 0, "same batch");
    let genuine = SpawnRequest::new("u2", "impl", "implementer", 0, "genuine");

    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[
                same_batch.to_event().unwrap(),
                SpawnResult::ok(&same_batch.id, "done").to_event().unwrap(),
            ],
        )
        .expect("append the same-batch pair in ONE call");

    store
        .append("run", ExpectedRevision::Any, &[genuine.to_event().unwrap()])
        .expect("append the genuine request in its own batch");
    std::thread::sleep(Duration::from_millis(20));
    store
        .append(
            "run",
            ExpectedRevision::Any,
            &[SpawnResult::ok(&genuine.id, "done").to_event().unwrap()],
        )
        .expect("append the genuine result in a LATER, separate batch");

    let events = store
        .read_stream("run", 0, Direction::Forward)
        .expect("read the stream back");

    let m = project(&events);

    let implementer = m
        .spawn_timing
        .get("implementer")
        .expect("the genuine cross-batch pair must still fold");
    assert_eq!(
        implementer.count, 1,
        "ONLY the genuine cross-batch pair may fold - the same-batch zero-duration pair must \
         never enter the aggregate as a fabricated zero"
    );
    assert!(
        implementer.total > Duration::ZERO,
        "the folded pair's duration must be the genuine one, not the suspect zero"
    );
    assert_eq!(
        m.unpaired_spawns, 1,
        "the same-batch pair's non-positive duration must fold into unpaired_spawns as \
         SUSPECT, not vanish or silently enter the mean as a real zero"
    );
}
