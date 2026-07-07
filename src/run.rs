//! Run scoping (spec 06, unit 1 - Gap 11): a run is the slice of the append-only
//! `run` stream that begins with a [`TYPE_RUN_STARTED`] event carrying a fresh run id.
//! The conductor folds ready work from ONLY the current run's slice, so a fresh run
//! never resurrects the non-terminal residue of an aborted prior run; prior runs stay
//! visible as memory (decisions, findings, `rigger peers`) but can never become live
//! work.
//!
//! Three read-models fold run state from the one stream - the [`crate::ledger`]
//! (durable run state), the [`crate::spawn`] frontier (the stepwise wave), and the
//! [`crate::metrics`] (`rigger stats`) - and all three scope through [`current_run`].
//! The run vocabulary (the `RunStarted` event type and the `run_id` metadata key every
//! conductor-emitted event carries) lives here so those modules share one source of
//! truth rather than re-declaring it.

use serde::{Deserialize, Serialize};

use crate::conductor::STREAM;
use crate::eventstore::{Direction, Error, Event, EventStore, ExpectedRevision};

/// The event a run opens with: a fresh run id plus the acceptance criteria the run
/// satisfies. Deliberately NOT one of the lifecycle events the ledger folds - the
/// ledger and the context graph ignore an unknown type - so only the run-scoping
/// helpers here read it, exactly as [`crate::spawn::TYPE_SPAWN_REQUESTED`] is read only
/// by the spawn fold.
pub const TYPE_RUN_STARTED: &str = "RunStarted";

/// The metadata key that stamps every conductor-emitted event with the id of the run it
/// belongs to (spec 06, unit 1). Its value is the current run's uuid; a pre-run-id
/// (legacy) event carries no such key and so belongs to no run - it folds as "before the
/// first RunStarted" and never becomes live work.
pub const META_RUN_ID: &str = "run_id";

/// The body of a [`TYPE_RUN_STARTED`] event: the run id and the criteria fingerprint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStarted {
    /// The fresh run id (a uuid) every event in this run is scoped by.
    pub run: String,
    /// The acceptance criteria this run satisfies - the per-campaign fingerprint that
    /// tells a RESUME (same criteria: adopt this run) from a NEW campaign (different
    /// criteria: begin a fresh run). Empty for a no-spec workflow run.
    #[serde(default)]
    pub criteria: Vec<String>,
}

impl RunStarted {
    /// Decode a [`TYPE_RUN_STARTED`] body, or `None` if it is malformed (so a corrupt
    /// event is simply ignored, like every other fold here).
    fn from_event(e: &Event) -> Option<RunStarted> {
        serde_json::from_slice(&e.data).ok()
    }

    /// Build the appendable event for this run start, with the run id stamped in
    /// [`META_RUN_ID`] so the RunStarted itself belongs to its own run's slice.
    fn to_event(&self) -> Result<Event, serde_json::Error> {
        Ok(Event::new(TYPE_RUN_STARTED, serde_json::to_vec(self)?)
            .with_meta(META_RUN_ID, &self.run))
    }
}

/// The latest [`RunStarted`] in `events`, decoded, or `None` when no run has started.
fn latest(events: &[Event]) -> Option<RunStarted> {
    events
        .iter()
        .rev()
        .find(|e| e.type_ == TYPE_RUN_STARTED)
        .and_then(RunStarted::from_event)
}

/// The current run's slice of `events`: the contiguous suffix from the LAST
/// [`TYPE_RUN_STARTED`] onward. When no run has started (a legacy store, or one this
/// feature has never scoped), the WHOLE slice is returned - so a store predating run
/// scoping folds exactly as before, and every fold that runs directly over raw events
/// is unchanged until a run actually begins.
///
/// A run's events are exactly this suffix: events are appended in order and a new run's
/// `RunStarted` is always appended after every prior run's events, so scoping by the
/// last boundary is scoping by run. Everything before that boundary - a prior run's
/// events, or pre-run-id legacy events - is excluded and can never become live work.
pub fn current_run(events: &[Event]) -> &[Event] {
    match events.iter().rposition(|e| e.type_ == TYPE_RUN_STARTED) {
        Some(i) => &events[i..],
        None => events,
    }
}

/// The id of the current (latest) run, or `None` when no run has started.
pub fn current_run_id(events: &[Event]) -> Option<String> {
    latest(events).map(|r| r.run)
}

