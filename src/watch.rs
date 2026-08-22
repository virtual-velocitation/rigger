//! The driver-independent watchdog (spec 69, criterion 2): `rigger watch` folds the
//! FIVE SIGNALS `rigger-watch-a-run` names for a manual look - escalated blockers,
//! heartbeat staleness vs live agent processes, dash liveness, reject-recurrence
//! trend, and frontier progress - into one line per anomaly, naming signal, subject,
//! and response, PLUS a sixth safety check (store integrity) the automated command
//! runs beyond the skill's five-point human skim. "Covers every signal the watch
//! skill names" (spec 69, Done-when) is a superset relation, not equality - store
//! integrity is the automation's own addition, not a look-signal a human is asked to
//! check by hand.
//!
//! Everything here is PURE over already-gathered inputs ([`detect`] takes a
//! [`WatchInputs`] built from data the caller already read) so it is unit-testable
//! without a store, a clock, or a process table, and so it can never reach for the
//! driver - exactly the process that may be dead (spec 69: "it must work with the
//! driver dead"). The composition root (`src/main.rs::cmd_watch`) does the I/O: reads
//! the run's event stream and the whole log (for store integrity), tries the step
//! lock, gathers liveness-marker ages, and probes the dash - then hands the resolved
//! facts in here.
//!
//! The five signal NAMES below are pinned verbatim against `rigger-watch-a-run`
//! (spec 69, criterion 1's own headline test, `watch_a_run_names_all_five_signals_
//! each_mapped_to_its_response` in `src/docs.rs`) - the exact strings that skill's
//! rendered body contains, so the command and the skill can never silently drift
//! apart on what a signal is called.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use serde::Deserialize;

use crate::eventstore::{Event, Position, Revision, NO_STREAM};
use crate::{ledger, spawn};

/// The doc location a STORE INTEGRITY anomaly names for the documented repair
/// procedure (spec 71: "repair stays a documented operator procedure, not a
/// command"). There is no response SKILL for this signal - it is not one of the
/// five `rigger-watch-a-run` enumerates - so the anomaly line names this reference
/// instead. THE canonical location for this constant: `rigger validate`'s own
/// order-signature advisory (spec 71, `main.rs::order_signature_advisories`) reads
/// it from here too, rather than declaring its own copy, so the two surfaces that
/// both name this doc section can never drift apart in wording.
pub const ORDER_SIGNATURE_REPAIR_DOC_REF: &str =
    "docs/architecture.md, section 5.1.3: The store defends its own order";

/// A stream whose position order and revision order DISAGREE (spec 71): the signature a
/// write leaves when it lands at a revision the stream's own cursor would never reissue on
/// its own - typically a build predating the `MAX(revision)` append cursor landing in a
/// revision hole the derived-index compaction opened by deleting rows. On a stream that has
/// only ever been appended to by a correct writer, position order and revision order always
/// agree; this struct names the rows where they do not.
///
/// THE single shared shape for this concern (spec 69 criterion 2 / spec 71 criterion 3): both
/// `rigger validate`'s order-signature advisory and this module's own store-integrity signal
/// (6th, beyond `rigger-watch-a-run`'s five) detect the SAME corruption over the SAME
/// algorithm, so it lives here once rather than as two parallel reimplementations that would
/// have to be kept in sync by hand on any future fix.
#[derive(Debug)]
pub struct OrderSignature {
    pub stream: String,
    /// Out-of-order rows found in this stream: a row whose revision does not exceed the
    /// highest revision already seen (in position order) for the same stream.
    pub rows: usize,
    /// The inclusive global-position range spanning the first and last out-of-order row.
    pub first_position: Position,
    pub last_position: Position,
}

/// Find every [`OrderSignature`] in `events`, which callers hand in already in POSITION
/// order (exactly what `EventStore::read_all` returns). Pure over already-read events, so it
/// is unit-tested without touching a store.
///
/// Walks the events once, tracking each stream's running maximum revision. A row whose
/// revision does not exceed that maximum is OUT OF ORDER and is counted against its stream;
/// a row that only extends the maximum raises it but is never itself flagged. One signature
/// per affected stream is returned, in first-affected-row order; a clean log returns an
/// empty vec.
pub fn order_signatures(events: &[Event]) -> Vec<OrderSignature> {
    struct Acc {
        max_revision: Revision,
        rows: usize,
        first_position: Position,
        last_position: Position,
    }
    let mut by_stream: Vec<(String, Acc)> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for e in events {
        let idx = *index.entry(e.stream.clone()).or_insert_with(|| {
            by_stream.push((
                e.stream.clone(),
                Acc {
                    max_revision: NO_STREAM,
                    rows: 0,
                    first_position: 0,
                    last_position: 0,
                },
            ));
            by_stream.len() - 1
        });
        let (_, acc) = &mut by_stream[idx];
        if e.revision <= acc.max_revision {
            if acc.rows == 0 {
                acc.first_position = e.position;
            }
            acc.last_position = e.position;
            acc.rows += 1;
        } else {
            acc.max_revision = e.revision;
        }
    }
    by_stream
        .into_iter()
        .filter(|(_, acc)| acc.rows > 0)
        .map(|(stream, acc)| OrderSignature {
            stream,
            rows: acc.rows,
            first_position: acc.first_position,
            last_position: acc.last_position,
        })
        .collect()
}

/// The diagnose-churn threshold (spec 69 Design: "reject-recurrence at the diagnose
/// threshold (>= 3 ...")). Fixed by the spec text, independent of the run's
/// configured `defaults.max_retries` (a different bound, for a different purpose -
/// when the conductor escalates, not when an operator should look).
pub const REJECT_RECURRENCE_DIAGNOSE_THRESHOLD: u32 = 3;

/// The frontier-stall threshold (spec 69 Design: "a spawn id with >= 3 unconsumed
/// results").
pub const FRONTIER_STALL_THRESHOLD: u32 = 3;

/// The dead-driver conjunction's STORE-QUIET bound (spec 69 Design: "store quiet a
/// full hour").
pub const DEAD_DRIVER_QUIET_BOUND: Duration = Duration::from_secs(3600);

/// The dead-driver conjunction's HEARTBEAT-STALE bound (spec 69 Design: "every
/// heartbeat stale >30 min").
pub const DEAD_DRIVER_HEARTBEAT_BOUND: Duration = Duration::from_secs(30 * 60);

/// The default poll interval `rigger watch`'s streaming mode uses absent
/// `--interval <s>` (spec 69 Design: "default 180s").
pub const DEFAULT_INTERVAL_SECS: u64 = 180;

