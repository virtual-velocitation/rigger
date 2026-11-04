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
//! progress <id> "<activity>"`, and the host and the supervisor record what they see on it
//! ([`SpawnLaunched`], [`StopFailure`], [`ApiRetry`], [`SupervisorBeat`]). rigger READS it live -
//! unit 2's consolidator folds it into the per-agent view, [`crate::metrics::grep_fallbacks`]
//! counts this store's `grep-fallback:` lines for the dash, and the host classes a failed session
//! from its [`StopFailure`] records, counts a run's failed launches from its closes
//! ([`failed_closes_since`]), times a session's silence from its [`ApiRetry`] and
//! [`SupervisorBeat`] records, and a supervisor's start closes every launch it holds open
//! ([`open_launches`]) - never in a run-stream fold, so the isolation that keeps replay
//! byte-identical is preserved. A close carries the run stream's revision its writer last read,
//! the one order a progress record takes against a run-stream event.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::eventstore::{Direction, Error, Event, EventStore, Revision};
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
/// with ([`AgentProgress`], [`SpawnLaunched`], [`StopFailure`], [`ApiRetry`],
/// [`SupervisorBeat`]).
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
    /// The run stream's revision a hold-probe line probed (the revision of the hold's latest
    /// `RunHeld`, [`crate::hold_probe::OwedProbe::line`]), `None` on every other line.
    /// [`crate::hold_probe::owed_probe`] counts a hold's probes by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<Revision>,
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

/// The [`AgentProgress`] events among `progress_events`, in their order: the one reader of the
/// agents' progress lines on a progress-store slice, which also carries [`SpawnLaunched`],
/// [`StopFailure`], [`ApiRetry`] and [`SupervisorBeat`] records. It leaves a line's body
/// undecoded, so a reader that renders a malformed body rather than dropping it keeps the line.
pub fn agent_progress_events(progress_events: &[Event]) -> impl Iterator<Item = &Event> {
    progress_events
        .iter()
        .filter(|e| e.type_ == TYPE_AGENT_PROGRESS)
}

/// Each [`AgentProgress`] line [`agent_progress_events`] reads, beside its event, in their
/// order: a body that does not decode names no line.
pub fn decoded_progress(
    progress_events: &[Event],
) -> impl Iterator<Item = (&Event, AgentProgress)> {
    agent_progress_events(progress_events)
        .filter_map(|e| Some((e, serde_json::from_slice(&e.data).ok()?)))
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
    /// first launch, and the ordinal after the highest recorded for the spawn for every later
    /// one, whatever brought it: a relaunch after an API-side fault, the resume fallback or a
    /// restart. Distinct from the spawn id's own `#<attempt>` remediation counter.
    pub launch: u32,
    /// The session id the host minted for this launch (`--session-id`/`--resume`).
    pub session_id: String,
    /// The session this launch CONTINUES (`claude -p --resume <session_id>`), or `None`
    /// for a fresh launch: a later attempt or review round of a role continues the session
    /// its earlier spawn on the same unit recorded (`SpawnOpts::resumed_from`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumed_from: Option<String>,
    /// Unix seconds the host started this launch. Caller-supplied: this type never reads
    /// the clock itself, so it stays usable from a pure/core call site.
    pub started: u64,
    /// How this launch ENDED, or `None` while still open (spec 104 criteria 5/6; the
    /// architecture addendum §4.1's own "at exit it closes the record with `ended:
    /// completed | interrupted | fault | stopped` and the class"): one of
    /// [`ENDED_COMPLETED`] (a first non-error `result` landed), [`ENDED_INTERRUPTED`] (the
    /// host ended a `--resume` launch that yielded no `system/init` and fell back to a fresh
    /// one, or a host with a hold sink closed in place of a `fault` an ending whose class holds
    /// or waits, or any ending on a held run, or a supervisor's start found the launch still
    /// open), [`ENDED_FAULT`] (any other session that left no usable `result`), or
    /// [`ENDED_STOPPED`] (THE STOP's silence expiry). A plain string so the record reads
    /// whatever a later writer's ending says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// The failure class this launch ended under, criterion 5's `AgentFailure` category: a
    /// `fault` close's and that of an `interrupted` close written in a `fault`'s place, which keeps
    /// why the launch ended. `None` for `completed`, `stopped`, the resume fallback's `interrupted`
    /// and the start-up reconciliation's, which carry no class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// The run stream's revision the host last read when it wrote this close, `None` on an
    /// open record, on a close written before spec 105, when the host holds no sink to read
    /// it from and when that read failed. [`failed_closes_since`] orders a close against a
    /// `RunReleased` by it, an unstamped close by the last stamp before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<Revision>,
    /// The directory the host launched the session in, empty on a close, on an open record
    /// written before spec 105 and for a launch in the current dir. Its one reader is a
    /// supervisor start's reconciliation (`reconcile_open_launches` in `src/cli/run.rs`), which,
    /// for a launch [`open_launches`] finds open, reaps what is still rooted under it only when it
    /// lies strictly under the project's scratch root.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
}

