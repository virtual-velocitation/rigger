//! Live agent-progress telemetry (spec 14, Gap 27): a SEPARATE, non-replayed event store
//! that records what an agent is doing BETWEEN milestones.
//!
//! The event store the conductor drives records only milestones - `DecisionMade`,
//! `SpawnResult`, `GateVerdict` - so an agent can work for many minutes (grounding,
//! reading, editing, running gates) with the run stream showing a blackout. This store
//! closes that blind spot WITHOUT touching the replay-authoritative log: it lives in its
//! own file (`.rigger/progress.db`, a sibling of `.rigger/graph.db`), so NO run-stream fold -
//! the ledger, the spawn frontier, [`crate::metrics::project`], the conductor, the context
//! graph - can ever read it, and the run stream, its projections, and replay are
//! byte-identical whether or not any progress was ever emitted. Agents WRITE it via `rigger
//! progress <id> "<activity>"`; rigger READS it for PRESENTATION ONLY - unit 2's consolidator
//! folds it into the live per-agent view, and [`crate::metrics::grep_fallbacks`] counts this
//! store's `grep-fallback:` lines for the dash - never in a run-stream fold, so the isolation
//! that keeps replay byte-identical is preserved.

use std::collections::HashMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::eventstore::{Direction, Error, Event, EventStore};
use crate::run::META_RUN_ID;
use crate::spawn::{self, WaveItem};

/// The stream the progress store's events live on WITHIN its own db file. Its file already
/// isolates it from the run stream; the dedicated stream name keeps the store
/// self-describing (and lets a consolidator read exactly the progress events).
pub const STREAM: &str = "progress";

/// The stream ONE run's progress lives on within the progress store (spec 101): `progress/<run
/// id>`, or [`STREAM`] itself for a report made before any run started. A stream per run is the
/// run boundary in this store, so a reader folds its run's progress by reading that stream from
/// its start - exactly the run's reports, never another run's.
pub fn stream_of(run_id: &str) -> String {
    if run_id.is_empty() {
        STREAM.to_string()
    } else {
        format!("{STREAM}/{run_id}")
    }
}

/// Run `run_id`'s progress reports out of the progress `store`, oldest first: its own stream
/// ([`stream_of`]) read from the start, so the read costs exactly the run's reports - the one
/// read `rigger status`, the dash and the `rigger_activity` tool fold.
pub fn read_run(store: &dyn EventStore, run_id: &str) -> Result<Vec<Event>, Error> {
    store.read_stream(&stream_of(run_id), 0, Direction::Forward)
}

/// A progress-store record that appends as one event of its own [`EVENT_TYPE`](Self::EVENT_TYPE),
/// stamped with the run it belongs to - the ONE shape every record this store holds is built
/// with ([`AgentProgress`], [`SpawnLaunched`], [`StopFailure`]).
pub trait RunStamped: Serialize {
    /// The event type this record serializes as.
    const EVENT_TYPE: &'static str;

    /// Build the appendable event, stamped with the run it belongs to (via [`META_RUN_ID`],
    /// the same key the conductor stamps on run events) so unit 2's consolidator can scope
    /// progress to the current run. An empty `run_id` (no run started yet) carries no stamp.
    /// `pub(crate)`: the impure write path ([`crate::progress_store`]) is the only caller
    /// outside this module.
    #[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the *_store sibling under core-only
    fn to_stamped_event(&self, run_id: &str) -> Result<Event, serde_json::Error> {
        let ev = Event::new(Self::EVENT_TYPE, serde_json::to_vec(self)?);
        Ok(if run_id.is_empty() {
            ev
        } else {
            ev.with_meta(META_RUN_ID, run_id)
        })
    }
}

/// The event type an [`AgentProgress`] serializes as. It is deliberately NOT in the run
/// store, so nothing that folds the run stream can observe it - the isolation is the file
/// boundary, not a fold that skips this type.
pub const TYPE_AGENT_PROGRESS: &str = "AgentProgress";