/// The closed set of anomaly signals the watchdog reports. The first five are named
/// BY THE SAME STRING `rigger-watch-a-run` uses (see [`Signal::name`]); the sixth,
/// [`Signal::StoreIntegrity`], is the automation's own addition beyond the skill's
/// five-signal human skim (module doc). Declared in the spec's own listed order so a
/// derived [`Ord`] sorts anomalies in that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signal {
    Escalated,
    DeadDriver,
    DashNotServing,
    RejectRecurrence,
    FrontierStall,
    StoreIntegrity,
}

/// The five signal names, in Design order, verbatim against `rigger-watch-a-run`'s
/// rendered body - the single canonical list both this command and that skill's own
/// pin test check against (module doc: pinned so command and skill cannot drift).
pub const SKILL_SIGNAL_NAMES: [&str; 5] = [
    "escalated blockers",
    "heartbeat staleness",
    "dash liveness",
    "reject-recurrence trend",
    "frontier progress",
];

impl Signal {
    /// The canonical name printed on the anomaly line. The first five are the EXACT
    /// strings `rigger-watch-a-run` names (see [`SKILL_SIGNAL_NAMES`]); the response
    /// text is what "signal, subject, and response" (spec 69 Design) means.
    pub fn name(&self) -> &'static str {
        match self {
            Signal::Escalated => SKILL_SIGNAL_NAMES[0],
            Signal::DeadDriver => SKILL_SIGNAL_NAMES[1],
            Signal::DashNotServing => SKILL_SIGNAL_NAMES[2],
            Signal::RejectRecurrence => SKILL_SIGNAL_NAMES[3],
            Signal::FrontierStall => SKILL_SIGNAL_NAMES[4],
            Signal::StoreIntegrity => "store integrity",
        }
    }

    /// The response named for this signal (spec 69 Design: "each signal maps BY NAME
    /// to its response skill ... stall: stop the driver and diagnose before another
    /// round spends"). Four of the five name a response SKILL; the frontier-progress
    /// stall names the spec's own directive text instead (never a fifth invented
    /// skill - `rigger-watch-a-run`'s own pin test forbids that); store integrity
    /// names the documented repair reference (spec 71), since it has no skill of its
    /// own.
    pub fn response(&self) -> &'static str {
        match self {
            Signal::Escalated => "rigger-handle-an-escalation",
            Signal::DeadDriver => "rigger-resume-a-run",
            Signal::DashNotServing => "rigger-restore-the-dash",
            Signal::RejectRecurrence => "rigger-diagnose-churn",
            Signal::FrontierStall => "stop the driver and diagnose before another round spends",
            Signal::StoreIntegrity => ORDER_SIGNATURE_REPAIR_DOC_REF,
        }
    }
}

/// One printed anomaly line's content: the [`Signal`], the SUBJECT it is about (a
/// unit id, a spawn id, a stream name, or a fixed label for a run-level condition),
/// a numeric MAGNITUDE driving re-alert-on-increment dedup (a streak or result
/// count; `0` for a signal with no natural magnitude), and human DETAIL folded into
/// the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anomaly {
    pub signal: Signal,
    pub subject: String,
    pub magnitude: u32,
    pub detail: String,
}

impl Anomaly {
    /// The dedup identity: signal + subject, WITHOUT the magnitude - [`Dedup::step`]
    /// compares magnitude separately so an increment re-alerts under the SAME key
    /// rather than being read as a brand-new anomaly.
    fn key(&self) -> (Signal, String) {
        (self.signal, self.subject.clone())
    }

    /// The one line `rigger watch` prints: `<signal>: <subject> - <detail> (respond:
    /// <response>)` - names signal, subject, and response exactly as spec 69 Design
    /// requires. Hyphens only (a gate rejects em dashes).
    pub fn line(&self) -> String {
        format!(
            "{}: {} - {} (respond: {})",
            self.signal.name(),
            self.subject,
            self.detail,
            self.signal.response()
        )
    }
}

/// The dash liveness probe's outcome (spec 69 Design signal 3), resolved by the
/// caller's I/O (a marker-or-URL read plus a real serve probe) and handed in already
/// classified - `detect` never touches a socket or a file.
///
/// TWO breadcrumbs can independently record a dash, and only ONE of the three real
/// dash-launching drivers writes both: the `rigger step` drive path writes a
/// per-project MARKER (port + pid) alongside the URL, but `rigger run` and `rigger
/// serve` (`spawn_run_dashboard`/`spawn_run_dashboard_detached`'s guard-bound callers)
/// write ONLY the URL breadcrumb, never a marker - so the caller's probe falls back to
/// the URL's own port when no marker exists, and [`NotServing`](DashProbe::NotServing)
/// carries `pid: None` in that case rather than inventing one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DashProbe {
    /// Neither a marker nor a URL was ever recorded for this project - a headless run
    /// by choice (e.g. `dash: off` / `RIGGER_NO_DASH`), not an anomaly.
    NotRecorded,
    /// Something answers the recorded port with the dash's own signature.
    Serving,
    /// Nothing verifiably serves the recorded port - the "hung holder" or
    /// dead-process case `rigger-restore-the-dash` diagnoses. `pid` names the
    /// culprit ONLY when a marker actually recorded one; `None` when the probe fell
    /// back to the URL breadcrumb alone (no marker was ever written, e.g. `rigger
    /// run` / `rigger serve`) - there is genuinely no pid to name in that case, never
    /// a guess.
    NotServing { pid: Option<u32>, port: u16 },
}