impl RunStamped for SpawnLaunched {
    const EVENT_TYPE: &'static str = TYPE_SPAWN_LAUNCHED;
}

impl SpawnLaunched {
    /// Build the CLOSING record for an already-open launch (spec 104 criteria 5/6): the
    /// progress store is append-only (spec 93), so closing is a SEPARATE event, never a
    /// mutation of the open record - this is the one place that shape is assembled, so
    /// FAILURE CLASS, STOP, THE RESUME FALLBACK and the supervisor reconciliation share it
    /// rather than each building a divergent literal. `session_id` identifies WHICH launch is
    /// closing; `started`/`resumed_from` are left at their defaults because only the LATEST
    /// record's [`is_open`](Self::is_open) decides open-vs-closed - the
    /// original open record (already durable) is what carries those, the
    /// [`dir`](Self::dir) included. An empty `class`
    /// stores as `None` (see [`class`](Self::class) for the closes that carry none). The
    /// [`revision`](Self::revision) stays `None`: the host stamps its own closes, and a
    /// supervisor's reconciliation close orders against nothing.
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
            revision: None,
            dir: String::new(),
        }
    }

    /// Whether this record leaves its launch open: it carries no [`ended`](Self::ended). The one
    /// spelling of an open record, which [`open_launches`], [`latest_stop_failure_class`] and
    /// [`failed_closes_since`] read.
    pub fn is_open(&self) -> bool {
        self.ended.is_none()
    }
}

/// The `ended` of a launch whose first `result` was not an error.
pub const ENDED_COMPLETED: &str = "completed";

/// The `ended` of a resume that fell back, of an ending a hold answers and of a launch a
/// supervisor's start found open: no launch failed.
pub const ENDED_INTERRUPTED: &str = "interrupted";

/// The `ended` of a launch that left no usable `result`.
pub const ENDED_FAULT: &str = "fault";

/// The `ended` of a launch THE STOP ended.
pub const ENDED_STOPPED: &str = "stopped";

/// How many consecutive failed closes hold a run `api-unstable`.
pub const API_UNSTABLE_CLOSES: usize = 3;

/// Whether a close stamped `close` lies after the `RunReleased` at revision `release`, the one
/// order a progress record takes against a release. PURE. A close stamped at the release's own
/// revision follows it, since its writer read it, and an unstamped close follows no stamped
/// release.
pub fn closed_after_release(close: Option<Revision>, release: Option<Revision>) -> bool {
    close >= release
}