/// The prefix an agent stamps on a progress line when it fell back to grep over the PROJECT'S
/// SOURCES because the graph could not answer (spec 58): `rigger progress <id> 'grep-fallback:
/// <what the graph did not answer>'`. Every such line is a graph-coverage gap recorded IN the
/// event log; [`crate::metrics::grep_fallbacks`] counts them per run so the fallback rate is
/// visible run-over-run. It is the single source of truth for what a fallback line looks like,
/// shared by the writer (the grounding pointer's instruction) and the reader (the metric).
pub const GREP_FALLBACK_PREFIX: &str = "grep-fallback:";

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
    /// Whether this progress line reports a grep fallback over the project's sources - its
    /// [`activity`](AgentProgress::activity) begins with [`GREP_FALLBACK_PREFIX`] (a single
    /// leading run of whitespace tolerated). This is the ONE definition of a fallback line;
    /// [`crate::metrics::grep_fallbacks`] counts the lines this returns `true` for.
    pub fn is_grep_fallback(&self) -> bool {
        self.activity.trim_start().starts_with(GREP_FALLBACK_PREFIX)
    }
}

impl RunStamped for AgentProgress {
    const EVENT_TYPE: &'static str = TYPE_AGENT_PROGRESS;
}

/// The event type a [`SpawnLaunched`] record serializes as (spec 104 criterion 1: THE
/// LAUNCH IS TYPED). Recorded to the progress store - never the run stream, same file
/// boundary as [`TYPE_AGENT_PROGRESS`] - the moment the host starts a child agent process,
/// so a fresh process can always tell whether a spawn's latest launch ever actually
/// started, from the log alone.
pub const TYPE_SPAWN_LAUNCHED: &str = "SpawnLaunched";

/// One host-issued launch of a spawn's agent process
/// (`docs/architecture-addendum-claude-code-integration.md` §4.1). The host mints
/// [`session_id`](Self::session_id) and records this BEFORE starting the child - "before
/// the child starts" is the caller's ordering to keep, not something this pure type can
/// enforce itself. The SAME type serves both halves of the record (spec 104 criteria
/// 5/6): an OPEN write carries `started` with `ended`/`class` both `None`; a CLOSING write
/// ([`SpawnLaunched::closed`]) is a SEPARATE appended event (the progress store is
/// append-only) for the same `spawn`/`launch`, with `ended`/`class` set - the LATEST record
/// per (`spawn`, `launch`) key is the one that says whether the launch is still open.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnLaunched {
    /// The deterministic spawn id this launch belongs to (e.g. `u/implementer#0`).
    pub spawn: String,
    /// The 0-based ordinal of this launch within the spawn's current attempt: 0 for the
    /// first launch, N for the Nth relaunch after an API-side fault. Distinct from the
    /// spawn id's own `#<attempt>` remediation counter.
    pub launch: u32,
    /// The session id the host minted for this launch (`--session-id`/`--resume`).
    pub session_id: String,
    /// The session this launch CONTINUES (`claude -p --resume <session_id>`), or `None`
    /// for a fresh launch - every launch this spec (104) itself performs, since resuming
    /// a held session is spec 105's concern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumed_from: Option<String>,
    /// Unix seconds the host started this launch. Caller-supplied: this type never reads
    /// the clock itself, so it stays usable from a pure/core call site.
    pub started: u64,
    /// How this launch ENDED, or `None` while still open (spec 104 criteria 5/6; the
    /// architecture addendum §4.1's own "at exit it closes the record with `ended:
    /// completed | interrupted | fault | stopped` and the class"): one of `"completed"`
    /// (a real result landed), `"interrupted"` (the supervisor found this record still
    /// open at start-up), `"fault"`
    /// (criterion 5's own no-result API-side ending), or `"stopped"` (criterion 6's
    /// wall-clock expiry). Deliberately a plain string, not an enum: THE STREAM,
    /// FAILURE CLASS and STOP each write their own literal, so none of the three has to
    /// land before another can close a record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// The failure class this launch ended under (criterion 5's `AgentFailure` category,
    /// or this driver's own infra class for a wall-clock STOP) - `None` for `completed`
    /// and `interrupted`, which carry no failure to classify.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
}

impl RunStamped for SpawnLaunched {
    const EVENT_TYPE: &'static str = TYPE_SPAWN_LAUNCHED;
}