/// Ensure a run is active for `criteria`, returning its run id.
///
/// If the latest run in the store was started for the SAME criteria, that run is
/// ADOPTED (resumed): nothing is appended, so a resume, an idle re-run, and a replay
/// step are all idempotent - no duplicate RunStarted fragments the run, and the
/// driver's `done` detection is preserved. Otherwise a FRESH run BEGINS: a new uuid
/// RunStarted stamped with `criteria` is appended and its id returned.
///
/// The criteria are the per-campaign fingerprint. They are the one signal derivable
/// from the log alone that distinguishes "continue the campaign in flight" from "a new
/// campaign over the same store", without re-minting on every step (which would split
/// one campaign across many runs). A legacy store with no RunStarted begins its first
/// run here; its prior events stay before that boundary and never become live work
/// (Gap 11: a new run no longer resurrects history's zombies).
pub fn ensure_started(store: &dyn EventStore, criteria: &[String]) -> Result<String, Error> {
    let events = store.read_stream(STREAM, 0, Direction::Forward)?;
    if let Some(run) = latest(&events) {
        if run.criteria.as_slice() == criteria {
            return Ok(run.run);
        }
    }
    start_fresh(store, criteria)
}

/// Begin a FRESH run for `criteria`, UNCONDITIONALLY: mint a new uuid `RunStarted` and
/// append it, returning the new run id.
///
/// Unlike [`ensure_started`], this never adopts the latest run even when its criteria
/// match. It is the operator's explicit "start over" (`rigger run --fresh`) - the evented
/// recovery from a run wedged in a terminal state whose spec is UNCHANGED. A plan-critique
/// escalation is terminal within its run slice (the resume short-circuit holds the fan-out
/// forever), and the escalation-recovery every other case relies on - fix the spec, so its
/// criteria change and `ensure_started` mints a fresh run - does not apply when the spec is
/// correct and the escalation was a defect since fixed. Without this, `ensure_started`
/// would adopt the wedged run on every relaunch.
///
/// This is additive, not destructive: the prior run stays in the log as history and
/// cross-run context (its decisions and findings remain visible through the whole-stream
/// graph and `rigger peers`); the new boundary simply begins a clean slice AFTER it, so
/// the conductor folds ready work from an empty prior state ([`current_run`] scopes to the
/// new boundary) and the wedged gate runs anew. It does not touch the git run branch, so a
/// fresh run starts over atop whatever that branch already holds.
pub fn start_fresh(store: &dyn EventStore, criteria: &[String]) -> Result<String, Error> {
    let started = RunStarted {
        run: uuid::Uuid::new_v4().to_string(),
        criteria: criteria.to_vec(),
    };
    let ev = started
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize RunStarted: {e}")))?;
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))?;
    Ok(started.run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;

    fn ev(type_: &str, data: &str) -> Event {
        Event::new(type_, data.as_bytes().to_vec())
    }

    fn run_started(run: &str, criteria: &[&str]) -> Event {
        RunStarted {
            run: run.to_string(),
            criteria: criteria.iter().map(|s| s.to_string()).collect(),
        }
        .to_event()
        .unwrap()
    }

    #[test]
    fn current_run_is_the_whole_slice_when_no_run_has_started() {
        // A legacy store (pre-run-id events, no RunStarted) folds whole-stream, exactly
        // as before run scoping - so every existing direct fold is untouched.
        let events = vec![
            ev("UnitStarted", r#"{"id":"a"}"#),
            ev("UnitStarted", r#"{"id":"b"}"#),
        ];
        assert_eq!(current_run(&events).len(), 2);
        assert!(current_run_id(&events).is_none());
    }

    #[test]
    fn current_run_is_the_suffix_from_the_last_run_started() {
        // Two prior units, then a run begins, then one live unit. The current run is the
        // RunStarted and everything after it - never the prior units.
        let events = vec![
            ev("UnitStarted", r#"{"id":"zombie-1"}"#),
            ev("UnitStarted", r#"{"id":"zombie-2"}"#),
            run_started("r1", &["crit"]),
            ev("UnitStarted", r#"{"id":"live"}"#),
        ];
        let slice = current_run(&events);
        assert_eq!(
            slice.len(),
            2,
            "RunStarted + the one live unit, never the zombies"
        );
        assert_eq!(slice[0].type_, TYPE_RUN_STARTED);
        assert_eq!(current_run_id(&events).as_deref(), Some("r1"));
        // None of the prior zombie units are in the current run's slice.
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("zombie")),
            "prior-run residue is excluded from the current run"
        );
    }

    #[test]
    fn current_run_is_the_suffix_from_the_latest_of_several_runs() {
        let events = vec![
            run_started("r1", &["a"]),
            ev("UnitStarted", r#"{"id":"r1-unit"}"#),
            run_started("r2", &["b"]),
            ev("UnitStarted", r#"{"id":"r2-unit"}"#),
        ];
        let slice = current_run(&events);
        assert_eq!(slice.len(), 2);
        assert_eq!(current_run_id(&events).as_deref(), Some("r2"));
        assert!(String::from_utf8_lossy(&slice[1].data).contains("r2-unit"));
    }

    #[test]
    fn ensure_started_mints_a_fresh_run_on_an_empty_store() {
        let store = Store::open(":memory:").unwrap();
        let run = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert!(!run.is_empty(), "a fresh run id is minted");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let starts: Vec<_> = events
            .iter()
            .filter(|e| e.type_ == TYPE_RUN_STARTED)
            .collect();
        assert_eq!(starts.len(), 1, "exactly one RunStarted is appended");
        assert_eq!(
            starts[0].meta.get(META_RUN_ID).map(String::as_str),
            Some(run.as_str()),
            "the RunStarted carries its own run id in metadata"
        );
        assert_eq!(
            RunStarted::from_event(starts[0]).unwrap().criteria,
            ["crit"]
        );
    }

    #[test]
    fn ensure_started_adopts_the_same_criteria_run_without_re_minting() {
        // Resume / idle re-run / replay step: the same criteria adopt the existing run and
        // append NOTHING, so a run is never fragmented across step processes.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        let again = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(first, again, "the same criteria adopt the same run id");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            1,
            "adopting a run appends no second RunStarted"
        );
    }

    #[test]
    fn start_fresh_mints_a_new_run_even_when_the_criteria_match() {
        // `rigger run --fresh`: the operator's explicit "start over". Where `ensure_started`
        // ADOPTS a same-criteria run, `start_fresh` ALWAYS appends a new boundary, so a run
        // wedged in a terminal state (e.g. an escalated plan-critique) whose spec is
        // unchanged can be re-run cleanly. The prior run stays in the log; the new slice
        // begins after it.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        // Some residue lands in the first run (a terminal escalation, say).
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[ev(
                    "UnitStatus",
                    r#"{"id":"plan-critique","status":"escalated"}"#,
                )],
            )
            .unwrap();

        let fresh = start_fresh(&store, &["crit".to_string()]).unwrap();
        assert_ne!(
            first, fresh,
            "start_fresh mints a distinct run even though the criteria are identical"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            2,
            "--fresh appended a second RunStarted rather than adopting the wedged run"
        );
        // The current slice is the fresh boundary onward - the prior run's escalated residue
        // sits BEFORE it and can never seed live work, so the gate runs anew.
        let slice = current_run(&events);
        assert_eq!(current_run_id(&events).as_deref(), Some(fresh.as_str()));
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("escalated")),
            "the prior run's terminal residue is excluded from the fresh run's slice"
        );
        // A subsequent ensure_started (as the conductor calls internally) ADOPTS the fresh
        // boundary - so `rigger run --fresh` drives the clean run it just began.
        let adopted = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(
            adopted, fresh,
            "the conductor adopts the freshly-started run"
        );
    }

    #[test]
    fn ensure_started_mints_a_new_run_when_the_criteria_change() {
        // A new campaign (different acceptance criteria) begins a fresh run, so the prior
        // campaign's residue is left behind the new boundary.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["old".to_string()]).unwrap();
        let second = ensure_started(&store, &["new".to_string()]).unwrap();
        assert_ne!(first, second, "changed criteria begin a distinct run");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == TYPE_RUN_STARTED)
                .count(),
            2,
            "the new campaign appended its own RunStarted"
        );
        assert_eq!(current_run_id(&events).as_deref(), Some(second.as_str()));
    }

    #[test]
    fn ensure_started_over_legacy_events_leaves_them_before_the_boundary() {
        // The Gap 11 case: a store holding stale non-terminal units of an aborted
        // pre-run-id run. The first run begins here; the stale units are before the
        // RunStarted, so the current run's slice never contains them.
        let store = Store::open(":memory:").unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[
                    ev("UnitStarted", r#"{"id":"u-zombie"}"#),
                    ev(
                        "SpawnRequested",
                        r#"{"id":"u-zombie/implementer#0","unit":"u-zombie"}"#,
                    ),
                ],
            )
            .unwrap();

        ensure_started(&store, &["crit".to_string()]).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let slice = current_run(&events);
        assert_eq!(slice.len(), 1, "only the RunStarted is in the fresh run");
        assert_eq!(slice[0].type_, TYPE_RUN_STARTED);
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("zombie")),
            "the aborted prior run's zombies are before the boundary and never live"
        );
    }
}