/// The count of [`ENDED_FAULT`] and [`ENDED_STOPPED`] closes across the run's `progress_events`
/// (one run's progress stream, in stream order) since the later of the last
/// [`ENDED_COMPLETED`] close and the last `RunReleased`, whose revision is `released`. PURE.
///
/// Closes order by [`SpawnLaunched::revision`]: no run-stream event lands during an outage, so
/// closes share a stamp, and a tie falls to progress-stream order. A close counts when it lies
/// after `released` ([`closed_after_release`]). An unstamped close takes the stamp of the last
/// stamped close before it in progress-stream order, since it was written after that one, so a
/// `completed` close whose writer's head read failed still ends the count where it stands; with no
/// stamped close before it, a close written before spec 105 among them, it orders before every
/// stamped one and counts as it stands. An
/// [`ENDED_INTERRUPTED`] close neither counts nor resets: the resume fallback, a hold or a
/// supervisor start's reconciliation wrote it, and no launch failed.
pub fn failed_closes_since(progress_events: &[Event], released: Option<Revision>) -> usize {
    let mut stamp = None;
    let mut closes: Vec<SpawnLaunched> = decoded_launches(progress_events)
        .map(|(_, launch)| launch)
        .filter(|launch| !launch.is_open())
        .map(|mut close| {
            close.revision = close.revision.or(stamp);
            stamp = close.revision;
            close
        })
        .collect();
    closes.sort_by_key(|close| close.revision);
    closes
        .iter()
        .rev()
        .take_while(|close| {
            close.ended.as_deref() != Some(ENDED_COMPLETED)
                && closed_after_release(close.revision, released)
        })
        .filter(|close| matches!(close.ended.as_deref(), Some(ENDED_FAULT | ENDED_STOPPED)))
        .count()
}

/// Every launch `progress_events` leave open, each with the run its record is stamped with (""
/// for none), ordered by run, spawn and launch. PURE. The latest record of each (run, spawn,
/// launch) decides: a launch is open while its latest record carries no `ended`. A supervisor's
/// start reads it over the whole progress store, every run's stream at once, to close each launch
/// no live process still answers for.
pub fn open_launches(progress_events: &[Event]) -> Vec<(String, SpawnLaunched)> {
    let mut latest = BTreeMap::new();
    for (event, record) in decoded_launches(progress_events) {
        let run = event.meta.get(META_RUN_ID).cloned().unwrap_or_default();
        latest.insert((run, record.spawn.clone(), record.launch), record);
    }
    latest
        .into_iter()
        .filter(|(_, record)| record.is_open())
        .map(|((run, _, _), record)| (run, record))
        .collect()
}

/// The [`SpawnLaunched`] records of the latest launch of `spawn` among `progress_events`, one
/// run's records (spec 105, RESUME BY SESSION), in stream order: those of the highest ordinal they
/// hold for that id, its open record first unless it was lost, or none when they hold none. Opening
/// and closing records both count, so a launch whose open record was lost still names its ordinal.
/// A record that does not decode names nothing. PURE: the store crate's `latest_launch_records`
/// reads the run's records and folds them here.
pub fn latest_launch_records_of(progress_events: &[Event], spawn: &str) -> Vec<SpawnLaunched> {
    let mut records: Vec<SpawnLaunched> = decoded_launches(progress_events)
        .map(|(_, record)| record)
        .filter(|record| record.spawn == spawn)
        .collect();
    let latest = records.iter().map(|record| record.launch).max();
    records.retain(|record| Some(record.launch) == latest);
    records
}

/// Each [`SpawnLaunched`] record among `progress_events`, beside its event, in their order: a
/// record that does not decode names no launch.
fn decoded_launches(progress_events: &[Event]) -> impl Iterator<Item = (&Event, SpawnLaunched)> {
    progress_events
        .iter()
        .filter_map(|e| Some((e, decode_launch(e)?)))
}