impl SpawnLaunched {
    /// Build the CLOSING record for an already-open launch (spec 104 criteria 5/6): the
    /// progress store is append-only (spec 93), so closing is a SEPARATE event, never a
    /// mutation of the open record - this is the one place that shape is assembled, so
    /// FAILURE CLASS, STOP and the supervisor reconciliation share it rather than each
    /// building a divergent literal. `session_id` identifies WHICH launch is closing;
    /// `started`/`resumed_from` are left at their defaults because only the LATEST
    /// record's `ended` decides open-vs-closed - the
    /// original open record (already durable) is what carries those. An empty `class`
    /// stores as `None` (the `completed`/`interrupted` endings carry no failure class).
    pub fn closed(
        spawn: impl Into<String>,
        launch: u32,
        session_id: impl Into<String>,
        ended: &str,
        class: &str,
    ) -> SpawnLaunched {
        SpawnLaunched {
            spawn: spawn.into(),
            launch,
            session_id: session_id.into(),
            resumed_from: None,
            started: 0,
            ended: Some(ended.to_string()),
            class: if class.is_empty() {
                None
            } else {
                Some(class.to_string())
            },
        }
    }
}

/// The event type a [`StopFailure`] record serializes as (spec 104 criterion 5: A FAILURE
/// HAS A CLASS). Recorded to the progress store - never the run stream, same file boundary
/// as [`TYPE_SPAWN_LAUNCHED`] - by `rigger hook stop-failure --spawn <id> --class <category>`,
/// the per-spawn settings' `StopFailure` hook family (Design's THE HOOKS: "criterion 5's,
/// command, record and injection both"). Claude Code invokes that command the moment a turn
/// ends on one of [`crate::conductor::AgentFailure`]'s categories, so the class
/// survives even when the stream's own last line never arrives.
pub const TYPE_STOP_FAILURE: &str = "StopFailure";

/// One `StopFailure` hook firing: the spawn it ended and the error category Claude Code
/// reported for it. Keyed by [`spawn`](Self::spawn) ALONE - the hook command's own argv
/// (`--spawn <id> --class <category>`) carries no launch ordinal - so
/// [`latest_stop_failure_class`] folds "latest wins" exactly like [`AgentProgress`]'s own
/// per-id fold does, never a second per-launch scoping scheme the command has no argument
/// to carry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopFailure {
    /// The deterministic spawn id this firing belongs to (e.g. `u/implementer#0`).
    pub spawn: String,
    /// Claude Code's own error-category string (e.g. `authentication_failed`), exactly as
    /// carried on the hook's installed `--class` argument - never re-derived or normalized
    /// here, so a category this crate's [`crate::conductor::AgentFailure`] does
    /// not yet recognize still records faithfully rather than being coerced at write time.
    pub class: String,
}

impl RunStamped for StopFailure {
    const EVENT_TYPE: &'static str = TYPE_STOP_FAILURE;
}

