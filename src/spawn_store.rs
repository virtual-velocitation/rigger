//! The process half of [`crate::spawn`] (spec 93, criterion 1: THE CORE LANE IS PURE):
//! persisting a spawn request/result to a LIVE run's event store. [`crate::spawn`] keeps
//! the model (`SpawnRequest`/`SpawnResult`/`WaveItem`/`Step`) and the pure fold
//! (`step_result`, `recorded`, `result_of`, `prompt_for`, ...) - everything a page or a
//! test can compute from an already-read `&[Event]` slice with no store at all. This
//! module is the OTHER half: the handful of functions that actually append to a
//! `&dyn EventStore`, split into its own file (never a function-by-function `#[cfg]`
//! inside `spawn.rs`) so the pure half can compile for `wasm32-unknown-unknown` with no
//! store adapter in reach. `crate::spawn` stays the single vocabulary owner (ids, event
//! types, the request/result shapes); this module only ever appends what `spawn` already
//! knows how to serialize.

use crate::conductor::STREAM;
use crate::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision, Position};
use crate::spawn::{SpawnRequest, SpawnResult};

/// Persist a parked spawn request to the run's event log as a
/// [`crate::spawn::TYPE_SPAWN_REQUESTED`] event, returning its global position.
///
/// This is exactly what a step does when it reaches an UNRECORDED spawn at the
/// frontier: the request becomes a durable fact, so the next step process (and the
/// thin driver draining the wave) sees the identical call, and the budget breaker
/// counts spawns from the log rather than an in-memory counter. A serialization
/// failure is surfaced as a backend error rather than panicking.
pub fn park(store: &dyn EventStore, req: &SpawnRequest) -> Result<Position, Error> {
    park_in_run(store, req, "")
}

/// Park `req` as a [`crate::spawn::TYPE_SPAWN_REQUESTED`] event stamped with the run it
/// belongs to, so the parked spawn is attributable to its run (spec 06, unit 1): the
/// conductor threads the current run id onto every spawn and the replay driver parks
/// through here, so a `SpawnRequested` carries the same `run_id` metadata as the
/// unit/gate events the conductor emits for that run. An empty `run_id` stamps no
/// metadata (a caller outside a run - e.g. the pure-fold tests), so [`park`] is exactly
/// this with no run. This is the single park authority; [`park`] delegates to it.
pub fn park_in_run(
    store: &dyn EventStore,
    req: &SpawnRequest,
    run_id: &str,
) -> Result<Position, Error> {
    let mut ev = req
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize spawn request {}: {e}", req.id)))?;
    if !run_id.is_empty() {
        ev = ev.with_meta(crate::run::META_RUN_ID, run_id);
    }
    one_position(store, &req.id, &ev)
}

/// The global position of the ONE event `ev` landed at, appended to the spawn stream.
/// `subject` names WHAT was being recorded - the spawn id - so a failure reads as the
/// thing the operator asked for rather than as an internal type name.
///
/// What an absence means is not decided here: [`crate::eventstore::Appended::one`] decides
/// it, once, for every single-event append in the codebase. This function only names the subject it
/// was recording, so the one failure it can raise says what was lost.
fn one_position(store: &dyn EventStore, subject: &str, ev: &Event) -> Result<Position, Error> {
    store
        .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(ev))?
        .one(&what(&ev.type_, subject))
}

/// How a spawn-stream write names itself in a failure: the event TYPE, the SUBJECT it was
/// about, and the stream it was bound for. One phrasing for every seam in this module, so
/// the compare-and-append half and the plain-append half cannot drift into two vocabularies
/// for the same loss.
fn what(type_: &str, subject: &str) -> String {
    format!("the {type_} of {subject} on {STREAM:?}")
}

