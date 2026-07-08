//! Live agent-progress telemetry (spec 14, Gap 27): a SEPARATE, non-replayed event store
//! that records what an agent is doing BETWEEN milestones.
//!
//! The event store the conductor drives records only milestones - `DecisionMade`,
//! `SpawnResult`, `GateVerdict` - so an agent can work for many minutes (grounding,
//! reading, editing, running gates) with the run stream showing a blackout. This store
//! closes that blind spot WITHOUT touching the replay-authoritative log: it lives in its
//! own file (`.rigger/progress.db`, a sibling of `.rigger/graph.db`), so NO run fold - the
//! ledger, the spawn frontier, the metrics, the conductor, the context graph - can ever
//! read it, and the run stream, its projections, and replay are byte-identical whether or
//! not any progress was ever emitted. Agents WRITE it via `rigger progress <id>
//! "<activity>"`; rigger READS it (unit 2's consolidator) to PRESENT a live per-agent view.

use std::collections::HashMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::eventstore::{Error, Event, EventStore, ExpectedRevision, Position};
use crate::run::META_RUN_ID;
use crate::spawn::{self, WaveItem};

/// The stream the progress store's events live on WITHIN its own db file. Its file already
/// isolates it from the run stream; the dedicated stream name keeps the store
/// self-describing (and lets a consolidator read exactly the progress events).
pub const STREAM: &str = "progress";

/// The event type an [`AgentProgress`] serializes as. It is deliberately NOT in the run
/// store, so nothing that folds the run stream can observe it - the isolation is the file
/// boundary, not a fold that skips this type.
pub const TYPE_AGENT_PROGRESS: &str = "AgentProgress";

/// One fine-grained progress report: the spawn it belongs to and a short human line of what
/// that agent just did (a grep, a build, a commit, a decision). The event's recorded
/// position and `recorded_at` order a spawn's reports and give each an age, so no explicit
/// sequence field is carried - the store's append ordering subsumes it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProgress {
    /// The deterministic spawn id this report is about (e.g. `s12-unit4/implementer#0`).
    pub id: String,
    /// A short, human, one-line description of the agent's latest step.
    pub activity: String,
}

impl AgentProgress {
    /// Build the appendable event, stamped with the run it belongs to (via [`META_RUN_ID`],
    /// the same key the conductor stamps on run events) so unit 2's consolidator can scope
    /// progress to the current run. An empty `run_id` (no run started yet) carries no stamp.
    fn to_event(&self, run_id: &str) -> Result<Event, serde_json::Error> {
        let ev = Event::new(TYPE_AGENT_PROGRESS, serde_json::to_vec(self)?);
        Ok(if run_id.is_empty() {
            ev
        } else {
            ev.with_meta(META_RUN_ID, run_id)
        })
    }
}

/// Record one progress report to the progress `store`, stamped with `run_id`. Append-only
/// and side-effect-free beyond the one event: a pure write, cheap to call after every
/// significant step. Returns the global position of the appended event.
pub fn record(
    store: &dyn EventStore,
    run_id: &str,
    id: &str,
    activity: &str,
) -> Result<Position, Error> {
    let progress = AgentProgress {
        id: id.to_string(),
        activity: activity.to_string(),
    };
    let ev = progress
        .to_event(run_id)
        .map_err(|e| Error::Backend(format!("serialize AgentProgress: {e}")))?;
    store.append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))
}

/// A live per-agent view (spec 14, unit 2): for one in-flight spawn, what stage it is at,
/// what it is currently doing (the latest progress report), how long since it last reported
/// activity and last touched its liveness marker, and its last run-stream milestone with its
/// age. The blackout this feature closes is exactly `milestone_age_s` >> `activity_age_s`:
/// the run store went quiet while the agent kept working.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentActivity {
    /// The deterministic spawn id.
    pub id: String,
    /// The unit the spawn belongs to.
    pub unit: String,
    /// The lifecycle stage the spawn is (implementer, adversary, adjudicator, ...).
    pub stage: String,
    /// The agent's latest reported activity, or `None` if it has reported none yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_activity: Option<String>,
    /// Whole seconds since that latest activity was reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_age_s: Option<u64>,
    /// Whole seconds since the spawn last touched its spec-10 liveness marker (rigger reads
    /// the marker in Rust and PRESENTS this, so no consumer stats the file).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liveness_age_s: Option<u64>,
    /// The type of the most recent run-stream milestone for this spawn's unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_milestone: Option<String>,
    /// Whole seconds since that milestone - the size of the current event-store blackout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_age_s: Option<u64>,
}