/// Everything [`detect`] needs, already gathered by the caller (store, process
/// table, and status - never the driver). Two DIFFERENT event slices, because the
/// signals are scoped differently: the five run signals read this project's
/// CURRENT RUN stream (mirroring `rigger status`'s own scope), while store
/// integrity reads the WHOLE log across every stream (mirroring `rigger validate`'s
/// order-signature detector, spec 71) - a disordered stream is a store-wide fault,
/// not a per-run one.
pub struct WatchInputs<'a> {
    /// This project's current run's event slice (`conductor::STREAM`, run-scoped).
    pub run_events: &'a [Event],
    /// The full log across every stream, position-ordered (for store integrity).
    pub full_events: &'a [Event],
    /// The moment the caller gathered these inputs.
    pub now: SystemTime,
    /// When the run's last event was recorded, or `None` for an empty run.
    pub last_event_at: Option<SystemTime>,
    /// Whether NO `rigger step` currently holds the step lock (true = free = no
    /// step process is running right now).
    pub step_lock_free: bool,
    /// Heartbeat-marker AGE in seconds, keyed by spawn id, for the run's CURRENTLY
    /// PARKED frontier (mirrors `rigger status`'s own liveness-age fold). Empty when
    /// nothing is parked, which reads as "every heartbeat stale" vacuously.
    pub wave_liveness_ages: &'a BTreeMap<String, u64>,
    /// The dash liveness probe's already-classified outcome.
    pub dash: DashProbe,
    /// When THIS run began - its own [`crate::run::TYPE_RUN_STARTED`] event's
    /// `recorded_at` - or `None` when no run has started yet in the scope handed in.
    /// Used ONLY to scope Signal 3 (dash liveness); see
    /// [`Self::dash_breadcrumb_written_at`] for why.
    pub run_started_at: Option<SystemTime>,
    /// The last-modified moment of whichever breadcrumb file backed [`Self::dash`]'s
    /// classification (the marker when one was read, else the `dash.url` file), or
    /// `None` when neither breadcrumb exists ([`DashProbe::NotRecorded`], where
    /// Signal 3 never fires anyway) or its mtime could not be read.
    ///
    /// Round-6 fix (round-5 reject cause
    /// adv2-u69c1-r5-uphold-sdet-second-run-stale-marker): both dash breadcrumb
    /// files are project-level singletons an EARLIER, already-finished run may have
    /// left dead behind (see the module doc + [`DashProbe`] doc) - the round-5
    /// `!run.done()` gate alone only closes the DONE-run half of that; a FRESH,
    /// NOT-DONE run that never itself touched the dash still inherited an earlier
    /// run's stale breadcrumb as a false anomaly, since neither breadcrumb file
    /// carries a run identity to tell the two apart. Per
    /// adv2-u69c1-r5-root-cause-marker-lacks-run-identity, reshaping
    /// [`crate::dash::DashMarker`] to add one risks regressing spec 39's own
    /// documented cross-run-persistent-singleton idempotency contract (the marker
    /// is DELIBERATELY meant to survive across runs), so this pairs with
    /// [`Self::run_started_at`] instead: a SEPARATE per-run fact, derived from the
    /// breadcrumb file's own mtime, that answers "does THIS breadcrumb PROVABLY
    /// predate this run's own start?" without touching the marker's shape at all.
    /// The burden of proof runs toward reporting, not suppressing: `detect` only
    /// suppresses when BOTH this and [`Self::run_started_at`] are known and the
    /// breadcrumb is strictly older - either unknown still reports, exactly as
    /// before this field existed (round-3/4's own established behavior for a
    /// project with a dash breadcrumb but no run recorded yet).
    pub dash_breadcrumb_written_at: Option<SystemTime>,
}

#[derive(Deserialize)]
struct UnitFailedCause {
    id: String,
    #[serde(default)]
    cause: String,
}

/// The unit's per-cause CONSECUTIVE failure streak (spec 69 Design: reject-recurrence
/// "counted and re-alerted PER FAILURE CAUSE - see the cause wire"): the trailing run
/// of `UnitFailed` events, in log order, that share the CURRENT (most recent) cause.
/// A cause change resets the streak to `1` - "a changed cause is progress" (spec 69
/// Design), never counted as continued churn. `cause` defaults to `"unknown"` on a
/// pre-criterion-3 event that carries none (additive, serde-defaulted; spec 69 c3's
/// own contract). Pure over the already-scoped run events.
fn reject_recurrence_streak(run_events: &[Event], unit_id: &str) -> (String, u32) {
    let mut cause = String::new();
    let mut streak = 0u32;
    for e in run_events {
        if e.type_ != ledger::TYPE_UNIT_FAILED {
            continue;
        }
        let Ok(p) = serde_json::from_slice::<UnitFailedCause>(&e.data) else {
            continue;
        };
        if p.id != unit_id {
            continue;
        }
        let this_cause = if p.cause.is_empty() {
            "unknown".to_string()
        } else {
            p.cause
        };
        if streak > 0 && this_cause == cause {
            streak += 1;
        } else {
            cause = this_cause;
            streak = 1;
        }
    }
    (cause, streak)
}

/// How many [`spawn::TYPE_SPAWN_RESULT`] events each spawn id has recorded, keyed by
/// id - NOT deduped to the latest (unlike [`spawn::result_of`]): a spawn answered
/// more than once is exactly the STALLED-FRONTIER signature (spec 69 Design: "a
/// spawn answered more than twice without the run advancing burns full agent cost
/// per round"). Malformed result bodies are skipped (never panicked on), matching
/// every other read-model in this codebase's fail-soft-on-decode convention.
fn spawn_result_counts(run_events: &[Event]) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    for e in run_events {
        if e.type_ == spawn::TYPE_SPAWN_RESULT {
            if let Ok(r) = spawn::SpawnResult::from_event(e) {
                *counts.entry(r.id).or_insert(0u32) += 1;
            }
        }
    }
    counts
}

/// Streams where POSITION order and REVISION order disagree (spec 71's own
/// corruption signature: a row whose revision does not exceed the highest revision
/// already seen, in position order, for its stream), returned as `(stream, count)`
/// pairs at the granularity this command's anomaly line needs (a count, not the
/// full position-range detail `validate`'s advisory prints). Delegates to
/// [`order_signatures`] - the SAME shared detector `rigger validate`'s own
/// order-signature advisory (spec 71) calls - rather than a second parallel
/// implementation of the running-max-revision algorithm; the two surfaces can
/// never drift apart on what counts as out of order.
fn out_of_order_streams(events: &[Event]) -> Vec<(String, usize)> {
    order_signatures(events)
        .into_iter()
        .map(|s| (s.stream, s.rows))
        .collect()
}