/// Persist a recorded spawn result to the run's event log as a
/// [`crate::spawn::TYPE_SPAWN_RESULT`] event, returning its global position. This is
/// exactly what `rigger result <id>` does once a courier has run the parked agent: the
/// outcome becomes a durable fact, so the next step process replays it instead of
/// re-running the agent.
pub fn record_result(store: &dyn EventStore, res: &SpawnResult) -> Result<Position, Error> {
    let ev = res
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize spawn result {}: {e}", res.id)))?;
    one_position(store, &res.id, &ev)
}

/// Record `res` to the run's event log ONLY when the spawn has no result yet, as a
/// single atomic compare-and-append that never clobbers a result already recorded - the
/// write half of `rigger result --if-absent`. Returns `Some(position)` when it recorded,
/// `None` when a result already existed (the idempotent no-op).
///
/// The thin driver's death courier calls this to record a died-worker failure IFF the
/// worker did not already self-report. It supersedes the two-process `rigger reported
/// <id> || rigger result <id> --error` guard, which reads in one process and writes in
/// another and so leaves a TOCTOU window: a self-report (or a reviewer's already-emitted
/// approve) landing between the read and the write is clobbered by the courier's
/// `--error` - since [`record_result`]/[`crate::spawn::result_of`] are last-write-wins -
/// force-failing an approved unit on the next replay. Collapsing the check and the write
/// into one atomic operation closes that window.
///
/// Atomicity rests on the store's optimistic concurrency (the port's only cross-backend
/// primitive): read the stream, and if no [`crate::spawn::TYPE_SPAWN_RESULT`] for `res.id`
/// is present, append under an [`ExpectedRevision`] pinned to the revision just read. A
/// concurrent append that landed after the read (the racing self-report, or any other
/// writer) makes that expectation CONFLICT; we re-read and re-decide, so the write lands
/// at most once and a self-report that won the race is honored (the re-check now sees it
/// and returns `None`). Only a genuine [`Error::Conflict`] retries; any other backend error
/// surfaces.
pub fn record_result_if_absent(
    store: &dyn EventStore,
    res: &SpawnResult,
) -> Result<Option<Position>, Error> {
    let ev = res
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize spawn result {}: {e}", res.id)))?;
    loop {
        let events = store.read_stream(STREAM, 0, Direction::Forward)?;
        if crate::spawn::result_of(&events, &res.id)
            .map_err(|e| Error::Backend(format!("decode results for {}: {e}", res.id)))?
            .is_some()
        {
            // A result already exists - leave it untouched (the no-op the courier wants).
            return Ok(None);
        }
        // Pin the append to the exact revision we just read: any event appended since
        // (Forward reads ascending, so `.last()` is the current head) fails the check.
        let expected = match events.last() {
            Some(e) => ExpectedRevision::Exact(e.revision),
            None => ExpectedRevision::NoStream,
        };
        match store.append(STREAM, expected, std::slice::from_ref(&ev)) {
            // `Ok(None)` from THIS function means "a result already stood, so I chose to
            // write nothing" - the idempotent no-op. A store that wrote nothing is a
            // different answer entirely, and the shared authority raises it as the failure
            // it is rather than letting it collapse into the no-op.
            Ok(appended) => return appended.one(&what(&ev.type_, &res.id)).map(Some),
            // The stream moved under us; re-read and re-decide. If the racing writer
            // recorded THIS id, the re-check returns `None` and nothing is clobbered.
            Err(Error::Conflict { .. }) => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Filter, Revision, Subscription};
    use crate::spawn::{
        is_recorded, recorded, result_of, step_result, ROLE_IMPLEMENTER, TYPE_SPAWN_RESULT,
    };

    #[test]
    fn parking_persists_the_request_and_it_folds_back_from_the_log() {
        let store = Store::open(":memory:").unwrap();
        let req = SpawnRequest::new("u", "implement", ROLE_IMPLEMENTER, 0, "do it")
            .with_model("sonnet")
            .with_blast_radius(vec!["a.rs".into()]);

        park(&store, &req).unwrap();

        // The parked request is a durable fact on the run stream and reads back
        // identically - the persistence the replay driver and budget breaker rely on.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let recorded = recorded(&events).unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[&req.id], req);
        assert!(is_recorded(&events, &req.id));
        assert!(!is_recorded(&events, "u/implementer#1"));
    }

    #[test]
    fn recording_a_result_persists_it_and_result_of_reads_it_back() {
        let store = Store::open(":memory:").unwrap();
        // No result yet -> the spawn is still parked at the frontier.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(result_of(&events, "u/implementer#0").unwrap().is_none());

        record_result(&store, &SpawnResult::ok("u/implementer#0", "done")).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert_eq!(got.output, "done");
        // A different id has no result of its own.
        assert!(result_of(&events, "u/implementer#1").unwrap().is_none());
    }

    #[test]
    fn record_result_if_absent_records_only_when_no_result_exists() {
        // The write half of `rigger result --if-absent`: with no result yet it records,
        // returning the new position, and `result_of` reads it back.
        let store = Store::open(":memory:").unwrap();
        let pos =
            record_result_if_absent(&store, &SpawnResult::ok("u/implementer#0", "done")).unwrap();
        assert!(pos.is_some(), "an absent result must be recorded");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert_eq!(got.output, "done");
    }

    #[test]
    fn record_result_if_absent_is_a_noop_that_never_clobbers_an_existing_result() {
        // The anti-clobber invariant the death courier relies on: once a worker has
        // self-reported, a later `--if-absent` (the courier's died-worker `--error`)
        // records NOTHING and leaves the self-report standing - the same guarantee the
        // two-process `rigger reported <id> || rigger result <id> --error` guard gave,
        // now in ONE atomic step so no self-report can land in the check-then-record gap.
        let store = Store::open(":memory:").unwrap();
        record_result(&store, &SpawnResult::ok("u/implementer#0", "self-reported")).unwrap();

        let skipped = record_result_if_absent(
            &store,
            &SpawnResult::failed("u/implementer#0", "died without reporting"),
        )
        .unwrap();
        assert!(
            skipped.is_none(),
            "an already-recorded result must not be re-recorded (return None)"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let results = events
            .iter()
            .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
            .count();
        assert_eq!(
            results, 1,
            "the `--if-absent` no-op must append no second result event"
        );
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert!(
            !got.is_error(),
            "the self-reported success must stand un-clobbered"
        );
        assert_eq!(got.output, "self-reported");
    }

    /// A store wrapper that simulates a CONCURRENT writer committing in the window
    /// between `record_result_if_absent`'s `read_stream` and its compare-and-append:
    /// on the FIRST append it slips `racing` onto the stream (under `Any`, so it always
    /// lands and advances the head), which makes the caller's revision-pinned append
    /// CONFLICT. This drives the `Err(Error::Conflict) => continue` retry arm
    /// DETERMINISTICALLY every run - the arm that IS the "records atomically" guarantee,
    /// which a purely sequential test never reaches. Every other method delegates
    /// straight through to the real store.
    struct RaceOnFirstAppend {
        inner: Store,
        racing: std::sync::Mutex<Option<Event>>,
    }

    impl RaceOnFirstAppend {
        fn new(inner: Store, racing: Event) -> Self {
            Self {
                inner,
                racing: std::sync::Mutex::new(Some(racing)),
            }
        }
    }

    impl EventStore for RaceOnFirstAppend {
        fn append(
            &self,
            stream: &str,
            expected: ExpectedRevision,
            events: &[Event],
        ) -> Result<crate::eventstore::Appended, Error> {
            // The concurrent writer: land it once, just before the caller's first
            // append, so the stream head moves under the caller's pinned expectation
            // and the real store returns a genuine Conflict.
            if let Some(ev) = self.racing.lock().unwrap().take() {
                self.inner
                    .append(stream, ExpectedRevision::Any, std::slice::from_ref(&ev))?;
            }
            self.inner.append(stream, expected, events)
        }

        fn read_stream(
            &self,
            stream: &str,
            from: Revision,
            dir: Direction,
        ) -> Result<Vec<Event>, Error> {
            self.inner.read_stream(stream, from, dir)
        }

        fn read_all(
            &self,
            from: Position,
            dir: Direction,
            filter: &Filter,
        ) -> Result<Vec<Event>, Error> {
            self.inner.read_all(from, dir, filter)
        }

        fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
            self.inner.subscribe_all(from, filter)
        }

        fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
            self.inner.subscribe_stream(stream, from)
        }
    }

    #[test]
    fn record_result_if_absent_retries_when_a_racing_append_conflicts() {
        // A DIFFERENT writer commits between our read and our compare-and-append, so the
        // revision-pinned append CONFLICTS. The retry loop must re-read, re-decide, and -
        // since THIS id still has no result - land it exactly once. Recording the absent
        // result over a moving stream is the whole point of the loop; if the
        // `Err(Conflict) => continue` arm is dropped (e.g. replaced by a panic or an
        // early return) this test fails.
        let inner = Store::open(":memory:").unwrap();
        // The racing writer records some OTHER unit's result (an unrelated concurrent
        // courier), so after the conflict our id is still absent and must be recorded.
        let racing = SpawnResult::ok("other/implementer#0", "unrelated")
            .to_event()
            .unwrap();
        let store = RaceOnFirstAppend::new(inner, racing);

        let pos =
            record_result_if_absent(&store, &SpawnResult::ok("u/implementer#0", "done")).unwrap();
        assert!(
            pos.is_some(),
            "the racing append forced a conflict; the retry must still record the absent result"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        // Exactly one result for OUR id - recorded once, not duplicated by the retry.
        let ours = events
            .iter()
            .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
            .filter_map(|e| SpawnResult::from_event(e).ok())
            .filter(|r| r.id == "u/implementer#0")
            .count();
        assert_eq!(ours, 1, "the retry must record our result exactly once");
        assert_eq!(
            result_of(&events, "u/implementer#0")
                .unwrap()
                .unwrap()
                .output,
            "done"
        );
        // The concurrent writer's unrelated record survives alongside it (nothing lost).
        assert!(
            result_of(&events, "other/implementer#0").unwrap().is_some(),
            "the concurrent writer's record must survive the retry"
        );
    }

    #[test]
    fn record_result_if_absent_honors_a_self_report_that_won_the_race() {
        // The TOCTOU window the atomic CAS closes: the worker's own self-report lands in
        // the gap between the courier's read (which saw nothing) and its append. The
        // pinned append CONFLICTS; on retry the re-check now SEES the self-report and
        // returns None, so the courier's died-worker `--error` never clobbers the
        // success. Dropping either the retry arm or the in-loop re-check fails this.
        let inner = Store::open(":memory:").unwrap();
        // The racing writer is the worker itself, self-reporting SUCCESS for OUR id.
        let racing = SpawnResult::ok("u/implementer#0", "self-reported")
            .to_event()
            .unwrap();
        let store = RaceOnFirstAppend::new(inner, racing);

        // The death courier, believing the worker died, fires `--if-absent --error`.
        let skipped =
            record_result_if_absent(&store, &SpawnResult::failed("u/implementer#0", "died"))
                .unwrap();
        assert!(
            skipped.is_none(),
            "the self-report won the race; the re-check on retry must make this a no-op"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let results = events
            .iter()
            .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
            .count();
        assert_eq!(
            results, 1,
            "the losing courier must append no second result (no clobber, no duplicate)"
        );
        let got = result_of(&events, "u/implementer#0").unwrap().unwrap();
        assert!(
            !got.is_error(),
            "the self-reported success must stand, not be force-failed by the courier"
        );
        assert_eq!(got.output, "self-reported");
    }

    #[test]
    fn record_result_if_absent_is_atomic_across_two_connections() {
        // The criterion on the REAL topology: the death courier runs in a SEPARATE
        // PROCESS from the worker, so two sqlite connections (two `Store` handles on one
        // on-disk db, NO shared in-process mutex) genuinely overlap. This is the case an
        // in-process single-`Store` test cannot reach - one `Mutex<Connection>` serializes
        // its appends so they never contend - which is exactly why the sequential tests
        // above give false confidence. Here the worker self-reports SUCCESS via the plain
        // path while the courier fires `--if-absent --error`, round-synchronized so they
        // collide on the same id every round.
        //
        // Invariants (records atomically: no lost, no orphan, no hard-fail):
        //   - the courier's `--if-absent` never hard-fails (no cross-connection lock error);
        //   - the worker's self-report is never dropped;
        //   - every id ends with the worker's SUCCESS, never force-failed by the courier -
        //     because whenever the courier's `--error` could land, the worker's later
        //     success supersedes it, and whenever the success landed first the courier
        //     re-checks and no-ops.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let path = path.to_str().unwrap().to_string();

        // Two connections on one file, opened up front so we race only the appends.
        let worker_store = std::sync::Arc::new(Store::open(&path).unwrap());
        let courier_store = std::sync::Arc::new(Store::open(&path).unwrap());

        const ROUNDS: usize = 40;
        let ids: Vec<String> = (0..ROUNDS).map(|i| format!("u/implementer#{i}")).collect();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let w_ids = ids.clone();
        let w_barrier = barrier.clone();
        let w_store = worker_store.clone();
        let worker = std::thread::spawn(move || {
            let mut errs = 0usize;
            for id in &w_ids {
                w_barrier.wait();
                if record_result(w_store.as_ref(), &SpawnResult::ok(id, "self-reported")).is_err() {
                    errs += 1;
                }
            }
            errs
        });

        let c_ids = ids.clone();
        let c_barrier = barrier.clone();
        let c_store = courier_store.clone();
        let courier = std::thread::spawn(move || {
            let mut errs = 0usize;
            for id in &c_ids {
                c_barrier.wait();
                if record_result_if_absent(c_store.as_ref(), &SpawnResult::failed(id, "died"))
                    .is_err()
                {
                    errs += 1;
                }
            }
            errs
        });

        let worker_errs = worker.join().unwrap();
        let courier_errs = courier.join().unwrap();
        assert_eq!(
            courier_errs, 0,
            "the courier's --if-absent must never hard-fail on a cross-connection race"
        );
        assert_eq!(
            worker_errs, 0,
            "the worker's self-report must never be dropped on a cross-connection race"
        );

        let events = worker_store
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap();
        for id in &ids {
            let got = result_of(&events, id)
                .unwrap()
                .unwrap_or_else(|| panic!("{id} must have a recorded result (no orphan, no lost)"));
            assert!(
                !got.is_error(),
                "{id} must end with the worker's success, never force-failed by the courier"
            );
            assert_eq!(
                got.output, "self-reported",
                "the self-report must stand for {id}"
            );
        }
    }

    #[test]
    fn step_wave_reads_back_through_park_and_record_result_end_to_end() {
        // Integration smoke test: `park`/`record_result` (this module) compose correctly
        // with `crate::spawn::step_result` (the pure fold) through a real store - the
        // seam the thin driver actually drives every step.
        let store = Store::open(":memory:").unwrap();
        let old = SpawnRequest::new("plan", "plan", ROLE_IMPLEMENTER, 0, "plan it");
        park(&store, &old).unwrap();
        record_result(&store, &SpawnResult::ok(&old.id, "planned")).unwrap();
        park(
            &store,
            &SpawnRequest::new("b", "implement", ROLE_IMPLEMENTER, 0, "b"),
        )
        .unwrap();
        park(
            &store,
            &SpawnRequest::new("a", "implement", ROLE_IMPLEMENTER, 0, "a"),
        )
        .unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let step = step_result(&events).unwrap();
        let ids: Vec<&str> = step.wave.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["a/implementer#0", "b/implementer#0"],
            "the wave is every unanswered spawn, id-ordered, never the answered `plan`"
        );
        assert!(!step.done);
    }
}