/// The latest `StopFailure` class recorded for `spawn_id` - "latest wins", the store's own
/// append order breaking any tie, exactly like [`AgentProgress`]'s per-id fold in
/// [`consolidate`]. PURE (no IO): `progress_events` is already-read progress-store events.
/// This is FAILURE CLASS's first-priority source (spec 104 criterion 5, Design: "the class
/// from, in order: the record written by the `StopFailure` hook ... else `unknown`"). `None`
/// when no `StopFailure` event names this spawn.
pub fn latest_stop_failure_class(progress_events: &[Event], spawn_id: &str) -> Option<String> {
    let mut found = None;
    for e in progress_events {
        if e.type_ != TYPE_STOP_FAILURE {
            continue;
        }
        if let Ok(sf) = serde_json::from_slice::<StopFailure>(&e.data) {
            if sf.spawn == spawn_id {
                found = Some(sf.class);
            }
        }
    }
    found
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

    /// A run's progress lives on its own stream; a report made before any run started lives on
    /// the unstamped [`STREAM`].
    #[test]
    fn each_run_reports_on_its_own_stream_and_a_run_less_report_on_the_bare_one() {
        assert_eq!(stream_of("run-c"), "progress/run-c");
        assert_eq!(stream_of(""), "progress");
    }
    use crate::eventstore::Event;
    use crate::run::META_RUN_ID;
    use crate::spawn::SpawnEvent;
    use std::collections::HashMap;
    use std::time::SystemTime;

    #[test]
    fn consolidate_joins_frontier_progress_liveness_and_milestone() {
        // spec 14, criterion 2: for each in-flight spawn the consolidator yields its stage +
        // latest activity + activity-age + liveness-age + last milestone (and the milestone's
        // age - the blackout). The clock is passed in, so the ages are deterministic.
        use std::time::Duration;

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);

        // The unit started 5 min ago; its implementer is parked (in-flight, no result).
        let mut started = Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec());
        started.recorded_at = now - Duration::from_secs(300);
        let req = crate::spawn::test_request("u", "u", "implementer", 0, "do it");
        let run_events = vec![started, req.to_event().unwrap()];

        // Two progress reports; the later one (20s ago) is the current activity.
        let mut p1 = AgentProgress {
            id: req.id.clone(),
            activity: "grep #1".into(),
        }
        .to_stamped_event("run-1")
        .unwrap();
        p1.recorded_at = now - Duration::from_secs(200);
        let mut p2 = AgentProgress {
            id: req.id.clone(),
            activity: "grep #12: conductor.rs".into(),
        }
        .to_stamped_event("run-1")
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
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
        let req = crate::spawn::test_request("u", "u", "adjudicator", 0, "judge");
        let view = consolidate(&[req.to_event().unwrap()], &[], &HashMap::new(), now).unwrap();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].latest_activity, None);
        assert_eq!(view[0].activity_age_s, None);
        assert_eq!(view[0].liveness_age_s, None);
        assert_eq!(view[0].last_milestone, None);
    }

    #[test]
    fn spawn_launched_to_event_carries_the_type_and_run_stamp() {
        // spec 104 criterion 1: the launch record serializes as SpawnLaunched, stamped
        // with its run exactly like AgentProgress - the event boundary this store's
        // isolation depends on, not a new mechanism.
        let launched = SpawnLaunched {
            spawn: "u104-launch/implementer#0".into(),
            launch: 0,
            session_id: "11111111-1111-4111-8111-111111111111".into(),
            resumed_from: None,
            started: 1_700_000_000,
            ended: None,
            class: None,
        };
        let ev = launched.to_stamped_event("run-9").unwrap();
        assert_eq!(ev.type_, TYPE_SPAWN_LAUNCHED);
        assert_eq!(ev.meta.get(META_RUN_ID).map(String::as_str), Some("run-9"));
        let back: SpawnLaunched = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, launched);
    }

    #[test]
    fn spawn_launched_omits_resumed_from_when_none() {
        // A fresh launch (every launch spec 104 itself performs) must not serialize a
        // `resumed_from` field at all - the future resume reader (spec 105) can then
        // treat "field absent" and "field null" the same way, and today's payload stays
        // minimal.
        let launched = SpawnLaunched {
            spawn: "u/implementer#0".into(),
            launch: 0,
            session_id: "s".into(),
            resumed_from: None,
            started: 1,
            ended: None,
            class: None,
        };
        let ev = launched.to_stamped_event("").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&ev.data).unwrap();
        assert!(
            v.get("resumed_from").is_none(),
            "resumed_from must be omitted, not null: {v:?}"
        );
        assert!(
            v.get("ended").is_none(),
            "ended must be omitted while open, not null: {v:?}"
        );
        assert!(
            v.get("class").is_none(),
            "class must be omitted while open, not null: {v:?}"
        );
        assert!(
            !ev.meta.contains_key(META_RUN_ID),
            "an empty run_id carries no stamp, same as AgentProgress"
        );
    }

    #[test]
    fn spawn_launched_carries_a_relaunchs_resumed_from() {
        let launched = SpawnLaunched {
            spawn: "u/implementer#0".into(),
            launch: 1,
            session_id: "new-session".into(),
            resumed_from: Some("old-session".into()),
            started: 2,
            ended: None,
            class: None,
        };
        let ev = launched.to_stamped_event("run-1").unwrap();
        let back: SpawnLaunched = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back.resumed_from.as_deref(), Some("old-session"));
        assert_eq!(back.launch, 1);
    }

    // ---- ended/class closing (spec 104 criteria 5/6) ----

    /// `SpawnLaunched::closed` over `ended`/`class` records `ended` and the class as `class`
    /// (an empty class stored as `None`); the record is returned for further checks.
    fn assert_closed(ended: &str, class: &str, want_class: Option<&str>) -> SpawnLaunched {
        let closing = SpawnLaunched::closed("u/implementer#0", 0, "sess-1", ended, class);
        assert_eq!(closing.ended.as_deref(), Some(ended));
        assert_eq!(closing.class.as_deref(), want_class);
        closing
    }

    crate::test_cases! {
        spawn_launched_closed_builds_a_closing_record_with_ended_and_class: {
            let closing = assert_closed("stopped", "infra", Some("infra"));
            assert_eq!(closing.spawn, "u/implementer#0");
            assert_eq!(closing.launch, 0);
            assert_eq!(closing.session_id, "sess-1");
        };
        // completed/interrupted carry no failure class.
        spawn_launched_closed_stores_an_empty_class_as_none: assert_closed("completed", "", None);
    }

    #[test]
    fn spawn_launched_closed_round_trips_through_json() {
        let closing = SpawnLaunched::closed("u/implementer#0", 2, "sess-9", "fault", "rate_limit");
        let ev = closing.to_stamped_event("run-1").unwrap();
        let back: SpawnLaunched = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, closing);
    }

    // ---- StopFailure (spec 104 criterion 5: A FAILURE HAS A CLASS) ----

    #[test]
    fn stop_failure_to_event_carries_the_type_and_run_stamp() {
        let sf = StopFailure {
            spawn: "u104-fail-class/implementer#0".into(),
            class: "authentication_failed".into(),
        };
        let ev = sf.to_stamped_event("run-9").unwrap();
        assert_eq!(ev.type_, TYPE_STOP_FAILURE);
        assert_eq!(ev.meta.get(META_RUN_ID).map(String::as_str), Some("run-9"));
        let back: StopFailure = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, sf);
    }

    #[test]
    fn stop_failure_omits_the_run_stamp_when_run_id_is_empty() {
        let sf = StopFailure {
            spawn: "u/implementer#0".into(),
            class: "rate_limit".into(),
        };
        let ev = sf.to_stamped_event("").unwrap();
        assert!(
            !ev.meta.contains_key(META_RUN_ID),
            "an empty run_id carries no stamp, same as SpawnLaunched/AgentProgress"
        );
    }

    #[test]
    fn latest_stop_failure_class_returns_none_when_no_record_names_the_spawn() {
        let events = vec![StopFailure {
            spawn: "other/implementer#0".into(),
            class: "rate_limit".into(),
        }
        .to_stamped_event("run-1")
        .unwrap()];
        assert_eq!(latest_stop_failure_class(&events, "u/implementer#0"), None);
    }

    #[test]
    fn latest_stop_failure_class_the_latest_record_for_the_spawn_wins() {
        // Two firings for the SAME spawn (a relaunch can each carry its own) - append order
        // breaks the tie, "latest wins", exactly like AgentProgress's own per-id fold.
        let events = vec![
            StopFailure {
                spawn: "u/implementer#0".into(),
                class: "rate_limit".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
            StopFailure {
                spawn: "other/implementer#0".into(),
                class: "overloaded".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
            StopFailure {
                spawn: "u/implementer#0".into(),
                class: "authentication_failed".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
        ];
        assert_eq!(
            latest_stop_failure_class(&events, "u/implementer#0").as_deref(),
            Some("authentication_failed")
        );
    }

    #[test]
    fn latest_stop_failure_class_ignores_a_differently_typed_event() {
        let mut events = vec![AgentProgress {
            id: "u/implementer#0".into(),
            activity: "not a stop failure".into(),
        }
        .to_stamped_event("run-1")
        .unwrap()];
        events.push(
            StopFailure {
                spawn: "u/implementer#0".into(),
                class: "billing_error".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
        );
        assert_eq!(
            latest_stop_failure_class(&events, "u/implementer#0").as_deref(),
            Some("billing_error")
        );
    }
}