/// Fold [`WatchInputs`] into the anomalies to report - `rigger watch`'s whole domain
/// core, PURE and store/process/status-only (never the driver). One [`Anomaly`] per
/// condition found; an empty vec on a clean run. Deterministically sorted by
/// [`Signal`] (Design order) then subject.
pub fn detect(inputs: &WatchInputs) -> Vec<Anomaly> {
    let run = ledger::project(inputs.run_events).unwrap_or_default();
    let mut out = Vec::new();

    // Signal 1: escalated blockers.
    for (id, u) in &run.units {
        if u.status == ledger::Status::Escalated {
            out.push(Anomaly {
                signal: Signal::Escalated,
                subject: id.clone(),
                magnitude: 0,
                detail: "escalated - awaiting a human".to_string(),
            });
        }
    }

    // Signal 4: reject-recurrence trend, counted per cause.
    for (id, u) in &run.units {
        if u.status != ledger::Status::Failed {
            continue;
        }
        let (cause, streak) = reject_recurrence_streak(inputs.run_events, id);
        if streak >= REJECT_RECURRENCE_DIAGNOSE_THRESHOLD {
            out.push(Anomaly {
                signal: Signal::RejectRecurrence,
                subject: id.clone(),
                magnitude: streak,
                detail: format!("reject-recurrence #{streak} (cause: {cause})"),
            });
        }
    }

    // Signal 5: frontier progress (a spawn stalled at the frontier).
    for (id, count) in spawn_result_counts(inputs.run_events) {
        if count >= FRONTIER_STALL_THRESHOLD {
            out.push(Anomaly {
                signal: Signal::FrontierStall,
                subject: id,
                magnitude: count,
                detail: format!("{count} recorded results without the run advancing"),
            });
        }
    }

    // Signal 2: heartbeat staleness vs live agent processes (the dead-driver
    // conjunction). Never fires on a DONE run - a finished run's quiet store is
    // success, not a dead driver.
    if !run.done() {
        let quiet = inputs
            .last_event_at
            .and_then(|t| inputs.now.duration_since(t).ok())
            .is_some_and(|age| age >= DEAD_DRIVER_QUIET_BOUND);
        let every_heartbeat_stale = inputs
            .wave_liveness_ages
            .values()
            .all(|age| Duration::from_secs(*age) > DEAD_DRIVER_HEARTBEAT_BOUND);
        if quiet && inputs.step_lock_free && every_heartbeat_stale {
            out.push(Anomaly {
                signal: Signal::DeadDriver,
                subject: "run".to_string(),
                magnitude: 0,
                detail: "store quiet an hour, no step process, every heartbeat stale past 30m"
                    .to_string(),
            });
        }
    }

    // Signal 3: dash liveness. Never fires on a DONE run, mirroring Signal 2 above -
    // `.rigger/dash.url` and `.rigger/dash.marker` are project-level singleton files
    // that are never removed once their dash exits, so a finished run's stale
    // breadcrumb is success (the dash did its job and stopped), not a permanent
    // anomaly (round-4 reject: adv-u69c1r4-dash-anomaly-permanent-false-positive).
    if !run.done() {
        if let DashProbe::NotServing { pid, port } = &inputs.dash {
            // Round-6 fix (round-5 reject cause adv2-u69c1-r5-uphold-sdet-second-run-
            // stale-marker): `!run.done()` alone only closes the DONE-run half of the
            // stale-breadcrumb problem. Both dash breadcrumb files are project-level
            // singletons an EARLIER, already-finished run may have left dead behind, so a
            // FRESH, NOT-DONE run that never itself touched the dash must not inherit that
            // stale breadcrumb either. Neither file carries a run id (see
            // `dash_breadcrumb_written_at`'s doc for why this does not reshape the marker
            // to add one), so suppress ONLY when the breadcrumb DEFINITIVELY predates THIS
            // run - the one per-run fact available without touching that shape. The
            // burden of proof is on demonstrating staleness, not on demonstrating
            // freshness: either side unknown (no run has started yet in this project - the
            // `watch_once_output_matches_what_restore_the_dash_promises_about_a_dead_
            // marker` integration test's own shape, `rigger watch --once` run before any
            // `rigger step` - or the breadcrumb's mtime could not be read) still reports,
            // exactly as it did before this fix; only a PROVEN-earlier breadcrumb is new
            // grounds to suppress.
            let breadcrumb_predates_this_run = matches!(
                (inputs.dash_breadcrumb_written_at, inputs.run_started_at),
                (Some(written), Some(started)) if written < started
            );
            if !breadcrumb_predates_this_run {
                let detail = match pid {
                    Some(pid) => format!("marker names dead pid {pid} on port {port}"),
                    // No marker was ever recorded (rigger run / rigger serve) - the probe
                    // fell back to the recorded dash.url's own port; genuinely no pid to
                    // name.
                    None => {
                        format!("recorded dash.url port {port} does not answer (no marker, no pid)")
                    }
                };
                out.push(Anomaly {
                    signal: Signal::DashNotServing,
                    subject: "dash".to_string(),
                    magnitude: 0,
                    detail,
                });
            }
        }
    }

    // Signal 6 (beyond the skill's five): store integrity.
    for (stream, rows) in out_of_order_streams(inputs.full_events) {
        out.push(Anomaly {
            signal: Signal::StoreIntegrity,
            subject: stream,
            magnitude: rows as u32,
            detail: format!("{rows} row(s) where position order and revision order disagree"),
        });
    }

    out.sort_by_key(|a| (a.signal, a.subject.clone()));
    out
}

/// Streaming-mode dedup (spec 69 Design: "Alerts dedupe until cleared, DEDUP STATE
/// LIVES IN PROCESS MEMORY ONLY"). Kept in the caller's process, never persisted -
/// a restarted watch re-alerts standing anomalies once, which is the accepted,
/// spec-named consequence for a fresh observer.
#[derive(Default)]
pub struct Dedup {
    seen: BTreeMap<(Signal, String), u32>,
}