/// The [`SpawnLaunched`] record `e` carries; `None` for another type or a body that does not
/// decode.
fn decode_launch(e: &Event) -> Option<SpawnLaunched> {
    (e.type_ == TYPE_SPAWN_LAUNCHED)
        .then(|| serde_json::from_slice(&e.data).ok())
        .flatten()
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
/// [`latest_stop_failure_class`] scopes it to its launch by where it sits in the stream.
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

/// The latest `StopFailure` class recorded for `spawn_id` since that spawn's last launch opened -
/// "latest wins", the store's own append order breaking any tie, exactly like
/// [`AgentProgress`]'s per-id fold in [`consolidate`]. An open [`SpawnLaunched`] of the spawn
/// starts the count over, so a record of an earlier launch never classes a later one (spec 105),
/// by stream order and never by a comparison of seconds. PURE (no IO): `progress_events` is
/// already-read progress-store events, the host reading it after its own launch's open record.
/// `None` when no `StopFailure` event follows that open record.
pub fn latest_stop_failure_class(progress_events: &[Event], spawn_id: &str) -> Option<String> {
    let mut found = None;
    for e in progress_events {
        if let Some(launch) = decode_launch(e) {
            if launch.spawn == spawn_id && launch.is_open() {
                found = None;
            }
        } else if e.type_ == TYPE_STOP_FAILURE {
            if let Ok(sf) = serde_json::from_slice::<StopFailure>(&e.data) {
                if sf.spawn == spawn_id {
                    found = Some(sf.class);
                }
            }
        }
    }
    found
}

/// The event type an [`ApiRetry`] record serializes as (spec 105 LIVENESS). Recorded to the
/// progress store - never the run stream, same file boundary as [`TYPE_SPAWN_LAUNCHED`] - by
/// the Claude Code host, once per `system/api_retry` line of a session's stream.
pub const TYPE_API_RETRY: &str = "ApiRetry";

/// One retry a session announced on its stream: which spawn, how long it said it would wait
/// and why. Spec 105's LIVENESS names this record so an announced wait is never read as
/// silence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiRetry {
    /// The deterministic spawn id this retry belongs to (e.g. `u/implementer#0`).
    pub spawn: String,
    /// The delay the line announced before the retry, in milliseconds; `0` when it named none.
    pub delay_ms: u64,
    /// Claude Code's own error-category string (e.g. `rate_limit`), exactly as the line
    /// carried it; `unknown` when it named none.
    pub category: String,
}

impl RunStamped for ApiRetry {
    const EVENT_TYPE: &'static str = TYPE_API_RETRY;
}

/// The event type a [`SupervisorBeat`] record serializes as (spec 105 LIVENESS). Recorded to the
/// progress store - never the run stream, same file boundary as [`TYPE_SPAWN_LAUNCHED`] - by
/// `rigger run`, once per [`SUPERVISOR_BEAT_INTERVAL`].
pub const TYPE_SUPERVISOR_BEAT: &str = "SupervisorBeat";

/// How long a supervisor waits between two beats (spec 105 LIVENESS).
pub const SUPERVISOR_BEAT_INTERVAL: Duration = Duration::from_secs(30);

/// One beat of the supervisor, on the stream of the run current at that beat: the store's
/// `recorded_at` says when a supervisor was there, which is all spec 105's LIVENESS asks of it, so
/// the record itself carries nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorBeat {}