/// Consolidate rigger's signals into a live per-agent view for the current run's in-flight
/// frontier. PURE (no IO): `run_events` is the current run's slice, `progress_events` the
/// progress store's events for this run, `liveness_ages` the seconds-since-touch per spawn id
/// the caller read from the markers, and `now` fixes the clock for the age arithmetic
/// (deterministic in tests). One entry per in-flight spawn - a parked spawn with no recorded
/// result, the [`spawn::step_result`] frontier - ordered as that frontier is.
pub fn consolidate(
    run_events: &[Event],
    progress_events: &[Event],
    liveness_ages: &HashMap<String, u64>,
    now: SystemTime,
) -> Result<Vec<AgentActivity>, serde_json::Error> {
    let frontier = spawn::step_result(run_events)?.wave;

    // Latest progress per spawn id: the store appends in order, so a later event for the same
    // id overwrites the earlier one, leaving the most recent activity + when it was reported.
    let mut latest_prog: HashMap<String, (String, SystemTime)> = HashMap::new();
    for e in progress_events {
        if e.type_ != TYPE_AGENT_PROGRESS {
            continue;
        }
        if let Ok(ap) = serde_json::from_slice::<AgentProgress>(&e.data) {
            latest_prog.insert(ap.id, (ap.activity, e.recorded_at));
        }
    }

    // Latest run-stream milestone per unit: the most recent event whose data `id` is the unit
    // id (UnitStarted / UnitStatus / UnitIntegrated and the like - the unit's own lifecycle).
    let mut latest_ms: HashMap<String, (String, SystemTime)> = HashMap::new();
    for e in run_events {
        if let Some(uid) = event_unit_id(e) {
            latest_ms.insert(uid, (e.type_.clone(), e.recorded_at));
        }
    }

    let age = |t: SystemTime| now.duration_since(t).ok().map(|d| d.as_secs());
    Ok(frontier
        .into_iter()
        .map(|w: WaveItem| {
            let (latest_activity, activity_age_s) = match latest_prog.get(&w.id) {
                Some((a, t)) => (Some(a.clone()), age(*t)),
                None => (None, None),
            };
            let (last_milestone, milestone_age_s) = match latest_ms.get(&w.unit) {
                Some((ty, t)) => (Some(ty.clone()), age(*t)),
                None => (None, None),
            };
            AgentActivity {
                liveness_age_s: liveness_ages.get(&w.id).copied(),
                id: w.id,
                unit: w.unit,
                stage: w.stage,
                latest_activity,
                activity_age_s,
                last_milestone,
                milestone_age_s,
            }
        })
        .collect())
}