impl Dedup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Given this poll's CURRENT anomalies, return only the ones to PRINT: one not
    /// seen on the prior poll, or one whose MAGNITUDE increased since - a climbing
    /// churn count is itself new information, so it re-alerts under the same
    /// signal+subject key rather than being suppressed as a repeat (spec 69
    /// Done-when: "re-alerts a churn count on each increment"). An anomaly no longer
    /// present is dropped from memory, so if it recurs LATER it re-alerts fresh
    /// ("dedupes a persisting anomaly UNTIL IT CLEARS").
    pub fn step(&mut self, current: Vec<Anomaly>) -> Vec<Anomaly> {
        let mut still_present: BTreeMap<(Signal, String), u32> = BTreeMap::new();
        let mut to_print = Vec::new();
        for a in current {
            let key = a.key();
            if self.seen.get(&key) != Some(&a.magnitude) {
                to_print.push(a.clone());
            }
            still_present.insert(key, a.magnitude);
        }
        self.seen = still_present;
        to_print
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: &str, json: &str) -> Event {
        Event::new(type_, json.as_bytes().to_vec())
    }

    fn positioned(mut events: Vec<Event>) -> Vec<Event> {
        for (i, e) in events.iter_mut().enumerate() {
            e.position = (i + 1) as u64;
            e.revision = i as Revision;
            if e.stream.is_empty() {
                e.stream = "run".to_string();
            }
        }
        events
    }

    fn empty_inputs<'a>(events: &'a [Event], ages: &'a BTreeMap<String, u64>) -> WatchInputs<'a> {
        WatchInputs {
            run_events: events,
            full_events: events,
            now: SystemTime::now(),
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: ages,
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        }
    }

    // --- Signal::name() is pinned verbatim against rigger-watch-a-run ---

    #[test]
    fn the_five_signal_names_match_skill_signal_names_in_design_order() {
        assert_eq!(
            [
                Signal::Escalated.name(),
                Signal::DeadDriver.name(),
                Signal::DashNotServing.name(),
                Signal::RejectRecurrence.name(),
                Signal::FrontierStall.name(),
            ],
            SKILL_SIGNAL_NAMES
        );
    }

    #[test]
    fn the_four_named_responses_and_the_stall_directive_match_the_skills_own_pin_table() {
        // The exact five (signal, response) pairs `rigger-watch-a-run`'s own headline
        // test pins (spec 69 c1: watch_a_run_names_all_five_signals_each_mapped_to_
        // its_response, src/docs.rs) - so this command and that skill cannot silently
        // drift on what each signal is called or what it routes to.
        assert_eq!(Signal::Escalated.response(), "rigger-handle-an-escalation");
        assert_eq!(Signal::DeadDriver.response(), "rigger-resume-a-run");
        assert_eq!(Signal::DashNotServing.response(), "rigger-restore-the-dash");
        assert_eq!(Signal::RejectRecurrence.response(), "rigger-diagnose-churn");
        assert!(Signal::FrontierStall
            .response()
            .contains("stop the driver and diagnose"));
        // Never a fifth invented skill name for the stall signal.
        assert!(!Signal::FrontierStall.response().starts_with("rigger-"));
    }

    // --- detect(): a clean run prints nothing ---

    #[test]
    fn a_clean_store_detects_no_anomalies() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"c"}"#),
        ]);
        assert!(detect(&empty_inputs(&events, &BTreeMap::new())).is_empty());
    }

    #[test]
    fn an_empty_run_detects_no_anomalies() {
        assert!(detect(&empty_inputs(&[], &BTreeMap::new())).is_empty());
    }

    // --- Signal 1: escalated blockers ---

    #[test]
    fn an_escalated_unit_is_reported_naming_the_unit_and_the_escalation_response() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u-esc"}"#),
        ]);
        let anomalies = detect(&empty_inputs(&events, &BTreeMap::new()));
        assert_eq!(anomalies.len(), 1);
        let a = &anomalies[0];
        assert_eq!(a.signal, Signal::Escalated);
        assert_eq!(a.subject, "u-esc");
        let line = a.line();
        assert!(line.contains("escalated blockers"));
        assert!(line.contains("u-esc"));
        assert!(line.contains("rigger-handle-an-escalation"));
    }

    // --- Signal 4: reject-recurrence, per cause ---

    #[test]
    fn a_unit_at_reject_recurrence_three_same_cause_is_reported() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":2,"cause":"gate:fmt"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":3,"cause":"gate:fmt"}"#,
            ),
        ]);
        let anomalies = detect(&empty_inputs(&events, &BTreeMap::new()));
        assert_eq!(anomalies.len(), 1);
        let a = &anomalies[0];
        assert_eq!(a.signal, Signal::RejectRecurrence);
        assert_eq!(a.subject, "u");
        assert_eq!(a.magnitude, 3);
        assert!(a.detail.contains("gate:fmt"));
        assert!(a.line().contains("rigger-diagnose-churn"));
    }

    #[test]
    fn two_failures_below_threshold_is_not_reported() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":2,"cause":"gate:fmt"}"#,
            ),
        ]);
        assert!(detect(&empty_inputs(&events, &BTreeMap::new())).is_empty());
    }

    #[test]
    fn a_cause_change_resets_the_streak_so_three_failures_split_across_two_causes_do_not_alert() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":2,"cause":"gate:fmt"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            // A changed cause is progress, not continued churn (spec 69 Design):
            // the streak restarts at 1, so this is only #1 of the new cause.
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u","attempts":3,"cause":"integrate-conflict"}"#,
            ),
        ]);
        assert!(detect(&empty_inputs(&events, &BTreeMap::new())).is_empty());
    }

    #[test]
    fn a_cause_less_failure_reads_as_unknown_and_still_counts_toward_the_streak() {
        // Additive, serde-defaulted: a prior event with no `cause` at all (predating
        // spec 69 c3's cause wire) reads as "unknown", not a decode failure.
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":1}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":2}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":3}"#),
        ]);
        let anomalies = detect(&empty_inputs(&events, &BTreeMap::new()));
        assert_eq!(anomalies.len(), 1);
        assert!(anomalies[0].detail.contains("unknown"));
    }

    // --- Signal 5: frontier progress (stalled) ---

    #[test]
    fn a_spawn_answered_three_times_is_reported_as_a_frontier_stall() {
        let events = positioned(vec![
            ev(
                spawn::TYPE_SPAWN_RESULT,
                r#"{"id":"u/implementer#0","output":"a"}"#,
            ),
            ev(
                spawn::TYPE_SPAWN_RESULT,
                r#"{"id":"u/implementer#0","output":"b"}"#,
            ),
            ev(
                spawn::TYPE_SPAWN_RESULT,
                r#"{"id":"u/implementer#0","output":"c"}"#,
            ),
        ]);
        let anomalies = detect(&empty_inputs(&events, &BTreeMap::new()));
        assert_eq!(anomalies.len(), 1);
        let a = &anomalies[0];
        assert_eq!(a.signal, Signal::FrontierStall);
        assert_eq!(a.subject, "u/implementer#0");
        assert_eq!(a.magnitude, 3);
        assert!(a.line().contains("frontier progress"));
        assert!(a
            .line()
            .contains("stop the driver and diagnose before another round spends"));
    }

    #[test]
    fn a_spawn_answered_twice_is_below_the_frontier_stall_threshold() {
        let events = positioned(vec![
            ev(spawn::TYPE_SPAWN_RESULT, r#"{"id":"u/implementer#0"}"#),
            ev(spawn::TYPE_SPAWN_RESULT, r#"{"id":"u/implementer#0"}"#),
        ]);
        assert!(detect(&empty_inputs(&events, &BTreeMap::new())).is_empty());
    }

    // --- Signal 2: dead driver (the conjunction) ---

    #[test]
    fn a_quiet_store_with_no_step_process_and_every_heartbeat_stale_is_a_dead_driver() {
        let events = positioned(vec![ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#)]);
        let now = SystemTime::now();
        let ages: BTreeMap<String, u64> = [("u/implementer#0".to_string(), 3600u64)]
            .into_iter()
            .collect();
        let inputs = WatchInputs {
            run_events: &events,
            full_events: &events,
            now,
            last_event_at: Some(now - Duration::from_secs(4000)),
            step_lock_free: true,
            wave_liveness_ages: &ages,
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        let anomalies = detect(&inputs);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].signal, Signal::DeadDriver);
        assert_eq!(anomalies[0].subject, "run");
    }

    #[test]
    fn a_running_step_process_suppresses_the_dead_driver_alert() {
        let events = positioned(vec![ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#)]);
        let now = SystemTime::now();
        let inputs = WatchInputs {
            run_events: &events,
            full_events: &events,
            now,
            last_event_at: Some(now - Duration::from_secs(4000)),
            // A step IS running - not dead, just slow.
            step_lock_free: false,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(detect(&inputs).is_empty());
    }

    #[test]
    fn a_fresh_heartbeat_suppresses_the_dead_driver_alert_even_with_a_quiet_store() {
        // The tuned false positive spec 69 names: "an alert firing on quiet-but-
        // heartbeating work teaches operators to ignore the watchdog." A long-running
        // test/build can leave the store quiet an hour while an agent is genuinely
        // still working - one fresh heartbeat must suppress the whole conjunction.
        let events = positioned(vec![ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#)]);
        let now = SystemTime::now();
        let ages: BTreeMap<String, u64> = [("u/implementer#0".to_string(), 60u64)]
            .into_iter()
            .collect();
        let inputs = WatchInputs {
            run_events: &events,
            full_events: &events,
            now,
            last_event_at: Some(now - Duration::from_secs(4000)),
            step_lock_free: true,
            wave_liveness_ages: &ages,
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(detect(&inputs).is_empty());
    }

    #[test]
    fn a_heartbeat_ten_minutes_stale_does_not_cross_the_thirty_minute_bound() {
        // Boundary-straddling: 10 minutes (600s) sits well BELOW the real 30-minute
        // (1800s) bound but well ABOVE a degenerate 90s bound (`30 + 60`, a plausible
        // `*` -> `+` mutant of the `30 * 60` that builds
        // [`DEAD_DRIVER_HEARTBEAT_BOUND`]) - so this pins the bound is actually 30
        // MINUTES, not just "some threshold a big number clears and a small one
        // doesn't."
        let events = positioned(vec![ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#)]);
        let now = SystemTime::now();
        let ages: BTreeMap<String, u64> = [("u/implementer#0".to_string(), 600u64)]
            .into_iter()
            .collect();
        let inputs = WatchInputs {
            run_events: &events,
            full_events: &events,
            now,
            last_event_at: Some(now - Duration::from_secs(4000)),
            step_lock_free: true,
            wave_liveness_ages: &ages,
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(
            detect(&inputs).is_empty(),
            "a heartbeat only 10 minutes stale must not cross the 30-minute dead-driver bound"
        );
    }

    #[test]
    fn a_heartbeat_exactly_thirty_minutes_stale_does_not_yet_cross_the_bound() {
        // The bound is STRICTLY greater than 30 minutes (spec 69 Design: "every
        // heartbeat stale >30 min") - a heartbeat at EXACTLY the bound has not yet
        // crossed it. Pins `>` against a `>=` mutant of the comparison itself, and
        // (since it sits exactly on `DEAD_DRIVER_HEARTBEAT_BOUND`) against a mutant
        // that shrinks the bound's derivation too.
        let events = positioned(vec![ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#)]);
        let now = SystemTime::now();
        let ages: BTreeMap<String, u64> = [(
            "u/implementer#0".to_string(),
            DEAD_DRIVER_HEARTBEAT_BOUND.as_secs(),
        )]
        .into_iter()
        .collect();
        let inputs = WatchInputs {
            run_events: &events,
            full_events: &events,
            now,
            last_event_at: Some(now - Duration::from_secs(4000)),
            step_lock_free: true,
            wave_liveness_ages: &ages,
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(
            detect(&inputs).is_empty(),
            "a heartbeat AT exactly the 30-minute bound has not yet crossed it (the bound is a \
             strict >, not >=)"
        );
    }

    #[test]
    fn a_done_run_never_reports_a_dead_driver_however_quiet_the_store() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"c"}"#),
        ]);
        let now = SystemTime::now();
        let inputs = WatchInputs {
            run_events: &events,
            full_events: &events,
            now,
            last_event_at: Some(now - Duration::from_secs(999_999)),
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(detect(&inputs).is_empty());
    }

    /// Round-4 reject (adv-u69c1r4-dash-anomaly-permanent-false-positive): `.rigger/
    /// dash.url` and `.rigger/dash.marker` are project-level singleton files never
    /// removed once a dash exits, so a done run's stale breadcrumb must not read as
    /// an anomaly either - mirrors
    /// `a_done_run_never_reports_a_dead_driver_however_quiet_the_store` above but
    /// for Signal 3 instead of Signal 2, closing the exact gap that test's own
    /// neighbor left open.
    #[test]
    fn a_done_run_never_reports_a_dead_dash_either() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"c"}"#),
        ]);
        let inputs = WatchInputs {
            run_events: &events,
            full_events: &events,
            now: SystemTime::now(),
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotServing {
                pid: None,
                port: 7420,
            },
            // Irrelevant here: `!run.done()` short-circuits before either field is read.
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(detect(&inputs).is_empty());
    }

    // --- Signal 3: dash liveness ---

    #[test]
    fn a_dash_marker_naming_a_dead_pid_is_reported() {
        let now = SystemTime::now();
        let inputs = WatchInputs {
            run_events: &[],
            full_events: &[],
            now,
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotServing {
                pid: Some(424_242),
                port: 7420,
            },
            // The breadcrumb was written AFTER this run began - THIS run's own dash.
            run_started_at: Some(now - Duration::from_secs(60)),
            dash_breadcrumb_written_at: Some(now - Duration::from_secs(30)),
        };
        let anomalies = detect(&inputs);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].signal, Signal::DashNotServing);
        assert!(anomalies[0].detail.contains("424242"));
        assert!(anomalies[0].line().contains("rigger-restore-the-dash"));
    }

    /// The marker-absent case (`rigger run` / `rigger serve`, which record only the URL
    /// breadcrumb, never a marker - see `DashProbe` docs): a dead port is STILL reported,
    /// with no pid invented. This is the round-3 reject's own root cause
    /// (adv-u69c1r3-watch-once-inherits-marker-absent-blindspot) - before this fix
    /// `NotServing` required a `u32` pid unconditionally, so the caller had nothing to
    /// construct here and had to map this exact shape to `NotRecorded`, the non-anomaly
    /// variant, leaving `rigger watch --once` silently blind for 2 of the 3 real drivers.
    #[test]
    fn a_dead_dash_url_with_no_marker_is_reported_without_inventing_a_pid() {
        let now = SystemTime::now();
        let inputs = WatchInputs {
            run_events: &[],
            full_events: &[],
            now,
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotServing {
                pid: None,
                port: 7420,
            },
            // The breadcrumb was written AFTER this run began - THIS run's own dash.
            run_started_at: Some(now - Duration::from_secs(60)),
            dash_breadcrumb_written_at: Some(now - Duration::from_secs(30)),
        };
        let anomalies = detect(&inputs);
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].signal, Signal::DashNotServing);
        assert!(anomalies[0].detail.contains("7420"));
        assert!(
            !anomalies[0].detail.contains("pid ") || anomalies[0].detail.contains("no pid"),
            "a marker-absent report must never claim a specific pid: {}",
            anomalies[0].detail
        );
        assert!(anomalies[0].line().contains("rigger-restore-the-dash"));
    }

    #[test]
    fn no_dash_ever_recorded_is_not_an_anomaly() {
        let inputs = WatchInputs {
            run_events: &[],
            full_events: &[],
            now: SystemTime::now(),
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(detect(&inputs).is_empty());
    }

    #[test]
    fn a_serving_dash_is_not_an_anomaly() {
        let inputs = WatchInputs {
            run_events: &[],
            full_events: &[],
            now: SystemTime::now(),
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::Serving,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(detect(&inputs).is_empty());
    }

    /// Round-6 fix (round-5 reject cause adv2-u69c1-r5-uphold-sdet-second-run-stale-marker):
    /// a dead marker whose mtime PREDATES this run's own `RunStarted` is an EARLIER run's
    /// leftover breadcrumb, not this run's own dash - `!run.done()` alone (the round-5 fix)
    /// only closes the done-run half of that; this closes the not-done half directly at the
    /// pure `detect` level, mirroring `watch_once_reports_no_dash_anomaly_for_a_fresh_run_
    /// that_inherits_an_earlier_runs_dead_marker` (tests/cli.rs) which drives the identical
    /// shape through the real compiled binary and I/O seam.
    #[test]
    fn a_dead_marker_predating_this_runs_own_start_is_not_this_runs_anomaly() {
        let now = SystemTime::now();
        let inputs = WatchInputs {
            run_events: &[],
            full_events: &[],
            now,
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotServing {
                pid: Some(424_242),
                port: 7420,
            },
            // This run began AFTER the breadcrumb was last written - an earlier run's
            // leftover, never this run's own.
            run_started_at: Some(now - Duration::from_secs(30)),
            dash_breadcrumb_written_at: Some(now - Duration::from_secs(60)),
        };
        assert!(
            detect(&inputs).is_empty(),
            "a breadcrumb written BEFORE this run began must never be reported as this run's \
             own dead dash"
        );
    }

    /// The burden of proof runs toward REPORTING, not suppressing: only a breadcrumb
    /// DEFINITIVELY (both sides known) older than this run's own start is grounds to
    /// suppress. Either side unknown - no run has started yet in this project (the
    /// realistic case: a dash breadcrumb exists but `rigger step` never has, exactly the
    /// shape `watch_once_output_matches_what_restore_the_dash_promises_about_a_dead_
    /// marker`, tests/cli.rs, drives through the real binary), or the breadcrumb's mtime
    /// could not be read - must still report, exactly as Signal 3 always has.
    #[test]
    fn unknown_run_started_at_or_breadcrumb_mtime_still_reports_the_dead_dash() {
        let now = SystemTime::now();
        let ages = BTreeMap::new();

        for (run_started_at, dash_breadcrumb_written_at, case) in [
            (None, None, "both sides unknown - e.g. no run recorded yet"),
            (
                None,
                Some(now),
                "run_started_at unknown, breadcrumb mtime known",
            ),
            (
                Some(now),
                None,
                "breadcrumb mtime unknown, run_started_at known",
            ),
        ] {
            let inputs = WatchInputs {
                run_events: &[],
                full_events: &[],
                now,
                last_event_at: None,
                step_lock_free: true,
                wave_liveness_ages: &ages,
                dash: DashProbe::NotServing {
                    pid: Some(424_242),
                    port: 7420,
                },
                run_started_at,
                dash_breadcrumb_written_at,
            };
            assert_eq!(detect(&inputs).len(), 1, "{case}");
        }
    }

    // --- order_signatures: the shared detector `rigger validate`'s own order-signature
    // advisory (spec 71) also calls, so its own boundary coverage (including the duplicate-
    // revision case below) protects BOTH callers at once, not just this module's. ---

    fn order_sig_ev(stream: &str, position: u64, revision: Revision) -> Event {
        let mut e = ev("Seed", "{}");
        e.stream = stream.to_string();
        e.position = position;
        e.revision = revision;
        e
    }

    /// Two orders coexist per event: position (global, store-assigned, never itself
    /// disordered) and revision (per-stream, also store-assigned). On a healthy stream the
    /// two agree - later position always means higher revision. `order_signatures` walks
    /// `events` (which callers hand it already in POSITION order, exactly as
    /// `EventStore::read_all` returns them) tracking each stream's running maximum revision;
    /// a row whose revision does not exceed that maximum is OUT OF ORDER. Only the rows that
    /// broke the maximum are flagged - a row that only extends it is untouched - and an
    /// unrelated, cleanly-ordered stream interleaved by position draws nothing at all. Row 6
    /// is a DUPLICATE revision (equal to, not less than, the running maximum) - a distinct
    /// corruption shape from a strict decrease: two events can never legitimately share one
    /// revision in the same stream, so `<=` (not `<`) must catch this too.
    #[test]
    fn order_signatures_flags_rows_whose_revision_does_not_exceed_the_streams_running_maximum() {
        let events = vec![
            order_sig_ev("run", 1, 0),
            order_sig_ev("run", 2, 1),
            order_sig_ev("other", 3, 0), // unrelated, clean stream, interleaved by position
            order_sig_ev("run", 4, 2),
            order_sig_ev("run", 5, 1), // OUT OF ORDER: 1 <= running max 2
            order_sig_ev("run", 6, 2), // OUT OF ORDER (duplicate): 2 <= running max 2, still
            order_sig_ev("run", 7, 5), // back in order: 5 > 2
            order_sig_ev("other", 8, 1),
        ];
        let signatures = order_signatures(&events);
        assert_eq!(
            signatures.len(),
            1,
            "only the disordered stream is flagged: {signatures:?}"
        );
        let s = &signatures[0];
        assert_eq!(s.stream, "run");
        assert_eq!(s.rows, 2, "two rows broke the running maximum: {s:?}");
        assert_eq!(s.first_position, 5);
        assert_eq!(s.last_position, 6);
    }

    /// A log where every stream's revisions strictly increase with position - the shape a
    /// correctly functioning append always produces on its own - draws no signature.
    #[test]
    fn order_signatures_is_empty_on_a_cleanly_ordered_log() {
        let events = vec![
            order_sig_ev("run", 1, 0),
            order_sig_ev("other", 2, 0),
            order_sig_ev("run", 3, 1),
            order_sig_ev("other", 4, 1),
            order_sig_ev("run", 5, 2),
        ];
        assert!(
            order_signatures(&events).is_empty(),
            "a cleanly ordered log must draw no signature"
        );
    }

    // --- Signal 6: store integrity (out-of-order tail) ---

    #[test]
    fn an_out_of_order_tail_is_reported_naming_the_stream_and_row_count() {
        let mut events = vec![ev("E", "{}"), ev("E", "{}"), ev("E", "{}")];
        for (i, e) in events.iter_mut().enumerate() {
            e.stream = "s".to_string();
            e.position = (i + 1) as u64;
        }
        // Position order 1,2,3 with revisions 0,3,1: row 3 (revision 1) does not
        // exceed the running max (3) - an out-of-order tail.
        events[0].revision = 0;
        events[1].revision = 3;
        events[2].revision = 1;
        let inputs = WatchInputs {
            run_events: &[],
            full_events: &events,
            now: SystemTime::now(),
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        let anomalies = detect(&inputs);
        assert_eq!(anomalies.len(), 1);
        let a = &anomalies[0];
        assert_eq!(a.signal, Signal::StoreIntegrity);
        assert_eq!(a.subject, "s");
        assert_eq!(a.magnitude, 1);
        assert!(a
            .line()
            .contains("position order and revision order disagree"));
        assert!(a.line().contains(ORDER_SIGNATURE_REPAIR_DOC_REF));
    }

    #[test]
    fn a_cleanly_ordered_log_reports_no_store_integrity_anomaly() {
        let mut events = vec![ev("E", "{}"), ev("E", "{}"), ev("E", "{}")];
        for (i, e) in events.iter_mut().enumerate() {
            e.stream = "s".to_string();
            e.position = (i + 1) as u64;
            e.revision = i as Revision;
        }
        let inputs = WatchInputs {
            run_events: &[],
            full_events: &events,
            now: SystemTime::now(),
            last_event_at: None,
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        assert!(detect(&inputs).is_empty());
    }

    // --- The seeded multi-anomaly scenario (spec 69 Done-when's own combination) ---

    #[test]
    fn a_store_seeded_with_a_multi_result_spawn_an_escalated_unit_reject_recurrence_three_and_an_out_of_order_tail_prints_one_line_each(
    ) {
        let mut run_events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-fail"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-fail","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-fail"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-fail","attempts":2,"cause":"gate:fmt"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-fail"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-fail","attempts":3,"cause":"gate:fmt"}"#,
            ),
            ev(
                spawn::TYPE_SPAWN_RESULT,
                r#"{"id":"u-stall/implementer#0"}"#,
            ),
            ev(
                spawn::TYPE_SPAWN_RESULT,
                r#"{"id":"u-stall/implementer#0"}"#,
            ),
            ev(
                spawn::TYPE_SPAWN_RESULT,
                r#"{"id":"u-stall/implementer#0"}"#,
            ),
        ]);
        // An out-of-order tail on a DIFFERENT stream in the full log.
        let mut ooo = vec![ev("E", "{}"), ev("E", "{}"), ev("E", "{}")];
        for (i, e) in ooo.iter_mut().enumerate() {
            e.stream = "other".to_string();
            e.position = (100 + i) as u64;
        }
        ooo[0].revision = 0;
        ooo[1].revision = 5;
        ooo[2].revision = 2;
        let mut full_events = run_events.clone();
        full_events.extend(ooo);

        let inputs = WatchInputs {
            run_events: &run_events,
            full_events: &full_events,
            now: SystemTime::now(),
            last_event_at: run_events.last().map(|e| e.recorded_at),
            step_lock_free: true,
            wave_liveness_ages: &BTreeMap::new(),
            dash: DashProbe::NotRecorded,
            run_started_at: None,
            dash_breadcrumb_written_at: None,
        };
        let anomalies = detect(&inputs);
        let signals: Vec<Signal> = anomalies.iter().map(|a| a.signal).collect();
        assert_eq!(
            signals,
            vec![
                Signal::Escalated,
                Signal::RejectRecurrence,
                Signal::FrontierStall,
                Signal::StoreIntegrity,
            ]
        );
        assert!(anomalies.iter().all(|a| !a.line().is_empty()));
        run_events.clear(); // silence an unused-mut warning on some toolchains
        let _ = run_events;
    }

    // --- Dedup ---

    #[test]
    fn dedup_suppresses_a_persisting_anomaly_at_the_same_magnitude() {
        let mut d = Dedup::new();
        let a = Anomaly {
            signal: Signal::Escalated,
            subject: "u".to_string(),
            magnitude: 0,
            detail: "escalated".to_string(),
        };
        assert_eq!(d.step(vec![a.clone()]), vec![a.clone()]);
        // Same anomaly, same poll again: suppressed.
        assert!(d.step(vec![a.clone()]).is_empty());
        assert!(d.step(vec![a]).is_empty());
    }

    #[test]
    fn dedup_re_alerts_when_the_magnitude_increments() {
        let mut d = Dedup::new();
        let at = |n: u32| Anomaly {
            signal: Signal::RejectRecurrence,
            subject: "u".to_string(),
            magnitude: n,
            detail: format!("#{n}"),
        };
        assert_eq!(d.step(vec![at(3)]), vec![at(3)]);
        assert!(d.step(vec![at(3)]).is_empty());
        // The churn count climbed: re-alert.
        assert_eq!(d.step(vec![at(4)]), vec![at(4)]);
        assert!(d.step(vec![at(4)]).is_empty());
    }

    #[test]
    fn dedup_re_alerts_a_cleared_and_later_recurring_anomaly() {
        let mut d = Dedup::new();
        let a = Anomaly {
            signal: Signal::DashNotServing,
            subject: "dash".to_string(),
            magnitude: 0,
            detail: "dead".to_string(),
        };
        assert_eq!(d.step(vec![a.clone()]), vec![a.clone()]);
        // It clears: an empty poll.
        assert!(d.step(vec![]).is_empty());
        // It recurs: re-alerts fresh, not suppressed as a stale repeat.
        assert_eq!(d.step(vec![a.clone()]), vec![a]);
    }
}