impl RunStamped for SupervisorBeat {
    const EVENT_TYPE: &'static str = TYPE_SUPERVISOR_BEAT;
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
    /// Whole seconds the spawn's spec-10 liveness marker has stayed untouched (rigger reads
    /// the marker in Rust and PRESENTS this, so no consumer stats the file): outside the
    /// run's holds on `rigger status`, wall clock on every other view.
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
/// progress store's events for this run, `liveness_ages` the marker age per spawn id the
/// caller read from the markers, and `now` fixes the clock for the age arithmetic
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
    for (e, ap) in decoded_progress(progress_events) {
        latest_prog.insert(ap.id, (ap.activity, e.recorded_at));
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
            revision: None,
        }
        .to_stamped_event("run-1")
        .unwrap();
        p1.recorded_at = now - Duration::from_secs(200);
        let mut p2 = AgentProgress {
            id: req.id.clone(),
            activity: "grep #12: conductor.rs".into(),
            revision: None,
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
            revision: None,
            dir: "/scratch/u104-launch".into(),
        };
        assert_stamped_with_its_run(launched, TYPE_SPAWN_LAUNCHED);
    }

    /// `record` serializes as the event type `type_`, stamped with the run `run-9`, and reads
    /// back as itself.
    fn assert_stamped_with_its_run<R>(record: R, type_: &str)
    where
        R: RunStamped + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let ev = record.to_stamped_event("run-9").unwrap();
        assert_eq!(ev.type_, type_);
        assert_eq!(ev.meta.get(META_RUN_ID).map(String::as_str), Some("run-9"));
        let back: R = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, record);
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
            revision: None,
            dir: String::new(),
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
            v.get("dir").is_none(),
            "an empty dir must be omitted, not an empty string: {v:?}"
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
            revision: None,
            dir: String::new(),
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

    // ---- the launch's directory and the start-up reconciliation (spec 105, RESUME BY SESSION) ----

    /// The record opening launch `launch` of `spawn` in `dir`, its session `s<launch>`.
    fn open_in(spawn: &str, launch: u32, dir: &str) -> SpawnLaunched {
        SpawnLaunched {
            spawn: spawn.into(),
            launch,
            session_id: format!("s{launch}"),
            started: 1,
            dir: dir.into(),
            ..SpawnLaunched::default()
        }
    }

    /// The directory rides an open record that names one and stays off a record that has none,
    /// a close included, so a close serializes as it did before the field existed and a record
    /// written before it decodes with no directory.
    #[test]
    fn a_launch_directory_rides_an_open_record_and_stays_off_a_close() {
        let rows: [(&str, SpawnLaunched, serde_json::Value); 2] = [
            (
                "an open record in a directory",
                open_in("u/implementer#0", 0, "/scratch/u"),
                serde_json::json!({
                    "spawn": "u/implementer#0",
                    "launch": 0,
                    "session_id": "s0",
                    "started": 1,
                    "dir": "/scratch/u",
                }),
            ),
            (
                "a close",
                SpawnLaunched::closed("u/implementer#0", 0, "s0", ENDED_INTERRUPTED, ""),
                serde_json::json!({
                    "spawn": "u/implementer#0",
                    "launch": 0,
                    "session_id": "s0",
                    "started": 0,
                    "ended": "interrupted",
                }),
            ),
        ];
        for (name, record, body) in rows {
            assert_eq!(serde_json::to_value(&record).unwrap(), body, "{name}");
            assert_eq!(
                serde_json::from_value::<SpawnLaunched>(body).unwrap(),
                record,
                "{name}"
            );
        }
    }

    /// Every launch the progress store holds open, by the run its record is stamped with: the
    /// latest record of a (run, spawn, launch) decides, every other record type and a record
    /// that does not decode pass, and the answer is ordered by run, spawn and launch.
    #[test]
    fn the_open_launches_are_the_latest_records_of_each_run_spawn_and_launch_left_open() {
        const U: &str = "u/implementer#0";
        const V: &str = "v/reviewer#0";
        let on = |run: &str, record: &SpawnLaunched| record.to_stamped_event(run).unwrap();
        let close = |spawn: &str, launch: u32, ended: &str| {
            SpawnLaunched::closed(spawn, launch, format!("s{launch}"), ended, "")
        };
        let progress_line = AgentProgress {
            id: U.into(),
            activity: "a step".into(),
            revision: None,
        }
        .to_stamped_event("run-a")
        .unwrap();
        let undecodable =
            Event::new(TYPE_SPAWN_LAUNCHED, b"not json".to_vec()).with_meta(META_RUN_ID, "run-a");
        type Open = (&'static str, SpawnLaunched);
        let rows: [(&str, Vec<Event>, Vec<Open>); 10] = [
            ("an empty store", vec![], vec![]),
            (
                "one open launch",
                vec![on("run-a", &open_in(U, 0, "/s/u"))],
                vec![("run-a", open_in(U, 0, "/s/u"))],
            ),
            (
                "a launch its completed close ended",
                vec![
                    on("run-a", &open_in(U, 0, "/s/u")),
                    on("run-a", &close(U, 0, ENDED_COMPLETED)),
                ],
                vec![],
            ),
            (
                "a launch an interrupted close ended",
                vec![
                    on("run-a", &open_in(U, 0, "/s/u")),
                    on("run-a", &close(U, 0, ENDED_INTERRUPTED)),
                ],
                vec![],
            ),
            (
                "a launch reopened behind its close",
                vec![
                    on("run-a", &open_in(U, 0, "/s/old")),
                    on("run-a", &close(U, 0, ENDED_FAULT)),
                    on("run-a", &open_in(U, 0, "/s/new")),
                ],
                vec![("run-a", open_in(U, 0, "/s/new"))],
            ),
            (
                "a later launch of a spawn whose earlier launch closed",
                vec![
                    on("run-a", &open_in(U, 0, "/s/u")),
                    on("run-a", &close(U, 0, ENDED_FAULT)),
                    on("run-a", &open_in(U, 1, "")),
                ],
                vec![("run-a", open_in(U, 1, ""))],
            ),
            (
                "one launch on two runs, closed on one",
                vec![
                    on("run-a", &open_in(U, 0, "/s/a")),
                    on("run-b", &open_in(U, 0, "/s/b")),
                    on("run-b", &close(U, 0, ENDED_COMPLETED)),
                ],
                vec![("run-a", open_in(U, 0, "/s/a"))],
            ),
            (
                "a record no run stamped",
                vec![on("", &open_in(U, 0, "/s/u"))],
                vec![("", open_in(U, 0, "/s/u"))],
            ),
            (
                "another record type and an undecodable record",
                vec![progress_line, stop_failure(U, "rate_limit"), undecodable],
                vec![],
            ),
            (
                "ordered by run, spawn and launch",
                vec![
                    on("run-b", &open_in(U, 0, "")),
                    on("run-a", &open_in(V, 0, "")),
                    on("run-a", &open_in(U, 10, "")),
                    on("run-a", &open_in(U, 2, "")),
                ],
                vec![
                    ("run-a", open_in(U, 2, "")),
                    ("run-a", open_in(U, 10, "")),
                    ("run-a", open_in(V, 0, "")),
                    ("run-b", open_in(U, 0, "")),
                ],
            ),
        ];
        for (name, events, expected) in rows {
            let expected: Vec<(String, SpawnLaunched)> = expected
                .into_iter()
                .map(|(run, launch)| (run.to_string(), launch))
                .collect();
            assert_eq!(open_launches(&events), expected, "{name}");
        }
    }

    // ---- StopFailure (spec 104 criterion 5: A FAILURE HAS A CLASS) ----

    #[test]
    fn stop_failure_to_event_carries_the_type_and_run_stamp() {
        let sf = StopFailure {
            spawn: "u104-fail-class/implementer#0".into(),
            class: "authentication_failed".into(),
        };
        assert_stamped_with_its_run(sf, TYPE_STOP_FAILURE);
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

    // ---- ApiRetry (spec 105 LIVENESS) ----

    #[test]
    fn api_retry_to_event_carries_the_type_the_run_stamp_and_its_three_fields() {
        let retry = ApiRetry {
            spawn: "u105/implementer#0".into(),
            delay_ms: 618,
            category: "rate_limit".into(),
        };
        let ev = retry.to_stamped_event("run-9").unwrap();
        assert_eq!(ev.type_, "ApiRetry");
        assert_eq!(ev.meta.get(META_RUN_ID).map(String::as_str), Some("run-9"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&ev.data).unwrap(),
            serde_json::json!({
                "spawn": "u105/implementer#0",
                "delay_ms": 618,
                "category": "rate_limit",
            }),
            "the record's wire names are the three spec 105 LIVENESS names"
        );
        let back: ApiRetry = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, retry);
    }

    // ---- SupervisorBeat (spec 105 LIVENESS) ----

    #[test]
    fn supervisor_beat_to_event_carries_the_type_the_run_stamp_and_an_empty_body() {
        let ev = SupervisorBeat::default().to_stamped_event("run-9").unwrap();
        assert_eq!(ev.type_, "SupervisorBeat");
        assert_eq!(ev.meta.get(META_RUN_ID).map(String::as_str), Some("run-9"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&ev.data).unwrap(),
            serde_json::json!({}),
            "a beat says nothing but its type and the time the store stamps"
        );
    }

    #[test]
    fn the_supervisor_beats_every_thirty_seconds() {
        assert_eq!(SUPERVISOR_BEAT_INTERVAL, Duration::from_secs(30));
    }

    /// The record opening launch `launch` of `spawn`, as a stamped event.
    fn opened(spawn: &str, launch: u32) -> Event {
        let mut record = SpawnLaunched::closed(spawn, launch, "s", ENDED_FAULT, "");
        record.ended = None;
        record.to_stamped_event("run-1").unwrap()
    }

    /// A launch close `ended` at `revision`, as a stamped event.
    fn closing(ended: &str, revision: Option<Revision>) -> Event {
        SpawnLaunched {
            revision,
            ..SpawnLaunched::closed("u/implementer#0", 0, "s", ended, "")
        }
        .to_stamped_event("run-1")
        .unwrap()
    }

    /// A `StopFailure` of `class` on `spawn`, as a stamped event.
    fn stop_failure(spawn: &str, class: &str) -> Event {
        StopFailure {
            spawn: spawn.into(),
            class: class.into(),
        }
        .to_stamped_event("run-1")
        .unwrap()
    }

    /// A `StopFailure` classes only the launch it follows in stream order: one written before the
    /// launch opened is an earlier launch's, however it reads; a close, and another spawn's
    /// launch, open nothing of this spawn's.
    #[test]
    fn a_stop_failure_classes_only_the_launch_it_follows() {
        const SPAWN: &str = "u/implementer#0";
        const OTHER: &str = "other/implementer#0";
        let rows: [(&str, Vec<Event>, Option<&str>); 6] = [
            (
                "a record ahead of the launch",
                vec![stop_failure(SPAWN, "billing_error"), opened(SPAWN, 0)],
                None,
            ),
            (
                "a record behind the launch",
                vec![opened(SPAWN, 0), stop_failure(SPAWN, "billing_error")],
                Some("billing_error"),
            ),
            (
                "an earlier launch's record, its launch closed",
                vec![
                    opened(SPAWN, 0),
                    stop_failure(SPAWN, "billing_error"),
                    closing(ENDED_FAULT, None),
                    opened(SPAWN, 1),
                ],
                None,
            ),
            (
                "the later launch's own record over the earlier one's",
                vec![
                    opened(SPAWN, 0),
                    stop_failure(SPAWN, "billing_error"),
                    opened(SPAWN, 1),
                    stop_failure(SPAWN, "rate_limit"),
                ],
                Some("rate_limit"),
            ),
            (
                "another spawn's launch opening behind the record",
                vec![
                    opened(SPAWN, 0),
                    stop_failure(SPAWN, "billing_error"),
                    opened(OTHER, 0),
                ],
                Some("billing_error"),
            ),
            (
                "the launch's own close behind the record",
                vec![
                    opened(SPAWN, 0),
                    stop_failure(SPAWN, "billing_error"),
                    closing(ENDED_FAULT, None),
                ],
                Some("billing_error"),
            ),
        ];
        for (name, events, expected) in rows {
            assert_eq!(
                latest_stop_failure_class(&events, SPAWN).as_deref(),
                expected,
                "{name}"
            );
        }
    }

    /// The count `api-unstable` reads (spec 105): failed closes newest first, ordered by revision,
    /// back to the later of the last `completed` close and the last `RunReleased`. A close
    /// stamped equal to the release is behind it, a tie of closes falls to stream order, and an
    /// unstamped close takes the stamp of the last stamped close before it in stream order, so it
    /// orders first only when no stamped close stands before it.
    #[test]
    fn the_count_of_failed_closes_runs_since_the_later_of_the_last_completed_and_release() {
        const F: &str = ENDED_FAULT;
        const S: &str = ENDED_STOPPED;
        const C: &str = ENDED_COMPLETED;
        const I: &str = ENDED_INTERRUPTED;
        type Close = (&'static str, Option<Revision>);
        let rows: [(&str, Vec<Close>, Option<Revision>, usize); 20] = [
            ("no closes", vec![], None, 0),
            ("two faults", vec![(F, Some(1)), (F, Some(1))], None, 2),
            (
                "three faults",
                vec![(F, Some(1)), (F, Some(1)), (F, Some(1))],
                None,
                3,
            ),
            (
                "four faults",
                vec![(F, Some(1)), (F, Some(1)), (F, Some(1)), (F, Some(1))],
                None,
                4,
            ),
            (
                "a stopped close counts beside a fault",
                vec![(F, Some(1)), (S, Some(2)), (F, Some(2))],
                None,
                3,
            ),
            (
                "a completed close resets the count",
                vec![(F, Some(1)), (F, Some(1)), (C, Some(2)), (F, Some(3))],
                None,
                1,
            ),
            (
                "a completed close at the head leaves nothing",
                vec![(F, Some(1)), (F, Some(1)), (C, Some(1))],
                None,
                0,
            ),
            (
                "an interrupted close neither counts nor resets",
                vec![(F, Some(1)), (I, Some(1)), (F, Some(1))],
                None,
                2,
            ),
            (
                "a close stamped below the release",
                vec![(F, Some(4))],
                Some(5),
                0,
            ),
            (
                "a close stamped at the release",
                vec![(F, Some(5))],
                Some(5),
                1,
            ),
            (
                "a close stamped above the release",
                vec![(F, Some(6))],
                Some(5),
                1,
            ),
            (
                "the release later than the last completed close",
                vec![(C, Some(3)), (F, Some(4)), (F, Some(6))],
                Some(5),
                1,
            ),
            (
                "the last completed close later than the release",
                vec![(F, Some(6)), (C, Some(7)), (F, Some(8))],
                Some(5),
                1,
            ),
            (
                "unstamped closes count as they stand with no release",
                vec![(F, None), (F, None), (F, Some(3))],
                None,
                3,
            ),
            (
                "unstamped closes stand ahead of a release",
                vec![(F, None)],
                Some(0),
                0,
            ),
            (
                "a completed close whose head read failed ends the streak",
                vec![(F, Some(1)), (F, Some(1)), (C, None), (F, Some(1))],
                None,
                1,
            ),
            (
                "a fault tied behind a completed close counts",
                vec![(C, Some(3)), (F, Some(3))],
                None,
                1,
            ),
            (
                "a completed close tied behind a fault resets it",
                vec![(F, Some(3)), (C, Some(3))],
                None,
                0,
            ),
            (
                "the order is the revision, never the stream",
                vec![(C, Some(5)), (F, Some(4))],
                None,
                0,
            ),
            (
                "a later revision stands behind an earlier write",
                vec![(F, Some(5)), (C, Some(4))],
                None,
                1,
            ),
        ];
        for (name, closes, released, expected) in rows {
            let mut events: Vec<Event> = closes
                .iter()
                .map(|(ended, revision)| closing(ended, *revision))
                .collect();
            events.push(opened("u/implementer#9", 0));
            events.push(stop_failure("u/implementer#9", "rate_limit"));
            assert_eq!(failed_closes_since(&events, released), expected, "{name}");
        }
    }

    /// The run-stream revision stamp rides a record that carries it and stays off every record
    /// that has none, so a sparse body folds as `None`: a launch close beside an open record or a
    /// close written before the stamp existed, and a probe line beside any other progress line.
    #[test]
    fn a_revision_stamp_round_trips_and_stays_off_an_unstamped_record() {
        fn assert_stamp<T>(stamped: T, unstamped: [T; 2], stamp: fn(&T) -> Option<Revision>)
        where
            T: Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
        {
            let body = serde_json::to_value(&stamped).unwrap();
            assert_eq!(body["revision"], 7, "{stamped:?}");
            assert_eq!(stamp(&serde_json::from_value(body).unwrap()), Some(7));
            for unstamped in unstamped {
                let body = serde_json::to_value(&unstamped).unwrap();
                assert_eq!(body.get("revision"), None, "{unstamped:?}");
                assert_eq!(stamp(&serde_json::from_value(body).unwrap()), None);
            }
        }
        assert_stamp(
            serde_json::from_slice(&closing(ENDED_FAULT, Some(7)).data).unwrap(),
            [
                SpawnLaunched::closed("u", 0, "s", ENDED_FAULT, ""),
                SpawnLaunched::default(),
            ],
            |launch: &SpawnLaunched| launch.revision,
        );
        assert_stamp(
            AgentProgress {
                id: "hold-probe".into(),
                activity: "probing".into(),
                revision: Some(7),
            },
            [
                AgentProgress {
                    id: "u/implementer#0".into(),
                    activity: "a step".into(),
                    revision: None,
                },
                AgentProgress::default(),
            ],
            |line: &AgentProgress| line.revision,
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
            revision: None,
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