/// The `id` field of a lifecycle event's JSON data - the unit id for `UnitStarted` /
/// `UnitStatus` / `UnitIntegrated` and the like - or `None` for an event carrying no such
/// field (or non-JSON data). A minimal parse: just enough to attribute a run event to a unit
/// for the "last milestone" view; events keyed on something else (a decision id, a spawn id)
/// simply do not contribute a unit milestone.
fn event_unit_id(e: &Event) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(&e.data).ok()?;
    v.get("id")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::Direction;

    #[test]
    fn progress_lands_in_its_own_store_never_the_run_stream() {
        // spec 14, criterion 1: `rigger progress` records to the progress store, NEVER the
        // run stream - the isolation is the file boundary. A separate store cannot enter a
        // run fold, so the run stream and its projection are byte-identical with or without
        // progress. Two distinct stores stand in for `.rigger/events.db` and
        // `.rigger/progress.db`.
        let run = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();

        // A run stream with a lifecycle event.
        run.append(
            conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec())],
        )
        .unwrap();
        let before = run
            .read_stream(conductor::STREAM, 0, Direction::Forward)
            .unwrap();

        // Record progress to the SEPARATE store.
        record(
            &progress,
            "run-1",
            "u/implementer#0",
            "grep #3: conductor.rs",
        )
        .unwrap();

        // The run stream did not grow, and the run store holds NO AgentProgress anywhere -
        // progress never entered the replay-authoritative log.
        let after = run
            .read_stream(conductor::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            before.len(),
            after.len(),
            "recording progress must not touch the run stream"
        );
        assert!(
            run.read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "no AgentProgress may land in the run store"
        );

        // The report landed in the progress store's progress stream, stamped with its run.
        let p = progress.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].type_, TYPE_AGENT_PROGRESS);
        assert_eq!(
            p[0].meta.get(META_RUN_ID).map(String::as_str),
            Some("run-1"),
            "the report is scoped to the run that emitted it"
        );
        let ap: AgentProgress = serde_json::from_slice(&p[0].data).unwrap();
        assert_eq!(ap.id, "u/implementer#0");
        assert_eq!(ap.activity, "grep #3: conductor.rs");
    }

    #[test]
    fn consolidate_joins_frontier_progress_liveness_and_milestone() {
        // spec 14, criterion 2: for each in-flight spawn the consolidator yields its stage +
        // latest activity + activity-age + liveness-age + last milestone (and the milestone's
        // age - the blackout). The clock is passed in, so the ages are deterministic.
        use crate::spawn::SpawnRequest;
        use std::time::Duration;

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);

        // The unit started 5 min ago; its implementer is parked (in-flight, no result).
        let mut started = Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec());
        started.recorded_at = now - Duration::from_secs(300);
        let req = SpawnRequest::new("u", "u", "implementer", 0, "do it");
        let run_events = vec![started, req.to_event().unwrap()];

        // Two progress reports; the later one (20s ago) is the current activity.
        let mut p1 = AgentProgress {
            id: req.id.clone(),
            activity: "grep #1".into(),
        }
        .to_event("run-1")
        .unwrap();
        p1.recorded_at = now - Duration::from_secs(200);
        let mut p2 = AgentProgress {
            id: req.id.clone(),
            activity: "grep #12: conductor.rs".into(),
        }
        .to_event("run-1")
        .unwrap();
        p2.recorded_at = now - Duration::from_secs(20);
        let progress_events = vec![p1, p2];

        let liveness = HashMap::from([(req.id.clone(), 20u64)]);

        let view = consolidate(&run_events, &progress_events, &liveness, now).unwrap();
        assert_eq!(view.len(), 1, "one in-flight spawn");
        let a = &view[0];
        assert_eq!(a.id, req.id);
        assert_eq!(a.unit, "u");
        assert_eq!(a.stage, "u");
        assert_eq!(
            a.latest_activity.as_deref(),
            Some("grep #12: conductor.rs"),
            "the LATEST report wins"
        );
        assert_eq!(a.activity_age_s, Some(20));
        assert_eq!(a.liveness_age_s, Some(20));
        assert_eq!(a.last_milestone.as_deref(), Some("UnitStarted"));
        assert_eq!(
            a.milestone_age_s,
            Some(300),
            "the blackout: 5 min since the last store event, vs 20s of live activity"
        );
    }

    #[test]
    fn consolidate_reports_none_for_a_spawn_that_has_not_yet_progressed() {
        // An in-flight spawn with no progress and no marker yet: it still appears (from the
        // frontier), with the activity/liveness fields absent rather than fabricated.
        use crate::spawn::SpawnRequest;
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
        let req = SpawnRequest::new("u", "u", "adjudicator", 0, "judge");
        let view = consolidate(&[req.to_event().unwrap()], &[], &HashMap::new(), now).unwrap();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].latest_activity, None);
        assert_eq!(view[0].activity_age_s, None);
        assert_eq!(view[0].liveness_age_s, None);
        assert_eq!(view[0].last_milestone, None);
    }

    #[test]
    fn an_empty_run_id_records_without_a_run_stamp() {
        // Before any run has started (a legacy or bootstrapping store) progress still
        // records - it simply carries no run scope.
        let progress = Store::open(":memory:").unwrap();
        record(&progress, "", "u/implementer#0", "starting").unwrap();
        let p = progress.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(p.len(), 1);
        assert!(
            !p[0].meta.contains_key(META_RUN_ID),
            "no run stamp when no run has started"
        );
    }
}
