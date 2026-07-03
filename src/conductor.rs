//! The conductor executes a workflow: it walks the stage DAG in dependency order,
//! runs each stage's agent through the AgentDriver port and its gates through the
//! gate::Runner port, advances units under the safety rails, and emits the event
//! stream that both the ledger and the context graph project from. It is the
//! top-level use case; it depends only on ports and domain, never on an adapter.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::{AgentDef, Config, Stage};
use crate::contextgraph::{self, Graph, Projection};
use crate::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use crate::gate::{self, Gate};
use crate::grounder::Grounder;
use crate::ledger::{self, RunState};
use crate::safety;
use crate::spawn::{
    self, lens_role, spawn_id, spawn_retry_id, ROLE_ADJUDICATOR, ROLE_ADVERSARY, ROLE_IMPLEMENTER,
};
use crate::worktree::Worktree;

/// The run's event stream name.
pub const STREAM: &str = "run";

/// The bounded fan-out pool size (§6): at most this many agents run concurrently
/// in a wave or a fan-out stage. Items beyond the cap wait for a slot - all still
/// complete, just never more than MAX_CONCURRENCY at once.
pub const MAX_CONCURRENCY: usize = 4;

/// The event a planning stage's agent emits to add a unit to the run DAG at
/// runtime (the living-DAG / spawnUnit mechanic).
pub const TYPE_UNIT_PROPOSED: &str = "UnitProposed";
/// Gate-autonomy ratchet events: a gate's trust moving up or down.
pub const TYPE_GATE_PROMOTED: &str = "GatePromoted";
pub const TYPE_GATE_DEMOTED: &str = "GateDemoted";
/// A proposed unit with no spec criterion - refused (anti-fragmentation, §8).
pub const TYPE_SCOPE_CREEP: &str = "ScopeCreep";
/// The spawn budget is spent - the circuit-breaker tripped (§4.4, §8).
pub const TYPE_BUDGET_EXHAUSTED: &str = "BudgetExhausted";
/// The run is halting because the plan left a spec criterion uncovered - the
/// coverage gap is a spec defect, not something to silently deviate around (§4.4).
pub const TYPE_SPEC_DEFECT: &str = "SpecDefect";
/// The run aborted: un-integrated work is dropped, integrated work is kept (§4.4).
pub const TYPE_TASK_ABORTED: &str = "TaskAborted";
/// A Manual-autonomy gate pauses its unit awaiting human review (§4.3).
pub const TYPE_MANUAL_REVIEW: &str = "ManualReview";
/// A deferred gate FAILED when it ran at the run's phase boundary (§4.3). Surfaced
/// truthfully: the ledger folds it (`RunState::deferred_gate_failed`) so the run is
/// never reported fully done with a red deferred gate. Kept in sync with
/// `ledger::TYPE_DEFERRED_GATE_FAILED`.
pub const TYPE_DEFERRED_GATE_FAILED: &str = ledger::TYPE_DEFERRED_GATE_FAILED;

/// The metadata key carrying an event's deterministic REPLAY KEY (spec 04, criterion
/// 4). A stepwise/replay run re-executes `conductor::run` over recorded history on
/// EVERY step; an event stamped with a replay key is appended AT MOST ONCE across those
/// re-runs, so replay appends no duplicate unit-lifecycle event or gate verdict. The
/// key is a pure function of the run structure (the unit id, a phase or gate token, and
/// the remediation attempt), never wall clock or randomness, so two step processes
/// compute the identical key for the identical event and the second recognizes the
/// first's as a replay. Folds and projections ignore it (like [`contextgraph::META_ACTOR`]);
/// only [`RunCtx::emit_keyed`] and the gate-verdict replay read it.
pub const META_REPLAY_KEY: &str = "replay_key";

/// The replay keys under which the spawn-budget breaker records its halt (Gap 13): a run
/// halts on budget AT MOST ONCE, so the single `BudgetExhausted` + `TaskAborted` pair is
/// keyed - like the green/verified/reviewed lifecycle - and lands exactly once. The
/// cross-step spawn fold makes a resume DETERMINISTICALLY re-reach the spent budget and
/// re-trip the breaker every step; keying dedups those re-trips (`replayed_keys` is seeded
/// from the log at run start), so the audit trail never double-counts the one halt
/// (finding adv-budget-exhausted-dup-across-steps). Fixed strings, not coordinate-derived:
/// there is one breaker per run.
const BUDGET_EXHAUSTED_KEY: &str = "budget-exhausted";
const TASK_ABORTED_KEY: &str = "task-aborted";

/// The metadata key carrying the REQUESTED model ALIAS on a spawn's recorded unit events
/// (spec 05 line 52). It is the workflow-configured alias the agent was spawned with
/// (`AgentDef::model`, the same value that rides the [`SpawnRequest`](crate::spawn::SpawnRequest)),
/// copied here onto the ledger unit-lifecycle events the conductor emits FOR that spawn -
/// UnitStarted and the green/verified/reviewed statuses - so every spawn's events name the
/// model that was asked for, not only the request event. Stamped as metadata (never a new
/// event type, per spec 05's Global constraints); folds and projections ignore it, exactly
/// like [`META_REPLAY_KEY`] and [`contextgraph::META_ACTOR`].
pub const META_MODEL_ALIAS: &str = "model_alias";

/// The metadata key carrying the RESOLVED model id that actually ran a spawn (spec 05
/// line 52). Unlike the requested [`META_MODEL_ALIAS`], the resolved id is known only
/// AFTER the agent runs: the worker reports it via `rigger result --meta
/// '{"resolved_model": ...}'` (see [`spawn::META_RESOLVED_MODEL`](crate::spawn::META_RESOLVED_MODEL)),
/// it lands in the spawn's [`SpawnResult`](crate::spawn::SpawnResult) `meta`, the replay
/// driver surfaces it on [`AgentResult::resolved_model`], and the conductor copies it here
/// onto the unit events it emits once it has consumed that spawn's result (green/verified
/// for the implementer, reviewed for the adjudicator). Empty (and so omitted) when the
/// worker reported none.
pub const META_MODEL_RESOLVED: &str = "model_resolved";

/// The replay key for a gate's verdict, keyed by the `(unit, attempt, gate)` coordinate
/// the gate ran under - so a step re-reaching an already-run gate REPLAYS its recorded
/// verdict instead of re-running the command (spec 04, criterion 4). Distinct attempts
/// are distinct gate runs (a re-implementation must re-gate), so only re-reaching the
/// SAME attempt's gate is a replay.
fn gate_verdict_key(unit: &str, attempt: u32, gate: &str) -> String {
    format!("{unit}/gate:{gate}#{attempt}")
}

/// The payload of a `GateVerdict` event, for seeding the gate-verdict replay cache and
/// for the ratchet's evidence. `evidence` defaults so a legacy verdict without it decodes.
#[derive(Deserialize)]
struct GateVerdictData {
    pass: bool,
    #[serde(default)]
    evidence: String,
}

/// The replay key for a DEFERRED gate's phase-boundary verdict. A deferred gate runs
/// once per run (not per unit/attempt), so the first step to reach the phase boundary
/// runs it and every later re-step replays it.
fn deferred_gate_verdict_key(gate: &str) -> String {
    format!("deferred/gate:{gate}")
}

/// The replay key for a deferred gate's DeferredGateFailed event. The failure is a
/// SEPARATE append from the GateVerdict, and the deferred replay guard keys off the
/// verdict while [`ledger::RunState::done`]/`fully_done` fold the FAILURE - so a crash
/// between the two appends would leave the recorded verdict replayed but the failure
/// lost, reporting a finished run with a red deferred gate (finding
/// adv-deferred-failed-lost-on-crash). Keying the failure lets the step after the crash
/// re-surface it from the replayed verdict exactly once, healing the gap idempotently.
fn deferred_gate_failed_key(gate: &str) -> String {
    format!("deferred/failed:{gate}")
}

/// The `commit` value a standalone review-only stage records when it reaches its
/// DAG-terminal state (item 7). A review stage integrates NO code artifact, so
/// rather than fabricating an integration with an empty commit hash - which reads
/// as a dropped/missing value - it records this EXPLICIT marker, truthfully saying
/// "this stage reviewed; it produced no artifact to commit".
pub const REVIEW_ONLY_NO_ARTIFACT: &str = "(review-only: no integrated artifact)";

#[derive(Debug, thiserror::Error)]
#[error("conductor: {0}")]
pub struct Error(pub String);

impl From<crate::eventstore::Error> for Error {
    fn from(e: crate::eventstore::Error) -> Self {
        Error(e.to_string())
    }
}
impl From<crate::worktree::Error> for Error {
    fn from(e: crate::worktree::Error) -> Self {
        Error(e.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(e.to_string())
    }
}

/// What an agent returns when it finishes.
#[derive(Clone, Debug, Default)]
pub struct AgentResult {
    pub output: String,
    /// The RESOLVED model id that actually ran this spawn (spec 05 line 52), or empty when
    /// unknown. The replay driver surfaces it from the worker's `rigger result --meta`
    /// report ([`SpawnResult::resolved_model`](crate::spawn::SpawnResult::resolved_model));
    /// the conductor copies it onto the spawn's unit events via [`META_MODEL_RESOLVED`].
    /// The blocking drivers (cli/workflow) do not learn it and leave it empty.
    pub resolved_model: String,
}

/// The result of running a stage's gates: whether they all passed, and the compact
/// evidence of any that failed (carried into the next attempt's prompt, item 3 /
/// spec 02).
struct GateOutcome {
    pass: bool,
    evidence: Vec<String>,
}

/// The outcome of a unit's three-tier review: whether the adjudicator approved, and
/// its verdict reasoning (the adjudicator's raw output). On approval the reason is
/// folded into the unit's `reviewed` evidence (item 4); on a reject it is threaded
/// into the next attempt's prompt (item 5).
struct ReviewOutcome {
    approved: bool,
    reason: String,
}

impl ReviewOutcome {
    fn approved(reason: String) -> Self {
        ReviewOutcome {
            approved: true,
            reason,
        }
    }
    fn rejected(reason: String) -> Self {
        ReviewOutcome {
            approved: false,
            reason,
        }
    }
}

/// The proof, carried into [`RunCtx::integrate_and_emit`], that a unit's three-tier
/// review EXPLICITLY APPROVED it (and its gates passed) - the ONLY thing that may
/// merge a unit onto the integration branch.
///
/// This is the fail-closed guard at the merge seam. Integration must run ONLY on an
/// explicit `approve`; on a review reject, on escalation, and on a gate failure
/// NOTHING merges and the unit's work stays on its own branch (`rigger/u/<id>`) for a
/// human. Before, that invariant lived ONLY in where `integrate_and_emit` was CALLED
/// (inside an `if review.approved` arm) - implicit, and a refactor that moved or added
/// a call site could silently merge rejected/escalated code, which is exactly the
/// real-run bug (an escalated unit's `feat(...)` commit landed on the run branch and
/// broke the suite). Making the approval an explicit, unforgeable VALUE the caller must
/// hand to `integrate_and_emit` converts the invariant from "trust the call site" into
/// a precondition the merge seam itself enforces: there is no way to construct one
/// except [`Self::approved`], so no path can merge without a real approve in hand.
#[derive(Clone, Copy)]
struct IntegrationApproval(());

impl IntegrationApproval {
    /// The unit's review explicitly APPROVED it and its gates passed - the work may
    /// merge. The ONLY constructor, so an `IntegrationApproval` value cannot exist
    /// without a real approve, and no rejected/escalated path can fabricate one.
    fn approved() -> Self {
        IntegrationApproval(())
    }
}

/// The specifics of a unit's previous failed attempt, threaded into the next
/// attempt's prompt (spec 02, targeted remediation). Empty on the first attempt.
#[derive(Default)]
struct PriorFailure {
    /// The compact PASS/FAIL evidence of each gate that failed last attempt.
    gate_evidence: Vec<String>,
    /// The adjudicator's rejection reasoning (its raw output) when review rejected.
    review_reason: String,
}

impl PriorFailure {
    fn is_empty(&self) -> bool {
        self.gate_evidence.is_empty() && self.review_reason.trim().is_empty()
    }

    /// A one-line summary of the failure, for the escalation lesson (spec 02: the
    /// escalation lesson must carry the concrete reason, not a generic placeholder).
    fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.gate_evidence.is_empty() {
            parts.push(format!("gates failed: {}", self.gate_evidence.join(" | ")));
        }
        if !self.review_reason.trim().is_empty() {
            parts.push(format!("review rejected: {}", self.review_reason.trim()));
        }
        parts.join("; ")
    }

    /// The first-class, clearly-delimited prior-failure block prepended to a retry
    /// prompt. Empty when there is no prior failure, so the first attempt's prompt is
    /// byte-identical to the historical prompt.
    fn block(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut b = String::from(
            "Your previous attempt failed the checks below. Fix exactly these - do not start over:\n",
        );
        for ev in &self.gate_evidence {
            b.push_str("Your previous attempt failed these gates: ");
            b.push_str(ev);
            b.push('\n');
        }
        if !self.review_reason.trim().is_empty() {
            b.push_str("Your previous attempt was rejected by review: ");
            b.push_str(self.review_reason.trim());
            b.push('\n');
        }
        b.push('\n');
        b
    }
}

/// How a unit ENTERS its lifecycle on this run (resume-continuity). A unit that ran
/// in a prior window CONTINUES from its recorded phase - its deterministic branch is
/// the durable checkpoint carrying the committed work - instead of restarting from
/// implement, so progress accumulates across capped windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResumePhase {
    /// No reusable prior work: the branch is missing/empty, or the unit's last
    /// recorded status is below `green` (it never got an implementation committed).
    /// Run the FULL lifecycle (implement -> gates -> three-tier review -> integrate) -
    /// the historical behavior.
    Fresh,
    /// The unit was implemented in a prior window (last status >= green/verified) and
    /// its branch carries committed work, but it was NOT yet approved+merged. SKIP the
    /// implementer spawn, recreate the worktree from the unit's branch, and continue
    /// the lifecycle from gates + the three-tier review on the committed code.
    Implemented,
    /// The unit's review was APPROVED in a prior window (last status `reviewed`) and
    /// its branch carries committed work, but the merge was interrupted (no
    /// UnitIntegrated). SKIP both implement and review and go straight to integrate -
    /// the work was approved, only the merge did not land.
    Reviewed,
}

/// Per-spawn options.
#[derive(Default)]
pub struct SpawnOpts {
    /// The spawn's DETERMINISTIC id (`{unit}/{role}#{attempt}`, see
    /// [`spawn_id`](crate::spawn::spawn_id)). A stepwise/replay driver keys on it to
    /// answer an already-recorded spawn from the log or to park an unrecorded one; the
    /// blocking drivers (cli/workflow) ignore it. Empty for a caller that does not use
    /// stepwise replay.
    pub id: String,
    /// The unit this spawn belongs to - the parked request's `unit` (and the display
    /// label's unit half). Empty when the caller does not park.
    pub unit: String,
    /// The stage that produced this spawn - the parked request's `stage` (the thin
    /// driver's per-unit `opts.phase` label half). Empty when the caller does not park.
    pub stage: String,
    /// The 0-based remediation attempt this spawn runs under (the same integer the
    /// deterministic `id` encodes after `#`). Every driver resolves the actual spawn
    /// model through [`AgentDef::model_for_attempt`](crate::config::AgentDef::model_for_attempt)
    /// with it, so a `model_ladder` agent (spec 10 unit 4) escalates one rung per attempt
    /// and the model that runs matches the [`META_MODEL_ALIAS`] the conductor stamps for
    /// the same attempt. 0 for a caller that does not remediate.
    pub attempt: u32,
    /// The agent's PERSONA - its role instructions, the markdown body of its
    /// `.rigger/agents/<id>.md` definition (`AgentDef::prompt`). It belongs as the
    /// agent's SYSTEM prompt, distinct from the grounded task `prompt`. The conductor
    /// is the SINGLE place that sets it (from `agent_def.prompt`), so BOTH drivers
    /// consume the same persona source and cannot diverge: the cli driver passes it as
    /// `--system-prompt`, the workflow driver carries it to the shim which passes it to
    /// the Agent SDK `query()` as `options.systemPrompt`. Empty when the agent declared
    /// no body.
    pub system_prompt: String,
    /// The working directory the agent runs in: an isolated worktree, or "" for
    /// the current dir.
    pub dir: String,
    /// Whether this spawn runs in an isolated git worktree (§6). False when the
    /// agent runs in the current dir (no repo, or `isolation: none`).
    pub isolation: bool,
    /// Whether this spawn is one of several running concurrently in a fan-out
    /// stage (§6). False for a single-worker stage.
    pub parallel: bool,
    /// The agent's blast-radius: the grounded seed files this spawn is scoped to
    /// (§5.3). The workflow driver carries it to the shim, which fetches
    /// blast-radius-filtered peer decisions and injects them at the tool boundary;
    /// the cli driver (a subprocess) cannot do mid-run injection and ignores it.
    pub blast_radius: Vec<String>,
    /// The id of the run this spawn belongs to (spec 06, unit 1), set by the conductor
    /// from the current run. A parking driver stamps it into the `SpawnRequested`
    /// event's [`crate::run::META_RUN_ID`] metadata so the parked spawn is attributable
    /// to its run; the blocking drivers ignore it. Empty for a caller outside a run.
    pub run_id: String,
}

/// AgentDriver spawns an agent to completion. The agent records events it emits
/// during its run by calling `emit`, so its decisions reach the log live (the
/// workflow driver wires emit to an in-process tool the agent calls).
pub trait AgentDriver: Send + Sync {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error>;
}

/// The sentinel a stepwise/replay [`AgentDriver`] embeds in its spawn error to signal
/// that a spawn was PARKED - persisted to the log and awaiting an out-of-process
/// result - rather than run to completion or genuinely failed. It uses control
/// characters no real error text carries, so [`is_parked`] recognizes it even after
/// the conductor wraps the driver error with stage/agent context (`format!("... {}",
/// e.0)` keeps the marker as a substring).
///
/// Parking is part of the `AgentDriver` PORT contract, so it lives here beside the
/// trait: the replay adapter constructs the signal via [`parked_spawn`] and the
/// conductor recognizes it via [`is_parked`], and the use case never has to name the
/// adapter to tell a park from a failure.
const PARKED_MARKER: &str = "\u{1}rigger:spawn-parked\u{1}";

/// Construct the PARK signal a stepwise driver returns for spawn `id`: it persisted the
/// unrecorded spawn request and cannot answer it in-process. On this signal the
/// conductor unwinds the unit CLEANLY - no `UnitFailed`, no remediation - and the step
/// ends once every in-flight spawn is parked at the frontier; a later step, after the
/// courier records the result, replays it. This is a normal failure, so a non-stepwise
/// driver (which never parks) is entirely unaffected.
pub fn parked_spawn(id: &str) -> Error {
    Error(format!(
        "{PARKED_MARKER} spawn {id:?} parked at the unrecorded frontier"
    ))
}

/// Whether `e` is a driver PARK signal (see [`parked_spawn`]) rather than a real spawn
/// failure. Robust to the conductor's own `format!("... {}", e.0)` wrapping at the
/// review spawn sites, since the marker survives as a substring.
pub fn is_parked(e: &Error) -> bool {
    e.0.contains(PARKED_MARKER)
}

/// The sentinel a REVIEW-TIER spawn (lens/adversary/adjudicator) embeds in its refusal
/// error when [`reserve_spawn`](RunCtx::reserve_spawn) denies it because the CUMULATIVE
/// spawn budget is spent (§4.4, §8). Like [`PARKED_MARKER`] it uses control characters no
/// real error text carries, so [`is_budget_refused`] recognizes it even after the review
/// site or the wave collapse wraps the error with stage/agent context.
///
/// A budget-refused review spawn is NOT a stage failure - it is symmetric with the
/// implementer's `Ok(false)` refusal in [`run_single_stage`](RunCtx::run_single_stage).
/// The refusal already set `budget_broke` on the [`RunCtx`], so once this sentinel unwinds
/// the unit CLEANLY (`run_wave` treats it as not-a-failure, mirroring a park), the run
/// loop's mid-wave `budget_broke()` check trips the ONE breaker path
/// ([`trip_budget_breaker`](RunCtx::trip_budget_breaker)): the run halts with a
/// `BudgetExhausted` event, never a raw error. That is what makes a run exceeding
/// `defaults.budget` at ANY spawn site - the implementer OR any review tier - abort
/// identically (spec 04, criterion 5; findings budget-review-tier-no-exhausted,
/// adv-confirm-review-tier-no-budgetexhausted, adv-budget-guard-cannot-assemble-reviewed-unit).
const BUDGET_MARKER: &str = "\u{1}rigger:spawn-budget-exhausted\u{1}";

/// Construct the budget-refusal signal a review-tier spawn returns when
/// [`reserve_spawn`](RunCtx::reserve_spawn) denies it: `tier` names the refused review
/// tier and `agent` the refused agent (for the audit trail), and the embedded
/// [`BUDGET_MARKER`] lets [`is_budget_refused`] recognize it through the conductor's own
/// `format!("... {}", e.0)` error wrapping. Only [`reserve_spawn`] returning `false`
/// produces this - and it sets `budget_broke` before it does - so the sentinel and the
/// breaker flag always travel together.
fn budget_refused(stage: &str, tier: &str, agent: &str) -> Error {
    Error(format!(
        "{BUDGET_MARKER} stage {stage:?} {tier} {agent:?}: spawn budget exhausted"
    ))
}

/// Whether `e` is a budget-refusal signal from a review-tier spawn (see
/// [`budget_refused`]) rather than a real spawn failure. Robust to the review sites' own
/// error wrapping, since the marker survives as a substring.
fn is_budget_refused(e: &Error) -> bool {
    e.0.contains(BUDGET_MARKER)
}

/// The number of times a reviewer whose result is DEGENERATE (empty or whitespace-only)
/// is respawned before the run halts (Gap 18, spec 07). A degenerate reviewer result is
/// an INFRASTRUCTURE fault, not a verdict, so the conductor respawns the SAME reviewer
/// under a fresh deterministic id ([`spawn_retry_id`]); bounded here so a persistently
/// broken reviewer agent/driver cannot loop the conductor forever. The reviewer runs at
/// most `1 + REVIEWER_RESPAWN_BOUND` times (its original spawn plus this many respawns)
/// before [`degenerate_reviewer`] halts the run.
const REVIEWER_RESPAWN_BOUND: u32 = 2;

/// The sentinel a degenerate-reviewer HALT (Gap 18, spec 07) embeds in its error so
/// [`run_wave`](RunCtx::run_wave) recognizes it through its own error wrapping and routes
/// it through a DEDICATED arm - like [`PARKED_MARKER`] and [`BUDGET_MARKER`] it uses
/// control characters no real error text carries. The dedicated arm propagates the loud
/// halt but emits NO per-unit lesson: a lesson there would misattribute the OPERATOR's
/// broken reviewer to the unit under review (finding adv-u2gap18-halt-lesson-
/// misattribution). `run_wave` STRIPS the marker before it surfaces, so the operator's
/// halt message stays clean.
const DEGENERATE_MARKER: &str = "\u{1}rigger:reviewer-degenerate\u{1}";

/// Construct the LOUD-HALT error the review path returns when a reviewer's original spawn
/// and all [`REVIEWER_RESPAWN_BOUND`] respawns each returned a degenerate result (Gap 18,
/// spec 07). It NAMES the dead reviewer - `tier` (lens/adversary/adjudicator), `agent`,
/// and the `stage` - so the operator sees WHICH spawn is failing, and names the REAL,
/// working recovery.
///
/// It carries the [`DEGENERATE_MARKER`] sentinel so [`run_wave`](RunCtx::run_wave) routes
/// it through the dedicated no-lesson arm ([`is_degenerate_reviewer`]) rather than the
/// generic wave-failure arm (which would emit a misattributing per-unit lesson). It still
/// PROPAGATES OUT of `run` (`run_wave` sets it as the wave's error), so a dead reviewer
/// HALTS the run loudly rather than escalating the unit. The respawn loop lives inside ONE
/// review attempt and never touches the unit's remediation counter, so halting here does
/// NOT charge the unit an attempt (no `UnitFailed`, no `UnitEscalated`).
///
/// RECOVERY (the honest one, not the dead "just re-run"): reviewer spawn results are
/// LAST-WRITE-WINS ([`spawn::result_of`] - a corrected re-record supersedes an earlier
/// one), so the operator recovers by re-driving the reviewer and recording a SUBSTANTIVE
/// result for one of its deterministic retry ids; the loop then replays that non-
/// degenerate result and folds normally. Re-running WITHOUT a corrected result just
/// replays the recorded empties and halts here again - which is why the message names the
/// re-record, not a bare re-run.
fn degenerate_reviewer(stage: &str, tier: &str, agent: &str, role: &str, attempt: u32) -> Error {
    let latest = spawn_retry_id(stage, role, attempt, REVIEWER_RESPAWN_BOUND);
    Error(format!(
        "{DEGENERATE_MARKER}stage {stage:?} {tier} {agent:?} returned empty/whitespace-only output \
         on all {} spawns (its original spawn plus {REVIEWER_RESPAWN_BOUND} respawns): a degenerate \
         reviewer result is an infrastructure failure, not a verdict - the run halts and the unit is \
         NOT charged a remediation attempt. Recover by re-driving the reviewer and recording a \
         SUBSTANTIVE result for one of its spawn ids (results are last-write-wins, so a corrected \
         re-record supersedes the empty one), e.g. `rigger result {latest:?} <substantive output>`; \
         then re-run. Re-running WITHOUT a corrected result replays the recorded empties and halts \
         here again.",
        REVIEWER_RESPAWN_BOUND + 1
    ))
}

/// Whether `e` is a degenerate-reviewer HALT signal (see [`degenerate_reviewer`]) rather
/// than a real stage failure. Robust to the review sites' own error wrapping, since the
/// [`DEGENERATE_MARKER`] survives as a substring.
fn is_degenerate_reviewer(e: &Error) -> bool {
    e.0.contains(DEGENERATE_MARKER)
}

/// The conductor's injected ports.
pub struct Deps<'a> {
    pub store: &'a dyn EventStore,
    pub driver: &'a dyn AgentDriver,
    pub gates: &'a dyn gate::Runner,
    /// A git repo to isolate each agent in via a throwaway worktree; empty
    /// disables isolation (the agent runs in the current directory).
    pub repo: String,
    /// Grounds each agent before it runs; None grounds nothing.
    pub grounder: Option<&'a dyn Grounder>,
    /// The live context-graph projection, folded during the run; None disables it.
    pub graph: Option<&'a dyn Projection>,
    /// The spec's acceptance criteria; when non-empty the coverage gate refuses a
    /// run unless every criterion is covered by a stage.
    pub criteria: Vec<String>,
}

#[derive(Deserialize)]
struct UnitProposed {
    id: String,
    #[serde(default)]
    agent: String,
    #[serde(default)]
    needs: Vec<String>,
    /// The spec criterion the proposed unit serves. The planner emits it as
    /// `criterion` (the PLAN_PROTOCOL shape); `coverage` is accepted as an alias so a
    /// hand-authored proposal using the stage `coverage` vocabulary still maps.
    #[serde(default, alias = "criterion")]
    coverage: String,
    #[serde(default)]
    gates: Vec<String>,
}

/// Run executes the workflow and returns the final run state, projected from the
/// events it emitted. Independent stages run concurrently in waves.
pub fn run(cfg: &Config, deps: &Deps) -> Result<RunState, Error> {
    validate_acyclic(&cfg.workflow.stages)?;

    // Run scoping (spec 06, unit 1 - Gap 11). Begin the run - or adopt the one already
    // in flight for these criteria - BEFORE reading any prior state, so the boundary is
    // in the log and every fold below scopes to it. A fresh campaign mints a new
    // `RunStarted`; a resume/idle/replay over the same criteria adopts the existing run
    // and appends nothing. The run id then rides every event this process emits.
    let run_id = crate::run::ensure_started(deps.store, &deps.criteria)?;

    // Resume by replay (§4.2): seed integrated/terminal from the existing log so a
    // crashed or re-run conductor skips work that already landed instead of
    // re-spawning every agent from scratch. Only the CURRENT run's slice is folded
    // (`crate::run::current_run`): a prior run's non-terminal residue sits before this
    // run's `RunStarted` and so can never seed ready work (the Gap 11 zombie fix), while
    // its decisions/findings stay visible as memory through the whole-stream graph.
    let all_prior = deps.store.read_stream(STREAM, 0, Direction::Forward)?;
    let prior_events = crate::run::current_run(&all_prior);
    let prior = ledger::project(prior_events).map_err(|e| Error(e.to_string()))?;
    // Replay idempotency (spec 04, criterion 4): seed the replay-key set from the prior
    // log's [`META_REPLAY_KEY`] metadata so a step re-running the conductor over recorded
    // history re-appends none of the keyed unit-lifecycle events it already emitted, and
    // re-reaching an already-run gate replays its recorded verdict.
    let replayed_keys: HashSet<String> = prior_events
        .iter()
        .filter_map(|e| e.meta.get(META_REPLAY_KEY).cloned())
        .collect();
    // Cross-step spawn budget (spec 04, criterion 5 / finding adv-budget-per-step-resets):
    // the authoritative spawn count is DERIVED from the log, not an in-memory counter that
    // resets every step process. Fold the DISTINCT spawn requests already recorded (keyed
    // by deterministic id, so a re-parked id is not double-counted) into `base_spawns` and
    // seed the running counter with it, so the breaker sees the run's WHOLE spawn history
    // and `defaults.budget` binds no matter how many `rigger step` processes the run spans.
    // Their ids seed `recorded_spawn_ids` so `reserve_spawn` can tell a REPLAY of an
    // already-recorded spawn (admit free) from a genuinely new one, without re-reading the
    // whole stream per spawn. The blocking drivers never park a request, so both are empty
    // for them and their in-process spawns are the whole count - historical behavior,
    // unchanged.
    let recorded_spawns = spawn::recorded(prior_events).map_err(|e| Error(e.to_string()))?;
    let base_spawns = recorded_spawns.len() as u32;
    let recorded_spawn_ids: HashSet<String> = recorded_spawns.into_keys().collect();
    // Gate-verdict replay cache (finding arch-gate-verdict-redundant-scan): seed the
    // key -> (pass, evidence) map ONCE here from the same prior log, so re-reaching an
    // already-run inline/deferred gate replays its verdict via an O(1) map lookup rather
    // than re-scanning the whole stream per gate per step. Only keyed GateVerdict events
    // (the gate runs) carry a replay key; the integrate-time GATED_BY artifact verdicts
    // do not, so they never seed a gate-run key.
    let gate_verdicts: HashMap<String, (bool, String)> = prior_events
        .iter()
        .filter(|e| e.type_ == contextgraph::TYPE_GATE_VERDICT)
        .filter_map(|e| {
            let key = e.meta.get(META_REPLAY_KEY)?.clone();
            let v: GateVerdictData = serde_json::from_slice(&e.data).ok()?;
            Some((key, (v.pass, v.evidence)))
        })
        .collect();

    // The RunCtx is created BEFORE the coverage check so a coverage gap can be
    // flagged as a spec defect through the event log (item 2 / §4.4) instead of
    // returning a bare error with no audit trail. It carries the per-unit prior
    // status (resume-continuity): a non-integrated unit that ran in a prior window
    // CONTINUES from its recorded phase (skip re-implement when its code already
    // exists on its branch, skip re-review when already approved) rather than
    // restarting from implement.
    let prior_status: HashMap<String, ledger::Status> = prior
        .units
        .values()
        .map(|u| (u.id.clone(), u.status))
        .collect();
    // A unit that FAILED in a prior window (but did not escalate) carries its folded
    // attempt count here, so a resumed run CONTINUES its bounded remediation from that
    // count rather than restarting at 0 - attempts accumulate across windows and the
    // unit escalates at the configured `max_retries` bound TOTAL, not per-window forever.
    let prior_attempts: HashMap<String, u32> = prior
        .units
        .values()
        .filter(|u| u.attempts > 0)
        .map(|u| (u.id.clone(), u.attempts))
        .collect();
    let ctx = RunCtx {
        cfg,
        deps,
        run_id,
        gate_tracker: Mutex::new(HashMap::new()),
        integrate_mu: Mutex::new(()),
        spawns: AtomicU32::new(base_spawns),
        base_spawns,
        recorded_spawn_ids,
        budget_broke: std::sync::atomic::AtomicBool::new(false),
        parked: std::sync::atomic::AtomicBool::new(false),
        manual_review: std::sync::atomic::AtomicBool::new(false),
        budget_halted: std::sync::atomic::AtomicBool::new(false),
        prior_status,
        prior_attempts,
        replayed_keys: Mutex::new(replayed_keys),
        gate_verdicts: Mutex::new(gate_verdicts),
    };

    let mut stages = cfg.workflow.stages.clone();
    let mut proposed: HashSet<String> = HashSet::new();
    let mut integrated: HashSet<String> = prior
        .units
        .values()
        .filter(|u| u.status == ledger::Status::Integrated)
        .map(|u| u.id.clone())
        .collect();
    let mut terminal: HashSet<String> = prior
        .units
        .values()
        .filter(|u| prior.is_terminal(&u.id))
        .map(|u| u.id.clone())
        .collect();

    // Deterministic decomposition baseline (§3.2): when the run is spec-driven
    // (`deps.criteria` non-empty), the conductor itself creates ONE implement unit per
    // acceptance criterion from the fan-out implement TEMPLATE, BEFORE any agent runs.
    // The template is a template, not a unit, so it is removed and the per-criterion
    // units replace it; each unit carries the criterion text as its `coverage`, so it
    // grounds on the real criterion and its UnitStarted records the real
    // spec_criterion - not the template's label, and never the `coverage: required`
    // bug. Each baseline unit `needs` the planner when one exists, so the planner runs
    // FIRST and refines this baseline (splitting a criterion, adding a sub-unit) via
    // UnitProposed. With no fan-out template the run synthesizes no baseline units and
    // falls back to the historical shape. The no-spec (empty criteria) path is
    // untouched: no template expansion, the workflow's own stages run as authored.
    if !deps.criteria.is_empty() {
        if let Some(template_name) = fan_out_template_name(&stages) {
            let template = stages.remove(&template_name).expect("template just found");
            let producer = producer_name(&stages);
            for (name, unit) in baseline_units(&template, &deps.criteria, producer.as_deref()) {
                stages.entry(name).or_insert(unit);
            }
        }
    }

    // Resume-safe dedup (the duplication fix, order-independent): fold any
    // ALREADY-EMITTED UnitProposed events from a PRIOR window and apply the
    // planner-supersedes-baseline dedup BEFORE the first wave can schedule anything.
    //
    // On a RESUME run the `plan` stage is already integrated (seeded into `integrated`
    // above), so its baselines are immediately ready - and `run_wave` would otherwise
    // run them in the FIRST iteration, BEFORE the bottom-of-loop `harvest_proposed`
    // folds the prior planner's proposals and supersedes the matching baselines. That
    // races a baseline to run as a DUPLICATE alongside the planner's unit for the same
    // criterion. Harvesting here, before any `run_wave`, makes the supersede hold on
    // resume regardless of scheduling order: a baseline whose criterion a prior
    // planner unit already cited is removed before it can be scheduled.
    //
    // On a FRESH run there are no UnitProposed events yet, so this is a no-op the first
    // time (the planner has not run); its proposals are harvested by the per-iteration
    // `harvest_proposed` below exactly as before. The `integrated`/`terminal` guards in
    // `harvest_proposed` keep a baseline that already started/integrated in a prior
    // window from being yanked out from under its own work.
    ctx.harvest_proposed(&mut stages, &mut proposed, &integrated, &terminal)?;

    // Coverage gate (§3.2, §8). A planner (`produces`) stage DEFERS coverage to
    // after planning: it has no units yet, so we run the planning wave + harvest
    // the proposed units FIRST, then check coverage against the extended DAG.
    // A run with no planner checks coverage up front, before any agent runs.
    if has_producer(&stages) {
        let ready = ready_stages(&stages, &integrated, &terminal);
        if !ready.is_empty() {
            ctx.run_wave(&stages, &ready, &mut integrated, &mut terminal)?;
            ctx.harvest_proposed(&mut stages, &mut proposed, &integrated, &terminal)?;
        }
    }
    ctx.check_coverage_or_flag(&stages, &deps.criteria)?;

    loop {
        let ready = ready_stages(&stages, &integrated, &terminal);
        if ready.is_empty() {
            break;
        }
        // checkBudget circuit-breaker (§4.4, §8): before each wave, if the spawn
        // budget is spent, trip the breaker - record it, abort the task, and pause
        // the loop. Resume replays the ledger and continues from where it stopped.
        if ctx.budget_tripped() {
            ctx.trip_budget_breaker()?;
            break;
        }
        ctx.run_wave(&stages, &ready, &mut integrated, &mut terminal)?;
        // The breaker also trips at SPAWN granularity, mid-wave (item 9): a single
        // wide wave can exhaust the budget partway through, refusing later spawns.
        // Record the breaker and stop here too, not only at the next wave boundary.
        if ctx.budget_broke() {
            ctx.trip_budget_breaker()?;
            break;
        }
        ctx.harvest_proposed(&mut stages, &mut proposed, &integrated, &terminal)?;
    }

    // Phase boundary (§4.3): the wave loop has converged - every ready unit reached a
    // terminal state and integrated on its INLINE gates. Now run the workflow's
    // deferred gates ONCE, here, at the run's end, rather than per unit inline. Each
    // deferred gate runs a single time; a failing one is surfaced truthfully (a
    // DeferredGateFailed event + the run reported not-fully-done).
    //
    // The tree is FINAL - a deferred verdict measures the fully-assembled tree, not a
    // partial one - exactly when NO unit is in a TRANSIENT pending state: none PARKED
    // (a stepwise/replay frontier a later step drains and integrates), none
    // MANUAL-REVIEW-PAUSED (awaiting a human who will approve+integrate it on a later
    // step), and no BUDGET HALT that left ready units unscheduled (a resume with a
    // fresh in-process budget completes them). Recording a run-scoped deferred verdict
    // against any of those partial trees would lock that result in forever - every
    // later step replays it and the assembled tree is never validated (findings
    // adv-deferred-replay-locks-partial-tree, rf-converged-ignores-budget-refusal,
    // adv-confirm-converged-nonpark-partial-tree). When the tree is not yet final the
    // deferred gate is DEFERRED to the step that drains the last pending unit.
    //
    // We do NOT additionally require every unit to be integrated/terminal. A unit that
    // ESCALATES is terminal-forever-yet-never-integrated, and `ready_stages` gates its
    // dependents on INTEGRATED deps, so those dependents never schedule and never enter
    // `terminal` - `stages.keys().all(terminal.contains)` would then stay false FOREVER
    // and permanently SUPPRESS the whole-tree deferred gate (e.g. a security scan) for
    // any workflow that escalates one unit but still assembles+delivers the rest
    // (finding adv-converged-escalated-dep-suppresses-deferred). Once nothing is
    // transiently pending, every remaining unit is settled - integrated, escalated,
    // verified-but-does-not-integrate, or blocked-forever behind such a unit - so the
    // as-assembled tree IS final and the deferred gate runs against it, once.
    let converged = !ctx.parked() && !ctx.manual_review_pending() && !ctx.budget_halted();
    ctx.run_deferred_gates(&stages, converged)?;

    let events = deps.store.read_stream(STREAM, 0, Direction::Forward)?;
    // Project the caller-visible run state from ONLY this run's slice (Gap 11, unit 1),
    // then stamp the live HALT reason (Gap 13) from the conductor's IN-PROCESS breaker
    // state, not from a fold of the log: a halt is a runtime condition of THIS process (a
    // resume with a raised budget clears it), so `rigger step` reads it here to print a
    // halt reason distinct from convergence and the thin driver stops loudly on it.
    let mut rs =
        ledger::project(crate::run::current_run(&events)).map_err(|e| Error(e.to_string()))?;
    rs.budget_halt = ctx.halt_reason();
    Ok(rs)
}

/// Whether the workflow has a planner stage that produces a DAG at runtime, which
/// defers the coverage gate until after planning (§3.2).
fn has_producer(stages: &BTreeMap<String, Stage>) -> bool {
    stages.values().any(|st| !st.produces.is_empty())
}

struct RunCtx<'a> {
    cfg: &'a Config,
    deps: &'a Deps<'a>,
    /// The id of the current run (spec 06, unit 1): the fresh id minted by
    /// [`crate::run::ensure_started`] when this run began, or the adopted id of the run
    /// in flight. Every event this process appends through [`append_and_fold`](RunCtx::append_and_fold)
    /// carries it in [`crate::run::META_RUN_ID`] metadata, and it is threaded onto each
    /// spawn (via [`SpawnOpts::run_id`]) so a parked request is attributable to its run.
    /// Empty only in the pure-helper test context, where nothing is appended.
    run_id: String,
    gate_tracker: Mutex<HashMap<String, Gate>>,
    integrate_mu: Mutex<()>,
    /// The CUMULATIVE spawn count for the budget circuit-breaker (§4.4, §8), across
    /// every step process the run spans: `base_spawns` (the distinct spawn requests
    /// already recorded in the log when this process started) plus the NEW spawns this
    /// process admits. Seeded from the log rather than reset to 0 each process, so
    /// `defaults.budget` binds across the many `rigger step` processes a stepwise run
    /// spans (spec 04, criterion 5 / finding adv-budget-per-step-resets). A replayed
    /// spawn (an id already recorded) never increments this - its budget was spent when
    /// it was first parked. The blocking drivers never park, so `base_spawns` is 0 for
    /// them and this counts their in-process spawns exactly as before.
    spawns: AtomicU32,
    /// The spawn count already recorded in the log when this process started - the value
    /// `spawns` is seeded to. The pre-wave breaker only halts a process that has itself
    /// admitted a NEW spawn beyond budget (`spawns > base_spawns`), so a resume whose
    /// ready frontier is entirely REPLAYS of already-recorded spawns is never aborted
    /// before it can replay and integrate that already-paid work; a genuinely new
    /// over-budget spawn is refused mid-wave by [`reserve_spawn`](RunCtx::reserve_spawn),
    /// which trips the breaker there instead.
    base_spawns: u32,
    /// The deterministic ids of the spawn requests already recorded in the log when this
    /// process started (the keys of [`spawn::recorded`]). Seeded ONCE at run start so
    /// [`reserve_spawn`](RunCtx::reserve_spawn) can classify a spawn as a REPLAY (id
    /// already recorded by an earlier step - admit free, its budget was already spent) or
    /// a genuinely NEW spawn (count it against the cumulative budget) with an O(1) lookup,
    /// rather than re-reading and re-folding the whole stream on every spawn. A spawn this
    /// process parks is not added here: it is reached only once per process (a parked unit
    /// unwinds and every spawn site has a distinct id), so the prior-only set is sufficient.
    recorded_spawn_ids: HashSet<String>,
    /// Set the moment a spawn is REFUSED because the budget is spent (item 9): the
    /// breaker now trips at spawn granularity, mid-wave, not only at wave boundaries.
    /// The run loop checks this after each wave to record the breaker and stop.
    budget_broke: std::sync::atomic::AtomicBool,
    /// Set the moment any in-flight spawn PARKS (the stepwise/replay driver hit an
    /// unrecorded frontier). A parked wave loop empties with units still un-integrated,
    /// so the run has NOT genuinely converged: a later step will drain the park and
    /// integrate more units. The phase boundary consults this so the DEFERRED gate is
    /// held until the step that fully assembles the tree - never recording a verdict
    /// against the partial/base tree a parked frontier leaves behind (finding
    /// adv-deferred-replay-locks-partial-tree).
    parked: std::sync::atomic::AtomicBool,
    /// Set the moment a stage PAUSES for human review (§4.3, its effective autonomy is
    /// Manual): the unit emitted ManualReview and returned pending WITHOUT parking, so
    /// it is terminal-inserted but not integrated - and its autonomy does not change
    /// across steps, so the run can never legitimately converge while it waits. The
    /// tree is therefore NOT final: a human has yet to approve, and the unit's work is
    /// missing from the assembled tree. The phase boundary consults this so a DEFERRED
    /// gate is HELD rather than recorded against the partial/base tree the paused unit
    /// leaves behind - a run-scoped verdict recorded now would lock that partial-tree
    /// result in forever (findings rf-converged-ignores-budget-refusal /
    /// adv-confirm-converged-nonpark-partial-tree).
    manual_review: std::sync::atomic::AtomicBool,
    /// Set the moment the spawn-budget breaker HALTS the run with ready units still
    /// unscheduled ([`trip_budget_breaker`](RunCtx::trip_budget_breaker), the single
    /// chokepoint for BOTH the pre-wave and mid-wave trip). A budget-halted run left
    /// work undone that a resume (with a fresh in-process budget) picks up, so the tree
    /// is NOT final. Like the manual-review case this is a TRANSIENT non-park pending
    /// state that terminal-inserts a non-integrated unit without setting `parked`, so
    /// the phase boundary holds the DEFERRED gate rather than record it against the
    /// partial tree the halt left behind (same finding pair as `manual_review`).
    budget_halted: std::sync::atomic::AtomicBool,
    /// Each unit's LAST recorded status from the folded prior log (resume-continuity):
    /// a non-integrated, non-terminal unit that ran in a prior window has a status
    /// here (green/verified/reviewed/...), which `run_single_stage` uses to CONTINUE
    /// the unit from where it stopped rather than restart from implement. A unit
    /// absent from the map (a fresh unit this run, or one with no prior progress) runs
    /// the full lifecycle. Integrated/terminal units are skipped before they ever
    /// reach the lifecycle, so their presence here is harmless.
    prior_status: HashMap<String, ledger::Status>,
    /// Each unit's folded remediation attempt count from the prior log (resume of a
    /// mid-remediation `Failed` unit). A unit that FAILED but did not yet ESCALATE
    /// across one window must continue its bounded remediation from the recorded
    /// count - NOT restart at 0 - so attempts ACCUMULATE across windows and the unit
    /// escalates at the configured `max_retries` bound TOTAL, instead of doing a fresh
    /// `max_retries` every window forever. `run_single_stage` seeds its `attempts` loop variable from this
    /// map (keyed by unit id); a unit absent from it starts at 0 (a fresh unit, or one
    /// that never failed). Integrated/escalated units are terminal and skipped before
    /// the lifecycle, so their presence here is harmless.
    prior_attempts: HashMap<String, u32>,
    /// The set of REPLAY KEYS already present in the run (spec 04, criterion 4): seeded
    /// at run start from the prior log's [`META_REPLAY_KEY`] metadata, then extended as
    /// this process emits. [`emit_keyed`](RunCtx::emit_keyed) consults it so a step
    /// re-running the conductor over recorded history appends each keyed unit-lifecycle
    /// event AT MOST ONCE - the log stays free of duplicate UnitStarted/green/verified/
    /// reviewed/ManualReview events no matter how many step processes replay it.
    replayed_keys: Mutex<HashSet<String>>,
    /// The recorded gate verdicts keyed by their replay key -> `(pass, evidence)`, seeded
    /// ONCE at run start from the prior log's `GateVerdict` events and extended as this
    /// process records new verdicts. [`recorded_gate_verdict`](RunCtx::recorded_gate_verdict)
    /// consults this map instead of re-reading and re-scanning the whole append-only
    /// stream on every inline/deferred gate of every step (finding
    /// arch-gate-verdict-redundant-scan): gate-verdict replay is O(1) per lookup, seeded
    /// from the same `prior_events` read that already seeded `replayed_keys`.
    gate_verdicts: Mutex<HashMap<String, (bool, String)>>,
}

#[cfg(test)]
impl<'a> RunCtx<'a> {
    /// A bare RunCtx for unit-testing pure helpers (e.g. the cwd-isolation guard) that
    /// only read `cfg`/`deps` - no prior log, no spawns. Not for driving a run.
    fn for_test(cfg: &'a Config, deps: &'a Deps<'a>) -> Self {
        // Mirror `run`'s log-derived seeding so budget tests can pre-park spawn requests
        // and see the breaker fold them from the store, exactly as a real step process
        // would. An empty store seeds 0 - the historical value for the pure-helper tests.
        let recorded_spawns = deps
            .store
            .read_stream(STREAM, 0, Direction::Forward)
            .ok()
            .and_then(|events| spawn::recorded(&events).ok())
            .unwrap_or_default();
        let base_spawns = recorded_spawns.len() as u32;
        let recorded_spawn_ids: HashSet<String> = recorded_spawns.into_keys().collect();
        RunCtx {
            cfg,
            deps,
            // The pure-helper tests append nothing that needs run scoping; an empty run
            // id makes `append_and_fold` stamp no run-id metadata.
            run_id: String::new(),
            gate_tracker: Mutex::new(HashMap::new()),
            integrate_mu: Mutex::new(()),
            spawns: AtomicU32::new(base_spawns),
            base_spawns,
            recorded_spawn_ids,
            budget_broke: std::sync::atomic::AtomicBool::new(false),
            parked: std::sync::atomic::AtomicBool::new(false),
            manual_review: std::sync::atomic::AtomicBool::new(false),
            budget_halted: std::sync::atomic::AtomicBool::new(false),
            prior_status: HashMap::new(),
            prior_attempts: HashMap::new(),
            replayed_keys: Mutex::new(HashSet::new()),
            gate_verdicts: Mutex::new(HashMap::new()),
        }
    }
}

impl RunCtx<'_> {
    fn emit(&self, type_: &str, payload: Value) -> Result<(), Error> {
        self.emit_with_actor("", type_, payload)
    }

    /// The conductor's SINGLE event-mutation authority: append one already-built event
    /// to the run stream and fold it into the live graph (so later agents read it). Both
    /// emit paths - the actor-tagged [`emit_with_actor`](RunCtx::emit_with_actor) and the
    /// replay-keyed [`emit_keyed`](RunCtx::emit_keyed) - route through here, so the
    /// expected-revision handling, the position stamp, and the post-append graph fold
    /// live in ONE place and can never silently diverge (finding
    /// arch-emit-keyed-dup-authority).
    fn append_and_fold(&self, mut ev: Event) -> Result<(), Error> {
        // Stamp the run id on every conductor-emitted event (spec 06, unit 1): the one
        // chokepoint every emit path routes through, so unit/status/gate-verdict/spec-
        // defect events are all attributable to their run. Skipped only when the run id
        // is empty (the pure-helper test context, which appends nothing meaningful).
        if !self.run_id.is_empty() {
            ev = ev.with_meta(crate::run::META_RUN_ID, &self.run_id);
        }
        let pos =
            self.deps
                .store
                .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))?;
        if let Some(g) = self.deps.graph {
            ev.position = pos;
            let _ = g.apply(&ev);
        }
        Ok(())
    }

    /// Emit an event, optionally stamping the acting agent in its metadata (the
    /// DECIDED-edge source), appending to the log and folding it into the live
    /// graph so later agents can read it.
    fn emit_with_actor(&self, actor: &str, type_: &str, payload: Value) -> Result<(), Error> {
        let mut ev = Event::new(type_, serde_json::to_vec(&payload)?);
        if !actor.is_empty() {
            ev = ev.with_meta(contextgraph::META_ACTOR, actor);
        }
        self.append_and_fold(ev)
    }

    /// Emit an event IDEMPOTENTLY under a deterministic replay `key` (spec 04, criterion
    /// 4): if the key was already recorded in the log (a prior step emitted it) or has
    /// already been emitted this process, the event is a REPLAY and the append is
    /// skipped. Otherwise it is appended - with the key stamped in [`META_REPLAY_KEY`]
    /// metadata so a later step recognizes the replay - and folded into the live graph,
    /// exactly like [`emit`](RunCtx::emit).
    ///
    /// This is how a step re-running the conductor over recorded history re-emits no
    /// duplicate unit-lifecycle event: UnitStarted, the green/verified/reviewed
    /// statuses, and the manual-review pause all flow through here, keyed by unit +
    /// phase (+ remediation attempt where an event legitimately recurs per attempt).
    fn emit_keyed(&self, key: &str, type_: &str, payload: Value) -> Result<(), Error> {
        self.emit_keyed_meta(key, type_, payload, &[])
    }

    /// [`emit_keyed`](RunCtx::emit_keyed) plus arbitrary EXTRA metadata stamped alongside
    /// the replay key (each `(key, value)` pair with a non-empty value; empties are
    /// omitted so the wire stays clean, exactly as [`emit_with_actor`](RunCtx::emit_with_actor)
    /// omits an empty actor). This is how a spawn's requested [`META_MODEL_ALIAS`] and
    /// worker-reported [`META_MODEL_RESOLVED`] ride the unit-lifecycle events the conductor
    /// emits for that spawn (spec 05 line 52) without a second event type: the model is
    /// audit metadata on an event that already exists, so folds/projections ignore it just
    /// like the replay key. The dedup guarantee is unchanged - a replayed key still appends
    /// nothing, so the extra metadata never manufactures a duplicate.
    fn emit_keyed_meta(
        &self,
        key: &str,
        type_: &str,
        payload: Value,
        extra: &[(&str, &str)],
    ) -> Result<(), Error> {
        {
            // A first insert means this key is new work; a repeat means the log already
            // carries it (seeded at run start) or this process already emitted it - a
            // replay, so append nothing. Holding the lock only around the set guard lets
            // concurrent units in a wave append their own keyed events in parallel.
            let mut keys = self.replayed_keys.lock().unwrap();
            if !keys.insert(key.to_string()) {
                return Ok(());
            }
        }
        let mut ev =
            Event::new(type_, serde_json::to_vec(&payload)?).with_meta(META_REPLAY_KEY, key);
        for (k, v) in extra {
            if !v.is_empty() {
                ev = ev.with_meta(*k, *v);
            }
        }
        self.append_and_fold(ev)
    }

    /// The requested model ALIAS an agent is spawned with for `attempt` - the cascade rung
    /// [`AgentDef::model_for_attempt`](crate::config::AgentDef::model_for_attempt) resolves
    /// (spec 10 unit 4), which is `AgentDef::model` for a ladder-less agent - or empty when
    /// the stage names no agent (a producer/review-only stage) or the agent is unknown. This
    /// is the SAME resolution the driver runs to pick the actual spawn model, so the alias
    /// stamped onto the spawn's unit events via [`META_MODEL_ALIAS`] (spec 05 line 52) names
    /// exactly the rung that ran for that attempt.
    fn agent_model(&self, agent_id: &str, attempt: u32) -> String {
        self.cfg
            .agents
            .get(agent_id)
            .map(|a| a.model_for_attempt(attempt))
            .unwrap_or_default()
    }

    /// The recorded outcome of a gate whose verdict was already emitted under `key` in a
    /// prior step - its `(pass, evidence)` - or `None` if this gate has not run yet. A
    /// step re-reaching an already-run gate REPLAYS this instead of re-running the
    /// command (spec 04, criterion 4): the log is the single source of truth for a
    /// gate's outcome, so a re-run never pays gate-duration time twice and never appends
    /// a second GateVerdict. Only [`emit_keyed`](RunCtx::emit_keyed)-stamped verdicts
    /// (the inline and deferred gate runs) carry a replay key; the integrate-time
    /// GATED_BY artifact verdicts do not, so they never match a gate-run key.
    ///
    /// Consults the `gate_verdicts` cache (seeded once at run start from the prior log,
    /// extended by [`emit_gate_verdict`](RunCtx::emit_gate_verdict) as this process runs
    /// gates), so a lookup is O(1) - never a fresh whole-stream scan per gate per step
    /// (finding arch-gate-verdict-redundant-scan).
    fn recorded_gate_verdict(&self, key: &str) -> Option<(bool, String)> {
        self.gate_verdicts.lock().unwrap().get(key).cloned()
    }

    /// Emit a gate's `GateVerdict` under its replay `key` (idempotent via
    /// [`emit_keyed`](RunCtx::emit_keyed)) and cache its outcome so a later
    /// [`recorded_gate_verdict`](RunCtx::recorded_gate_verdict) - this process or a
    /// re-step - replays it without re-running the command. The append and the cache
    /// insert are paired here so they can never drift.
    fn emit_gate_verdict(
        &self,
        key: &str,
        gid: &str,
        pass: bool,
        evidence: &str,
    ) -> Result<(), Error> {
        self.emit_keyed(
            key,
            contextgraph::TYPE_GATE_VERDICT,
            json!({"gate": gid, "pass": pass, "evidence": evidence}),
        )?;
        self.gate_verdicts
            .lock()
            .unwrap()
            .insert(key.to_string(), (pass, evidence.to_string()));
        Ok(())
    }

    /// The configured remediation depth: how many attempts a failed unit gets
    /// before escalation (§4.4). Comes from `defaults.max_retries`; absent (`0`)
    /// falls back to `safety::MAX_RETRIES` (3), the exact historical bound, so an
    /// un-set workflow escalates exactly as before. A higher value gives a subtle
    /// unit room to converge under the full strict review - it loosens the depth
    /// limit, never the review bar.
    fn max_retries(&self) -> u32 {
        let configured = self.cfg.workflow.defaults.max_retries;
        if configured == 0 {
            safety::MAX_RETRIES
        } else {
            configured
        }
    }

    /// Whether the pre-wave spawn-budget breaker has tripped (§4.4, §8): a positive
    /// `defaults.budget`, the CUMULATIVE spawn count (across steps) has reached it, AND
    /// this process itself admitted a NEW spawn beyond `base_spawns`.
    ///
    /// The `spawns > base_spawns` guard is what keeps a RESUME correct: a step whose
    /// ready frontier is entirely replays of already-recorded spawns has `spawns ==
    /// base_spawns`, so the breaker does NOT abort the step at its very first wave, before
    /// it has run any agent - it is free to replay that already-paid work in-process. A
    /// genuinely new over-budget spawn is refused mid-wave by
    /// [`reserve_spawn`](RunCtx::reserve_spawn), which trips the breaker there - so the cap
    /// is still enforced, just at the spawn that actually exceeds it.
    ///
    /// This guard is NOT a completion guarantee: assembling a REVIEWED unit needs NEW
    /// lens/adversary/adjudicator spawns whose ids are not yet recorded, so once the
    /// implementer replays free and the unit reaches `verified`, the first review-tier
    /// spawn is a new spawn that `reserve_spawn` refuses at a spent budget. That refusal
    /// now unwinds cleanly and trips the breaker (a `BudgetExhausted` event via
    /// [`budget_refused`]/[`is_budget_refused`] + the mid-wave `budget_broke()` check),
    /// exactly as the implementer's `Ok(false)` refusal does - so a run that exceeds
    /// `defaults.budget` at ANY spawn site aborts with `BudgetExhausted` (criterion 5).
    /// Completing such a unit requires the operator to raise `defaults.budget`; the count
    /// binds across steps, so the resume then has room for the review spawns and finishes
    /// integrating the unit (findings adv-budget-guard-cannot-assemble-reviewed-unit,
    /// budget-review-tier-no-exhausted).
    fn budget_tripped(&self) -> bool {
        let budget = self.cfg.workflow.defaults.budget;
        let spawns = self.spawns.load(Ordering::SeqCst);
        budget > 0
            && spawns > self.base_spawns
            && safety::budget_exhausted(budget as i64, spawns as i64)
    }

    /// Whether `spawn_id` was already parked in the run log by an EARLIER step process -
    /// i.e. this spawn is a REPLAY whose budget was spent when it was first parked. An
    /// O(1) lookup into `recorded_spawn_ids`, seeded once at run start; the blocking
    /// drivers never park, so this is always `false` for them and every one of their
    /// spawns counts against the budget, unchanged.
    fn spawn_is_recorded(&self, spawn_id: &str) -> bool {
        self.recorded_spawn_ids.contains(spawn_id)
    }

    /// Atomically reserve one spawn against the CUMULATIVE budget at SPAWN granularity
    /// (item 9), binding across step processes (spec 04, criterion 5). Returns `true`
    /// when the spawn is admitted and `false` when it is refused (the budget is spent);
    /// on refusal it sets `budget_broke` so the run loop records the breaker and halts.
    ///
    /// A spawn whose `spawn_id` is ALREADY recorded is a REPLAY: its budget was spent
    /// when it was first parked (it is already folded into `base_spawns`), so it is
    /// admitted WITHOUT counting again and can NEVER be refused - the already-paid work
    /// must be free to replay and integrate on a resume. A genuinely NEW spawn is
    /// reserved with a `fetch_update` on `spawns` (seeded to `base_spawns`): the
    /// check-and-increment is atomic, so concurrent lenses in one wide wave never
    /// overshoot, and the cap counts the run's WHOLE spawn history, not just this
    /// process's. A zero budget means unlimited - every spawn is admitted (new spawns
    /// still counted, so `spawns` stays an accurate cumulative total for reporting).
    fn reserve_spawn(&self, spawn_id: &str) -> bool {
        if self.spawn_is_recorded(spawn_id) {
            return true;
        }
        let budget = self.cfg.workflow.defaults.budget;
        if budget == 0 {
            self.spawns.fetch_add(1, Ordering::SeqCst);
            return true;
        }
        let admitted = self
            .spawns
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                if n < budget {
                    Some(n + 1)
                } else {
                    None
                }
            })
            .is_ok();
        if !admitted {
            self.budget_broke.store(true, Ordering::SeqCst);
        }
        admitted
    }

    /// Whether a spawn was refused mid-wave because the budget was spent (item 9).
    fn budget_broke(&self) -> bool {
        self.budget_broke.load(Ordering::SeqCst)
    }

    /// Whether any in-flight spawn PARKED this run (the stepwise/replay driver hit an
    /// unrecorded frontier). A parked run has not converged - a later step drains the
    /// frontier and integrates more units - so the phase boundary holds the deferred
    /// gate rather than record a verdict against the partial/base tree.
    fn parked(&self) -> bool {
        self.parked.load(Ordering::SeqCst)
    }

    /// Whether any unit is PAUSED for human review this run (§4.3): a Manual-autonomy
    /// stage emitted ManualReview and returned pending without integrating. Like a
    /// park, it is a TRANSIENT non-final state, so the phase boundary holds the
    /// deferred gate rather than record a verdict against the tree missing that unit.
    fn manual_review_pending(&self) -> bool {
        self.manual_review.load(Ordering::SeqCst)
    }

    /// Whether the spawn-budget breaker HALTED the run with ready units still
    /// unscheduled (item 9 / §4.4). A budget-halted run left work undone that a resume
    /// completes, so the tree is not final and the deferred gate is deferred.
    fn budget_halted(&self) -> bool {
        self.budget_halted.load(Ordering::SeqCst)
    }

    /// Trip the spawn-budget circuit-breaker (§4.4, §8): record `BudgetExhausted` with the
    /// budget and the spawns made, then record the `TaskAborted` that halts the run (abortTask,
    /// §4.4: integrated work is already committed and every per-stage worktree is removed as
    /// its stage finishes, so there is no un-integrated worktree left to discard - the abort
    /// is a durable audit record; the loop stops, a pause the resume replays past). Shared by
    /// the pre-wave check and the mid-wave (spawn-granularity, item 9) trip so both halt the
    /// run the same way, with one audit trail.
    fn trip_budget_breaker(&self) -> Result<(), Error> {
        // The breaker halts the run with ready units still unscheduled (the wave loop
        // only reaches here while `ready_stages` is non-empty). Flag it FIRST, before the
        // (possibly-deduped) emits below: `halt_reason` reads this in-process flag so
        // `rigger step` SURFACES the halt to the thin driver on EVERY tripping step - even a
        // resume whose keyed `BudgetExhausted` is a replay that appends nothing. The flag
        // also tells the phase boundary the tree is NOT final, so it DEFERS the deferred gate
        // - a resume (once the operator raises `defaults.budget`, since the count now binds
        // across steps) schedules the remaining work, and only the step that assembles the
        // full tree records the whole-tree verdict.
        self.budget_halted.store(true, Ordering::SeqCst);
        // Record the halt IDEMPOTENTLY (finding adv-budget-exhausted-dup-across-steps). The
        // cross-step spawn fold makes a resume deterministically re-reach the spent budget and
        // re-trip the breaker, so a NON-keyed emit would append a duplicate BudgetExhausted +
        // TaskAborted on each re-tripping step, double-reporting the ONE halt in the audit
        // trail. Keying both (like green/verified/reviewed) records the halt exactly once;
        // both route through the single `append_and_fold` authority, so nothing diverges.
        self.emit_keyed(
            BUDGET_EXHAUSTED_KEY,
            TYPE_BUDGET_EXHAUSTED,
            json!({
                "budget": self.cfg.workflow.defaults.budget,
                "spawns": self.spawns.load(Ordering::SeqCst),
            }),
        )?;
        self.emit_keyed(
            TASK_ABORTED_KEY,
            TYPE_TASK_ABORTED,
            json!({ "reason": "spawn budget exhausted" }),
        )
    }

    /// The run's live HALT reason when the spawn-budget breaker stopped this process with
    /// ready work unscheduled (Gap 13), or `None` when the run converged cleanly. Read from
    /// the IN-PROCESS `budget_halted` flag (set by [`trip_budget_breaker`](RunCtx::trip_budget_breaker)),
    /// NOT from the durable `BudgetExhausted` event: a halt is a condition of the CURRENT run
    /// process, so a resume with a raised `defaults.budget` - which admits the spawn and never
    /// trips the breaker - reports no halt, even though the earlier halt's `BudgetExhausted`
    /// still sits in the log. `rigger step` copies this onto its printed [`Step`](crate::spawn::Step)
    /// so the thin driver stops LOUDLY on a halt instead of reading `{"wave":[],"done":true}`
    /// as a clean completion.
    fn halt_reason(&self) -> Option<String> {
        if self.budget_halted() {
            Some(format!(
                "budget exhausted: {}/{} spawns",
                self.spawns.load(Ordering::SeqCst),
                self.cfg.workflow.defaults.budget,
            ))
        } else {
            None
        }
    }

    /// The coverage gate, routed through flagSpecDefect (§3.2, §4.4, §8): a remaining
    /// gap is a spec defect, so emit a SpecDefect event with the reason, then halt by
    /// returning the error (the conductor never silently deviates around a gap).
    fn check_coverage_or_flag(
        &self,
        stages: &BTreeMap<String, Stage>,
        criteria: &[String],
    ) -> Result<(), Error> {
        if let Some(reason) = coverage_gap(stages, criteria) {
            self.emit(TYPE_SPEC_DEFECT, json!({"reason": reason}))?;
            return Err(Error(reason));
        }
        Ok(())
    }

    fn run_wave(
        &self,
        stages: &BTreeMap<String, Stage>,
        ready: &[String],
        integrated: &mut HashSet<String>,
        terminal: &mut HashSet<String>,
    ) -> Result<(), Error> {
        // Safe-parallelism partitioning (§3.2, §8): when partitioning is requested
        // and a grounder can compute blast radii, split the ready stages into
        // batches that are DISJOINT by blast-radius and run the batches SEQUENTIALLY
        // (each batch still concurrent under the pool cap), so two stages whose blast
        // radii overlap never run at the same time and never share a worktree. With
        // no grounder or no partition request, the whole wave is one batch - the
        // historical single-wave behavior.
        let batches = self.partition_wave(stages, ready);
        let mut first_err = None;
        for batch in &batches {
            let results = self.run_batch(stages, batch);
            for (name, r) in results {
                terminal.insert(name.clone());
                match r {
                    Ok(true) => {
                        integrated.insert(name);
                    }
                    Ok(false) => {}
                    // A PARKED unit (the stepwise/replay driver hit an unrecorded
                    // frontier) is neither integrated nor failed: it left a
                    // SpawnRequested for the courier and unwound cleanly. Record no
                    // lesson and do not collapse the wave to an error - the run loop
                    // finds no newly-ready units and returns, so the step process ends
                    // once every in-flight spawn in the wave is parked. Flag the park so
                    // the phase boundary holds the deferred gate until a later step
                    // drains the frontier and the tree is fully assembled.
                    Err(e) if is_parked(&e) => {
                        self.parked.store(true, Ordering::SeqCst);
                    }
                    // A budget-refused review-tier spawn (lens/adversary/adjudicator) is
                    // NOT a stage failure either: [`reserve_spawn`] already set
                    // `budget_broke` before returning the refusal, so - exactly like the
                    // implementer's `Ok(false)` refusal - we unwind the unit cleanly here
                    // (no lesson, no first_err collapse) and let the run loop's mid-wave
                    // `budget_broke()` check trip the ONE breaker path, which records
                    // `BudgetExhausted` and halts. Without this branch a review-tier
                    // refusal would collapse the wave to a raw error that propagates out of
                    // `run` BEFORE the `budget_broke()` check, aborting with NO
                    // `BudgetExhausted` event and asymmetric with the implementer path -
                    // the exact defect the criterion 5 fold makes load-bearing once a
                    // resume starts with the budget already spent on a recorded implementer
                    // and then reaches its first review tier (findings
                    // budget-review-tier-no-exhausted,
                    // adv-confirm-review-tier-no-budgetexhausted,
                    // adv-budget-guard-cannot-assemble-reviewed-unit).
                    Err(e) if is_budget_refused(&e) => {}
                    // A degenerate-reviewer HALT (Gap 18) is an INFRASTRUCTURE fault, not a
                    // unit failure: the operator's reviewer agent/driver returned only
                    // empty results. Route it through its OWN arm (like the park/budget
                    // sentinels) - propagate the loud halt as the wave's error, but emit NO
                    // per-unit lesson: a lesson here would misattribute the operator's
                    // broken reviewer to the unit under review (finding adv-u2gap18-halt-
                    // lesson-misattribution). It charges no attempt (no UnitFailed/
                    // UnitEscalated - the halt writes nothing against the unit). Strip the
                    // recognition marker so the operator's halt message stays clean.
                    Err(e) if is_degenerate_reviewer(&e) => {
                        if first_err.is_none() {
                            first_err = Some(Error(e.0.replace(DEGENERATE_MARKER, "")));
                        }
                    }
                    Err(e) => {
                        // EVERY erroring stage leaves a record, not just the first
                        // (item 8): the wave collapses to a single returned error, so
                        // without this the run record could not explain the stages
                        // whose errors were dropped. Emit a lesson naming the stage
                        // and its error before the collapse, so the log accounts for
                        // each terminal stage. The error never propagated up
                        // mid-stage, so `emit_lesson` is best-effort and infallible.
                        self.emit_lesson(
                            None,
                            &name,
                            &format!("stage {name:?} failed in its wave: {}", e.0),
                        );
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Split a wave's ready stages into the batches that run sequentially (§3.2, §8).
    /// Partitioning applies when a grounder is present AND partitioning is requested
    /// (any ready stage sets `partition == "by-blast-radius"`, or `defaults.partition`
    /// does). Then each ready stage's blast-radius file set is computed by grounding
    /// its `coverage` (or name) and collecting the touched files, and the stages are
    /// partitioned disjoint by [`partition_by_blast_radius`]. Otherwise the whole wave
    /// is a single batch (the historical behavior).
    fn partition_wave(
        &self,
        stages: &BTreeMap<String, Stage>,
        ready: &[String],
    ) -> Vec<Vec<String>> {
        let grounder = match self.deps.grounder {
            Some(g) if self.partition_requested(stages, ready) => g,
            _ => return vec![ready.to_vec()],
        };
        let items: Vec<(String, Vec<String>)> = ready
            .iter()
            .map(|name| {
                let st = &stages[name];
                let query = if st.coverage.is_empty() {
                    name.as_str()
                } else {
                    st.coverage.as_str()
                };
                let mut files: Vec<String> = grounder
                    .ground(query, 8)
                    .into_iter()
                    .map(|r| r.file)
                    .collect();
                files.sort();
                files.dedup();
                (name.clone(), files)
            })
            .collect();
        partition_by_blast_radius(&items)
    }

    /// Whether by-blast-radius partitioning is requested for this wave (§3.2, §8): a
    /// ready stage sets `partition == "by-blast-radius"`, or `defaults.partition`
    /// does (the stage value, when set, otherwise the default).
    fn partition_requested(&self, stages: &BTreeMap<String, Stage>, ready: &[String]) -> bool {
        let by_blast = |p: &str| p.eq_ignore_ascii_case("by-blast-radius");
        ready.iter().any(|name| {
            let st = &stages[name];
            if !st.partition.is_empty() {
                by_blast(&st.partition)
            } else {
                by_blast(&self.cfg.workflow.defaults.partition)
            }
        })
    }

    /// Run one batch of stage names concurrently under the bounded fan-out pool
    /// (§6): chunks of at most MAX_CONCURRENCY, each chunk a scoped thread group.
    /// Every stage in the batch runs; never more than MAX_CONCURRENCY at once.
    fn run_batch(
        &self,
        stages: &BTreeMap<String, Stage>,
        batch: &[String],
    ) -> Vec<(String, Result<bool, Error>)> {
        let mut results: Vec<(String, Result<bool, Error>)> = Vec::with_capacity(batch.len());
        for chunk in batch.chunks(MAX_CONCURRENCY) {
            let chunk_results: Vec<(String, Result<bool, Error>)> = std::thread::scope(|s| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|name| {
                        let name = name.clone();
                        let st = stages[&name].clone();
                        s.spawn(move || {
                            let r = self.start_and_run_stage(&name, &st);
                            (name, r)
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            results.extend(chunk_results);
        }
        results
    }

    fn start_and_run_stage(&self, name: &str, st: &Stage) -> Result<bool, Error> {
        // UnitStarted carries the assigned agent, its dependencies, and the unit's
        // DETERMINISTIC branch, so the graph can project ASSIGNED_TO (unit->agent) and
        // BLOCKS (need->unit), and the ledger records the durable checkpoint branch
        // (resume-continuity) the unit's committed work persists on across runs.
        // UnitStarted is a once-per-unit checkpoint (replay-keyed on the unit id), so a
        // step re-running the conductor over recorded history does not re-append it -
        // the double-count that would otherwise deflate the metrics denominators on
        // every replay step (finding adv-replay-dup-lifecycle).
        // Stamp the requested model ALIAS (spec 05 line 52): UnitStarted is the spawn's
        // first recorded unit event and the alias is known at spawn time, so it names the
        // model asked for even before any result. The resolved id is not known yet - it
        // arrives on the spawn's later status events once the worker reports it.
        self.emit_keyed_meta(
            &format!("{name}/started"),
            ledger::TYPE_UNIT_STARTED,
            json!({
                "id": name,
                "unit": name,
                "spec_criterion": st.coverage,
                "criterion": st.coverage,
                "agent": st.agent,
                "needs": st.needs,
                "branch": unit_branch(name),
            }),
            // UnitStarted is a once-per-unit checkpoint, so it names the model the unit's
            // FIRST attempt asks for - rung 0 of any cascade. The per-attempt rungs a
            // remediating unit escalates through ride on the per-attempt green/verified
            // status events below (spec 10 unit 4).
            &[(META_MODEL_ALIAS, &self.agent_model(&st.agent, 0))],
        )?;
        self.run_stage(st)
    }

    fn run_stage(&self, st: &Stage) -> Result<bool, Error> {
        // Async manual-gate queue (§4.3): a stage whose effective autonomy is Manual
        // pauses - its gate is awaiting a human, so emit ManualReview and leave the
        // unit pending (Ok(false), NOT escalated). Independent units in the same wave
        // run concurrently and advance regardless. Only an explicit `autonomy: manual`
        // pauses; the AutoNotify default runs and integrates unattended.
        if self.stage_paused_for_review(st) {
            // The pause recurs every step while the unit awaits a human, so it is
            // replay-keyed on the unit id: the frontier reports one ManualReview, not a
            // fresh one each re-step (spec 04, criterion 4).
            self.emit_keyed(
                &format!("{}/manual-review", st.name),
                TYPE_MANUAL_REVIEW,
                json!({"id": st.name, "unit": st.name}),
            )?;
            // A paused unit is terminal-inserted but NOT integrated and does no work
            // this step, so the tree is not final: flag the pause so the phase boundary
            // holds the deferred gate rather than record it against a tree missing this
            // unit (findings rf-converged-ignores-budget-refusal /
            // adv-confirm-converged-nonpark-partial-tree).
            self.manual_review.store(true, Ordering::SeqCst);
            return Ok(false);
        }
        if is_fan_out(st) {
            return self.run_fan_out_stage(st);
        }
        // Resume-continuity: decide whether this unit CONTINUES from a prior window's
        // recorded phase (its deterministic branch carries committed work) or runs the
        // full lifecycle fresh. Computed before the worktree is created so a resumed
        // unit reuses its branch's work instead of re-implementing from scratch.
        let phase = self.resume_phase(st);
        let wt = self.stage_worktree(st)?;
        let dir = wt.as_ref().map(|w| w.dir.clone()).unwrap_or_default();
        let result = self.run_single_stage(st, wt.as_ref(), &dir, phase);
        if let Some(w) = &wt {
            let _ = w.remove();
            // The unit's branch is its DURABLE checkpoint (resume-continuity): it must
            // survive an interrupted unit so the next run reuses its committed work.
            // Delete it ONLY on a SUCCESSFUL integrate (Ok(true)), where the branch has
            // already merged into the base and the checkpoint has served its purpose -
            // the merged work lives in the base now. An un-integrated unit (Ok(false),
            // an Err, a pause/escalation, or a crash before this line) KEEPS its branch.
            // Deletion happens after the worktree dir is removed, since git refuses to
            // delete a branch that is still checked out in a worktree.
            if matches!(result, Ok(true)) {
                let _ = Worktree::delete_branch(&self.deps.repo, &w.branch);
            }
        }
        result
    }

    /// Whether this stage's gate is paused for human review (§4.3): its effective
    /// autonomy (the stage override, else `defaults.autonomy`) is Manual and it has a
    /// gate to pause on. `gate::decide` maps Manual to `Action::Pause`.
    fn stage_paused_for_review(&self, st: &Stage) -> bool {
        if st.gates.is_empty() {
            return false;
        }
        let raw = if st.autonomy.is_empty() {
            &self.cfg.workflow.defaults.autonomy
        } else {
            &st.autonomy
        };
        let probe = Gate {
            id: String::new(),
            run: String::new(),
            kind: gate::Kind::Core,
            autonomy: gate::Autonomy::parse(raw),
            history: Vec::new(),
        };
        gate::decide(&probe) == gate::Action::Pause
    }

    /// The review panel a unit reviews ITSELF with (§3.2): the stage's own `review`
    /// override when it sets one, otherwise the workflow-wide `defaults.review`.
    /// Declared once and inherited by every implementer unit, including the
    /// planner-proposed units that run through `run_single_stage`.
    fn effective_review_panel<'a>(&'a self, st: &'a Stage) -> &'a crate::config::ReviewPanel {
        if st.review.is_empty() {
            &self.cfg.workflow.defaults.review
        } else {
            &st.review
        }
    }

    /// The cwd-isolation invariant for EVERY spawned agent (the worktree-isolation
    /// fix): an agent NEVER runs in the live main-repo checkout. A spawn's `dir`
    /// becomes the agent's working directory (the cli driver's `current_dir`, the
    /// Agent SDK's `cwd`); an EMPTY `dir` makes the agent inherit the driver's own
    /// cwd, which - for `rigger workflow`, run from the repo root - IS the main
    /// checkout. With `bypassPermissions` on, the agent can then edit the live repo,
    /// so the implementer's edits (and any reviewer that writes via Bash/Edit) land
    /// in the checkout the gates and review never look at, and the unit can never
    /// converge. This guard makes that impossible BY CONSTRUCTION: whenever a repo is
    /// configured, every lifecycle spawn must carry a non-empty `dir` that is a
    /// worktree, never the repo toplevel itself. (A repo-less run has no checkout to
    /// corrupt, so an empty `dir` - the project cwd - is allowed there.)
    fn assert_isolated_cwd(&self, role: &str, agent_id: &str, dir: &str) -> Result<(), Error> {
        if self.deps.repo.is_empty() {
            // No repo => no main checkout to protect; the project cwd is the workspace.
            return Ok(());
        }
        if dir.is_empty() {
            return Err(Error(format!(
                "{role} {agent_id:?} would run in the main repo checkout (empty cwd \
                 inherits the driver's cwd = the repo root); a worktree dir is required"
            )));
        }
        if same_path(dir, &self.deps.repo) {
            return Err(Error(format!(
                "{role} {agent_id:?} would run directly in the main repo checkout \
                 ({dir:?}); a worktree dir distinct from the repo root is required"
            )));
        }
        Ok(())
    }

    /// Build a REVIEWER's spawn options. A reviewer writes no code to integrate, but
    /// it still has tools that touch the filesystem (every review agent has `Bash`;
    /// the sdet lens has `Edit`/`Write`), so it must NOT run in the live main checkout
    /// either - a `sed -i`, an `Edit`, a stray `git checkout` would corrupt the very
    /// repo the run integrates into. The faithful, read-only-intent cwd is the code
    /// the reviewer is judging: the UNIT'S worktree (where the implementer committed
    /// the diff under review). So a reviewer runs IN that worktree - it reads the
    /// unit's actual code, and any accidental write lands in the throwaway worktree,
    /// never the main repo. `assert_isolated_cwd` enforces the dir is a real worktree.
    // The reviewer coordinates (id, tier role, agent, worktree dir, attempt, parallelism,
    // stage) are each a distinct spawn input threaded straight into one SpawnOpts; the same
    // primitive-argument shape run_reviewer carries, allowed for the same reason.
    #[allow(clippy::too_many_arguments)]
    fn reviewer_spawn_opts(
        &self,
        id: &str,
        role: &str,
        agent_id: &str,
        dir: &str,
        attempt: u32,
        parallel: bool,
        st: &Stage,
    ) -> Result<SpawnOpts, Error> {
        self.assert_isolated_cwd(role, agent_id, dir)?;
        let agent_def = self.cfg.agents.get(agent_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown {role} {agent_id:?}",
                st.name
            ))
        })?;
        Ok(SpawnOpts {
            system_prompt: self.build_system_prompt(agent_def),
            dir: dir.to_string(),
            // A reviewer runs in the unit's existing worktree; it does not own a
            // fresh isolated worktree of its own (it must not get its writes merged).
            isolation: false,
            parallel,
            blast_radius: self.grounded_seed(st),
            // The reviewer's deterministic spawn id (its tier's role token + this
            // review's attempt), so a stepwise/replay driver answers or parks it the
            // same way it does the implementer.
            id: id.to_string(),
            unit: st.name.clone(),
            stage: st.name.clone(),
            // A reviewer keeps a FIXED tier - judgment is not laddered (spec 10 unit 4
            // exclusion) - so a scaffold reviewer declares no `model_ladder` and resolves
            // its single `model` regardless. The attempt is threaded through anyway so the
            // model the driver spawns and the alias the conductor stamps agree by
            // construction if a reviewer ever were given a ladder.
            attempt,
            run_id: self.run_id.clone(),
        })
    }

    /// Run the three-tier review of THIS unit's diff and return the outcome (whether
    /// it is approved, plus the adjudicator's verdict reasoning) (§3.2). The three
    /// tiers communicate THROUGH THE CONTEXT GRAPH - the system's actual cross-agent
    /// memory - not through the conductor hand-threading one agent's stdout into the
    /// next agent's prompt. TIER 1: the expert lenses review the diff in parallel and
    /// EMIT each finding as a ReviewFinding (the REVIEW_PROTOCOL); the projector folds
    /// each finding ABOUT the unit's files live. TIER 2: the adversary GROUNDS after
    /// the lenses, so `graph_context` already surfaces their findings, and it tries to
    /// prove them wrong, emitting its own findings the same way. TIER 3: the
    /// adjudicator grounds last, reads BOTH the lenses' and the adversary's findings
    /// from the graph, and renders the gating verdict - it approves ONLY on an
    /// explicit `approve` (fail-closed), blocking the merge otherwise no matter what
    /// the static gates said. So the three tiers inform each other via the graph
    /// (concurrent lenses see each other live via the side-car), never running
    /// mutually blind and never via spliced prompts. The lenses/adversary/adjudicator
    /// review the unit (they produce no code), so they DON'T own a fresh worktree -
    /// but they still run IN the unit's worktree (`dir`), reading the actual code
    /// under review, never in the live main checkout where a stray Bash/Edit would
    /// corrupt the repo (the worktree-isolation fix). After the adjudicator approves,
    /// the unit is marked `reviewed` and its evidence carries the verdict reason
    /// (item 4). An empty panel runs no review and approves trivially (the historical
    /// behavior).
    fn review_unit(&self, st: &Stage, dir: &str, attempt: u32) -> Result<ReviewOutcome, Error> {
        let panel = self.effective_review_panel(st);
        if panel.is_empty() {
            return Ok(ReviewOutcome::approved(String::new()));
        }
        let lenses = panel.lenses.clone();
        let adversary = panel.adversary.clone();
        let adjudicator = panel.adjudicator.clone();
        // TIER 1: the lenses emit their findings to the graph (REVIEW_PROTOCOL); the
        // projector folds them ABOUT the unit's files live.
        if !lenses.is_empty() {
            self.run_review_agents_concurrently(st, &lenses, dir, attempt)?;
        }
        // TIER 2: the adversary grounds AFTER the lenses, so `graph_context` surfaces
        // their findings; it tries to prove them wrong and emits its own findings.
        if !adversary.is_empty() {
            self.run_adversary(st, &adversary, dir, attempt)?;
        }
        if adjudicator.is_empty() {
            return Ok(ReviewOutcome::approved(String::new()));
        }
        // TIER 3: the adjudicator grounds last, reads the lenses' and adversary's
        // findings from the graph, and renders the gating verdict.
        let (approved, reason, adj_resolved) =
            self.run_adjudicator(st, &adjudicator, dir, attempt)?;
        if approved {
            // The adjudicator's verdict reason is folded into the unit's `reviewed`
            // evidence (item 4). Replay-keyed on unit + attempt: a repo-less unit that
            // reached `reviewed` but does not merge (on_pass: none) re-runs its review
            // over the recorded verdicts every step, so the status is appended once
            // (spec 04, criterion 4). It is the adjudicator SPAWN's unit event, so it
            // carries the adjudicator's requested alias and resolved id (spec 05 line 52).
            self.emit_keyed_meta(
                &format!("{}/reviewed#{attempt}", st.name),
                ledger::TYPE_UNIT_STATUS,
                json!({
                    "id": st.name,
                    "status": "reviewed",
                    "evidence": review_evidence(&reason),
                }),
                &[
                    (META_MODEL_ALIAS, &self.agent_model(&adjudicator, attempt)),
                    (META_MODEL_RESOLVED, &adj_resolved),
                ],
            )?;
            Ok(ReviewOutcome::approved(reason))
        } else {
            Ok(ReviewOutcome::rejected(reason))
        }
    }

    fn run_single_stage(
        &self,
        st: &Stage,
        wt: Option<&Worktree>,
        dir: &str,
        phase: ResumePhase,
    ) -> Result<bool, Error> {
        // Resume-continuity, Reviewed phase: the unit's review was APPROVED in a prior
        // window and its branch carries the committed, approved code - only the merge
        // was interrupted. Skip BOTH implement and review and integrate the existing
        // work directly. No implementer, no lenses/adversary/adjudicator spawn: the
        // adjudicator already approved, re-reviewing would re-litigate a settled
        // verdict and re-spend the budget. A `none`-on_pass unit (verified-but-never-
        // merged by design) still does not merge.
        if phase == ResumePhase::Reviewed {
            if !integrates(st) {
                return Ok(false);
            }
            // The prior window recorded `reviewed` - which is emitted ONLY on an
            // explicit adjudicator approve (`review_unit` / the fan-out review stage) -
            // so this resumed merge carries a real approval.
            let commit = self.integrate_and_emit(
                wt,
                &st.agent,
                &st.name,
                &st.gates,
                IntegrationApproval::approved(),
            )?;
            self.emit(
                ledger::TYPE_UNIT_INTEGRATED,
                json!({"id": st.name, "commit": commit}),
            )?;
            return Ok(true);
        }

        // Resume of a mid-remediation unit: seed the attempt counter from the prior
        // window's folded `UnitFailed attempts:N` so bounded remediation CONTINUES from
        // where it stopped instead of restarting at 0. A unit that failed twice across a
        // prior window resumes at 2, makes its 3rd (final) attempt this window (under the
        // default bound), and ESCALATES at the configured `max_retries` bound TOTAL - not
        // a fresh `max_retries` every window forever.
        // A unit with no prior failure (fresh, or never failed) starts at 0, unchanged.
        let mut attempts = self.prior_attempts.get(&st.name).copied().unwrap_or(0);
        // The last attempt's concrete failure, threaded into the NEXT attempt's
        // prompt (item 3 + 5 / spec 02). Empty on the first attempt, so that prompt
        // is unchanged.
        let mut prior = PriorFailure::default();
        // Resume-continuity, Implemented phase: the unit was implemented in a prior
        // window and its branch carries the committed code, but it was not yet
        // approved+merged. Skip the implementer spawn on the FIRST iteration and pick
        // the lifecycle up at gates + the three-tier review on the committed code.
        // Only the first iteration is skipped - if review then rejects, the retry
        // re-spawns the implementer normally to fix the rejected code.
        let mut skip_implement = phase == ResumePhase::Implemented;
        loop {
            // The implementer's requested model ALIAS for THIS attempt (spec 05 line 52),
            // stamped on every unit event this stage records for the implementer spawn.
            // Recomputed each iteration from the live `attempts` so a `model_ladder` agent
            // (spec 10 unit 4) escalates one rung per remediation attempt - the same rung
            // the driver spawns on for `attempts`. Empty for an agentless stage.
            let impl_alias = self.agent_model(&st.agent, attempts);
            let mut spawn_err: Option<String> = None;
            // The RESOLVED model id the implementer reported for THIS attempt, surfaced by
            // the replay driver from the worker's `--meta` report. Empty until the spawn's
            // result is consumed (and on the resume-skip path, which re-uses a prior
            // window's diff without a fresh spawn), so its metadata is then omitted.
            let mut resolved_model = String::new();
            if skip_implement {
                // Reuse the prior window's committed implementation: re-record the
                // `green` status (the ledger reflects the reused diff) without
                // re-spawning the implementer, then fall through to gates + review on
                // the existing code.
                let mut green = BTreeMap::new();
                green.insert(
                    "green".to_string(),
                    format!(
                        "resumed from prior window's branch {}",
                        unit_branch(&st.name)
                    ),
                );
                self.emit_keyed_meta(
                    &format!("{}/green#{attempts}", st.name),
                    ledger::TYPE_UNIT_STATUS,
                    json!({"id": st.name, "status": "green", "evidence": green}),
                    &[(META_MODEL_ALIAS, &impl_alias)],
                )?;
                // Subsequent iterations (after a review reject) re-implement normally.
                skip_implement = false;
            } else if !st.agent.is_empty() {
                let agent_def = self.cfg.agents.get(&st.agent).ok_or_else(|| {
                    Error(format!(
                        "stage {:?} references unknown agent {:?}",
                        st.name, st.agent
                    ))
                })?;
                // Budget breaker at spawn granularity (item 9): refuse this spawn if
                // the budget is spent. A refused implementer spawn stops the unit
                // (Ok(false), not escalated); the run loop records BudgetExhausted. The
                // reservation is keyed by the spawn's deterministic id so a REPLAY of an
                // already-recorded implementer (a resumed step) is admitted free.
                let implementer_id = spawn_id(&st.name, ROLE_IMPLEMENTER, attempts);
                if !self.reserve_spawn(&implementer_id) {
                    return Ok(false);
                }
                let prompt = self.build_prompt_with_failure(st, &prior);
                let emit = |t: &str, v: Value| self.emit_with_actor(&st.agent, t, v);
                // cwd-isolation invariant (the worktree-isolation fix): an implementer
                // that is SUPPOSED to be isolated must never run in the live main
                // checkout. When the agent declared isolation (the default) and a repo is
                // configured, it must have a worktree dir (distinct from the repo root);
                // an empty/repo-root dir there would let its edits and remediation fixes
                // land in the checkout the gates and review never inspect, so the unit
                // could never converge - the exact bug this guard closes. An agent that
                // DELIBERATELY opted out (`isolation: none`) runs in the project cwd by
                // design (§3.1, §6), so it is exempt. A guard failure is a spawn error
                // (remediate, do not abort) - the discipline a mid-spawn crash gets.
                let isolation_check = if self.agent_isolated(&st.agent) {
                    self.assert_isolated_cwd("implementer", &st.agent, dir)
                } else {
                    Ok(())
                };
                match isolation_check.and_then(|()| {
                    self.deps.driver.spawn(
                        agent_def,
                        &prompt,
                        &SpawnOpts {
                            system_prompt: self.build_system_prompt(agent_def),
                            dir: dir.to_string(),
                            isolation: wt.is_some(),
                            parallel: false,
                            blast_radius: self.grounded_seed(st),
                            id: implementer_id.clone(),
                            unit: st.name.clone(),
                            stage: st.name.clone(),
                            // The cascade rung the driver resolves for this attempt (spec 10
                            // unit 4) - the same `attempts` the alias stamp above uses.
                            attempt: attempts,
                            run_id: self.run_id.clone(),
                        },
                        &emit,
                    )
                }) {
                    Ok(result) => {
                        // The resolved model the worker reported via `--meta` (spec 05 line
                        // 52): captured here so the green status - and the verified status
                        // below, for the same spawn - both carry it. Empty on a live
                        // (non-replay) driver that does not learn the resolved id.
                        resolved_model = result.resolved_model;
                        // The green status records that the implementer produced a
                        // diff (item 4): the per-unit evidence names the agent that
                        // implemented it.
                        let mut green = BTreeMap::new();
                        green.insert("green".to_string(), format!("implemented by {}", st.agent));
                        // Replay-keyed on unit + attempt: a step replaying the recorded
                        // implementer result re-reaches this line every step, but the
                        // green status is appended once per attempt (spec 04, criterion 4).
                        // Stamped with the requested alias AND the resolved id the spawn ran
                        // as (spec 05 line 52).
                        self.emit_keyed_meta(
                            &format!("{}/green#{attempts}", st.name),
                            ledger::TYPE_UNIT_STATUS,
                            json!({"id": st.name, "status": "green", "evidence": green}),
                            &[
                                (META_MODEL_ALIAS, &impl_alias),
                                (META_MODEL_RESOLVED, &resolved_model),
                            ],
                        )?;
                    }
                    // A PARKED spawn (the stepwise/replay driver reached an unrecorded
                    // frontier) is NOT a failure: unwind this unit cleanly, with no
                    // UnitFailed and no remediation, so the step ends once every
                    // in-flight spawn is parked and a later step replays the result.
                    Err(e) if is_parked(&e) => return Err(e),
                    // A mid-spawn crash (usage limit, non-zero exit) is remediated,
                    // not propagated: it must not abort the whole run (§8).
                    Err(e) => spawn_err = Some(format!("agent {:?}: {}", st.agent, e.0)),
                }
            }

            // A fresh accumulator for THIS attempt's failure specifics (item 3 + 5).
            let mut next = PriorFailure::default();
            // The unit's own lifecycle (§3.2): implement -> the unit's gates -> the
            // three-tier review OF THIS UNIT -> integrate. The gates and the
            // adjudicator's verdict BOTH gate integration: a gate failure OR a reject
            // feeds back into this same loop (re-ground via build_prompt, re-implement
            // WITH THE FEEDBACK, re-review) and escalates after the retry bound. Review
            // runs only once the implementer's own gates are green, so the lenses never
            // review a red diff.
            if spawn_err.is_none() {
                // A `produces` (planner) stage emits a DAG, NOT code: its agent has
                // just proposed the units (UnitProposed events). It wrote no diff, so
                // there is nothing to gate, nothing for the three-tier review to read
                // (lenses/adversary/adjudicator would review an empty diff - pointless
                // and slow, and it stalled the live run before the implement units
                // could start), and nothing to merge. It reaches the DAG-terminal
                // `Integrated` TRUTHFULLY with the REVIEW_ONLY_NO_ARTIFACT marker (the
                // same no-code-artifact representation a standalone review stage uses),
                // so its dependents become ready - WITHOUT `review_unit` and WITHOUT a
                // code integrate. A non-producer stage keeps the full per-unit
                // lifecycle (implement -> gates -> three-tier review -> integrate)
                // below, unchanged.
                if is_producer(st) {
                    self.emit(
                        ledger::TYPE_UNIT_INTEGRATED,
                        json!({"id": st.name, "commit": REVIEW_ONLY_NO_ARTIFACT}),
                    )?;
                    return Ok(true);
                }
                // Commit the implementer's worktree BEFORE running the gates (§3.2),
                // so the gate measures EXACTLY the committed artifact that the
                // subsequent integrate merges - never a dirty worktree. A unit could
                // otherwise pass `cargo test` on uncommitted files (e.g. three new
                // tests the implementer wrote but never `git add`ed) while the
                // committed tree the adjudicator inspects is still short: a false
                // green that loops the unit forever on a reject it can never satisfy.
                // Committing here collapses gate-green to committed-green. The
                // worktree-less path (no `wt`, e.g. an `isolation: none` agent or a
                // repo-less run) has no commit step and is unchanged.
                if let Some(w) = wt {
                    w.commit(&format!("rigger: {} attempt {}", st.name, attempts + 1))?;
                }
                let gate_outcome = self.run_gates(st, dir, attempts)?;
                if gate_outcome.pass {
                    // The verified status carries the gate evidence (item 4): each
                    // gate that ran summarized for the ledger's per-unit evidence.
                    // Replay-keyed on unit + attempt so a re-step past this unit's
                    // recorded gates does not re-append it (spec 04, criterion 4). It is
                    // still an event of the implementer spawn, so it carries the same
                    // requested alias and resolved id as the green status (spec 05 line 52).
                    self.emit_keyed_meta(
                        &format!("{}/verified#{attempts}", st.name),
                        ledger::TYPE_UNIT_STATUS,
                        json!({
                            "id": st.name,
                            "status": "verified",
                            "evidence": verified_evidence(&st.gates),
                        }),
                        &[
                            (META_MODEL_ALIAS, &impl_alias),
                            (META_MODEL_RESOLVED, &resolved_model),
                        ],
                    )?;
                    let review = self.review_unit(st, dir, attempts)?;
                    if review.approved {
                        // on_pass governs integration (§3.2): empty or `merge` lands
                        // the work; any other value (e.g. `none`) runs the gates but
                        // never integrates - the verified, reviewed work stays
                        // un-merged.
                        if !integrates(st) {
                            return Ok(false);
                        }
                        // The gates passed AND the review explicitly approved: the only
                        // path that mints an `IntegrationApproval`, so the only path that
                        // can merge. The reject branch below has no approval to hand to
                        // `integrate_and_emit`, so it cannot land the unit's code.
                        //
                        // Gap 16 invariant (spec 06 unit 3): the verdict is folded and
                        // acted on HERE, and an approve returns before the `remediate`
                        // terminal check below ever runs. So an approval on a unit's
                        // FINAL permitted attempt integrates - `max_retries` gates only
                        // STARTING another attempt, it never overrides an approval. This
                        // ordering (verdict-fold before attempt-counter) is load-bearing;
                        // reversing it re-opens the bug where unit-2's approved-on-
                        // attempt-6 review was recorded as UnitFailed/UnitEscalated.
                        let commit = self.integrate_and_emit(
                            wt,
                            &st.agent,
                            &st.name,
                            &st.gates,
                            IntegrationApproval::approved(),
                        )?;
                        self.emit(
                            ledger::TYPE_UNIT_INTEGRATED,
                            json!({"id": st.name, "commit": commit}),
                        )?;
                        return Ok(true);
                    }
                    // A rejecting adjudicator is treated exactly like a gate failure:
                    // capture its reasoning for the next attempt's prompt (item 5) and
                    // fall through to remediation, do NOT integrate.
                    next.review_reason = review.reason;
                } else {
                    // Capture the failing gates' evidence for the next attempt's
                    // prompt (item 3 / spec 02).
                    next.gate_evidence = gate_outcome.evidence;
                }
            }

            let rem = safety::remediate(attempts, self.max_retries());
            attempts = rem.attempts;
            self.emit(
                ledger::TYPE_UNIT_FAILED,
                json!({"id": st.name, "attempts": attempts}),
            )?;
            if rem.decision == safety::Decision::Escalate {
                // The escalation lesson carries the CONCRETE final failure (spec 02):
                // the spawn crash, or the specific gate/review reason, never a generic
                // placeholder when one is available.
                let why = if let Some(e) = &spawn_err {
                    e.clone()
                } else if !next.is_empty() {
                    next.summary()
                } else {
                    "its gates or review would not pass".to_string()
                };
                self.emit_lesson(
                    wt,
                    &st.name,
                    &format!(
                        "unit {:?} escalated after {attempts} attempts; {why}",
                        st.name
                    ),
                );
                self.emit(ledger::TYPE_UNIT_ESCALATED, json!({"id": st.name}))?;
                return Ok(false);
            }
            // Carry this attempt's failure into the next iteration's prompt.
            prior = next;
            // otherwise loop and retry the stage (re-grounding + the prior-failure
            // block via build_prompt_with_failure)
        }
    }

    fn run_fan_out_stage(&self, st: &Stage) -> Result<bool, Error> {
        // A standalone review stage produces no code, so it has no UNIT worktree - but
        // its reviewers must STILL not run in the live main checkout (a stray Bash/Edit
        // would corrupt the repo the run integrates into). So we mint a throwaway
        // read-only worktree of the base HEAD and run every reviewer IN it: they see
        // the integrated code under review, and any accidental write lands in the
        // throwaway, never the main repo. A repo-less run has no checkout to protect,
        // so `dir` stays empty there (the project cwd, guarded by `assert_isolated_cwd`
        // which is a no-op when no repo is configured). The worktree is torn down on
        // every exit path here, including the early-return ones inside the loop.
        //
        // The review worktree derives from the stage AND the review attempt (spec 06);
        // seed the attempt from the prior log's folded remediation count exactly as
        // `run_fan_out_review_loop` does below, so both agree and a resumed step
        // recomputes the same deterministic worktree path.
        let attempt = self.prior_attempts.get(&st.name).copied().unwrap_or(0);
        let review_wt = self.review_only_worktree(st, attempt)?;
        let dir = review_wt
            .as_ref()
            .map(|w| w.dir.clone())
            .unwrap_or_default();
        let result = self.run_fan_out_review_loop(st, &dir);
        if let Some(w) = &review_wt {
            // A read-only review worktree carries no work to checkpoint, so the
            // transient dir AND its throwaway branch are both removed unconditionally.
            let _ = w.remove();
            let _ = Worktree::delete_branch(&self.deps.repo, &w.branch);
        }
        result
    }

    /// The standalone-review-stage loop, factored out of [`Self::run_fan_out_stage`]
    /// so the throwaway review worktree it runs in is torn down on EVERY exit path -
    /// the `?` early-returns here are caught by the wrapper, which always cleans up.
    /// `dir` is the read-only review worktree the reviewers run in (empty only on a
    /// repo-less run, where there is no main checkout to protect).
    fn run_fan_out_review_loop(&self, st: &Stage, dir: &str) -> Result<bool, Error> {
        // The fan-out lens set is `agents` when populated; a `strategy: fan-out`
        // stage that names a single `agent` (and no `agents`) runs that one agent as
        // its lone lens on the parallel path, so `strategy` is honored even without an
        // explicit lens list (§3.2).
        let lenses = fan_out_lenses(st);
        // Resume/replay-continuity, mirroring the per-unit path (`run_single_stage`):
        // seed the attempt counter from the prior log's folded `UnitFailed attempts:N`.
        // A rejected standalone-review stage is `Failed` - which is NOT terminal - so it
        // is re-seeded ready every step; without this seed each step would restart at
        // attempt 0, re-run the recorded rejecting reviews, and re-append a duplicate
        // UnitFailed (finding rf-fanout-replay-dup-unitfailed). Seeding makes a replay
        // step instead PARK at the next unrecorded review-attempt frontier without
        // re-emitting the recorded failure, and the escalation bound ACCUMULATES across
        // steps (the unit escalates at `max_retries` TOTAL, not per-window forever).
        let mut attempts = self.prior_attempts.get(&st.name).copied().unwrap_or(0);
        loop {
            // Three-tier review (§3.2), communicating THROUGH THE CONTEXT GRAPH (item
            // 1): the expert lenses review the diff in parallel and EMIT each finding
            // as a ReviewFinding the projector folds ABOUT the diff's files live (and
            // concurrent lenses see each other's via the side-car); THEN the adversary
            // GROUNDS after them, so `graph_context` surfaces their findings, and it
            // tries to prove them wrong (a higher bar than the lenses; it reviews the
            // reviews, it is not a parallel lens), emitting its own findings the same
            // way; THEN the neutral adjudicator grounds last, reads BOTH the lenses'
            // and the adversary's findings from the graph, and its verdict gates the
            // stage. So the three tiers inform each other via the graph, not via the
            // conductor splicing one agent's stdout into another's prompt.
            self.run_review_agents_concurrently(st, &lenses, dir, attempts)?;
            if !st.adversary.is_empty() {
                self.run_adversary(st, &st.adversary, dir, attempts)?;
            }
            // The neutral adjudicator's verdict gates the stage (§3.2), fail-closed:
            // it approves ONLY on an explicit `approve`, blocking integration
            // otherwise, no matter the static gates.
            let (approved, reason, adj_resolved) = if st.adjudicator.is_empty() {
                (true, String::new(), String::new())
            } else {
                self.run_adjudicator(st, &st.adjudicator, dir, attempts)?
            };

            let gates_pass = approved && self.run_gates(st, dir, attempts)?.pass;
            if gates_pass {
                // Gap 16 invariant (spec 06 unit 3): the adjudicator's verdict is folded
                // and acted on HERE - an approve (with green gates) integrates and returns
                // BEFORE the `remediate` terminal check below. So a standalone review
                // approved on its FINAL permitted attempt integrates; `max_retries` gates
                // only STARTING another attempt. This is the exact path that recorded
                // unit-2-the-adjudicator-persona's approved-on-attempt-6 review as
                // UnitFailed/UnitEscalated when the terminal check preceded the fold;
                // keep the verdict-fold ahead of the attempt-counter.
                //
                // on_pass governs integration (§3.2): any value other than empty /
                // `merge` runs the gates but does not mark the stage integrated.
                if !integrates(st) {
                    return Ok(false);
                }
                // A standalone review stage produces NO code artifact, so it must not
                // fabricate an integration (item 7). The terminal status is `reviewed`
                // and carries the adjudicator's verdict in its evidence; the unit then
                // reaches the DAG-terminal `Integrated` with an EXPLICIT "no artifact"
                // marker (REVIEW_ONLY_NO_ARTIFACT) instead of an empty commit hash
                // that reads as a dropped value. Dependents still see it as satisfied.
                //
                // `reviewed` is replay-keyed on unit + attempt (mirroring the per-unit
                // path's `verified`): it and `UnitIntegrated` are two separate appends,
                // so a crash between them - or a replay - re-reaches this line with the
                // reviewers/gates all replayed; keying `reviewed` skips the duplicate and
                // lets `UnitIntegrated` (terminal, never re-scheduled) land exactly once.
                self.emit_keyed_meta(
                    &format!("{}/reviewed#{attempts}", st.name),
                    ledger::TYPE_UNIT_STATUS,
                    json!({
                        "id": st.name,
                        "status": "reviewed",
                        "evidence": review_evidence(&reason),
                    }),
                    &[
                        (
                            META_MODEL_ALIAS,
                            &self.agent_model(&st.adjudicator, attempts),
                        ),
                        (META_MODEL_RESOLVED, &adj_resolved),
                    ],
                )?;
                self.emit(
                    ledger::TYPE_UNIT_INTEGRATED,
                    json!({"id": st.name, "commit": REVIEW_ONLY_NO_ARTIFACT}),
                )?;
                return Ok(true);
            }

            // Replay-keyed on the FAILING attempt so a replay that re-reaches this
            // recorded rejection (before the seeded frontier parks it) appends no
            // duplicate UnitFailed - the bound accumulates from the log, it is never
            // re-counted (finding rf-fanout-replay-dup-unitfailed).
            let failed_attempt = attempts;
            let rem = safety::remediate(attempts, self.max_retries());
            attempts = rem.attempts;
            self.emit_keyed(
                &format!("{}/failed#{failed_attempt}", st.name),
                ledger::TYPE_UNIT_FAILED,
                json!({"id": st.name, "attempts": attempts}),
            )?;
            if rem.decision == safety::Decision::Escalate {
                let why = if approved {
                    "its gates would not pass".to_string()
                } else if reason.trim().is_empty() {
                    "the adjudicator did not approve".to_string()
                } else {
                    format!("review rejected: {}", reason.trim())
                };
                self.emit_lesson(
                    None,
                    &st.name,
                    &format!(
                        "review stage {:?} escalated after {attempts} attempts; {why}",
                        st.name
                    ),
                );
                self.emit(ledger::TYPE_UNIT_ESCALATED, json!({"id": st.name}))?;
                return Ok(false);
            }
        }
    }

    /// Run the expert lenses (tier 1) concurrently. Each lens REVIEWS the diff and
    /// EMITS its findings to the shared context graph as ReviewFindings (the
    /// REVIEW_PROTOCOL), so the later tiers and concurrent lenses retrieve them via
    /// grounding + the side-car rather than via the conductor splicing one lens's
    /// stdout into another agent's prompt. The lenses produce no code to integrate, so
    /// they own NO worktree of their own and never integrate (item 6: a reviewing lens
    /// must not get its writes merged into the base repo) - they run IN the unit's
    /// worktree (`dir`), reading the code under review, never the live main checkout.
    fn run_review_agents_concurrently(
        &self,
        st: &Stage,
        agent_ids: &[String],
        dir: &str,
        attempt: u32,
    ) -> Result<(), Error> {
        // Bounded fan-out pool (§6): run the lenses in chunks of at most
        // MAX_CONCURRENCY, each chunk a scoped thread group. Every lens still runs;
        // never more than MAX_CONCURRENCY at once.
        for chunk in agent_ids.chunks(MAX_CONCURRENCY) {
            let chunk_results: Vec<Result<(), Error>> = std::thread::scope(|s| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|a| s.spawn(move || self.run_lens(st, a, dir, attempt)))
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for r in chunk_results {
                r?;
            }
        }
        Ok(())
    }

    /// Run a single review lens. A lens reviews - it writes no code - so it owns NO
    /// worktree and its output is never integrated (item 6); it runs IN the unit's
    /// worktree (`dir`) so it reads the actual code under review and any stray write
    /// (every lens has Bash; the sdet lens has Edit/Write) lands in the throwaway
    /// worktree, never the live main checkout. It is prompted with the grounded base
    /// prompt plus the REVIEW_PROTOCOL, so it EMITS each finding it raises to the
    /// shared context graph (the cross-agent memory), where the adversary, the
    /// adjudicator, and its fellow lenses retrieve it. Its stdout is no longer captured
    /// to thread into another agent's prompt - the graph is the channel. Budget-refused
    /// spawns (item 9) surface as an error so the run halts.
    fn run_lens(&self, st: &Stage, agent_id: &str, dir: &str, attempt: u32) -> Result<(), Error> {
        // A lens's output is not a verdict - it emits its findings to the graph - so the
        // substantive result is discarded here; the shared `run_reviewer` loop only needs
        // it to be non-degenerate (Gap 18) before the review proceeds.
        let prompt = self.build_review_prompt(st);
        self.run_reviewer(
            st,
            "lens",
            &lens_role(agent_id),
            agent_id,
            dir,
            attempt,
            true,
            // A lens's stdout is NOT its verdict - it emits findings to the graph - so an
            // empty stdout is degenerate only when it also emitted no ReviewFinding.
            false,
            &prompt,
        )?;
        Ok(())
    }

    /// Run a single reviewer spawn of any tier (lens/adversary/adjudicator) with the
    /// Gap-18 degenerate-result respawn loop, returning its GUARANTEED-non-degenerate
    /// result. This is the ONE authority for the check-and-respawn behavior across all
    /// three tiers - `run_lens`/`run_adversary` discard the returned result, while
    /// `run_adjudicator` reads its verdict.
    ///
    /// A DEGENERATE reviewer result is an INFRASTRUCTURE fault, not a verdict (spec 07):
    /// the conductor respawns the SAME reviewer under a deterministic `~retry{n}` id
    /// ([`spawn_retry_id`]) - a NEW spawn a stepwise/replay driver parks and answers
    /// independently - and only a NON-degenerate result is returned to fold into the review
    /// outcome. What COUNTS as degenerate is tier-specific (see
    /// [`reviewer_result_is_degenerate`](RunCtx::reviewer_result_is_degenerate)): the
    /// adjudicator's empty stdout is degenerate (its stdout is the verdict), while a
    /// lens/adversary is degenerate only when it emitted no ReviewFinding AND an empty
    /// stdout on the LIVE path (a replayed empty result is a valid graph-channel outcome).
    /// The loop is bounded at [`REVIEWER_RESPAWN_BOUND`] respawns; if every spawn (its
    /// original plus the respawns) is degenerate, it returns the [`degenerate_reviewer`]
    /// halt error - a [`DEGENERATE_MARKER`]-tagged error [`run_wave`](RunCtx::run_wave)
    /// routes through its dedicated no-lesson arm and propagates out of `run`, so a dead
    /// reviewer HALTS the run loudly rather than escalating the unit. The respawn loop
    /// lives INSIDE one review attempt: it never touches the unit's remediation counter, so
    /// a degenerate reviewer never charges the unit an attempt (spec 07 exclusion). The
    /// halt is RECOVERABLE on the replay driver: results are last-write-wins, so recording
    /// a substantive result for a retry id lets the next step replay it and fold normally
    /// (see [`degenerate_reviewer`]).
    ///
    /// `tier` is the human label the audit trail/`reviewer_spawn_opts` use; `role` is the
    /// deterministic-id role token (`lens_role(agent)` / [`ROLE_ADVERSARY`] /
    /// [`ROLE_ADJUDICATOR`]); `parallel` sets the reviewer's isolation-opt (§6, lenses run
    /// in parallel); `stdout_is_verdict` selects the tier-specific degeneracy signal (see
    /// [`reviewer_result_is_degenerate`](RunCtx::reviewer_result_is_degenerate)); `prompt`
    /// is the tier's already-grounded prompt. A budget-refused respawn surfaces the budget
    /// sentinel exactly like the original spawn.
    #[allow(clippy::too_many_arguments)]
    fn run_reviewer(
        &self,
        st: &Stage,
        tier: &str,
        role: &str,
        agent_id: &str,
        dir: &str,
        attempt: u32,
        parallel: bool,
        stdout_is_verdict: bool,
        prompt: &str,
    ) -> Result<AgentResult, Error> {
        let agent_def = self.cfg.agents.get(agent_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown {tier} {agent_id:?}",
                st.name
            ))
        })?;
        // retry 0 is the reviewer's ORIGINAL spawn (its plain `spawn_id`); each later
        // ordinal is a `~retry{n}` respawn. At most `1 + REVIEWER_RESPAWN_BOUND` spawns.
        for retry in 0..=REVIEWER_RESPAWN_BOUND {
            let id = spawn_retry_id(&st.name, role, attempt, retry);
            let opts = self.reviewer_spawn_opts(&id, tier, agent_id, dir, attempt, parallel, st)?;
            if !self.reserve_spawn(&id) {
                return Err(budget_refused(&st.name, tier, agent_id));
            }
            // Count the ReviewFindings this spawn emits to the graph - a lens/adversary's
            // REAL work channel (the REVIEW_PROTOCOL). A reviewer that emitted its findings
            // but self-reported an empty stdout DID its work and must not be misread as
            // degenerate (Gap 18, adv-u2gap18-empty-success-is-a-valid-outcome-misread-as-
            // degenerate). The callback fires only while the agent runs IN-PROCESS.
            let findings = std::cell::Cell::new(0u32);
            let emit = |t: &str, v: Value| {
                if t == contextgraph::TYPE_REVIEW_FINDING {
                    findings.set(findings.get() + 1);
                }
                self.emit_with_actor(agent_id, t, v)
            };
            let result = self
                .deps
                .driver
                .spawn(agent_def, prompt, &opts, &emit)
                .map_err(|e| Error(format!("stage {:?} {tier} {agent_id:?}: {}", st.name, e.0)))?;
            // A substantive result folds into the review; a degenerate one loops to respawn
            // the SAME reviewer under the next retry id.
            if !self.reviewer_result_is_degenerate(
                stdout_is_verdict,
                &id,
                &result,
                findings.get(),
            )? {
                return Ok(result);
            }
        }
        Err(degenerate_reviewer(&st.name, tier, agent_id, role, attempt))
    }

    /// Whether a reviewer spawn's `result` is DEGENERATE (Gap 18) - an infrastructure
    /// fault the conductor respawns/halts on rather than folding into the review. The
    /// signal DIFFERS by tier because each tier's WORK lands in a different channel:
    ///
    /// - The ADJUDICATOR's stdout IS its verdict (`stdout_is_verdict`), so an empty or
    ///   whitespace-only stdout is degenerate on EVERY path - including a recorded empty
    ///   result replayed on the stepwise path. That is the wedge Gap 18 must catch (and
    ///   let recover): `build_result` records an empty success with no non-empty check, so
    ///   an infra-broken adjudicator's empty result would otherwise fold as a silent
    ///   reject.
    /// - A LENS/ADVERSARY emits its findings to the GRAPH and its stdout is discarded, so
    ///   an empty stdout is the NORMAL outcome, never degeneracy by itself. It is
    ///   degenerate only when the conductor OBSERVED it produce nothing at all: zero
    ///   ReviewFindings from this spawn's emit callback AND an empty stdout. That
    ///   observation is possible only while the agent runs IN-PROCESS (the live drivers);
    ///   on the stepwise path the agent ran out-of-process and its findings, if any, are
    ///   already in the graph, so a REPLAYED result (one whose outcome is already recorded
    ///   in the log) is a VALID outcome, never degeneracy. This is why a healthy
    ///   lens/adversary that correctly emitted its findings and self-reported an empty
    ///   success is NOT misread as degenerate on the production replay driver
    ///   (adv-u2gap18-empty-success-is-a-valid-outcome-misread-as-degenerate), with no
    ///   fragile per-spawn actor attribution of the recorded findings.
    fn reviewer_result_is_degenerate(
        &self,
        stdout_is_verdict: bool,
        id: &str,
        result: &AgentResult,
        findings_emitted: u32,
    ) -> Result<bool, Error> {
        if !result.output.trim().is_empty() {
            return Ok(false);
        }
        if stdout_is_verdict {
            // The adjudicator's verdict IS its stdout: empty is degenerate on both the
            // live and the replay path.
            return Ok(true);
        }
        if findings_emitted > 0 {
            // A lens/adversary that emitted a ReviewFinding this spawn did its work.
            return Ok(false);
        }
        // Empty stdout, zero observed findings: degenerate only if the agent actually ran
        // in-process (live). A spawn whose result is ALREADY recorded was REPLAYED - the
        // agent ran out-of-process and its findings went to the graph, so its empty stdout
        // is a valid outcome, not degeneracy. The live drivers never record a spawn
        // result, so this read is `None` there and the in-process observation stands.
        let events = self
            .deps
            .store
            .read_stream(STREAM, 0, Direction::Forward)
            .map_err(|e| Error(e.to_string()))?;
        let replayed = spawn::result_of(&events, id)
            .map_err(|e| Error(e.to_string()))?
            .is_some();
        Ok(!replayed)
    }

    /// Run the adversary: a single agent that reviews the lenses' findings and the
    /// diff and tries to prove the lenses wrong (§3.2). It runs AFTER the lenses, so
    /// the lenses' ReviewFindings are already folded into the graph and its grounded
    /// prompt (via `graph_context`) surfaces them - it retrieves the lenses' findings
    /// through the graph, not from a hand-threaded block. It then EMITS its own
    /// findings (the REVIEW_PROTOCOL) so the adjudicator reads them the same way. Like
    /// the adjudicator it reviews - it produces no code to integrate, so it owns no
    /// worktree of its own; it runs IN the unit's worktree (`dir`), never the live
    /// main checkout - and unlike the adjudicator its output does NOT gate the stage;
    /// it informs the adjudicator's judgment via the graph.
    fn run_adversary(
        &self,
        st: &Stage,
        adv_id: &str,
        dir: &str,
        attempt: u32,
    ) -> Result<(), Error> {
        // Like a lens, the adversary emits its findings to the graph rather than
        // returning a verdict, so its substantive result is discarded; `run_reviewer`
        // only needs it non-degenerate (Gap 18) before the adjudicator grounds.
        let prompt = self.build_review_prompt(st);
        self.run_reviewer(
            st,
            "adversary",
            ROLE_ADVERSARY,
            adv_id,
            dir,
            attempt,
            false,
            // Like a lens, the adversary's stdout is discarded (findings go to the graph),
            // so an empty stdout is degenerate only when it emitted no ReviewFinding.
            false,
            &prompt,
        )?;
        Ok(())
    }

    /// Run the adjudicator and return whether it approves PLUS its raw output (the
    /// verdict reasoning). Its verdict gates the stage. The adjudicator grounds LAST,
    /// so the lenses' and the adversary's ReviewFindings are already in the graph and
    /// its grounded prompt (via `graph_context`) surfaces them - it weighs the prior
    /// tiers by retrieving their findings through the graph, not from a hand-threaded
    /// block. The reviewer produces no code to integrate. The returned output is the
    /// verdict reason: it is folded into the unit's `reviewed` evidence on approval
    /// (item 4) and into the next attempt's prompt on a reject (item 5).
    fn run_adjudicator(
        &self,
        st: &Stage,
        adj_id: &str,
        dir: &str,
        attempt: u32,
    ) -> Result<(bool, String, String), Error> {
        // Unlike the other tiers the adjudicator's result IS the verdict, so it is read
        // here (not discarded). `run_reviewer` guarantees it is non-degenerate before it
        // is judged (Gap 18): an empty/whitespace-only verdict is an infrastructure fault
        // that respawns or halts, never a silent reject.
        let prompt = self.build_prompt(st);
        let result = self.run_reviewer(
            st,
            "adjudicator",
            ROLE_ADJUDICATOR,
            adj_id,
            dir,
            attempt,
            false,
            // The adjudicator's stdout IS the gating verdict, so an empty/whitespace-only
            // stdout is degenerate on every path (including a replayed recorded result).
            true,
            &prompt,
        )?;
        // The resolved model the adjudicator ran as (spec 05 line 52) rides back with the
        // verdict so the `reviewed` status - this spawn's unit event - can carry it.
        Ok((
            verdict_approves(&result.output),
            result.output,
            result.resolved_model,
        ))
    }

    /// Run a stage's inline gates for its `attempt`, returning whether they all passed
    /// and the compact evidence of any failure. Each gate's verdict is REPLAY-KEYED on
    /// the `(unit, attempt, gate)` coordinate (spec 04, criterion 4): the first step to
    /// reach an unrecorded gate runs its command inline and records a `GateVerdict`;
    /// every later step re-reaching that same gate REPLAYS the recorded verdict without
    /// re-running the command (so a stepwise run that hits a cargo gate pays its
    /// duration once, not once per step) and appends no duplicate verdict. Distinct
    /// attempts are distinct gate runs - a re-implementation must re-gate - so only
    /// re-reaching the SAME attempt's gate is a replay.
    fn run_gates(&self, st: &Stage, dir: &str, attempt: u32) -> Result<GateOutcome, Error> {
        let mut outcome = GateOutcome {
            pass: true,
            evidence: Vec::new(),
        };
        // Per-unit build cache (Gap 19): a gate running INSIDE a unit's worktree builds into
        // a unit-keyed CARGO_TARGET_DIR that is the SIBLING of that worktree, so concurrent
        // units' divergent trees never poison one shared incremental cache - a compile error a
        // gate surfaces is then always this unit's own. Derived as the sibling of the ACTUAL
        // worktree `dir` the gate runs in (the single source it shares with the reclamation in
        // `Worktree::remove` / `sweep_terminal`, so the cache the gate builds is exactly the
        // one that is reclaimed): a `rigger-wt-<slug>` unit worktree -> `cargo-target-<slug>`.
        // A `rigger-review-*` review worktree or the worktree-less path (`dir` "", e.g. an
        // `isolation: none` agent or a repo-less run) has no per-unit tree to isolate, so
        // `unit_cache_sibling` returns None and the gate inherits the ambient/shared target.
        let target = crate::worktree::unit_cache_sibling(dir).unwrap_or_default();
        for gid in &st.gates {
            let gc = self
                .cfg
                .workflow
                .gates
                .get(gid)
                .cloned()
                .unwrap_or_default();
            let kind = gate::Kind::parse(&gc.kind);
            // Deferred gates are NOT run inline in the per-unit lifecycle (§4.3): they
            // are held until the run's phase boundary and run ONCE there (see
            // `run_deferred_gates`), so a unit integrates on its inline Core/Elevated
            // gates without paying the expensive deferred check per unit.
            if !kind.runs_inline() {
                continue;
            }
            let key = gate_verdict_key(&st.name, attempt, gid);
            // REPLAY a recorded verdict (spec 04, criterion 4): this gate already ran in
            // a prior step, so reuse its recorded pass/evidence and re-run NOTHING - not
            // the command, not the GateVerdict emit, not the ratchet. The recorded
            // outcome is authoritative, so the unit's verified/failed decision is
            // identical to the live run's.
            if let Some((pass, evidence)) = self.recorded_gate_verdict(&key) {
                if !pass {
                    outcome.pass = false;
                    outcome.evidence.push(format!("{gid}: {evidence}"));
                }
                continue;
            }
            let g = Gate {
                id: gid.clone(),
                run: gc.run,
                kind,
                autonomy: gate::Autonomy::Manual,
                history: Vec::new(),
            };
            let res = self.deps.gates.run(&g, dir, &target);
            // The compact gate evidence is threaded into the GateVerdict event
            // payload (item 3): a real run otherwise discarded it, so neither the
            // ledger nor the workflow driver ever saw WHY a gate passed or failed.
            // Replay-keyed (and cached) so a re-step replays this verdict instead of
            // re-running.
            self.emit_gate_verdict(&key, gid, res.pass, &res.evidence)?;
            let (promoted, demoted, autonomy) = self.record_gate(gid, kind, res.pass, &st.autonomy);
            if promoted {
                self.emit(
                    TYPE_GATE_PROMOTED,
                    json!({"gate": gid, "autonomy": autonomy.as_str()}),
                )?;
            } else if demoted {
                self.emit(
                    TYPE_GATE_DEMOTED,
                    json!({"gate": gid, "autonomy": autonomy.as_str()}),
                )?;
            }
            if !res.pass {
                outcome.pass = false;
                // Capture the failing gate's compact summary so the next attempt's
                // prompt names exactly which gate failed and why (item 3 / spec 02).
                outcome.evidence.push(format!("{gid}: {}", res.evidence));
            }
        }
        Ok(outcome)
    }

    /// Run the workflow's DEFERRED gates ONCE at the run's phase boundary (§4.3).
    ///
    /// Phase-boundary semantics: a deferred gate is held out of every unit's inline
    /// lifecycle (`run_gates` skips it via `Kind::runs_inline`) and run exactly once
    /// here, after the wave loop has converged - i.e. after every unit has integrated
    /// on its inline Core/Elevated gates. This trades per-unit cost for a single
    /// end-of-run check: an expensive, whole-run gate (a full integration test, a
    /// cross-cutting lint) pays once, not once per unit, and it sees the FULLY
    /// integrated tree rather than one unit's partial state.
    ///
    /// We collect the UNIQUE deferred gate ids referenced by ANY stage in the
    /// (possibly planner-extended) workflow, in deterministic stage/gate order, then
    /// run each once via the gate runner (in the base repo, dir ""). Each run emits a
    /// GateVerdict carrying its compact evidence (the same record an inline gate
    /// produces, so the ledger and graph see deferred verdicts too). A FAILING
    /// deferred gate is surfaced TRUTHFULLY: it emits a DeferredGateFailed event
    /// naming the gate and its evidence, which the ledger folds so the run is reported
    /// not-fully-done - a deferred failure never silently passes as success.
    ///
    /// `converged` gates the FIRST (recording) run of each deferred gate on the run
    /// having genuinely converged - every unit settled to a non-parked terminal state.
    /// Under stepwise replay an early step empties the wave loop with units still parked
    /// (the tree only partially assembled), and recording the verdict there would lock
    /// in a base/partial-tree result every later step replays (finding
    /// adv-deferred-replay-locks-partial-tree). A non-converged step therefore records
    /// nothing; the step that drains the last park runs the gate once against the fully
    /// integrated tree. A verdict ALREADY recorded is replayed regardless of
    /// `converged`, so re-stepping a completed run stays idempotent.
    fn run_deferred_gates(
        &self,
        stages: &BTreeMap<String, Stage>,
        converged: bool,
    ) -> Result<(), Error> {
        // Collect the unique deferred gate ids referenced across all stages, in a
        // deterministic order (stages iterate sorted by name; gates in declared
        // order), so the phase boundary is reproducible run to run.
        let mut seen: HashSet<String> = HashSet::new();
        let mut deferred: Vec<String> = Vec::new();
        for st in stages.values() {
            for gid in &st.gates {
                if seen.contains(gid) {
                    continue;
                }
                let gc = self
                    .cfg
                    .workflow
                    .gates
                    .get(gid)
                    .cloned()
                    .unwrap_or_default();
                if gate::Kind::parse(&gc.kind).runs_inline() {
                    continue;
                }
                seen.insert(gid.clone());
                deferred.push(gid.clone());
            }
        }
        for gid in &deferred {
            let key = deferred_gate_verdict_key(gid);
            // The gate's verdict: REPLAYED from the log when already recorded (spec 04,
            // criterion 4 - a deferred gate runs once per run, then every later re-step
            // replays its outcome with no re-run of the typically expensive whole-tree
            // command and no duplicate GateVerdict), otherwise run fresh - but ONLY once
            // the run has genuinely converged, so the command measures the fully
            // integrated tree rather than a parked frontier's partial/base tree.
            let (pass, evidence) = match self.recorded_gate_verdict(&key) {
                Some(v) => v,
                None => {
                    // Not yet recorded. On a non-converged step (a parked stepwise
                    // frontier, or a unit's dependents left unscheduled behind a
                    // parked/escalated dep) the tree is not fully assembled, so DEFER:
                    // record nothing and let the step that drains the last park run it.
                    if !converged {
                        continue;
                    }
                    let gc = self
                        .cfg
                        .workflow
                        .gates
                        .get(gid)
                        .cloned()
                        .unwrap_or_default();
                    let kind = gate::Kind::parse(&gc.kind);
                    let g = Gate {
                        id: gid.clone(),
                        run: gc.run,
                        kind,
                        autonomy: gate::Autonomy::Manual,
                        history: Vec::new(),
                    };
                    // The deferred gate runs in the base repo (the fully integrated tree).
                    // It measures the ONE integrated tree, so it keeps inheriting the
                    // shared build cache (empty target_dir) - a per-unit cache would be
                    // wrong here, and this is exactly the tree `rigger step`'s inline
                    // courier gates also build against (Gap 19).
                    let res = self.deps.gates.run(&g, "", "");
                    self.emit_gate_verdict(&key, gid, res.pass, &res.evidence)?;
                    // Feed the ratchet the same way an inline gate does, so a deferred
                    // gate's trust still moves on its history (its ceiling is Silent,
                    // like Core). Only the fresh run feeds it; a replay must not re-emit
                    // the (non-keyed) promote/demote events.
                    let (promoted, demoted, autonomy) = self.record_gate(gid, kind, res.pass, "");
                    if promoted {
                        self.emit(
                            TYPE_GATE_PROMOTED,
                            json!({"gate": gid, "autonomy": autonomy.as_str()}),
                        )?;
                    } else if demoted {
                        self.emit(
                            TYPE_GATE_DEMOTED,
                            json!({"gate": gid, "autonomy": autonomy.as_str()}),
                        )?;
                    }
                    if !res.pass {
                        // A lesson records WHY for the next run's grounding. It is
                        // advisory (unlike the DeferredGateFailed below, which gates
                        // done/fully_done), so it is emitted only on the fresh run.
                        self.emit_lesson(
                            None,
                            gid,
                            &format!(
                                "deferred gate {gid:?} failed at the phase boundary: {}",
                                res.evidence
                            ),
                        );
                    }
                    (res.pass, res.evidence)
                }
            };
            // Surface a failure truthfully AND idempotently, whether the verdict was run
            // fresh or replayed: a DeferredGateFailed naming the gate, folded by the
            // ledger so the run is reported not-fully-done. Keying it (a SEPARATE append
            // from the GateVerdict) heals a crash between the two appends - the step
            // after the crash replays the recorded verdict and re-emits the failure
            // exactly once - so a red deferred gate can never be reported as a finished
            // run (finding adv-deferred-failed-lost-on-crash).
            if !pass {
                self.emit_keyed(
                    &deferred_gate_failed_key(gid),
                    TYPE_DEFERRED_GATE_FAILED,
                    json!({"gate": gid, "evidence": evidence}),
                )?;
            }
        }
        Ok(())
    }

    /// Record a gate's run on the ratchet, seeding a newly-tracked gate's starting
    /// autonomy from the stage override (`stage_autonomy`, when non-empty) and
    /// otherwise from `defaults.autonomy` (§3.2, §4.3).
    fn record_gate(
        &self,
        gid: &str,
        kind: gate::Kind,
        pass: bool,
        stage_autonomy: &str,
    ) -> (bool, bool, gate::Autonomy) {
        let mut tracker = self.gate_tracker.lock().unwrap();
        let raw = if stage_autonomy.is_empty() {
            &self.cfg.workflow.defaults.autonomy
        } else {
            stage_autonomy
        };
        let autonomy = gate::Autonomy::parse(raw);
        // Carry the gate's real Kind onto the tracked gate so the ratchet honors
        // its ceiling: an Elevated gate can be promoted to AutoNotify but never
        // proposed for Silent (it always surfaces a notification a human can veto).
        let g = tracker.entry(gid.to_string()).or_insert_with(|| Gate {
            id: gid.to_string(),
            run: String::new(),
            kind,
            autonomy,
            history: Vec::new(),
        });
        g.history.push(gate::HistoryEntry { pass });
        let (new_a, demoted) = gate::auto_demote(g, pass);
        if demoted {
            g.autonomy = new_a;
            g.history.clear();
            return (false, true, g.autonomy);
        }
        if gate::propose_promotion(g) {
            // Promotion is PROPOSED, never auto-applied (§4.3): surface it but keep
            // the gate at its current autonomy until a human approves. The proposed
            // step is capped at the gate kind's ceiling.
            let proposed = gate::next_autonomy(g);
            g.history.clear();
            return (true, false, proposed);
        }
        (false, false, g.autonomy)
    }

    fn integrate_and_emit(
        &self,
        wt: Option<&Worktree>,
        agent_id: &str,
        unit_name: &str,
        gates: &[String],
        // The fail-closed merge guard: an `IntegrationApproval` exists ONLY when the
        // unit's review EXPLICITLY APPROVED it (and its gates passed). Taking it by
        // value here makes "approved" a precondition of the merge seam itself, not an
        // implicit property of the call site - a rejected or escalated unit can never
        // call this, because it has no approval to hand over.
        _approval: IntegrationApproval,
    ) -> Result<String, Error> {
        let wt = match wt {
            Some(w) => w,
            None => return Ok(String::new()),
        };
        // The unit's changed files span the commit-before-gates seam (§3.2): the
        // implementer's work is now committed, so a plain `git status` is clean -
        // we take the COMMITTED diff vs base unioned with any residual dirty files,
        // so the FILE_TOUCHED / GATED_BY edges and the reindex see the real artifact
        // set whether or not the unit was pre-committed.
        let files = wt.changed_since_base()?;
        if files.is_empty() {
            return Ok(String::new());
        }
        for f in &files {
            self.emit(
                contextgraph::TYPE_FILE_TOUCHED,
                json!({"path": f, "by": agent_id}),
            )?;
        }
        // GATED_BY (§7): record which gates govern each artifact this unit changed.
        // The changed-file list must be captured BEFORE integrate commits it (after
        // the commit the worktree is clean), so emit here while `files` is in scope.
        // Each (file, gate) GateVerdict carries the artifact, which the projector
        // folds into GATED_BY(artifact -> gate) - the edge a real run otherwise
        // never produced (Phase 2 carryover).
        for f in &files {
            for gid in gates {
                self.emit(
                    contextgraph::TYPE_GATE_VERDICT,
                    json!({"gate": gid, "pass": true, "artifact": f}),
                )?;
            }
        }
        let _lock = self.integrate_mu.lock().unwrap();
        let commit = wt.integrate(&format!("rigger: integrate {unit_name}"))?;
        if !commit.is_empty() {
            if let Some(g) = self.deps.grounder {
                g.reindex(&self.deps.repo, &files);
            }
        }
        Ok(commit)
    }

    /// Build the SYSTEM prompt the conductor threads into every spawn: the agent's
    /// PERSONA (its role - the markdown body of its definition) followed by the
    /// rigger-authored communication discipline ([`RIGGER_COMMUNICATION`]). This is
    /// the SINGLE persona-source path - every spawn site builds the system prompt
    /// here, so BOTH drivers (the cli `--system-prompt`, the workflow shim's
    /// `options.systemPrompt`) receive an identical persona + discipline and cannot
    /// diverge.
    fn build_system_prompt(&self, agent: &AgentDef) -> String {
        build_system_prompt(&agent.prompt)
    }

    /// The grounding QUERY for a stage. A normal unit grounds on its `coverage` (now
    /// the real criterion text), falling back to its name when it has none. A PLANNER
    /// (`produces`) stage must NOT ground on its `coverage` label (the `coverage:
    /// required` bug grep-matched the word "required" in LICENSE): it decomposes the
    /// WHOLE spec, so it grounds on the spec's acceptance criteria, joined into one
    /// query. With no criteria a producer falls back to its name. This is the single
    /// source of the query, so `grounded_seed` and `build_prompt_with_failure` ground
    /// the same way.
    fn ground_query(&self, st: &Stage) -> String {
        if is_producer(st) {
            if !self.deps.criteria.is_empty() {
                return self.deps.criteria.join(" ");
            }
            return st.name.clone();
        }
        if st.coverage.is_empty() {
            st.name.clone()
        } else {
            st.coverage.clone()
        }
    }

    /// The implementer agent id the conductor assigns each baseline unit: the fan-out
    /// implement template's `agent` (read from the ORIGINAL workflow, since the template
    /// is removed from the live `stages` once expanded). The planner is told this id so
    /// its refinements assign the same implementer. Empty when there is no template.
    fn implementer_agent(&self) -> String {
        fan_out_template_name(&self.cfg.workflow.stages)
            .and_then(|name| {
                self.cfg
                    .workflow
                    .stages
                    .get(&name)
                    .map(|st| st.agent.clone())
            })
            .unwrap_or_default()
    }

    /// The planner's refine protocol, with the run's acceptance criteria and the
    /// implementer agent id filled in. Empty for a non-producer stage.
    fn plan_protocol(&self, st: &Stage) -> String {
        if !is_producer(st) {
            return String::new();
        }
        let criteria = if self.deps.criteria.is_empty() {
            "(no enumerated acceptance criteria)".to_string()
        } else {
            self.deps
                .criteria
                .iter()
                .map(|c| format!("- {c}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "\n\n{}\n",
            PLAN_PROTOCOL
                .replace("{implementer}", &self.implementer_agent())
                .replace("{criteria}", &criteria)
        )
    }

    /// The stage's blast-radius: the distinct files the grounder surfaces for the
    /// stage's grounding query (its `coverage`/name, or the spec criteria for a
    /// planner), in ground order (§5.3). This is the same grounding `build_prompt` seeds
    /// the graph context from and `partition_wave` partitions by, so the blast-radius
    /// the side-car filters peer decisions against is exactly the files the agent was
    /// grounded on. Empty when no grounder is configured (best-effort but real, not
    /// always empty).
    fn grounded_seed(&self, st: &Stage) -> Vec<String> {
        let gr = match self.deps.grounder {
            Some(g) => g,
            None => return Vec::new(),
        };
        let query = self.ground_query(st);
        let mut seed: Vec<String> = Vec::new();
        for r in gr.ground(&query, 8) {
            if !seed.contains(&r.file) {
                seed.push(r.file);
            }
        }
        seed
    }

    fn build_prompt(&self, st: &Stage) -> String {
        self.build_prompt_with_failure(st, &PriorFailure::default())
    }

    /// Build a REVIEW agent's prompt: the grounded base prompt (which already
    /// surfaces, via `graph_context`, the decisions, lessons, AND findings other
    /// reviewers raised about the unit's files) plus the REVIEW_PROTOCOL telling this
    /// reviewer to emit each finding it raises as a ReviewFinding. This is how the
    /// three tiers communicate THROUGH the graph: a lens emits findings, the adversary
    /// and adjudicator (which ground after it) read them back from the graph, and a
    /// reviewer who emits its own findings feeds the next tier the same way.
    fn build_review_prompt(&self, st: &Stage) -> String {
        format!("{}{REVIEW_PROTOCOL}", self.build_prompt(st))
    }

    /// Build a stage's prompt, optionally prepending a first-class prior-failure
    /// block (spec 02 / item 3 + 5). On the first attempt `prior` is empty and the
    /// prompt is byte-identical to the historical `build_prompt`; on a retry the
    /// block names exactly the gates that failed (with their compact evidence) and
    /// the adjudicator's rejection reasoning, so the next attempt addresses the
    /// specific failure instead of a blind re-grounded restart.
    fn build_prompt_with_failure(&self, st: &Stage, prior: &PriorFailure) -> String {
        let mut b = String::new();
        b.push_str(&prior.block());
        let mut seed: Vec<String> = Vec::new();
        if let Some(gr) = self.deps.grounder {
            // Ground on the criterion (a unit) or the spec criteria (a planner), never
            // on the `coverage: required` label - see `ground_query`.
            let query = self.ground_query(st);
            let refs = gr.ground(&query, 8);
            if !refs.is_empty() {
                b.push_str("Relevant locations to read first:\n");
                for r in &refs {
                    b.push_str(&format!("- {}:{}  {}\n", r.file, r.line, r.text));
                    if !seed.contains(&r.file) {
                        seed.push(r.file.clone());
                    }
                }
                b.push('\n');
            }
        }
        b.push_str(&self.graph_context(&seed));
        b.push_str(EMIT_PROTOCOL);
        // A planner stage carries the refine protocol + the spec's acceptance criteria,
        // so it knows the baseline already exists and proposes only criterion-mapped
        // refinements (never re-decomposing, never scope creep).
        b.push_str(&self.plan_protocol(st));
        b
    }

    fn graph_context(&self, seed: &[String]) -> String {
        let graph = match self.deps.graph {
            Some(g) if !seed.is_empty() => g,
            _ => return String::new(),
        };
        let g = match graph.subgraph(seed, 2) {
            Ok(g) => g,
            Err(_) => return String::new(),
        };
        let mut b = String::new();
        // Every prompt slice is budgeted (Gap 15's principle, extended to all sections by
        // Gap 17): the decisions, lessons, and findings sections each render through ONE
        // shared budgeted-section writer (recent-N verbatim under a hard per-section byte
        // budget, the older remainder collapsed into a visible elision note that names the
        // count and the `rigger peers <file>` recovery). So no single section - not even
        // findings, the LARGER contributor on a hot file (measured ~95KiB ABOUT
        // conductor.rs, ~187KiB ABOUT main.rs, 4-8x the 24KiB decisions cap) - can blow the
        // prompt; the store keeps the full history, only the prompt slice narrows. The
        // findings subgraph is seeded on the unit's files and a ReviewFinding folds ABOUT
        // those files, so the same traversal that returns the GOVERNING decisions returns
        // the findings too: this is the graph path by which the adversary and adjudicator
        // (grounding AFTER the lenses) retrieve the lenses' findings, replacing the
        // conductor hand-threading one agent's stdout into another's prompt.
        write_capped_decisions(&mut b, &g, seed);
        write_capped_lessons(&mut b, &g, seed);
        write_capped_findings(&mut b, &g, seed);
        b
    }

    fn emit_lesson(&self, wt: Option<&Worktree>, unit_name: &str, summary: &str) {
        // The lesson is ABOUT the files the unit touched. The conductor commits the
        // worktree before gating (§3.2, FIX 2), so a plain `git status` is clean by
        // the time a unit escalates - we use `changed_since_base` (the committed diff
        // unioned with any residual dirty files) so the lesson still names the real
        // artifact, not an empty set.
        let about: Vec<String> = wt
            .and_then(|w| w.changed_since_base().ok())
            .unwrap_or_default();
        let uid = uuid::Uuid::new_v4().to_string();
        let id = format!("lesson-{unit_name}-{}", &uid[..8]);
        let _ = self.emit(
            contextgraph::TYPE_LESSON_LEARNED,
            json!({"id": id, "summary": summary, "about": about}),
        );
    }

    /// Whether the named agent runs in an isolated worktree (its `isolation` is
    /// not `none`). An unknown agent defaults to isolated, matching the prior
    /// repo-only behavior.
    fn agent_isolated(&self, agent_id: &str) -> bool {
        self.cfg
            .agents
            .get(agent_id)
            .map(|a| a.isolated())
            .unwrap_or(true)
    }

    /// Decide how a unit ENTERS its lifecycle on this run (resume-continuity).
    ///
    /// A unit whose deterministic branch carries committed work AND whose last
    /// recorded status proves how far it got CONTINUES from that phase instead of
    /// restarting from implement:
    /// - last status `reviewed` (the adjudicator approved, only the merge was
    ///   interrupted) -> [`ResumePhase::Reviewed`]: skip implement AND review, integrate.
    /// - last status `green`/`verified` (implemented + maybe gated, not yet approved)
    ///   -> [`ResumePhase::Implemented`]: skip the implementer spawn, re-run gates +
    ///   the three-tier review on the committed code.
    /// - anything below `green`, or no committed work on the branch, or a non-isolated
    ///   / repo-less run -> [`ResumePhase::Fresh`]: the full lifecycle, unchanged.
    ///
    /// Both the recorded status AND real committed work on the branch are required:
    /// the status alone could be stale (e.g. the prior worktree was lost), so we never
    /// skip implement unless the branch actually holds the code to build on.
    fn resume_phase(&self, st: &Stage) -> ResumePhase {
        // A repo-less or non-isolated unit has no branch to checkpoint on, so there is
        // never reusable prior work - it always runs fresh.
        if self.deps.repo.is_empty() || st.agent.is_empty() || !self.agent_isolated(&st.agent) {
            return ResumePhase::Fresh;
        }
        let prior = match self.prior_status.get(&st.name) {
            Some(s) => *s,
            None => return ResumePhase::Fresh,
        };
        // The unit's branch must actually carry committed work to build on; a recorded
        // status with an empty/missing branch (the prior worktree's commits were lost)
        // falls back to a fresh run rather than skipping a non-existent implementation.
        if !Worktree::branch_has_work(&self.deps.repo, &unit_branch(&st.name)) {
            return ResumePhase::Fresh;
        }
        match prior {
            // Approved last window; only the merge was interrupted -> integrate.
            ledger::Status::Reviewed => ResumePhase::Reviewed,
            // Implemented (and possibly gated) last window, not yet approved -> re-run
            // gates + review on the committed code, skip the implementer.
            ledger::Status::Green | ledger::Status::Verified => ResumePhase::Implemented,
            // Below green (pending/grounding/red), or a terminal status that should not
            // have reached the lifecycle: run fresh.
            _ => ResumePhase::Fresh,
        }
    }

    fn stage_worktree(&self, st: &Stage) -> Result<Option<Worktree>, Error> {
        if self.deps.repo.is_empty() || st.agent.is_empty() {
            return Ok(None);
        }
        // An agent declaring `isolation: none` runs in the current dir, no
        // worktree, even when a repo is set (§3.1, §6).
        if !self.agent_isolated(&st.agent) {
            return Ok(None);
        }
        // Both the BRANCH and the DIR are DETERMINISTIC, derived purely from the unit id
        // (Gap 12, spec 06): the branch is `rigger/u/<unit-slug>` and the dir is
        // `<scratch-root>/rigger-wt-<unit-slug>` (never the OS temp dir - Gap 14). The
        // same unit uses the same branch AND the same dir on every run, so:
        // - the git branch ref (which survives process death and worktree removal) is the
        //   unit's DURABLE checkpoint - a prior window's committed work is reused, not
        //   thrown away; and
        // - because the dir no longer carries a per-process UUID, a resume - or a step that
        //   SUPERSEDES a prior one that died - computes the SAME path, so `Worktree::create`
        //   ADOPTS that prior process's worktree with a direct path lookup instead of a
        //   porcelain parse. (This is sequential resume/supersede, not true concurrency:
        //   rigger drives unit-worktree creation single-threaded within one `rigger step`.)
        // `Worktree::create` handles a fresh branch (create off HEAD), an existing branch
        // with prior commits (check it out, reusing the work), adopt-or-prune of a stale
        // registration whose dir was deleted, and self-heal of a populated leftover dir at
        // the deterministic path (deregister/remove it, then re-add).
        let scratch = crate::worktree::scratch_root_from_env(
            &self.deps.repo,
            &self.cfg.workflow.defaults.workdir,
        );
        let dir = unit_worktree_dir(&scratch, &st.name);
        let wt = Worktree::create(&self.deps.repo, &dir, &unit_branch(&st.name))?;
        Ok(Some(wt))
    }

    /// A throwaway read-only worktree of the base HEAD for a STANDALONE review stage
    /// (one that integrates no code and so has no unit worktree). Its reviewers run IN
    /// it - reading the integrated code under review - so they never run in the live
    /// main checkout (where a stray Bash/Edit would corrupt the repo). Unlike the unit
    /// worktree, this carries NO durable checkpoint: it is created off a throwaway branch
    /// and removed (dir + branch) when the stage ends. A repo-less run has no checkout to
    /// protect, so it returns None (reviewers run in the project cwd, which is the
    /// workspace; `assert_isolated_cwd` is a no-op there).
    ///
    /// Both the dir and the throwaway branch derive DETERMINISTICALLY from the stage id
    /// AND the review `attempt` (Gap 12, spec 06) - no per-process UUID - so a resumed
    /// review step recomputes the same path and reclaims it instead of leaking a fresh
    /// worktree each process.
    fn review_only_worktree(&self, st: &Stage, attempt: u32) -> Result<Option<Worktree>, Error> {
        if self.deps.repo.is_empty() {
            return Ok(None);
        }
        let scratch = crate::worktree::scratch_root_from_env(
            &self.deps.repo,
            &self.cfg.workflow.defaults.workdir,
        );
        let dir = review_worktree_dir(&scratch, &st.name, attempt);
        // A throwaway branch (NOT the deterministic unit branch): this worktree is
        // read-only scaffolding, never a checkpoint, so its branch must not collide with -
        // or survive as - a unit's durable branch.
        let branch = review_branch(&st.name, attempt);
        // A review worktree must reflect the CURRENT base HEAD. Since it carries no durable
        // work, DISCARD any deterministic leftover (a crashed prior review step left the
        // branch+dir pinned at a now-STALE HEAD) before creating, so we recreate off the
        // current HEAD rather than ADOPT the stale checkout and review stale code
        // (adv-u4det-review-adopt-staleness). This is the opposite of the unit worktree,
        // whose durable branch is exactly what `create` reuses.
        Worktree::discard(&self.deps.repo, &dir, &branch)?;
        let wt = Worktree::create(&self.deps.repo, &dir, &branch)?;
        Ok(Some(wt))
    }

    fn harvest_proposed(
        &self,
        stages: &mut BTreeMap<String, Stage>,
        proposed: &mut HashSet<String>,
        integrated: &HashSet<String>,
        terminal: &HashSet<String>,
    ) -> Result<(), Error> {
        let events = self.deps.store.read_stream(STREAM, 0, Direction::Forward)?;
        // Run-scoped (Gap 11, completing spec-06 unit-1): only THIS run's proposals
        // fold. A prior run's UnitProposed must never resurrect as live work - its
        // terminal states are (correctly) scoped OUT of `terminal`, so an unscoped
        // harvest here re-parks ancient units at attempt #0. Observed live: the first
        // run under the scoped binary rose u-metrics-mod from a weeks-dead aborted
        // run, a second time, BECAUSE of the scoping it evaded.
        for e in crate::run::current_run(&events) {
            if e.type_ != TYPE_UNIT_PROPOSED {
                continue;
            }
            let u: UnitProposed = match serde_json::from_slice(&e.data) {
                Ok(u) => u,
                Err(_) => continue,
            };
            if u.id.is_empty() || proposed.contains(&u.id) {
                continue;
            }
            proposed.insert(u.id.clone());
            if stages.contains_key(&u.id) {
                continue;
            }
            // Anti-fragmentation (§8): in a spec-driven run, a proposed unit with no
            // spec criterion is scope creep - refuse it and record the event, never
            // silently add it to the DAG.
            if !self.deps.criteria.is_empty() && u.coverage.trim().is_empty() {
                self.emit(
                    TYPE_SCOPE_CREEP,
                    json!({"unit": u.id, "reason": "proposed unit has no spec_criterion"}),
                )?;
                continue;
            }
            // Supersede the deterministic baseline (the duplication fix): the planner,
            // told to REFINE, re-proposes one unit per criterion - exactly the baselines
            // the conductor already synthesized. Without this, a criterion would get TWO
            // units doing the same work (the baseline AND the planner's unit), colliding
            // at integrate and doubling the work. So a planner unit that cites a
            // criterion SUPERSEDES that criterion's baseline: we remove the baseline so
            // the planner's unit replaces it (one unit per criterion). The match is on
            // the criterion text the planner is given verbatim (PLAN_PROTOCOL), compared
            // with whitespace normalized on both sides. A baseline survives ONLY if NO
            // planner unit cites its criterion (the reliable fallback). Multiple planner
            // units for one criterion (a real split) all survive and the one baseline is
            // removed once - the next match finds no baseline left and simply adds.
            //
            // Ordering safety: a baseline that still needs the `plan` stage has not run
            // when planner units are harvested, so removing it before it runs is correct.
            // We GUARD on the baseline not having started or reached a terminal state
            // (not in `integrated`/`terminal`) so a baseline already underway or merged
            // in a prior wave/window is never yanked out from under its own work.
            if !u.coverage.trim().is_empty() {
                if let Some(baseline_id) = stages
                    .iter()
                    .find(|(name, st)| {
                        st.baseline
                            && !integrated.contains(*name)
                            && !terminal.contains(*name)
                            && normalize_ws(&st.coverage) == normalize_ws(&u.coverage)
                    })
                    .map(|(name, _)| name.clone())
                {
                    stages.remove(&baseline_id);
                }
            }
            stages.insert(
                u.id.clone(),
                Stage {
                    name: u.id,
                    agent: u.agent,
                    needs: u.needs,
                    coverage: u.coverage,
                    gates: u.gates,
                    ..Default::default()
                },
            );
        }
        Ok(())
    }
}

/// Normalize a criterion string for the supersede match (the duplication fix): trim,
/// then collapse every internal run of ASCII whitespace to a single space. The planner
/// is told to cite the criterion text VERBATIM (PLAN_PROTOCOL), so an exact match is
/// the contract; normalizing whitespace on both sides makes it robust to incidental
/// reflowing/indentation differences without loosening into fuzzy matching (a planner
/// that PARAPHRASES a criterion deliberately will not match, and is correctly treated
/// as a genuinely new sub-unit added on top of the surviving baseline).
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The evidence map folded into a unit's `verified` status (item 4): the gates that
/// governed the unit, under the `verified` key, so the ledger records WHAT verified
/// it. Empty when the unit ran no gates.
fn verified_evidence(gates: &[String]) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if !gates.is_empty() {
        m.insert(
            "verified".to_string(),
            format!("gates passed: {}", gates.join(", ")),
        );
    }
    m
}

/// The evidence map folded into a unit's `reviewed` status (item 4): the
/// adjudicator's verdict reason under the `review` key. An empty reason still
/// records that review approved, so the ledger's per-unit evidence is never empty
/// after a reviewed run.
fn review_evidence(reason: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let r = reason.trim();
    if r.is_empty() {
        m.insert("review".to_string(), "approved".to_string());
    } else {
        m.insert("review".to_string(), r.to_string());
    }
    m
}

/// An adjudicator's verdict gates the stage, FAIL-CLOSED: ONLY an explicit
/// `{"verdict":"approve"}` (the verdict field, case-insensitively "approve", on a
/// JSON line in the output) approves and lets integration proceed. Anything else -
/// no JSON, no `verdict` field, prose, `reject`, or any unrecognized value - does
/// NOT approve and routes the unit to remediation. A missing or unparseable verdict
/// is treated as a non-approval, never a silent pass.
fn verdict_approves(output: &str) -> bool {
    for line in output.lines().rev() {
        if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
            if let Some(verdict) = v.get("verdict").and_then(|x| x.as_str()) {
                return verdict.eq_ignore_ascii_case("approve");
            }
        }
    }
    false
}

const EMIT_PROTOCOL: &str = "Record each decision you make by calling the rigger_emit tool the moment you make it, with type \"DecisionMade\" and data:\n{\"id\":\"<short-id>\",\"summary\":\"<one line>\",\"governs\":[\"<file>\"],\"supersedes\":\"<prior-id-or-empty>\"}\nThis writes it to the shared event log live, so other agents see it immediately.";

/// The protocol a PLANNER (a `produces: dag`) stage follows. The conductor has ALREADY
/// created one deterministic baseline implement unit per acceptance criterion, so the
/// planner's job is to REFINE, not to decompose from scratch: split a criterion into
/// several units, or add a necessary sub-unit or dependency the baseline missed. Each
/// refinement is a `UnitProposed` carrying the spec criterion it serves; a proposed
/// unit that maps to NO criterion is scope creep and is refused. The `{criteria}`
/// placeholder is filled with the run's actual acceptance criteria, and
/// `{implementer}` with the implementer agent id the conductor assigned the baseline.
const PLAN_PROTOCOL: &str = "You are the planner. The conductor has ALREADY created one baseline implement unit per acceptance criterion below - the spec is decomposed by construction. Your job is to REFINE that baseline, not to re-decompose it:\n- If a criterion is too large for one unit, split it into several units (each still citing that same criterion).\n- If you discover a NECESSARY sub-unit or an ordering dependency the baseline missed, propose it.\nWhen your unit serves a criterion, your unit SUPERSEDES (replaces) that criterion's baseline - it does NOT run alongside it - so cite the criterion you serve EXACTLY. Copy the criterion text VERBATIM from the list below into the `criterion` field, character for character - do not paraphrase, summarize, or reword it. The conductor matches your unit to the baseline by that exact text; a paraphrase will NOT match and your unit will run as an EXTRA unit on top of the baseline (duplicated work). Several units citing the SAME verbatim criterion (a real split) all run and replace the one baseline.\nPropose each refinement the moment you decide it by calling the rigger_emit tool with type \"UnitProposed\" and data:\n{\"id\":\"<short-id>\",\"agent\":\"{implementer}\",\"criterion\":\"<the spec criterion it serves, copied verbatim>\",\"needs\":[\"<unit ids it depends on>\"]}\nNEVER propose a unit that maps to no acceptance criterion - that is scope creep and will be refused. Do not write code.\n\nThe acceptance criteria to refine against (copy one of these VERBATIM into each unit's `criterion`):\n{criteria}";

/// Rigger's communication discipline, appended to EVERY spawned agent's SYSTEM
/// prompt (after its persona) by [`RunCtx::build_system_prompt`], so every agent on
/// every driver path receives it. It REINFORCES the cadence the user-prompt
/// protocols (`EMIT_PROTOCOL` / `REVIEW_PROTOCOL`) carry the exact JSON shapes for:
/// emit decisions and findings the MOMENT you make them, check peers between
/// actions, and never silently contradict a peer - so concurrent agents stay
/// coordinated through the shared log instead of running blind. This is the single
/// persona-source path both drivers consume, so the cli and workflow agents receive
/// identical discipline.
const RIGGER_COMMUNICATION: &str = "\n\n# Rigger communication discipline (non-negotiable)\n\
You are one of several agents working concurrently against a shared event log. Other \
agents act on what you record, live - so your communication is part of the work, not \
an afterthought.\n\
- Record EVERY decision the MOMENT you make it by calling the `rigger_emit` tool with \
type `DecisionMade`. Emit immediately, one decision at a time - never batch them up \
to the end of your turn, because a concurrent agent must see your decision while it \
is still acting.\n\
- If you are a reviewer, record EVERY finding the moment you find it by calling \
`rigger_emit` with type `ReviewFinding`. Do not hold findings back to a final summary.\n\
- Between your actions, CHECK the `rigger_peers` tool (scoped to the files you are \
touching) to stay aware of what concurrent agents have already decided. Do this \
before you commit to a direction, not after.\n\
- NEVER silently contradict a peer's decision. If you must change a direction a peer \
already set, SUPERSEDE it explicitly: emit a new `DecisionMade` whose `supersedes` \
field cites the prior decision's id, and state why. Diverging silently splits the \
work; superseding explicitly keeps every agent on one story.\n\
The exact JSON shapes for these emits are given in your task instructions; this \
section governs the DISCIPLINE and cadence - follow it on every turn.";

/// Compose an agent's SYSTEM prompt: its `persona` (its role body) followed by the
/// rigger-authored [`RIGGER_COMMUNICATION`] discipline. An agent with no persona
/// (empty body) still receives the discipline, so every spawned agent gets it; the
/// persona, when present, leads so the role frames the discipline. This is the one
/// place persona and discipline are joined, keeping both driver paths identical.
fn build_system_prompt(persona: &str) -> String {
    format!("{persona}{RIGGER_COMMUNICATION}")
}

/// The protocol a REVIEW agent (a lens or the adversary) follows so its findings
/// reach the shared context graph - the cross-agent memory the three tiers
/// communicate THROUGH. A reviewer records each finding by calling rigger_emit the
/// moment it raises it; the projector folds it ABOUT the files it concerns, and the
/// later tiers (and concurrent lenses) retrieve it via grounding + rigger_peers,
/// never via the conductor splicing one agent's stdout into another's prompt.
const REVIEW_PROTOCOL: &str = "Record each review finding you raise by calling the rigger_emit tool the moment you raise it, with type \"ReviewFinding\" and data:\n{\"id\":\"<short-id>\",\"summary\":\"<one line>\",\"about\":[\"<file>\"]}\nThis writes it to the shared context graph live, so the adversary, the adjudicator, and your fellow reviewers see it immediately (via grounding and rigger_peers) and address or refute it.";

/// Gap-15 prompt budget: the most-recent governing decisions kept VERBATIM in a
/// prompt. Older ones collapse into a single visible elision note. The store keeps
/// the full history; only this prompt slice narrows.
const DECISIONS_VERBATIM_N: usize = 12;

/// Gap-15 prompt budget: a hard byte cap on the verbatim body of the
/// decisions-that-govern section. Verbatim rendering stops as soon as the next
/// entry would cross this budget (whichever of it and [`DECISIONS_VERBATIM_N`]
/// binds first), so a pile of chunky rejection verdicts can never blow the prompt.
const DECISIONS_BUDGET_BYTES: usize = 24 * 1024;

/// Gap-17 prompt budget: the most-recent lessons kept VERBATIM in a prompt. Lessons
/// are terse one-liners recorded once per escalation, so the count that keeps the
/// section actionable matches the decisions count.
const LESSONS_VERBATIM_N: usize = 12;

/// Gap-17 prompt budget: a hard byte cap on the verbatim body of the lessons section.
/// Lessons are the smallest of the three sections (short summaries, few of them), so
/// this is the tightest budget; the older remainder collapses into a visible note.
const LESSONS_BUDGET_BYTES: usize = 12 * 1024;

/// Gap-17 prompt budget: the most-recent findings kept VERBATIM in a prompt. Findings
/// are the PRIMARY cross-tier review channel (a later tier addresses or refutes them),
/// so a larger count survives verbatim than for decisions or lessons before the byte
/// budget takes over.
const FINDINGS_VERBATIM_N: usize = 24;

/// Gap-17 prompt budget: a hard byte cap on the verbatim body of the findings section.
/// Findings were measured 4-8x larger than decisions on a hot file (~95KiB about
/// conductor.rs, ~187KiB about main.rs, spec 07 line 11), so this is the LARGEST of the
/// three per-section budgets - still hard-bounded so an unbounded review history can
/// never blow the prompt, but generous enough to carry the findings a later tier weighs.
const FINDINGS_BUDGET_BYTES: usize = 48 * 1024;

/// The header for the decisions-that-govern injection - single-sourced so the
/// capped renderer and any test that asserts its presence agree byte-for-byte.
const DECISIONS_HEADER: &str =
    "Decisions that govern these files (do not contradict them; supersede explicitly if you must):";

/// Render ONE kind-filtered graph section (decisions, lessons, or findings) under the
/// shared Gap-17 prompt budget: the most-recent `verbatim_n` nodes render verbatim AND
/// stay under `budget_bytes` (whichever binds first), and the older remainder collapses
/// into ONE visible elision note naming the elided count and the `rigger peers <file>`
/// recovery command. This is the SINGLE budgeted-section writer behind all three
/// `graph_context` sections (the three thin wrappers below name each section's config),
/// so no section gets a divergent second capping mechanism and the earlier triplicated
/// node-section render loop (arch-u5) is collapsed into one. The store keeps the FULL
/// history - only this prompt slice narrows, and the trim is never silent.
///
/// `recency_rel` names the edge relation whose max source position dates each node so the
/// freshest survive the trim (ties break on id for a deterministic prompt): a decision is
/// dated by its own `GOVERNS` edge, a lesson or finding by its own `ABOUT` edge (all point
/// node -> file, so key on the `from` side). A superseded decision's `GOVERNS` edge is
/// invalidated (excluded from the subgraph), so it can reach a section only via the still
/// valid `SUPERSEDES` edge that carries its superseder's position; dating off the node's
/// OWN `recency_rel` edge alone denies it that inherited freshness, so a stale verdict
/// never crowds a current one out of the verbatim slice (sdet-u5 / arch-u5). `line` renders
/// one entry's body (the sections differ: findings name the raising reviewer, the rest do
/// not); `noun` names the unit in the elision note.
#[allow(clippy::too_many_arguments)]
fn write_capped_section(
    b: &mut String,
    g: &Graph,
    seed: &[String],
    kind: &str,
    recency_rel: &str,
    header: &str,
    noun: &str,
    verbatim_n: usize,
    budget_bytes: usize,
    line: impl Fn(&contextgraph::Node) -> String,
) {
    // Recency per node id: the max source position of its own `recency_rel` edges. Those
    // edges point node -> file, so key on the `from` (node) side only; a SUPERSEDES edge
    // (from = superseder) never dates the superseded node.
    let mut recency: BTreeMap<&str, u64> = BTreeMap::new();
    for e in &g.edges {
        if e.rel != recency_rel {
            continue;
        }
        let slot = recency.entry(e.from.as_str()).or_insert(0);
        *slot = (*slot).max(e.source);
    }
    // The nodes of this kind with a non-empty summary, most-recent first.
    let mut nodes: Vec<&contextgraph::Node> = g
        .nodes
        .iter()
        .filter(|n| n.kind == kind)
        .filter(|n| n.attrs.get("summary").is_some_and(|s| !s.is_empty()))
        .collect();
    nodes.sort_by(|a, c| {
        let ra = recency.get(a.id.as_str()).copied().unwrap_or(0);
        let rc = recency.get(c.id.as_str()).copied().unwrap_or(0);
        rc.cmp(&ra).then_with(|| a.id.cmp(&c.id))
    });
    if nodes.is_empty() {
        return;
    }

    b.push_str(header);
    b.push('\n');

    let mut used = 0usize;
    let mut kept = 0usize;
    for n in &nodes {
        let entry = line(n);
        if kept >= verbatim_n || used + entry.len() > budget_bytes {
            break;
        }
        used += entry.len();
        b.push_str(&entry);
        kept += 1;
    }

    let elided = nodes.len() - kept;
    if elided > 0 {
        let files = if seed.is_empty() {
            String::new()
        } else {
            format!(" {}", seed.join(" "))
        };
        b.push_str(&format!(
            "- (+{elided} older {noun}(s) elided to keep this prompt under budget - recover the full set with `rigger peers{files}`)\n",
        ));
    }
    b.push('\n');
}

/// Render one node's `- {id}: {summary}` line - the plain body shared by the decisions
/// and lessons sections.
fn plain_node_line(n: &contextgraph::Node) -> String {
    let summary = n.attrs.get("summary").map(String::as_str).unwrap_or("");
    format!("- {}: {}\n", n.id, summary)
}

/// Render the "Decisions that govern these files" section (Gap 15, spec 06 unit 5) via
/// the shared [`write_capped_section`] writer. Preserved as the decisions entry point so
/// the Gap-15 cap tests bind the exact rendered behavior unchanged; it only names the
/// decision-specific config (GOVERNS recency, the decisions header and budgets).
fn write_capped_decisions(b: &mut String, g: &Graph, seed: &[String]) {
    write_capped_section(
        b,
        g,
        seed,
        contextgraph::KIND_DECISION,
        contextgraph::REL_GOVERNS,
        DECISIONS_HEADER,
        "decision",
        DECISIONS_VERBATIM_N,
        DECISIONS_BUDGET_BYTES,
        plain_node_line,
    );
}

/// Render the "Lessons already learned" section (Gap 17) via the shared
/// [`write_capped_section`] writer. Lessons fold ABOUT the files they concern, so their
/// recency is dated off the `ABOUT` edge.
fn write_capped_lessons(b: &mut String, g: &Graph, seed: &[String]) {
    write_capped_section(
        b,
        g,
        seed,
        contextgraph::KIND_LESSON,
        contextgraph::REL_ABOUT,
        "Lessons already learned about these files (do not repeat these mistakes):",
        "lesson",
        LESSONS_VERBATIM_N,
        LESSONS_BUDGET_BYTES,
        plain_node_line,
    );
}

/// Render the "Findings other reviewers have already raised" section (Gap 17) via the
/// shared [`write_capped_section`] writer: the graph path by which a later review agent
/// retrieves the findings the lenses already emitted (each line names the raising reviewer
/// `by` so the agent can address or refute it). Findings fold ABOUT the files they
/// concern, so their recency is dated off the `ABOUT` edge; they run 4-8x larger than
/// decisions, so they carry the largest per-section byte budget ([`FINDINGS_BUDGET_BYTES`]).
fn write_capped_findings(b: &mut String, g: &Graph, seed: &[String]) {
    write_capped_section(
        b,
        g,
        seed,
        contextgraph::KIND_FINDING,
        contextgraph::REL_ABOUT,
        "Findings other reviewers have already raised about these files (address or refute them):",
        "finding",
        FINDINGS_VERBATIM_N,
        FINDINGS_BUDGET_BYTES,
        |n| {
            let by = n.attrs.get("by").map(String::as_str).unwrap_or("");
            if by.is_empty() {
                // No raising reviewer recorded: fall back to the shared plain body so the
                // `- {id}: {summary}` line lives in exactly one place (arch-u1gap17).
                plain_node_line(n)
            } else {
                let summary = n.attrs.get("summary").map(String::as_str).unwrap_or("");
                format!("- {by} ({}): {summary}\n", n.id)
            }
        },
    );
}

/// The DETERMINISTIC branch a unit's worktree uses across runs (resume-continuity):
/// `rigger/u/<unit-id>`. Derived purely from the unit id, so the SAME unit reuses the
/// SAME branch on every run - the git ref persists the unit's committed work across
/// process death and worktree removal, making the branch the unit's durable
/// checkpoint. The id is sanitized to the bytes git accepts in a ref component, so an
/// id with spaces or other ref-illegal characters still yields a valid, stable branch.
fn unit_branch(unit_id: &str) -> String {
    format!("rigger/u/{}", sanitize_for_path(unit_id))
}

/// The DETERMINISTIC worktree dir for a unit (Gap 12, spec 06): `<scratch-root>/
/// rigger-wt-<unit-slug>`, with NO per-process UUID. Because it derives purely from the
/// scratch root and the (sanitized) unit id, a resume - or a step that supersedes a prior
/// one that died - computes the SAME path, so adoption of that prior process's worktree is
/// a direct path lookup rather than a `git worktree list` porcelain parse. (Sequential
/// resume/supersede, not true concurrency: rigger drives unit-worktree creation
/// single-threaded within one `rigger step`.) Pairs with [`unit_branch`], which derives the
/// unit's durable branch from the same id.
fn unit_worktree_dir(scratch_root: &str, unit_id: &str) -> String {
    format!(
        "{scratch_root}/{}{}",
        crate::worktree::UNIT_WORKTREE_PREFIX,
        sanitize_for_path(unit_id)
    )
}

/// The DETERMINISTIC dir for a STANDALONE review stage's throwaway worktree (spec 06):
/// `<scratch-root>/rigger-review-<stage-slug>-<attempt>`, derived from the stage id and
/// the review attempt, NO per-process UUID. A resumed review step recomputes the same path
/// and RECLAIMS it - [`Self::review_only_worktree`] discards any leftover and recreates off
/// the current HEAD (never adopting a stale checkout) - instead of leaking a fresh worktree
/// each process. Unlike the unit worktree this carries no durable checkpoint;
/// [`Self::run_fan_out_stage`] removes both the dir and its [`review_branch`] when the stage
/// ends.
fn review_worktree_dir(scratch_root: &str, stage_id: &str, attempt: u32) -> String {
    format!(
        "{scratch_root}/rigger-review-{}-{attempt}",
        sanitize_for_path(stage_id)
    )
}

/// The DETERMINISTIC throwaway branch for a standalone review worktree (spec 06):
/// `rigger/review/<stage-slug>-<attempt>`. It is NOT a unit's durable `rigger/u/*`
/// checkpoint - it is read-only review scaffolding, removed with its worktree when the
/// stage ends - so it must never collide with, or survive as, a unit branch. Deriving it
/// from stage + attempt (no uuid) lets a resumed step recompute and reclaim it.
fn review_branch(stage_id: &str, attempt: u32) -> String {
    format!("rigger/review/{}-{attempt}", sanitize_for_path(stage_id))
}

/// Whether two filesystem paths name the same location. Used by the cwd-isolation
/// guard to refuse spawning an agent directly in the main repo checkout. Compares
/// the canonicalized paths when both resolve (so `.`, `..`, symlinks, and a trailing
/// slash do not let an alias of the repo root slip past); falls back to a trimmed
/// byte comparison when a path does not yet exist on disk (canonicalize would error).
fn same_path(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a.trim_end_matches('/') == b.trim_end_matches('/'),
    }
}

/// Map an arbitrary unit id to a stable token safe for a git ref component and a
/// filesystem path: ASCII alphanumerics kept, every other run of characters collapsed
/// to a single hyphen, leading/trailing hyphens trimmed. Deterministic - the same id
/// always yields the same token - so the derived branch and dir are stable run to run.
/// An id that sanitizes to empty falls back to a fixed token so the ref stays valid.
fn sanitize_for_path(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unit".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Whether a stage integrates its work when its gates pass (§3.2). `on_pass` is
/// empty (the default - integrate) or `merge`; any other value (e.g. `none`) runs
/// the gates but lands nothing.
fn integrates(st: &Stage) -> bool {
    st.on_pass.is_empty() || st.on_pass.eq_ignore_ascii_case("merge")
}

/// Greedily group stage names into disjoint batches by blast-radius (§3.2, §8).
/// `items` pairs each stage name with the set of files in its blast radius. A stage
/// joins the FIRST existing batch none of whose members share any file with it;
/// otherwise it opens a new batch. Stages with an empty blast radius conflict with
/// nothing and so all collapse into the first batch. The result is deterministic:
/// `items` is consumed in order and batches keep insertion order, so callers get a
/// stable partition for a stable (e.g. sorted) input. The guarantee: two stages
/// whose blast radii overlap never land in the same batch, so running the batches
/// sequentially keeps overlapping units off the same file at the same time - they
/// never share a worktree.
pub fn partition_by_blast_radius(items: &[(String, Vec<String>)]) -> Vec<Vec<String>> {
    let mut batches: Vec<Vec<String>> = Vec::new();
    // The accumulated file set of each batch, parallel to `batches`, so the
    // disjointness test is a set lookup rather than a re-scan of every member.
    let mut batch_files: Vec<HashSet<&str>> = Vec::new();
    for (name, files) in items {
        let want: HashSet<&str> = files.iter().map(|f| f.as_str()).collect();
        let mut placed = false;
        for (i, taken) in batch_files.iter_mut().enumerate() {
            if want.is_disjoint(taken) {
                batches[i].push(name.clone());
                taken.extend(want.iter().copied());
                placed = true;
                break;
            }
        }
        if !placed {
            batches.push(vec![name.clone()]);
            batch_files.push(want);
        }
    }
    batches
}

/// Whether a stage runs the fan-out (parallel-lens) path rather than the
/// single-worker path (§3.2). A stage takes the standalone fan-out path ONLY when it
/// is a standalone review stage: it carries an `agents` lens list (or `strategy:
/// fan-out`) and has NO `agent`. A stage that names an `agent` runs the per-unit
/// lifecycle in `run_single_stage` - implement -> the unit's gates -> the three-tier
/// review OF THIS UNIT -> integrate - even when it sets `strategy: fan-out` (which on
/// an implementer stage means "one implementer per ready unit", driven by the
/// partitioner and the planner-proposed units, not "run my lone agent as a lens").
/// So review and integration live INSIDE the unit's lifecycle, never as a separate
/// downstream stage.
fn is_fan_out(st: &Stage) -> bool {
    st.agent.is_empty() && (!st.agents.is_empty() || st.strategy.eq_ignore_ascii_case("fan-out"))
}

/// The implement TEMPLATE stage the conductor expands into one per-criterion unit
/// (the deterministic decomposition baseline). It is the stage that names an `agent`,
/// sets `strategy: fan-out` ("one implementer per ready unit"), and does NOT
/// `produces` a DAG (it is a worker, not the planner). There is normally exactly one;
/// the FIRST in stable (BTreeMap) order is chosen. Returns its name, or None when the
/// workflow has no fan-out implementer template (a non-decomposing workflow), in which
/// case the conductor synthesizes no baseline units and the no-spec path is unchanged.
fn fan_out_template_name(stages: &BTreeMap<String, Stage>) -> Option<String> {
    stages
        .iter()
        .find(|(_, st)| {
            !st.agent.is_empty()
                && st.strategy.eq_ignore_ascii_case("fan-out")
                && st.produces.is_empty()
        })
        .map(|(name, _)| name.clone())
}

/// Whether a stage `produces` a DAG at runtime (the planner that decomposes the spec).
fn is_producer(st: &Stage) -> bool {
    !st.produces.is_empty()
}

/// The name of the (first) `produces` planner stage, if any: baseline units depend on
/// it so they run only AFTER the planner has had its chance to refine the DAG.
fn producer_name(stages: &BTreeMap<String, Stage>) -> Option<String> {
    stages
        .iter()
        .find(|(_, st)| is_producer(st))
        .map(|(name, _)| name.clone())
}

/// A stable, unique, human-legible unit id derived from a criterion's text plus its
/// ordinal: a lowercased, hyphen-joined slug of the first words, prefixed `unit-<n>-`
/// so the id is deterministic, collision-free across criteria, and references the
/// criterion it serves. The ordinal alone guarantees uniqueness even when two criteria
/// slug identically; the slug makes the id readable in the event log.
fn unit_slug(n: usize, criterion: &str) -> String {
    let mut slug = String::new();
    for ch in criterion.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.trim_matches('-').len() >= 32 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("unit-{n}")
    } else {
        format!("unit-{n}-{slug}")
    }
}

/// The deterministic decomposition BASELINE (§3.2): given a fan-out implement
/// `template` stage and the spec's acceptance `criteria`, synthesize ONE implement
/// unit per criterion. Each unit inherits the template's executable shape - its
/// `agent`, `gates`, `on_pass`, and `partition` - but carries THE CRITERION TEXT as
/// its `coverage`, so it grounds on the real criterion (not the template's label) and
/// its `UnitStarted` records the real `spec_criterion`. Each unit `needs` the planner
/// (`producer`) when one exists, so the baseline runs only after the planner refines.
/// The template itself is NOT run as a unit - these per-criterion units replace it.
fn baseline_units(
    template: &Stage,
    criteria: &[String],
    producer: Option<&str>,
) -> Vec<(String, Stage)> {
    let mut needs = template.needs.clone();
    if let Some(p) = producer {
        if !needs.iter().any(|n| n == p) {
            needs.push(p.to_string());
        }
    }
    let mut units = Vec::with_capacity(criteria.len());
    for (i, criterion) in criteria.iter().enumerate() {
        let name = unit_slug(i + 1, criterion);
        units.push((
            name.clone(),
            Stage {
                name,
                agent: template.agent.clone(),
                gates: template.gates.clone(),
                on_pass: template.on_pass.clone(),
                partition: template.partition.clone(),
                needs: needs.clone(),
                // The criterion text IS the unit's coverage: it grounds on the
                // criterion, and its UnitStarted spec_criterion is the real criterion.
                coverage: criterion.clone(),
                // Mark it the deterministic baseline for this criterion, so a
                // planner-proposed unit citing the same criterion supersedes it in
                // `harvest_proposed` rather than duplicating the work.
                baseline: true,
                ..Default::default()
            },
        ));
    }
    units
}

/// The lens set a standalone review stage runs concurrently: its `agents` list when
/// populated, else its single `agent`, else empty (§3.2). A standalone review stage
/// always has `agents` (it has no `agent` - that is what routes it to the fan-out
/// path), so the `agent` fallback is defensive; an implementer stage with an `agent`
/// runs its per-unit lifecycle instead and never reaches here.
fn fan_out_lenses(st: &Stage) -> Vec<String> {
    if !st.agents.is_empty() {
        st.agents.clone()
    } else if !st.agent.is_empty() {
        vec![st.agent.clone()]
    } else {
        Vec::new()
    }
}

/// Whether a stage carries an LLM judge, i.e. a real verifier and not a mechanical
/// proxy. A stage covers a criterion only if it has one (§8 proxy-gap guard, item 5):
/// a worker agent, a fan-out lens set, or an adjudicator. A gate-command-only stage
/// is a mechanical proxy and does not satisfy a conceptual criterion.
fn has_llm_verifier(st: &Stage) -> bool {
    !st.agent.is_empty() || !st.agents.is_empty() || !st.adjudicator.is_empty()
}

/// coverage_gap is the coverage gate (§3.2, §8). Every spec criterion must be
/// covered by a stage that has a real (LLM-judge) verifier; a criterion covered only
/// by a mechanical gate counts as NOT covered (the proxy-gap guard, item 5). It runs
/// against the live `stages` map, so proposed planner units (which carry their own
/// `coverage`) count toward closing the gap. Returns the gap reason, or None if every
/// criterion is covered (or there are no criteria to enforce).
fn coverage_gap(stages: &BTreeMap<String, Stage>, criteria: &[String]) -> Option<String> {
    if criteria.is_empty() {
        return None;
    }
    let covered: HashSet<&str> = stages
        .values()
        .filter(|st| has_llm_verifier(st))
        .map(|st| st.coverage.trim())
        .filter(|c| !c.is_empty())
        .collect();
    let gaps: Vec<&str> = criteria
        .iter()
        .map(|c| c.trim())
        .filter(|c| !covered.contains(c))
        .collect();
    if gaps.is_empty() {
        return None;
    }
    Some(format!(
        "coverage gap - no stage with an LLM verifier covers: {}",
        gaps.join("; ")
    ))
}

fn ready_stages(
    stages: &BTreeMap<String, Stage>,
    integrated: &HashSet<String>,
    terminal: &HashSet<String>,
) -> Vec<String> {
    let mut ready: Vec<String> = stages
        .iter()
        .filter(|(name, st)| {
            !terminal.contains(*name) && st.needs.iter().all(|n| integrated.contains(n))
        })
        .map(|(name, _)| name.clone())
        .collect();
    ready.sort();
    ready
}

/// validate_acyclic checks that the stage DAG has no dependency cycle; a residual
/// cycle is a hard error (the config is already validated acyclic, so this is
/// defense in depth). It is a CYCLE CHECK, not a scheduler: the conductor schedules
/// waves with [`ready_stages`] (which selects every stage whose `needs` are all
/// integrated), so the Kahn-style topological order computed here is used ONLY to
/// detect a cycle - if every node can be peeled off in dependency order the graph is
/// acyclic, otherwise it is not. It therefore returns no order (item 10: the name
/// now matches the behavior - it does not compute an order anyone consumes).
fn validate_acyclic(stages: &BTreeMap<String, Stage>) -> Result<(), Error> {
    let mut indeg: HashMap<String, usize> = HashMap::new();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for (name, st) in stages {
        indeg.insert(name.clone(), st.needs.len());
        for need in &st.needs {
            dependents
                .entry(need.clone())
                .or_default()
                .push(name.clone());
        }
    }
    let mut queue: Vec<String> = indeg
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| n.clone())
        .collect();
    // The count of nodes successfully peeled off in dependency order: if it reaches
    // the stage count the graph is acyclic, otherwise a cycle blocked some nodes.
    let mut peeled = 0usize;
    while let Some(n) = queue.pop() {
        peeled += 1;
        if let Some(deps) = dependents.get(&n) {
            for dep in deps {
                if let Some(d) = indeg.get_mut(dep) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(dep.clone());
                    }
                }
            }
        }
    }
    if peeled != stages.len() {
        return Err(Error("workflow has a dependency cycle".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::Filter;
    use crate::gate::ExecRunner;
    use std::path::Path;

    #[test]
    fn unit_worktree_dir_derives_deterministically_from_scratch_root_and_unit_id() {
        // Gap 12 remainder (spec 06:48): the unit worktree DIR derives purely from the
        // scratch root and the unit id - NO per-process UUID - so every process (a
        // resume, a concurrent step) computes the SAME path and adoption is a lookup.
        let a = unit_worktree_dir("/scratch", "unit-4 worktree paths");
        let b = unit_worktree_dir("/scratch", "unit-4 worktree paths");
        assert_eq!(
            a, b,
            "the same unit must yield the same worktree dir across processes (no uuid)"
        );
        assert_eq!(
            a, "/scratch/rigger-wt-unit-4-worktree-paths",
            "the dir is <scratch>/rigger-wt-<unit-slug>, sanitized, with no uuid suffix"
        );
        assert_ne!(
            unit_worktree_dir("/scratch", "unit-a"),
            unit_worktree_dir("/scratch", "unit-b"),
            "distinct units get distinct dirs"
        );
    }

    #[test]
    fn review_worktree_dir_and_branch_derive_from_stage_and_attempt() {
        // Spec 06:48: review worktrees derive from stage + attempt (not a per-process
        // uuid), so a resumed review step recomputes the same path/branch and can
        // adopt-or-prune rather than leaking a fresh one each process.
        assert_eq!(
            review_worktree_dir("/scratch", "review stage", 2),
            "/scratch/rigger-review-review-stage-2"
        );
        assert_eq!(
            review_branch("review stage", 2),
            "rigger/review/review-stage-2"
        );
        assert_eq!(
            review_worktree_dir("/scratch", "s", 0),
            review_worktree_dir("/scratch", "s", 0),
            "same (stage, attempt) is deterministic"
        );
        assert_ne!(
            review_worktree_dir("/scratch", "s", 0),
            review_worktree_dir("/scratch", "s", 1),
            "a different attempt yields a different worktree dir"
        );
        assert_ne!(
            review_branch("s", 0),
            review_branch("s", 1),
            "a different attempt yields a different throwaway review branch"
        );
    }

    struct Stub {
        write_file: Option<String>,
        emits: Vec<(String, Value)>,
        /// Per-agent emits, in addition to the shared `emits`: lets one test give a
        /// single lens a ReviewFinding to emit so the test can assert the finding
        /// reaches a LATER tier through the graph (not through prompt threading).
        emits_by_agent: HashMap<String, Vec<(String, Value)>>,
        output: String,
        /// Per-agent canned output, overriding `output` for that agent id. Lets one
        /// test give a lens a distinct finding and the adjudicator a distinct verdict
        /// (item 1: assert the findings actually flow between tiers).
        output_by_agent: HashMap<String, String>,
        /// Per-SPAWN-ID canned output, overriding both `output_by_agent` and `output`
        /// for the spawn whose deterministic id ([`SpawnOpts::id`]) matches. Lets a Gap-18
        /// test drive the SAME reviewer agent to a DEGENERATE (empty) result on its
        /// original spawn id and a substantive result on its `~retry{n}` respawn id -
        /// which `output_by_agent` (keyed on the agent, identical across retries) cannot.
        output_by_spawn_id: HashMap<String, String>,
        /// Every spawn's deterministic id ([`SpawnOpts::id`]), in spawn order. Lets a
        /// Gap-18 test assert the exact deterministic respawn ids (`u/role#att~retry{n}`)
        /// the conductor minted for a degenerate reviewer.
        spawn_ids: Mutex<Vec<String>>,
        /// Per-agent RESOLVED model id the driver returns on [`AgentResult::resolved_model`]
        /// (spec 05 line 52), letting a test drive the live (non-replay) path where the
        /// conductor copies the resolved id onto the spawn's unit events.
        resolved_model_by_agent: HashMap<String, String>,
        fail_spawn: bool,
        last_prompt: Mutex<String>,
        /// Per-agent (isolation, parallel) the conductor passed at each spawn.
        opts_by_agent: Mutex<HashMap<String, (bool, bool)>>,
        /// Every working dir (SpawnOpts.dir = the agent's cwd) each agent was spawned
        /// with, in order, keyed by agent id. Used to prove the worktree-isolation
        /// invariant: every spawn's cwd is its worktree, never empty / the repo root.
        dirs_by_agent: Mutex<HashMap<String, Vec<String>>>,
        /// The persona (system prompt) the conductor threaded into SpawnOpts for each
        /// agent at spawn time - used to assert every driver path receives the agent's
        /// role, not just the cli path's own arg-building.
        system_prompt_by_agent: Mutex<HashMap<String, String>>,
        /// Every prompt each agent was spawned with, in order, keyed by agent id.
        /// Used to assert the cross-tier findings block (item 1) and the prior-failure
        /// block on a retry (items 3 + 5) reached the right agent's prompt.
        prompts_by_agent: Mutex<HashMap<String, Vec<String>>>,
        /// The order agents were spawned in, by id - used to assert the lenses ->
        /// adversary -> adjudicator three-tier review order.
        call_order: Mutex<Vec<String>>,
    }
    impl Stub {
        fn new() -> Self {
            Stub {
                write_file: None,
                emits: Vec::new(),
                emits_by_agent: HashMap::new(),
                output: String::new(),
                output_by_agent: HashMap::new(),
                output_by_spawn_id: HashMap::new(),
                spawn_ids: Mutex::new(Vec::new()),
                resolved_model_by_agent: HashMap::new(),
                fail_spawn: false,
                last_prompt: Mutex::new(String::new()),
                opts_by_agent: Mutex::new(HashMap::new()),
                dirs_by_agent: Mutex::new(HashMap::new()),
                system_prompt_by_agent: Mutex::new(HashMap::new()),
                prompts_by_agent: Mutex::new(HashMap::new()),
                call_order: Mutex::new(Vec::new()),
            }
        }

        /// Every prompt the named agent was spawned with, in spawn order.
        fn prompts_for(&self, agent_id: &str) -> Vec<String> {
            self.prompts_by_agent
                .lock()
                .unwrap()
                .get(agent_id)
                .cloned()
                .unwrap_or_default()
        }

        /// Every working dir (cwd) the named agent was spawned with, in spawn order.
        fn dirs_for(&self, agent_id: &str) -> Vec<String> {
            self.dirs_by_agent
                .lock()
                .unwrap()
                .get(agent_id)
                .cloned()
                .unwrap_or_default()
        }

        /// The persona (system prompt) the conductor threaded to the driver for the
        /// named agent, or None if it was never spawned.
        fn system_prompt_for(&self, agent_id: &str) -> Option<String> {
            self.system_prompt_by_agent
                .lock()
                .unwrap()
                .get(agent_id)
                .cloned()
        }

        /// Every spawn's deterministic id, in spawn order (Gap-18 tests assert the exact
        /// `~retry{n}` respawn ids the conductor minted for a degenerate reviewer).
        fn spawn_ids(&self) -> Vec<String> {
            self.spawn_ids.lock().unwrap().clone()
        }

        /// How many times the named agent was spawned this run (Gap-18 tests count a
        /// degenerate reviewer's original spawn + its bounded respawns).
        fn spawn_count(&self, agent_id: &str) -> usize {
            self.call_order
                .lock()
                .unwrap()
                .iter()
                .filter(|id| *id == agent_id)
                .count()
        }

        /// Whether the named agent was spawned at all this run (resume tests assert an
        /// implementer is NOT re-spawned, or a reviewer is NOT spawned, on resume).
        fn spawned(&self, agent_id: &str) -> bool {
            self.call_order
                .lock()
                .unwrap()
                .iter()
                .any(|id| id == agent_id)
        }
    }
    impl AgentDriver for Stub {
        fn spawn(
            &self,
            a: &AgentDef,
            prompt: &str,
            opts: &SpawnOpts,
            emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            *self.last_prompt.lock().unwrap() = prompt.to_string();
            self.opts_by_agent
                .lock()
                .unwrap()
                .insert(a.id.clone(), (opts.isolation, opts.parallel));
            self.dirs_by_agent
                .lock()
                .unwrap()
                .entry(a.id.clone())
                .or_default()
                .push(opts.dir.clone());
            self.system_prompt_by_agent
                .lock()
                .unwrap()
                .insert(a.id.clone(), opts.system_prompt.clone());
            self.prompts_by_agent
                .lock()
                .unwrap()
                .entry(a.id.clone())
                .or_default()
                .push(prompt.to_string());
            self.call_order.lock().unwrap().push(a.id.clone());
            self.spawn_ids.lock().unwrap().push(opts.id.clone());
            if self.fail_spawn {
                return Err(Error("simulated mid-spawn crash".into()));
            }
            // Only an ISOLATED spawn (a real worktree dir) writes its file: a review
            // agent spawns with an empty `dir`, and writing there would land the file
            // in the process cwd (the actual rigger repo) - a test leak. An implementer
            // always has a worktree dir, so this still exercises the write path.
            if let Some(f) = &self.write_file {
                if !opts.dir.is_empty() {
                    let _ = std::fs::write(Path::new(&opts.dir).join(f), "work\n");
                }
            }
            for (t, v) in &self.emits {
                emit(t, v.clone())?;
            }
            if let Some(per) = self.emits_by_agent.get(&a.id) {
                for (t, v) in per {
                    emit(t, v.clone())?;
                }
            }
            let output = self
                .output_by_spawn_id
                .get(&opts.id)
                .or_else(|| self.output_by_agent.get(&a.id))
                .cloned()
                .unwrap_or_else(|| self.output.clone());
            Ok(AgentResult {
                output,
                resolved_model: self
                    .resolved_model_by_agent
                    .get(&a.id)
                    .cloned()
                    .unwrap_or_default(),
            })
        }
    }

    fn agent(id: &str) -> AgentDef {
        AgentDef {
            id: id.to_string(),
            ..Default::default()
        }
    }

    /// An agent with a persona (the markdown body of its definition) - its role
    /// instructions, which the conductor must thread to the driver as the system
    /// prompt.
    fn agent_with_prompt(id: &str, prompt: &str) -> AgentDef {
        AgentDef {
            id: id.to_string(),
            prompt: prompt.to_string(),
            ..Default::default()
        }
    }

    fn gate_def(run: &str) -> config::Gate {
        config::Gate {
            run: run.to_string(),
            kind: "core".to_string(),
        }
    }

    #[test]
    fn integrates_a_passing_stage() {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["s"].status, ledger::Status::Integrated);
    }

    #[test]
    fn coverage_gate_refuses_an_uncovered_criterion() {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                coverage: "criterion one".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec!["criterion one".into(), "criterion two".into()],
        };
        assert!(run(&cfg, &deps).is_err());
    }

    #[test]
    fn planner_extends_the_dag() {
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            emits: vec![(
                TYPE_UNIT_PROPOSED.to_string(),
                json!({"id": "impl", "agent": "worker"}),
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["impl"].status, ledger::Status::Integrated);
    }

    #[test]
    fn planner_proposed_unit_with_a_coverage_criterion_runs_and_integrates() {
        // The living-DAG / spawnUnit mechanic (§3.2, §8): a `produces: dag` planner
        // stage emits a UnitProposed carrying its own `coverage` criterion; the
        // conductor harvests it into the run DAG, and because it covers a real spec
        // criterion it passes the post-plan coverage gate, then runs and integrates.
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                coverage: "the spec is decomposed".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            emits: vec![(
                TYPE_UNIT_PROPOSED.to_string(),
                json!({
                    "id": "impl-feature",
                    "agent": "worker",
                    "needs": ["plan"],
                    "coverage": "the feature is implemented",
                    "gates": ["ok"],
                }),
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            // Both criteria must be covered: the planner's own, and the proposed
            // unit's. The coverage gate (deferred until after planning) only passes
            // because the harvested unit closes the second one.
            criteria: vec![
                "the spec is decomposed".into(),
                "the feature is implemented".into(),
            ],
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["impl-feature"].status,
            ledger::Status::Integrated,
            "the planner-proposed unit must run and integrate as part of the extended DAG"
        );
        // The proposal really extended the DAG: the unit was started and integrated,
        // and it carried its coverage criterion into the run.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| {
                e.type_ == ledger::TYPE_UNIT_STARTED
                    && String::from_utf8_lossy(&e.data).contains("impl-feature")
            }),
            "the harvested unit must be started by the conductor"
        );
    }

    #[test]
    fn conductor_creates_one_baseline_unit_per_criterion() {
        // The deterministic decomposition baseline (§3.2): given the spec's acceptance
        // criteria [A, B, C] and a fan-out implement TEMPLATE, the conductor itself
        // synthesizes ONE implement unit per criterion - each carrying the REAL
        // criterion text as its coverage/spec_criterion (not the template label, never
        // "required") and the template's agent. The bare template is NOT run as a unit.
        let criteria = [
            "the metrics module projects first-pass yield",
            "rigger stats prints the report",
            "the projection is covered by a unit test",
        ];
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                strategy: "fan-out".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                // The template's own coverage label - the per-criterion units must NOT
                // inherit it; they carry the real criterion text instead.
                coverage: "each unit is implemented and integrates".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: criteria.iter().map(|c| c.to_string()).collect(),
        };
        let rs = run(&cfg, &deps).unwrap();

        // The bare template was NOT run as its own unit - the per-criterion units
        // replaced it.
        assert!(
            !rs.units.contains_key("implement"),
            "the fan-out template is a template, not a unit; it must not run as `implement`"
        );

        // Exactly one unit per criterion was started, each carrying the REAL criterion
        // as its spec_criterion (never the template label, never "required") and the
        // template's agent. Read the raw UnitStarted events to inspect both fields.
        let started: Vec<Value> = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap()
            .iter()
            .filter(|e| e.type_ == ledger::TYPE_UNIT_STARTED)
            .map(|e| serde_json::from_slice::<Value>(&e.data).unwrap())
            .collect();
        assert_eq!(
            started.len(),
            criteria.len(),
            "one baseline unit per acceptance criterion"
        );
        let field = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let mut got_criteria: Vec<String> =
            started.iter().map(|u| field(u, "spec_criterion")).collect();
        got_criteria.sort();
        let mut want: Vec<String> = criteria.iter().map(|c| c.to_string()).collect();
        want.sort();
        assert_eq!(
            got_criteria, want,
            "each baseline unit's spec_criterion is the REAL criterion text, not the template label or \"required\""
        );
        assert!(
            started.iter().all(|u| field(u, "agent") == "worker"),
            "each baseline unit is assigned the template's agent"
        );

        // And the per-unit ledger carries the real criterion, so each criterion's unit
        // actually ran and integrated.
        for c in criteria {
            let unit = rs
                .units
                .values()
                .find(|u| u.spec_criterion == c)
                .unwrap_or_else(|| panic!("a unit must cover criterion {c:?}"));
            assert_eq!(unit.status, ledger::Status::Integrated);
        }
    }

    #[test]
    fn producer_prompt_carries_the_criteria_and_plan_protocol_grounded_on_the_spec() {
        // A `produces: dag` planner stage must be wired: its prompt carries the spec's
        // acceptance criteria AND the PLAN_PROTOCOL (the refine-protocol), and it
        // GROUNDS on the spec criteria - NOT on a `coverage: required` label. A file
        // named for the criterion is found by grounding; a "required" decoy is NOT,
        // proving the literal coverage label is never the grounding query.
        let dir = tempfile::tempdir().unwrap();
        // A real source file whose content matches the criterion text - grounding on
        // the spec criteria must surface it.
        std::fs::write(
            dir.path().join("metrics.rs"),
            "// the metrics module projects first-pass yield\nfn project() {}\n",
        )
        .unwrap();
        // The decoy: a file mentioning "required" (as the LICENSE bug did). Grounding
        // on the spec criteria must NOT surface it; grounding on "required" would.
        std::fs::write(
            dir.path().join("LICENSE_DECOY.txt"),
            "this is required by the license terms\n",
        )
        .unwrap();

        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        // The planner stage carries the BUGGY `coverage: required` label, to prove the
        // fix grounds on the spec criteria regardless of that label.
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                coverage: "required".into(),
                ..Default::default()
            },
        );
        // A fan-out implement template, so the conductor's baseline closes the
        // criterion (coverage holds after planning) and the implementer agent id the
        // PLAN_PROTOCOL names is "worker".
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                strategy: "fan-out".into(),
                needs: vec!["plan".into()],
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let grep = crate::grounder::Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };
        let criterion = "the metrics module projects first-pass yield";
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: Some(&grep),
            graph: None,
            criteria: vec![criterion.to_string()],
        };
        run(&cfg, &deps).unwrap();

        let prompts = driver.prompts_for("planner");
        assert!(!prompts.is_empty(), "the planner must have been spawned");
        let prompt = &prompts[0];
        // The criteria reach the planner.
        assert!(
            prompt.contains(criterion),
            "the planner prompt must list the spec acceptance criteria; got:\n{prompt}"
        );
        // The PLAN_PROTOCOL (refine-protocol) reaches the planner, naming the
        // implementer agent and the UnitProposed shape.
        assert!(
            prompt.contains("UnitProposed") && prompt.contains("REFINE"),
            "the planner prompt must carry the PLAN_PROTOCOL refine instructions; got:\n{prompt}"
        );
        assert!(
            prompt.contains("\"agent\":\"worker\""),
            "the PLAN_PROTOCOL must tell the planner the implementer agent id; got:\n{prompt}"
        );
        // Grounding is on the SPEC, not the label: the criterion-matching file is
        // surfaced, the "required" decoy is not.
        assert!(
            prompt.contains("metrics.rs"),
            "the planner must ground on the spec criteria (surfacing metrics.rs); got:\n{prompt}"
        );
        assert!(
            !prompt.contains("LICENSE_DECOY"),
            "the planner must NOT ground on the `coverage: required` label (no LICENSE decoy); got:\n{prompt}"
        );
    }

    /// A criterion's deterministic baseline unit id, as the conductor synthesizes it
    /// from the criterion's ordinal + slug (the `baseline_units` / `unit_slug` shape).
    /// Lets a supersede test name the exact baseline id it expects removed.
    fn baseline_id(ordinal: usize, criterion: &str) -> String {
        unit_slug(ordinal, criterion)
    }

    /// A two-stage spec-driven workflow (a `produces` planner + a fan-out implement
    /// template) and a `planner`/`worker` agent pair, the shared scaffold of the
    /// supersede tests. The caller supplies the planner's UnitProposed emits.
    fn supersede_cfg() -> Config {
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                strategy: "fan-out".into(),
                needs: vec!["plan".into()],
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        cfg
    }

    #[test]
    fn a_prior_runs_proposal_never_resurrects_in_a_new_run() {
        // Gap 11, completing spec-06 unit-1: the proposal harvest is run-scoped. A
        // UnitProposed recorded BEFORE this run's RunStarted boundary (an aborted
        // prior run's zombie) must never enter the new run's DAG - its terminal
        // states are scoped out of `terminal`, so an unscoped harvest would re-park
        // it as a fresh unit at attempt #0 (observed live on the first scoped run).
        let crit_a = "criterion A: the metrics module is implemented";
        let cfg = supersede_cfg();
        let st = Store::open(":memory:").unwrap();
        // The zombie: a prior run's proposal, with non-empty coverage citing a
        // criterion this run does not have, sitting in the pre-boundary slice.
        st.append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                TYPE_UNIT_PROPOSED,
                serde_json::to_vec(&json!({
                    "id": "u-zombie-mod",
                    "agent": "worker",
                    "criterion": "an ancient criterion from an aborted run",
                    "gates": ["ok"],
                }))
                .unwrap(),
            )],
        )
        .unwrap();
        // The planner proposes nothing this run; the baseline covers criterion A.
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec![crit_a.to_string()],
        };
        let state = run(&cfg, &deps).unwrap();
        assert!(
            !state.units.contains_key("u-zombie-mod"),
            "a pre-boundary proposal must not enter the run: {:?}",
            state.units.keys().collect::<Vec<_>>()
        );
        let events = st.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_STARTED
                && String::from_utf8_lossy(&e.data).contains("u-zombie-mod")),
            "the zombie must never start"
        );
    }

    #[test]
    fn planner_unit_supersedes_the_matching_baseline() {
        // The duplication fix: a planner unit that cites a criterion VERBATIM SUPERSEDES
        // (replaces) that criterion's deterministic baseline - one unit per criterion,
        // not baseline + refinement both doing the same work. Given two criteria [A, B]
        // each with a baseline, a planner that proposes ONE unit citing criterion A must
        // remove A's baseline (A's final unit is the planner's, not the baseline) while
        // B's baseline survives untouched.
        let crit_a = "criterion A: the metrics module is implemented";
        let crit_b = "criterion B: the stats endpoint is implemented";
        let cfg = supersede_cfg();
        let st = Store::open(":memory:").unwrap();
        // The planner proposes exactly one unit, citing criterion A verbatim.
        let driver = Stub {
            emits: vec![(
                TYPE_UNIT_PROPOSED.to_string(),
                json!({
                    "id": "planner-unit-a",
                    "agent": "worker",
                    "criterion": crit_a,
                    "gates": ["ok"],
                }),
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec![crit_a.to_string(), crit_b.to_string()],
        };
        let rs = run(&cfg, &deps).unwrap();

        // The planner's unit for A ran and integrated, carrying criterion A.
        assert_eq!(
            rs.units["planner-unit-a"].status,
            ledger::Status::Integrated,
            "the planner unit citing criterion A must run and integrate"
        );
        assert_eq!(rs.units["planner-unit-a"].spec_criterion, crit_a);

        // Criterion A's BASELINE is gone - superseded, never run.
        let a_baseline = baseline_id(1, crit_a);
        assert!(
            !rs.units.contains_key(&a_baseline),
            "criterion A's baseline {a_baseline:?} must be superseded (removed), not run; units present: {:?}",
            rs.units.keys().collect::<Vec<_>>()
        );

        // Criterion B has NO planner unit, so its baseline survived and ran.
        let b_baseline = baseline_id(2, crit_b);
        assert_eq!(
            rs.units[&b_baseline].status,
            ledger::Status::Integrated,
            "criterion B's baseline {b_baseline:?} must survive and run (no planner unit covers B)"
        );
        assert_eq!(rs.units[&b_baseline].spec_criterion, crit_b);

        // EXACTLY two implement units total: planner-A + baseline-B. The plan stage is
        // a producer (it integrates with no code artifact), so exclude it; the two that
        // remain are the implement units, one per criterion - no duplication.
        let implement_units: Vec<&str> = rs
            .units
            .values()
            .filter(|u| u.id != "plan")
            .map(|u| u.id.as_str())
            .collect();
        assert_eq!(
            implement_units.len(),
            2,
            "exactly two implement units (planner-A + baseline-B), one per criterion; got: {implement_units:?}"
        );
        // And exactly one unit per criterion - never two doing the same work.
        for c in [crit_a, crit_b] {
            let n = rs.units.values().filter(|u| u.spec_criterion == c).count();
            assert_eq!(
                n, 1,
                "criterion {c:?} must be served by exactly one unit, got {n}"
            );
        }
    }

    #[test]
    fn a_criterion_with_no_planner_unit_keeps_its_baseline() {
        // The reliable fallback: a baseline survives ONLY if no planner unit covers its
        // criterion. With a planner that proposes NOTHING, every criterion's baseline
        // remains and runs - the deterministic decomposition stands on its own.
        let crit_a = "criterion A: the metrics module is implemented";
        let crit_b = "criterion B: the stats endpoint is implemented";
        let cfg = supersede_cfg();
        let st = Store::open(":memory:").unwrap();
        // The planner proposes nothing (no emits) - pure baseline decomposition.
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec![crit_a.to_string(), crit_b.to_string()],
        };
        let rs = run(&cfg, &deps).unwrap();

        for (ordinal, crit) in [(1, crit_a), (2, crit_b)] {
            let id = baseline_id(ordinal, crit);
            assert_eq!(
                rs.units[&id].status,
                ledger::Status::Integrated,
                "criterion {crit:?} baseline {id:?} must survive and run when no planner unit covers it"
            );
            assert_eq!(rs.units[&id].spec_criterion, crit);
        }
        // Exactly one unit per criterion (the two baselines), no more.
        let implement_units = rs.units.values().filter(|u| u.id != "plan").count();
        assert_eq!(
            implement_units, 2,
            "both baselines run, one per criterion, no duplication"
        );
    }

    #[test]
    fn planner_refinement_split_is_still_harvested() {
        // Refinement STILL works on top of the baseline: when the planner SPLITS one
        // criterion into several units (each citing that SAME criterion verbatim), all
        // the split units survive and run, and they REPLACE the one baseline for that
        // criterion - the split is the refined decomposition, not baseline + refinement
        // both. A genuinely new planner unit is still added; it simply supersedes the
        // baseline it refines instead of duplicating it.
        let criterion = "the feature is implemented";
        let cfg = supersede_cfg();
        let st = Store::open(":memory:").unwrap();
        // The planner splits the one criterion into TWO sub-units, both citing it
        // verbatim (the PLAN_PROTOCOL shape).
        let driver = Stub {
            emits: vec![
                (
                    TYPE_UNIT_PROPOSED.to_string(),
                    json!({
                        "id": "refine-part-1",
                        "agent": "worker",
                        "criterion": criterion,
                        "gates": ["ok"],
                    }),
                ),
                (
                    TYPE_UNIT_PROPOSED.to_string(),
                    json!({
                        "id": "refine-part-2",
                        "agent": "worker",
                        "criterion": criterion,
                        "gates": ["ok"],
                    }),
                ),
            ],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec![criterion.to_string()],
        };
        let rs = run(&cfg, &deps).unwrap();
        // Both split units were harvested, ran, and integrated.
        for id in ["refine-part-1", "refine-part-2"] {
            assert_eq!(
                rs.units[id].status,
                ledger::Status::Integrated,
                "the planner split unit {id:?} must be harvested and run"
            );
            assert_eq!(rs.units[id].spec_criterion, criterion);
        }
        // The one baseline for the criterion was SUPERSEDED by the split - it did not
        // run alongside the split units (no duplication).
        let baseline = baseline_id(1, criterion);
        assert!(
            !rs.units.contains_key(&baseline),
            "the criterion's baseline {baseline:?} must be superseded by the split, not run too"
        );
        // The criterion is served by exactly the two split units - no extra baseline.
        let serving: Vec<&str> = rs
            .units
            .values()
            .filter(|u| u.spec_criterion == criterion)
            .map(|u| u.id.as_str())
            .collect();
        assert_eq!(
            serving.len(),
            2,
            "the criterion is served by the two split units only; got: {serving:?}"
        );
    }

    #[test]
    fn resume_dedups_baselines_before_running_them() {
        // The resume ordering bug (the order-independent supersede fix): on a RESUME
        // run the `plan` stage is ALREADY integrated, so its per-criterion baselines are
        // immediately ready and the FIRST wave would run them - BEFORE the bottom-of-loop
        // harvest folds the prior window's planner proposals and supersedes the matching
        // baselines. The bug ran criterion A's baseline as a DUPLICATE alongside the
        // planner's unit for A. The fix harvests the already-emitted UnitProposed events
        // BEFORE any wave, so the supersede holds on resume: A's baseline never runs while
        // B's (which no planner unit covers) still does.
        let crit_a = "criterion A: the metrics module is implemented";
        let crit_b = "criterion B: the stats endpoint is implemented";
        let cfg = supersede_cfg();
        let st = Store::open(":memory:").unwrap();

        // Seed the log as a PRIOR window left it: the planner ran, integrated, and
        // proposed one unit citing criterion A verbatim - exactly A's baseline. On
        // resume the planner does NOT run again (`plan` is terminal), so the only source
        // of A's supersede is folding this already-emitted UnitProposed before the wave.
        seed_events_in_run(
            &st,
            &[crit_a, crit_b],
            &[
                Event::new(
                    ledger::TYPE_UNIT_STARTED,
                    serde_json::to_vec(&json!({"id": "plan", "agent": "planner"})).unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_INTEGRATED,
                    serde_json::to_vec(&json!({"id": "plan", "commit": "deadbeef"})).unwrap(),
                ),
                Event::new(
                    TYPE_UNIT_PROPOSED,
                    serde_json::to_vec(&json!({
                        "id": "planner-unit-a",
                        "agent": "worker",
                        "criterion": crit_a,
                        "needs": ["plan"],
                        "gates": ["ok"],
                    }))
                    .unwrap(),
                ),
            ],
        );

        // The planner emits NOTHING this run - it already ran in the prior window. The
        // ONLY UnitProposed in play is the seeded one, so a correct resume must fold it
        // before the first wave.
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec![crit_a.to_string(), crit_b.to_string()],
        };
        let rs = run(&cfg, &deps).unwrap();

        // Criterion A's BASELINE is superseded before it could be scheduled: it never
        // started its implementer (no UnitStarted for it). Asserted on the raw log so a
        // SPAWN is what we measure, not just the final projection.
        let a_baseline = baseline_id(1, crit_a);
        assert!(
            !rs.units.contains_key(&a_baseline),
            "criterion A's baseline {a_baseline:?} must be superseded on resume, not run; units present: {:?}",
            rs.units.keys().collect::<Vec<_>>()
        );
        let started_ids: Vec<String> = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap()
            .iter()
            .filter(|e| e.type_ == ledger::TYPE_UNIT_STARTED)
            .map(|e| {
                serde_json::from_slice::<Value>(&e.data)
                    .unwrap()
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert!(
            !started_ids.contains(&a_baseline),
            "criterion A's baseline {a_baseline:?} must NEVER start its implementer on resume; started: {started_ids:?}"
        );

        // The planner's unit for A is the one that ran and integrated A.
        assert_eq!(
            rs.units["planner-unit-a"].status,
            ledger::Status::Integrated,
            "the planner's unit for criterion A must run and integrate on resume"
        );

        // Criterion B has no planner unit, so ITS baseline survives and runs - the dedup
        // is targeted, not a blanket suppression of every baseline on resume.
        let b_baseline = baseline_id(2, crit_b);
        assert_eq!(
            rs.units[&b_baseline].status,
            ledger::Status::Integrated,
            "criterion B's baseline {b_baseline:?} must still run on resume (no planner unit covers B)"
        );

        // The implementer (`worker`) is spawned for EXACTLY two units - planner-A and
        // baseline-B - never three. A third worker spawn is the duplicate baseline-A
        // running, which is the bug. This reads the Stub's recorded spawns directly:
        // each implement unit spawns its implementer exactly once on the passing path
        // (gate `ok` is `true`, no review tier is configured in `supersede_cfg`).
        let worker_spawns = driver
            .call_order
            .lock()
            .unwrap()
            .iter()
            .filter(|id| *id == "worker")
            .count();
        assert_eq!(
            worker_spawns, 2,
            "exactly two implementer spawns on resume (planner-A + baseline-B); a third is the \
             duplicate baseline-A the resume bug ran"
        );

        // And exactly one unit served criterion A - the planner's - never the baseline too.
        let serving_a = rs
            .units
            .values()
            .filter(|u| u.spec_criterion == crit_a)
            .count();
        assert_eq!(
            serving_a, 1,
            "criterion A must be served by exactly one unit on resume, not the baseline too"
        );
    }

    #[test]
    fn decomposes_the_real_spec_01_into_per_criterion_units() {
        // End-to-end against the REAL repo: load `specs/01-observability.md`'s actual
        // acceptance criteria and the REAL `.rigger/workflow.yml`, then run the conductor
        // through the real decomposition path. A stub planner stands in for the slow live
        // review (the decomposition itself is what the conductor does, not the agent), so
        // this asserts the PROOF the dogfood run wants: the conductor emits one
        // UnitStarted per real spec criterion, each carrying the REAL criterion text
        // (metrics/stats), never the `coverage: required` label. This is exactly the path
        // the live `rigger workflow specs/01-observability.md` drives.
        let mut cfg = config::load(".").expect("the repo's own .rigger config must load");
        // Neutralize the real cargo gate COMMANDS to `true` so this test exercises the
        // decomposition path without recursively invoking cargo (the gate IDENTITIES and
        // the stage graph stay exactly as authored - only the shell command is stubbed).
        for g in cfg.workflow.gates.values_mut() {
            g.run = "true".into();
        }
        let spec_text = std::fs::read_to_string("specs/01-observability.md")
            .expect("the real spec 01 must be present");
        let criteria = crate::spec::extract_criteria(&spec_text);
        assert_eq!(
            criteria.len(),
            4,
            "spec 01 has four Done-when acceptance criteria"
        );

        let st = Store::open(":memory:").unwrap();
        // The stub stands in for every live agent (no slow review needed): the planner
        // proposes nothing - the conductor's deterministic baseline alone covers every
        // criterion - and the review tiers approve, so the plan stage integrates and the
        // per-criterion implement wave runs. The decomposition under test is the
        // conductor's, not the agents'.
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: criteria.clone(),
        };
        run(&cfg, &deps).expect("the real spec must decompose and run without a coverage gap");

        // One UnitStarted per real criterion, carrying the REAL criterion as
        // spec_criterion - the proof the loop now decomposes the spec.
        let started: Vec<Value> = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap()
            .iter()
            .filter(|e| e.type_ == ledger::TYPE_UNIT_STARTED)
            .map(|e| serde_json::from_slice::<Value>(&e.data).unwrap())
            .collect();
        let started_criteria: Vec<String> = started
            .iter()
            .map(|v| {
                v.get("spec_criterion")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        for c in &criteria {
            assert!(
                started_criteria.iter().any(|s| s == c),
                "a per-criterion unit must carry the REAL criterion {c:?}; got {started_criteria:?}"
            );
        }
        assert!(
            started_criteria.iter().all(|s| s != "required"),
            "no unit may carry the bogus `coverage: required` label as its criterion"
        );
        // The real criteria are about metrics/stats, not LICENSE.
        assert!(
            started_criteria
                .iter()
                .any(|s| s.contains("metrics") || s.contains("rigger stats")),
            "the decomposed criteria must be the real spec-01 metrics/stats criteria"
        );
    }

    #[test]
    fn ratchet_promotes_a_reliable_gate() {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        for (name, need) in [("s1", None), ("s2", Some("s1")), ("s3", Some("s2"))] {
            cfg.workflow.stages.insert(
                name.into(),
                Stage {
                    name: name.into(),
                    agent: "a".into(),
                    needs: need.map(|n| vec![n.to_string()]).unwrap_or_default(),
                    gates: vec!["ok".into()],
                    ..Default::default()
                },
            );
        }
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let promoted = events.iter().any(|e| {
            e.type_ == TYPE_GATE_PROMOTED && String::from_utf8_lossy(&e.data).contains("\"ok\"")
        });
        assert!(
            promoted,
            "a gate that passed PROMOTE_THRESHOLD times should be promoted"
        );
    }

    #[test]
    fn elevated_gate_is_never_promoted_to_silent() {
        // Under a real run a gate seeds at the default auto_notify autonomy (a
        // manual gate would pause the stage instead of running). From there a
        // reliable Core gate is promoted to `silent`, but an Elevated gate is
        // capped at auto_notify and must NEVER be proposed for silent. Run both
        // through the identical three-clean-passes ratchet and contrast.
        let promotions_for = |kind: &str| -> Vec<String> {
            let mut cfg = Config::default();
            cfg.agents.insert("a".into(), agent("a"));
            let mut g = gate_def("true");
            g.kind = kind.to_string();
            cfg.workflow.gates.insert("ok".into(), g);
            let mut prev: Option<&str> = None;
            for name in ["s1", "s2", "s3", "s4"] {
                cfg.workflow.stages.insert(
                    name.into(),
                    Stage {
                        name: name.into(),
                        agent: "a".into(),
                        needs: prev.map(|n| vec![n.to_string()]).unwrap_or_default(),
                        gates: vec!["ok".into()],
                        ..Default::default()
                    },
                );
                prev = Some(name);
            }
            let st = Store::open(":memory:").unwrap();
            let driver = Stub::new();
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &ExecRunner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
            st.read_all(0, Direction::Forward, &Filter::default())
                .unwrap()
                .iter()
                .filter(|e| e.type_ == TYPE_GATE_PROMOTED)
                .map(|e| String::from_utf8_lossy(&e.data).into_owned())
                .collect()
        };

        // The Core gate, seeded at auto_notify, is promoted to silent: this is
        // the baseline the elevated gate must NOT reach.
        let core = promotions_for("core");
        assert!(
            core.iter().any(|p| p.contains("silent")),
            "a reliable Core gate must be promoted to silent (baseline): {core:?}"
        );

        // The Elevated gate, under the very same ratchet, is never promoted to
        // silent - its auto_notify ceiling holds.
        let elevated = promotions_for("elevated");
        assert!(
            elevated.iter().all(|p| !p.contains("silent")),
            "an elevated gate must NEVER be promoted to silent: {elevated:?}"
        );
    }

    #[test]
    fn learns_from_escalation() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("bad".into(), gate_def("false"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["bad".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            write_file: Some("broken.rs".into()),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["s"].status, ledger::Status::Escalated);
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let lesson = events.iter().any(|e| {
            e.type_ == contextgraph::TYPE_LESSON_LEARNED
                && String::from_utf8_lossy(&e.data).contains("broken.rs")
        });
        assert!(
            lesson,
            "escalation should record a lesson about the touched file"
        );
    }

    #[test]
    fn feeds_graph_decisions_into_the_prompt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("modifier.rs"), "fn modifier() {}\n").unwrap();
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();
        let mut e = Event::new(
            contextgraph::TYPE_DECISION_MADE,
            serde_json::to_vec(&json!({
                "id": "d1", "summary": "uses the generic engine pipeline", "governs": ["modifier.rs"],
            }))
            .unwrap(),
        );
        e.position = 999;
        graph.apply(&e).unwrap();

        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                coverage: "modifier".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let grep = crate::grounder::Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: Some(&grep),
            graph: Some(&graph),
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let prompt = driver.last_prompt.lock().unwrap().clone();
        assert!(
            prompt.contains("generic engine pipeline"),
            "the agent should be fed the decision governing modifier.rs; prompt was:\n{prompt}"
        );
    }

    #[test]
    fn decisions_prompt_injection_is_capped_under_budget_with_elision_note() {
        // Gap 15 / spec-06 unit 5: a pile of K rejection-round verdicts (each recorded
        // as a DecisionMade governing the unit's file) must NOT be concatenated
        // verbatim into the prompt. The most-recent decisions stay verbatim under a
        // hard byte budget; the older remainder collapses into ONE visible elision note
        // naming the count and the `rigger peers <file>` recovery command. The store
        // keeps the full history - only the prompt slice narrows.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("modifier.rs"), "fn modifier() {}\n").unwrap();
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();

        // K governing decisions, oldest (d0) first, each a chunky verdict; the event
        // position increases with i, so recency = i (newest = d{K-1}).
        const K: usize = 200;
        for i in 0..K {
            let summary = format!("REJECT_MARKER_{i} {}", "x".repeat(2400));
            let mut e = Event::new(
                contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&json!({
                    "id": format!("d{i}"),
                    "summary": summary,
                    "governs": ["modifier.rs"],
                }))
                .unwrap(),
            );
            e.position = (i as u64) + 1;
            graph.apply(&e).unwrap();
        }

        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                coverage: "modifier".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let grep = crate::grounder::Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: Some(&grep),
            graph: Some(&graph),
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let prompt = driver.last_prompt.lock().unwrap().clone();

        // Uncapped, the decisions section alone would be ~500KB; the cap holds the
        // whole prompt well under budget (plus slack for the small fixed sections).
        assert!(
            prompt.len() < DECISIONS_BUDGET_BYTES + 8 * 1024,
            "capped decisions prompt must stay under budget; len={}",
            prompt.len()
        );
        // Newest verdicts survive verbatim; the oldest are elided from the prompt.
        assert!(
            prompt.contains("REJECT_MARKER_199 "),
            "the newest decision must be kept verbatim"
        );
        assert!(
            !prompt.contains("REJECT_MARKER_0 "),
            "the oldest decision must be elided from the prompt (it stays in the store)"
        );
        // The verbatim set is capped to at most N (the byte budget may bind sooner).
        let verbatim = (0..K)
            .filter(|i| prompt.contains(&format!("REJECT_MARKER_{i} ")))
            .count();
        assert!(
            (1..=DECISIONS_VERBATIM_N).contains(&verbatim),
            "verbatim decisions must be capped to <= N; got {verbatim}"
        );
        // The trim is VISIBLE: one elision note naming the elided count and the
        // `rigger peers <file>` recovery command.
        let elided = K - verbatim;
        assert!(
            prompt.contains(&format!("+{elided} older decision")),
            "the elision note must name the elided count ({elided})"
        );
        assert!(
            prompt.contains("rigger peers modifier.rs"),
            "the elision note must name the `rigger peers <file>` recovery command"
        );
    }

    // Build a decisions subgraph from a list of (id, summary, supersedes) tuples,
    // applied in order so each event's position increases (older first), then return
    // the rendered capped-decisions section for `seed`.
    fn render_capped_decisions(decisions: &[(&str, String, &str)], seed: &[String]) -> String {
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();
        for (i, (id, summary, supersedes)) in decisions.iter().enumerate() {
            let mut payload = json!({
                "id": id,
                "summary": summary,
                "governs": seed,
            });
            if !supersedes.is_empty() {
                payload["supersedes"] = json!(supersedes);
            }
            let mut e = Event::new(
                contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&payload).unwrap(),
            );
            e.position = (i as u64) + 1;
            graph.apply(&e).unwrap();
        }
        let g = graph.subgraph(seed, 2).unwrap();
        let mut b = String::new();
        write_capped_decisions(&mut b, &g, seed);
        b
    }

    #[test]
    fn a_superseded_decision_never_outranks_a_current_one() {
        // sdet-u5 / arch-u5 carry-forward: recency is the GOVERNS-edge source, so a
        // SUPERSEDED decision (its GOVERNS edge invalidated, reachable only via the
        // still-valid SUPERSEDES edge that carries its superseder's position) can no
        // longer inherit that fresh position and crowd a genuinely-current decision out
        // of the verbatim slice. d_a is superseded by the newest d_c; d_b is a current,
        // middle-aged decision. The stale d_a must rank BELOW the current d_b.
        let seed = vec!["modifier.rs".to_string()];
        let out = render_capped_decisions(
            &[
                ("d_a", "the FIRST verdict, later superseded".into(), ""),
                ("d_b", "a CURRENT middle-aged verdict".into(), ""),
                ("d_c", "the NEWEST verdict, supersedes d_a".into(), "d_a"),
            ],
            &seed,
        );
        let idx = |needle: &str| {
            out.find(needle)
                .unwrap_or_else(|| panic!("decision {needle} must render; output was:\n{out}"))
        };
        let (ia, ib, ic) = (idx("- d_a:"), idx("- d_b:"), idx("- d_c:"));
        assert!(
            ic < ib,
            "the newest current decision (d_c) must rank first; output was:\n{out}"
        );
        assert!(
            ia > ib,
            "the superseded decision (d_a) must rank below the current d_b, not inherit its \
             superseder's recency; output was:\n{out}"
        );
    }

    #[test]
    fn the_verbatim_count_cap_binds_on_many_small_decisions() {
        // sdet-u5-verbatim-n-cap-untested: with many TINY decisions the byte budget never
        // binds, so this isolates the recent-N verbatim arm. Exactly N are kept verbatim
        // and the rest collapse into the elision note. A regression removing the N cap
        // would render all of them (bytes never bind) and fail this.
        let seed = vec!["modifier.rs".to_string()];
        let total = DECISIONS_VERBATIM_N + 8;
        let decisions: Vec<(String, String, &str)> = (0..total)
            .map(|i| (format!("small{i:03}"), format!("tiny verdict {i}"), ""))
            .collect();
        let borrowed: Vec<(&str, String, &str)> = decisions
            .iter()
            .map(|(id, s, sup)| (id.as_str(), s.clone(), *sup))
            .collect();
        let out = render_capped_decisions(&borrowed, &seed);
        let kept = (0..total)
            .filter(|i| out.contains(&format!("small{i:03}:")))
            .count();
        assert_eq!(
            kept, DECISIONS_VERBATIM_N,
            "exactly N tiny decisions must be kept verbatim (the count cap binds, not bytes); \
             output was:\n{out}"
        );
        assert!(
            out.contains(&format!("+{} older decision", total - DECISIONS_VERBATIM_N)),
            "the elision note must name the count the N cap trims; output was:\n{out}"
        );
    }

    #[test]
    fn the_byte_budget_cap_binds_before_the_count_on_chunky_decisions() {
        // sdet-u5-verbatim-n-cap-untested: with N chunky decisions the 24KiB byte budget
        // binds BEFORE the recent-N count, so this isolates the byte-budget arm. Fewer
        // than N are kept and the body stays under budget. A regression removing the byte
        // cap would keep all N (~3KiB each => far over budget) and fail this.
        let seed = vec!["modifier.rs".to_string()];
        // Each line is ~3KiB, so ~8 fit under the 24KiB budget - fewer than N=12.
        let chunk = "y".repeat(3000);
        let decisions: Vec<(String, String, &str)> = (0..DECISIONS_VERBATIM_N)
            .map(|i| (format!("chunk{i:03}"), format!("verdict {i} {chunk}"), ""))
            .collect();
        let borrowed: Vec<(&str, String, &str)> = decisions
            .iter()
            .map(|(id, s, sup)| (id.as_str(), s.clone(), *sup))
            .collect();
        let out = render_capped_decisions(&borrowed, &seed);
        let kept = (0..DECISIONS_VERBATIM_N)
            .filter(|i| out.contains(&format!("chunk{i:03}:")))
            .count();
        assert!(
            kept < DECISIONS_VERBATIM_N,
            "the byte budget must trim below the N count on chunky decisions; kept={kept}"
        );
        assert!(
            kept >= 1,
            "at least one chunky decision must still render verbatim; kept={kept}"
        );
        assert!(
            out.len() < DECISIONS_BUDGET_BYTES + 1024,
            "the verbatim body must stay under the byte budget; len={}",
            out.len()
        );
    }

    // Build a findings subgraph from a list of (id, by, summary) tuples, applied in
    // order so each event's position increases (older first), then return the rendered
    // capped-findings section for `seed`. Mirrors `render_capped_decisions` for the
    // findings half of Gap 17.
    fn render_capped_findings(findings: &[(&str, &str, String)], seed: &[String]) -> String {
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();
        for (i, (id, by, summary)) in findings.iter().enumerate() {
            let mut e = Event::new(
                contextgraph::TYPE_REVIEW_FINDING,
                serde_json::to_vec(&json!({
                    "id": id,
                    "by": by,
                    "summary": summary,
                    "about": seed,
                }))
                .unwrap(),
            );
            e.position = (i as u64) + 1;
            graph.apply(&e).unwrap();
        }
        let g = graph.subgraph(seed, 2).unwrap();
        let mut b = String::new();
        write_capped_findings(&mut b, &g, seed);
        b
    }

    #[test]
    fn findings_prompt_injection_is_capped_under_budget_with_elision_note() {
        // Gap 17 / spec 07 line 36: findings run 4-8x larger than decisions, so an
        // unbounded pile of them ABOUT a hot file could blow the prompt on its own. The
        // findings section rides the SAME budgeted-section writer as decisions: the
        // most-recent findings stay verbatim under FINDINGS_BUDGET_BYTES, the older
        // remainder collapses into ONE visible elision note naming the count and the
        // `rigger peers <file>` recovery command. The store keeps the full history - only
        // the prompt slice narrows.
        let seed = vec!["conductor.rs".to_string()];
        // K chunky findings, oldest (f000) first; recency grows with i, so f{K-1} is newest.
        const K: usize = 300;
        let findings: Vec<(String, String, String)> = (0..K)
            .map(|i| {
                (
                    format!("f{i:03}"),
                    format!("lens{}", i % 3),
                    format!("FINDING_MARKER_{i} {}", "z".repeat(2400)),
                )
            })
            .collect();
        let borrowed: Vec<(&str, &str, String)> = findings
            .iter()
            .map(|(id, by, s)| (id.as_str(), by.as_str(), s.clone()))
            .collect();
        let out = render_capped_findings(&borrowed, &seed);

        // Uncapped, the findings section alone would be ~700KB; the cap holds it under
        // its per-section budget (plus slack for the header and the elision note line).
        assert!(
            out.len() < FINDINGS_BUDGET_BYTES + 2 * 1024,
            "capped findings section must stay under its byte budget; len={}",
            out.len()
        );
        // Newest survives verbatim; oldest is elided (it stays in the store).
        assert!(
            out.contains("FINDING_MARKER_299 "),
            "the newest finding must be kept verbatim"
        );
        assert!(
            !out.contains("FINDING_MARKER_0 "),
            "the oldest finding must be elided from the prompt (it stays in the store)"
        );
        // The trim is VISIBLE: one elision note naming the elided count and the
        // `rigger peers <file>` recovery command, exactly like the decisions section.
        let verbatim = (0..K)
            .filter(|i| out.contains(&format!("FINDING_MARKER_{i} ")))
            .count();
        assert!(
            verbatim >= 1,
            "at least one finding must still render verbatim; got {verbatim}"
        );
        let elided = K - verbatim;
        assert!(
            out.contains(&format!("+{elided} older finding")),
            "the elision note must name the elided finding count ({elided})"
        );
        assert!(
            out.contains("rigger peers conductor.rs"),
            "the elision note must name the `rigger peers <file>` recovery command"
        );
        // The finding line still names the raising reviewer (`by`) and the id, preserving
        // the prior write_findings format that a later reviewer reads.
        assert!(
            out.contains("lens2 (f299):"),
            "a finding line must name the reviewer and id; output was:\n{}",
            &out[..out.len().min(400)]
        );
    }

    #[test]
    fn the_verbatim_count_cap_binds_on_many_small_findings() {
        // sdet-u1gap17-findings-n-cap-undiscriminated: the byte-budget test above only
        // exercises the byte arm (chunky findings bind FINDINGS_BUDGET_BYTES first). With
        // many TINY findings the byte budget never binds, so this isolates the recent-N
        // arm: exactly FINDINGS_VERBATIM_N are kept verbatim, asserted against that FIXED
        // count (not a self-referencing byte threshold), so a regression ballooning
        // FINDINGS_VERBATIM_N renders all of them and fails this.
        let seed = vec!["conductor.rs".to_string()];
        let total = FINDINGS_VERBATIM_N + 10;
        let findings: Vec<(String, String, String)> = (0..total)
            .map(|i| {
                (
                    format!("f{i:03}"),
                    format!("lens{}", i % 3),
                    format!("tiny finding {i}"),
                )
            })
            .collect();
        let borrowed: Vec<(&str, &str, String)> = findings
            .iter()
            .map(|(id, by, s)| (id.as_str(), by.as_str(), s.clone()))
            .collect();
        let out = render_capped_findings(&borrowed, &seed);
        let kept = (0..total)
            .filter(|i| out.contains(&format!("(f{i:03}):")))
            .count();
        assert_eq!(
            kept, FINDINGS_VERBATIM_N,
            "exactly N tiny findings must be kept verbatim (the count cap binds, not bytes); \
             output was:\n{out}"
        );
        assert!(
            out.contains(&format!("+{} older finding", total - FINDINGS_VERBATIM_N)),
            "the elision note must name the count the N cap trims; output was:\n{out}"
        );
    }

    // Build a lessons subgraph from a list of (id, summary) tuples, applied in order so
    // each event's position increases (older first), then return the rendered
    // capped-lessons section for `seed`. Mirrors `render_capped_findings` for the lessons
    // half of Gap 17 (lessons carry no `by`, so their line is the plain `- id: summary`).
    fn render_capped_lessons(lessons: &[(&str, String)], seed: &[String]) -> String {
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();
        for (i, (id, summary)) in lessons.iter().enumerate() {
            let mut e = Event::new(
                contextgraph::TYPE_LESSON_LEARNED,
                serde_json::to_vec(&json!({
                    "id": id,
                    "summary": summary,
                    "about": seed,
                }))
                .unwrap(),
            );
            e.position = (i as u64) + 1;
            graph.apply(&e).unwrap();
        }
        let g = graph.subgraph(seed, 2).unwrap();
        let mut b = String::new();
        write_capped_lessons(&mut b, &g, seed);
        b
    }

    #[test]
    fn lessons_prompt_injection_is_capped_under_budget_with_elision_note() {
        // sdet-u1gap17-lessons-cap-render-untested / Gap 17 / spec 07 line 36: the lessons
        // half of this unit's charter had ZERO render coverage, which is exactly why a dead
        // recovery note shipped past all three tiers. This pins the whole rendered contract:
        // the header, freshest-first recency, the byte cap firing on a pile of chunky
        // lessons, and ONE visible elision note naming the elided count and the honest
        // `rigger peers <file>` recovery (now backed by sidecar::lessons_for). The store
        // keeps the full history - only the prompt slice narrows.
        let seed = vec!["conductor.rs".to_string()];
        // K chunky lessons, oldest (l000) first; recency grows with i, so l{K-1} is newest.
        const K: usize = 200;
        let lessons: Vec<(String, String)> = (0..K)
            .map(|i| {
                (
                    format!("l{i:03}"),
                    format!("LESSON_MARKER_{i} {}", "w".repeat(2400)),
                )
            })
            .collect();
        let borrowed: Vec<(&str, String)> = lessons
            .iter()
            .map(|(id, s)| (id.as_str(), s.clone()))
            .collect();
        let out = render_capped_lessons(&borrowed, &seed);

        // The section renders under its own header.
        assert!(
            out.contains("Lessons already learned about these files"),
            "the lessons section must render its header; output was:\n{}",
            &out[..out.len().min(400)]
        );
        // Uncapped, the lessons section alone would be ~500KB; the cap holds it under its
        // per-section budget (plus slack for the header and the elision note line).
        assert!(
            out.len() < LESSONS_BUDGET_BYTES + 2 * 1024,
            "capped lessons section must stay under its byte budget; len={}",
            out.len()
        );
        // Freshest-first: the newest survives verbatim, the oldest is elided (kept in store).
        assert!(
            out.contains("LESSON_MARKER_199 "),
            "the newest lesson must be kept verbatim"
        );
        assert!(
            !out.contains("LESSON_MARKER_0 "),
            "the oldest lesson must be elided from the prompt (it stays in the store)"
        );
        // The trim is VISIBLE: one elision note naming the elided count and the honest
        // `rigger peers <file>` recovery command.
        let verbatim = (0..K)
            .filter(|i| out.contains(&format!("LESSON_MARKER_{i} ")))
            .count();
        assert!(
            verbatim >= 1,
            "at least one lesson must still render verbatim; got {verbatim}"
        );
        let elided = K - verbatim;
        assert!(
            out.contains(&format!("+{elided} older lesson")),
            "the elision note must name the elided lesson count ({elided})"
        );
        assert!(
            out.contains("rigger peers conductor.rs"),
            "the elision note must name the `rigger peers <file>` recovery command \
             (backed by sidecar::lessons_for so it is not a dead promise)"
        );
    }

    #[test]
    fn the_verbatim_count_cap_binds_on_many_small_lessons() {
        // sdet-u1gap17-lessons-cap-render-untested (N arm): with many TINY lessons the byte
        // budget never binds, so this isolates the recent-N arm - exactly LESSONS_VERBATIM_N
        // are kept, asserted against that FIXED count so a regression ballooning
        // LESSONS_VERBATIM_N (bytes never bind) renders all of them and fails. Also pins the
        // freshest-first ordering: the newest lesson outranks an older one in the slice.
        let seed = vec!["conductor.rs".to_string()];
        let total = LESSONS_VERBATIM_N + 8;
        let lessons: Vec<(String, String)> = (0..total)
            .map(|i| (format!("small{i:03}"), format!("tiny lesson {i}")))
            .collect();
        let borrowed: Vec<(&str, String)> = lessons
            .iter()
            .map(|(id, s)| (id.as_str(), s.clone()))
            .collect();
        let out = render_capped_lessons(&borrowed, &seed);
        let kept = (0..total)
            .filter(|i| out.contains(&format!("small{i:03}:")))
            .count();
        assert_eq!(
            kept, LESSONS_VERBATIM_N,
            "exactly N tiny lessons must be kept verbatim (the count cap binds, not bytes); \
             output was:\n{out}"
        );
        // Freshest-first: the newest (highest index) survives, the oldest (small000) elides.
        assert!(
            out.contains(&format!("small{:03}:", total - 1)),
            "the newest lesson must survive the N cap; output was:\n{out}"
        );
        assert!(
            !out.contains("small000:"),
            "the oldest lesson must be elided by the N cap; output was:\n{out}"
        );
        assert!(
            out.contains(&format!("+{} older lesson", total - LESSONS_VERBATIM_N)),
            "the elision note must name the count the N cap trims; output was:\n{out}"
        );
    }

    #[test]
    fn resume_skips_already_integrated_units() {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        run(&cfg, &deps).unwrap(); // resume on the same store
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let starts = events
            .iter()
            .filter(|e| {
                e.type_ == ledger::TYPE_UNIT_STARTED
                    && String::from_utf8_lossy(&e.data).contains("\"id\":\"s\"")
            })
            .count();
        assert_eq!(
            starts, 1,
            "a resumed run must not restart an integrated unit"
        );
    }

    /// Append events into the run stream as if a prior window had emitted them, so a
    /// resumed `run` folds them and continues from the recorded phase.
    fn seed_events_in_run(st: &Store, criteria: &[&str], events: &[Event]) {
        // Seed `events` as the prior-window state of a run started for `criteria` (spec
        // 06, unit 1 run scoping): a leading `RunStarted` so the conductor ADOPTS this
        // run when driven with the SAME criteria and folds the seeded state as its own
        // resume state, rather than minting a fresh run that leaves the seed behind the
        // boundary. `criteria` must match the `Deps::criteria` the test drives `run` with.
        let started = Event::new(
            crate::run::TYPE_RUN_STARTED,
            serde_json::to_vec(&json!({ "run": "test-run", "criteria": criteria })).unwrap(),
        );
        st.append(
            STREAM,
            ExpectedRevision::Any,
            std::slice::from_ref(&started),
        )
        .unwrap();
        st.append(STREAM, ExpectedRevision::Any, events).unwrap();
    }

    /// Commit a file on a unit's DETERMINISTIC branch exactly as a prior window would:
    /// create the branch via a worktree, write+commit the file, then remove the
    /// transient worktree dir (the branch ref survives as the durable checkpoint).
    fn commit_on_unit_branch(repo: &str, unit_id: &str, file: &str, content: &str) {
        let branch = unit_branch(unit_id);
        let dir = std::env::temp_dir().join(format!(
            "rigger-seed-{}-{}",
            sanitize_for_path(unit_id),
            &uuid::Uuid::new_v4().to_string()[..8]
        ));
        let wt = crate::worktree::Worktree::create(repo, dir.to_str().unwrap(), &branch).unwrap();
        std::fs::write(Path::new(&wt.dir).join(file), content).unwrap();
        let committed = wt.commit("rigger: prior window work").unwrap();
        assert!(!committed.is_empty(), "the prior window must commit work");
        wt.remove().unwrap();
        assert!(
            crate::worktree::Worktree::branch_has_work(repo, &branch),
            "the seeded branch must carry committed work"
        );
    }

    #[test]
    fn resume_reuses_a_units_branch_instead_of_reimplementing() {
        // Resume-continuity: a unit recorded at `verified` in a prior window, with its
        // deterministic branch carrying committed work, must NOT re-run the
        // implementer on resume. It picks the lifecycle up at gates + the three-tier
        // review on the committed code and integrates - building on the prior window's
        // work rather than throwing it away under a per-run-uuid branch.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // The prior window implemented unit `s` and committed it on the deterministic
        // branch, reaching `verified` (gates passed) but not yet approved+merged.
        commit_on_unit_branch(&repo_path, "s", "feature.rs", "fn feature() {}\n");

        let st = Store::open(":memory:").unwrap();
        seed_events_in_run(
            &st,
            &[],
            &[
                Event::new(
                    ledger::TYPE_UNIT_STARTED,
                    serde_json::to_vec(
                        &json!({"id": "s", "agent": "worker", "branch": unit_branch("s")}),
                    )
                    .unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_STATUS,
                    serde_json::to_vec(&json!({"id": "s", "status": "green"})).unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_STATUS,
                    serde_json::to_vec(&json!({"id": "s", "status": "verified"})).unwrap(),
                ),
            ],
        );

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("judge".into(), agent("judge"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                review: crate::config::ReviewPanel {
                    lenses: vec!["lens".into()],
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        // The adjudicator approves; the unit must integrate via review, not implement.
        // The lens returns a substantive (non-degenerate) review so it does not trip the
        // Gap-18 respawn loop.
        let driver = Stub {
            output_by_agent: HashMap::from([
                ("judge".to_string(), r#"{"verdict":"approve"}"#.to_string()),
                ("lens".to_string(), "reviewed: no blocker".to_string()),
            ]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        assert!(
            !driver.spawned("worker"),
            "the implementer must NOT be re-spawned on resume - the prior window's branch is reused"
        );
        assert!(
            driver.spawned("judge"),
            "the resumed unit must still proceed through the three-tier review"
        );
        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Integrated,
            "the resumed unit must integrate, building on the reused branch"
        );
        assert!(
            repo.path().join("feature.rs").exists(),
            "the prior window's committed file must land in the base on resume-integrate"
        );
    }

    #[test]
    fn stamps_the_model_alias_and_resolved_id_on_live_lifecycle_events() {
        // spec 05 line 52, the LIVE (non-replay) path: the conductor copies the resolved
        // model each spawn returns on `AgentResult::resolved_model` - independent of the
        // driver - onto the unit events it records for that spawn, alongside the requested
        // alias. This exercises the adjudicator/`reviewed` path the replay-driver test does
        // not, and proves the copy is not replay-specific.
        let store = Store::open(":memory:").unwrap();

        let mut cfg = Config::default();
        cfg.agents.insert(
            "worker".into(),
            AgentDef {
                id: "worker".into(),
                model: "sonnet".into(),
                ..Default::default()
            },
        );
        cfg.agents.insert(
            "lens".into(),
            AgentDef {
                id: "lens".into(),
                model: "haiku".into(),
                ..Default::default()
            },
        );
        cfg.agents.insert(
            "judge".into(),
            AgentDef {
                id: "judge".into(),
                model: "opus".into(),
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "u".into(),
            Stage {
                name: "u".into(),
                agent: "worker".into(),
                // Repo-less: the review approves and then `on_pass: none` stops before
                // integrate, so no git repo is needed - `reviewed` is still emitted for the
                // adjudicator spawn.
                on_pass: "none".into(),
                review: crate::config::ReviewPanel {
                    lenses: vec!["lens".into()],
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        // Each spawn reports the concrete model it ran as (what a worker records via
        // `rigger result --meta` on the stepwise path); the adjudicator approves.
        let driver = Stub {
            output_by_agent: HashMap::from([
                ("judge".to_string(), r#"{"verdict":"approve"}"#.to_string()),
                // A substantive lens result so it does not trip the Gap-18 respawn loop.
                ("lens".to_string(), "reviewed: no blocker".to_string()),
            ]),
            resolved_model_by_agent: HashMap::from([
                (
                    "worker".to_string(),
                    "claude-sonnet-4-5-20250101".to_string(),
                ),
                ("judge".to_string(), "claude-opus-4-8-20260101".to_string()),
            ]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let status_meta = |want: &str, key: &str| -> Option<String> {
            events
                .iter()
                .find(|e| {
                    e.type_ == ledger::TYPE_UNIT_STATUS
                        && String::from_utf8_lossy(&e.data)
                            .contains(&format!("\"status\":\"{want}\""))
                })
                .and_then(|e| e.meta.get(key).cloned())
        };

        // The implementer spawn's green and verified events carry the worker's requested
        // alias and the resolved id it ran as.
        for st in ["green", "verified"] {
            assert_eq!(
                status_meta(st, META_MODEL_ALIAS).as_deref(),
                Some("sonnet"),
                "{st} carries the implementer's requested alias"
            );
            assert_eq!(
                status_meta(st, META_MODEL_RESOLVED).as_deref(),
                Some("claude-sonnet-4-5-20250101"),
                "{st} carries the implementer's resolved id"
            );
        }
        // The adjudicator spawn's reviewed event carries the judge's alias and resolved id.
        assert_eq!(
            status_meta("reviewed", META_MODEL_ALIAS).as_deref(),
            Some("opus"),
            "reviewed carries the adjudicator's requested alias"
        );
        assert_eq!(
            status_meta("reviewed", META_MODEL_RESOLVED).as_deref(),
            Some("claude-opus-4-8-20260101"),
            "reviewed carries the adjudicator's resolved id"
        );
        // UnitStarted carries the requested alias (known before any result).
        let started = events
            .iter()
            .find(|e| e.type_ == ledger::TYPE_UNIT_STARTED)
            .expect("the unit started");
        assert_eq!(
            started.meta.get(META_MODEL_ALIAS).map(String::as_str),
            Some("sonnet")
        );
    }

    /// Build a repo-less single stage with a lens + adjudicator review panel and
    /// `on_pass: none` (so the approved review folds and emits `reviewed` without needing
    /// a git repo to integrate into). The Gap-18 tests below drive its reviewers to
    /// degenerate (empty) results and assert the respawn/halt behavior.
    fn degenerate_reviewer_cfg() -> Config {
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("sdet".into(), agent("sdet"));
        cfg.agents.insert("judge".into(), agent("judge"));
        cfg.workflow.stages.insert(
            "u".into(),
            Stage {
                name: "u".into(),
                agent: "worker".into(),
                on_pass: "none".into(),
                review: crate::config::ReviewPanel {
                    lenses: vec!["sdet".into()],
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        cfg
    }

    fn has_status(events: &[Event], status: &str) -> bool {
        events.iter().any(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(&format!("\"status\":\"{status}\""))
        })
    }

    #[test]
    fn a_degenerate_adjudicator_result_respawns_and_a_substantive_retry_folds_normally() {
        // Gap 18 (spec 07): a reviewer result that is empty/whitespace-only is an
        // INFRASTRUCTURE fault, not a verdict. The adjudicator's ORIGINAL spawn returns
        // whitespace-only; the conductor respawns it under the deterministic
        // `~retry1` id, and that substantive (approving) result folds NORMALLY.
        let store = Store::open(":memory:").unwrap();
        let cfg = degenerate_reviewer_cfg();
        let driver = Stub {
            output_by_spawn_id: HashMap::from([
                // Original adjudicator spawn: whitespace only -> degenerate, respawned.
                (spawn_id("u", ROLE_ADJUDICATOR, 0), "   \n\t  ".to_string()),
                // First respawn: a real approving verdict -> folds normally.
                (
                    spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 1),
                    r#"{"verdict":"approve"}"#.to_string(),
                ),
            ]),
            // The lens returns a substantive review so only the ADJUDICATOR exercises the
            // degenerate-respawn path under test.
            output_by_agent: HashMap::from([("sdet".to_string(), "lens: no blocker".to_string())]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).expect("a substantive retry folds the review normally");

        // The adjudicator was spawned exactly twice: the degenerate original + one retry.
        assert_eq!(
            driver.spawn_count("judge"),
            2,
            "the degenerate adjudicator is respawned exactly once before it returns a verdict"
        );
        // The respawn used the DETERMINISTIC retry-suffixed id, not a fresh/random one.
        let ids = driver.spawn_ids();
        assert!(
            ids.contains(&spawn_id("u", ROLE_ADJUDICATOR, 0)),
            "the original spawn keeps its plain id: {ids:?}"
        );
        assert!(
            ids.contains(&spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 1)),
            "the respawn uses the deterministic ~retry1 id: {ids:?}"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        // The substantive verdict folded: `reviewed` is emitted ONLY on an explicit approve.
        assert!(
            has_status(&events, "reviewed"),
            "the approving retry result folds into the review outcome (reviewed emitted)"
        );
        // The unit was NOT charged a remediation attempt: no UnitFailed, no UnitEscalated.
        assert!(
            !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
            "a degenerate-then-recovered review must not charge the unit an attempt"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_ESCALATED),
            "a degenerate-then-recovered review must not escalate the unit"
        );
        // The implementer ran exactly once - no remediation re-implement.
        assert_eq!(driver.spawn_count("worker"), 1);
    }

    #[test]
    fn a_degenerate_lens_result_respawns_the_lens_before_the_review_proceeds() {
        // The respawn loop wraps ALL THREE reviewer roles: a degenerate tier-1 LENS is
        // respawned the same way an adjudicator is (a shared helper, one authority), so a
        // flaky lens spawn does not corrupt the review.
        let store = Store::open(":memory:").unwrap();
        let cfg = degenerate_reviewer_cfg();
        let driver = Stub {
            output_by_spawn_id: HashMap::from([
                // Original lens spawn: empty stdout AND (no emits_by_agent set) no
                // ReviewFinding -> the conductor OBSERVED it produce nothing in-process, so
                // it is degenerate and respawned.
                (spawn_id("u", &lens_role("sdet"), 0), String::new()),
                // First lens respawn: a substantive review (its output is folded via
                // the graph, not a verdict, so any non-empty text lets the review proceed).
                (
                    spawn_retry_id("u", &lens_role("sdet"), 0, 1),
                    "reviewed the diff; no blocking defect".to_string(),
                ),
            ]),
            output_by_agent: HashMap::from([(
                "judge".to_string(),
                r#"{"verdict":"approve"}"#.to_string(),
            )]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).expect("the review proceeds once the degenerate lens recovers");

        assert_eq!(
            driver.spawn_count("sdet"),
            2,
            "the degenerate lens is respawned exactly once"
        );
        assert!(
            driver
                .spawn_ids()
                .contains(&spawn_retry_id("u", &lens_role("sdet"), 0, 1)),
            "the lens respawn uses the deterministic ~retry1 id"
        );
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(has_status(&events, "reviewed"), "the review still approves");
        assert!(
            !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
            "a degenerate lens must not charge the unit an attempt"
        );
    }

    #[test]
    fn a_lens_that_emitted_a_finding_but_reports_empty_stdout_is_not_degenerate() {
        // Fix 1 for adj-u2gap18 / adv-u2gap18-empty-success-is-a-valid-outcome-misread-as-
        // degenerate: a lens's stdout is NOT its verdict - it emits findings to the graph -
        // so an EMPTY stdout is the normal outcome, NOT degeneracy, when the lens emitted a
        // ReviewFinding. The lens here emits one ReviewFinding and reports empty stdout; it
        // must fold on its FIRST spawn (no respawn), and the review must proceed.
        let store = Store::open(":memory:").unwrap();
        let cfg = degenerate_reviewer_cfg();
        let driver = Stub {
            // The lens does its real work through the graph (a ReviewFinding) and returns
            // an EMPTY stdout - exactly the healthy shape the prior version misread.
            emits_by_agent: HashMap::from([(
                "sdet".to_string(),
                vec![(
                    contextgraph::TYPE_REVIEW_FINDING.to_string(),
                    json!({"id": "f-sdet-1", "summary": "a real concern", "about": ["u"]}),
                )],
            )]),
            output_by_agent: HashMap::from([
                // sdet's stdout is deliberately empty (its finding is the work).
                ("sdet".to_string(), String::new()),
                ("judge".to_string(), r#"{"verdict":"approve"}"#.to_string()),
            ]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).expect("a finding-emitting lens with empty stdout is not degenerate");

        // The lens was spawned EXACTLY ONCE - its empty stdout was not misread as degenerate
        // (no respawn), because it emitted a ReviewFinding.
        assert_eq!(
            driver.spawn_count("sdet"),
            1,
            "a lens that emitted a ReviewFinding is not degenerate on an empty stdout"
        );
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        // Its finding really did reach the log (the graph is the channel), and the review
        // approved normally.
        assert!(
            events
                .iter()
                .any(|e| e.type_ == contextgraph::TYPE_REVIEW_FINDING),
            "the lens's ReviewFinding is recorded"
        );
        assert!(
            has_status(&events, "reviewed"),
            "the review approves normally"
        );
        assert!(
            !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
            "a healthy finding-emitting lens charges no attempt"
        );
    }

    #[test]
    fn a_reviewer_that_only_ever_returns_degenerate_output_halts_the_run_loudly_naming_it() {
        // Gap 18: when the respawn bound (two respawns) exhausts with only degenerate
        // results, the unit does NOT lose the attempt - the run halts LOUDLY, naming the
        // dead reviewer. It is an operator infrastructure problem, NOT code for
        // remediation, so it propagates as an Error out of `run` (no UnitFailed/escalate).
        let store = Store::open(":memory:").unwrap();
        let cfg = degenerate_reviewer_cfg();
        // The adjudicator returns whitespace-only on EVERY spawn, including both
        // respawns; the lens returns a substantive review so the halt is provably the
        // adjudicator's, not the lens's.
        let driver = Stub {
            output_by_agent: HashMap::from([
                ("judge".to_string(), "  \n ".to_string()),
                ("sdet".to_string(), "lens: no blocker".to_string()),
            ]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let err = match run(&cfg, &deps) {
            Ok(_) => panic!("an all-degenerate reviewer must halt the run"),
            Err(e) => e,
        };
        // The halt NAMES the dead reviewer (its agent id, the tier, and the unit) so the
        // operator can see WHICH spawn is failing.
        assert!(
            err.0.contains("\"judge\"") && err.0.contains("adjudicator") && err.0.contains("\"u\""),
            "the loud halt must name the dead reviewer, its tier, and the unit: {}",
            err.0
        );
        // The surfaced halt message is CLEAN - the internal recognition marker is stripped
        // by run_wave before it reaches the operator.
        assert!(
            !err.0.contains(DEGENERATE_MARKER),
            "the operator-facing halt must not carry the internal sentinel marker: {:?}",
            err.0
        );
        // It names the REAL recovery (re-record a substantive result; last-write-wins), not
        // the dead "just re-run" promise the adjudicator rejected at adj-u2gap18.
        assert!(
            err.0.contains("rigger result") && err.0.contains("last-write-wins"),
            "the halt must name the working recovery (a corrected re-record), not a bare re-run: {}",
            err.0
        );

        // The adjudicator was spawned exactly THREE times: the original + two respawns.
        assert_eq!(
            driver.spawn_count("judge"),
            3,
            "the respawn bound is two: original + 2 respawns, then halt"
        );
        // Each respawn used the deterministic retry-suffixed id.
        let ids = driver.spawn_ids();
        for id in [
            spawn_id("u", ROLE_ADJUDICATOR, 0),
            spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 1),
            spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 2),
        ] {
            assert!(
                ids.contains(&id),
                "expected deterministic spawn id {id}: {ids:?}"
            );
        }

        // The unit was NOT charged an attempt: an infrastructure halt records no
        // UnitFailed and no UnitEscalated - it is the operator's problem, not the unit's.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
            "a degenerate-reviewer halt must not charge the unit an attempt (no UnitFailed)"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_ESCALATED),
            "a degenerate-reviewer halt is not a code defect for remediation (no UnitEscalated)"
        );
        // And it emits NO per-unit lesson: routing the halt through run_wave's dedicated
        // degenerate arm (not the generic wave-failure arm) keeps it from misattributing
        // the operator's broken reviewer to the unit under review as a LESSON_LEARNED
        // (fix for adv-u2gap18-halt-lesson-misattribution / sdet-u2-halt-emits-unit-lesson-
        // misattribution).
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED),
            "a degenerate-reviewer halt must emit no per-unit lesson (no misattribution)"
        );
        // The implementer ran exactly once - the halt did not restart the unit lifecycle.
        assert_eq!(driver.spawn_count("worker"), 1);
    }

    #[test]
    fn resume_integrates_an_already_approved_unit_without_re_reviewing() {
        // Resume-continuity: a unit whose log shows review APPROVED (`reviewed`) but no
        // UnitIntegrated - the merge was interrupted - must integrate on resume with NO
        // lens/adversary/adjudicator spawns. The verdict was already settled; re-review
        // would re-litigate it and re-spend the budget.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        commit_on_unit_branch(&repo_path, "s", "feature.rs", "fn feature() {}\n");

        let st = Store::open(":memory:").unwrap();
        seed_events_in_run(
            &st,
            &[],
            &[
                Event::new(
                    ledger::TYPE_UNIT_STARTED,
                    serde_json::to_vec(
                        &json!({"id": "s", "agent": "worker", "branch": unit_branch("s")}),
                    )
                    .unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_STATUS,
                    serde_json::to_vec(&json!({"id": "s", "status": "verified"})).unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_STATUS,
                    serde_json::to_vec(&json!({"id": "s", "status": "reviewed"})).unwrap(),
                ),
            ],
        );

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("judge".into(), agent("judge"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                review: crate::config::ReviewPanel {
                    lenses: vec!["lens".into()],
                    adversary: "adversary".into(),
                    adjudicator: "judge".into(),
                },
                ..Default::default()
            },
        );

        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        assert!(
            !driver.spawned("worker"),
            "an approved unit must not re-implement on resume"
        );
        assert!(
            !driver.spawned("lens") && !driver.spawned("adversary") && !driver.spawned("judge"),
            "an already-approved unit must integrate with NO review spawns on resume"
        );
        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Integrated,
            "the approved-but-uninterrupted-merge unit must integrate on resume"
        );
        assert!(
            repo.path().join("feature.rs").exists(),
            "the approved work must land in the base"
        );
    }

    #[test]
    fn a_failed_unit_is_not_terminal_and_resumes() {
        // A unit that FAILED a review/gate in a prior window but did NOT yet escalate
        // (attempts:1 of MAX_RETRIES) is mid-remediation, not terminal. A resumed run
        // must NOT skip it - it re-runs the unit, reusing its deterministic branch (the
        // rejected code persists) and continuing its remediation. Here the resumed
        // attempt is approved, so the unit finishes; the point is it RAN at all rather
        // than being seeded into `terminal` and skipped forever.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // The prior window implemented unit `s`, committed it on the deterministic
        // branch, but its review/gate FAILED once (UnitFailed attempts:1) - it is
        // mid-remediation, not escalated.
        commit_on_unit_branch(&repo_path, "s", "feature.rs", "fn feature() {}\n");

        let st = Store::open(":memory:").unwrap();
        seed_events_in_run(
            &st,
            &[],
            &[
                Event::new(
                    ledger::TYPE_UNIT_STARTED,
                    serde_json::to_vec(
                        &json!({"id": "s", "agent": "worker", "branch": unit_branch("s")}),
                    )
                    .unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_FAILED,
                    serde_json::to_vec(&json!({"id": "s", "attempts": 1})).unwrap(),
                ),
            ],
        );

        // The folded prior state proves the unit is NOT terminal (the regression this
        // fix closes): a Failed-but-not-escalated unit must be eligible to resume.
        let prior = ledger::project(
            &st.read_all(0, Direction::Forward, &Filter::default())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(prior.units["s"].status, ledger::Status::Failed);
        assert_eq!(prior.units["s"].attempts, 1);
        assert!(
            !prior.is_terminal("s"),
            "a Failed-but-not-escalated unit must not be terminal - else resume skips it forever"
        );

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("judge".into(), agent("judge"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                review: crate::config::ReviewPanel {
                    lenses: vec!["lens".into()],
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        // The resumed attempt's review is approved, so the unit integrates this window.
        // The lens returns a substantive review so it does not trip the Gap-18 respawn loop.
        let driver = Stub {
            write_file: Some("feature.rs".into()),
            output_by_agent: HashMap::from([
                ("judge".to_string(), r#"{"verdict":"approve"}"#.to_string()),
                ("lens".to_string(), "reviewed: no blocker".to_string()),
            ]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        // The unit RAN on resume - it was not skipped as terminal. A Failed unit's
        // recorded status falls to the Fresh resume-phase, so it re-implements (the
        // worker spawns) to address the recorded findings, reusing its branch.
        assert!(
            driver.spawned("worker"),
            "a Failed-but-not-escalated unit must resume and re-run, not be skipped as terminal"
        );
        assert!(
            crate::worktree::Worktree::branch_has_work(&deps.repo, &unit_branch("s"))
                || repo.path().join("feature.rs").exists(),
            "the resumed unit reuses its deterministic branch - the prior rejected code persists"
        );
        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Integrated,
            "the resumed unit must be able to finish its remediation, not stay stuck"
        );
        assert!(
            repo.path().join("feature.rs").exists(),
            "the prior window's committed file (reused from the branch) lands on integrate"
        );
    }

    #[test]
    fn remediation_attempts_accumulate_across_resume_and_escalate_at_the_bound() {
        // Attempts ACCUMULATE across windows: a unit that already failed twice in a
        // prior window (UnitFailed attempts:2) resumes, makes EXACTLY ONE more attempt
        // (the 3rd), and ESCALATES at MAX_RETRIES TOTAL - it does NOT do a fresh
        // MAX_RETRIES on top of the prior 2. The attempt counter is threaded through
        // resume so the bound counts across windows, not per-window forever.
        assert_eq!(
            safety::MAX_RETRIES,
            3,
            "this test assumes the default bound of 3 (2 prior + 1 resumed = escalate)"
        );

        let st = Store::open(":memory:").unwrap();
        // Prior window: the unit already failed twice (attempts:2), one short of the
        // bound. The next failure must escalate.
        seed_events_in_run(
            &st,
            &[],
            &[
                Event::new(
                    ledger::TYPE_UNIT_STARTED,
                    serde_json::to_vec(&json!({"id": "implement", "agent": "worker"})).unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_FAILED,
                    serde_json::to_vec(&json!({"id": "implement", "attempts": 2})).unwrap(),
                ),
            ],
        );

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true")); // static gates pass
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );

        // The adjudicator always rejects, so the resumed attempt can only fail again -
        // and with attempts already at 2, that next failure must escalate.
        let driver = Stub {
            output: r#"{"verdict":"reject","issues":[]}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        // The unit escalated this window (not churned, not skipped).
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Escalated,
            "with 2 prior attempts, the resumed unit must escalate on its next failure"
        );

        // EXACTLY ONE more attempt ran this window: one worker spawn, NOT a fresh
        // MAX_RETRIES (3) on top of the prior 2. The attempt counter resumed at 2.
        let order = driver.call_order.lock().unwrap().clone();
        let worker_spawns = order.iter().filter(|a| *a == "worker").count();
        assert_eq!(
            worker_spawns, 1,
            "the resumed unit must do ONLY the final (3rd) attempt, not a fresh batch; spawns were {order:?}"
        );

        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        // Exactly one UnitEscalated, and it is final.
        let escalations = events
            .iter()
            .filter(|e| e.type_ == ledger::TYPE_UNIT_ESCALATED)
            .count();
        assert_eq!(
            escalations, 1,
            "the unit escalates exactly once at the bound"
        );
        // The escalation lesson records the attempt count at MAX_RETRIES (3 total),
        // not 5 (2 prior + a fresh 3): the bound counted across windows.
        let escalated_at_bound = events.iter().any(|e| {
            e.type_ == contextgraph::TYPE_LESSON_LEARNED
                && String::from_utf8_lossy(&e.data)
                    .contains(&format!("escalated after {} attempts", safety::MAX_RETRIES))
        });
        assert!(
            escalated_at_bound,
            "escalation must record MAX_RETRIES total attempts, proving the count accumulated across the resume"
        );
        // The final folded attempt count is the bound, not the bound-plus-the-prior.
        assert_eq!(
            rs.units["implement"].attempts,
            safety::MAX_RETRIES,
            "the final attempts must reach the bound exactly - the prior count carried over, it did not reset"
        );
    }

    #[test]
    fn an_escalated_unit_stays_terminal_on_resume() {
        // Escalation is FINAL: a unit that genuinely reached MAX_RETRIES and escalated
        // stays terminal and is skipped on resume - no re-run, no fresh attempts.
        let st = Store::open(":memory:").unwrap();
        seed_events_in_run(
            &st,
            &[],
            &[
                Event::new(
                    ledger::TYPE_UNIT_STARTED,
                    serde_json::to_vec(&json!({"id": "s", "agent": "worker"})).unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_FAILED,
                    serde_json::to_vec(&json!({"id": "s", "attempts": 3})).unwrap(),
                ),
                Event::new(
                    ledger::TYPE_UNIT_ESCALATED,
                    serde_json::to_vec(&json!({"id": "s"})).unwrap(),
                ),
            ],
        );

        // The prior state confirms the escalated unit IS terminal.
        let prior = ledger::project(
            &st.read_all(0, Direction::Forward, &Filter::default())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(prior.units["s"].status, ledger::Status::Escalated);
        assert!(
            prior.is_terminal("s"),
            "an escalated unit must be terminal - escalation is final"
        );

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );

        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        assert!(
            !driver.spawned("worker"),
            "an escalated unit must be skipped on resume - escalation is final, no re-run"
        );
        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Escalated,
            "an escalated unit stays escalated across resume"
        );
        // No second UnitStarted for the skipped unit.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let starts = events
            .iter()
            .filter(|e| {
                e.type_ == ledger::TYPE_UNIT_STARTED
                    && String::from_utf8_lossy(&e.data).contains("\"id\":\"s\"")
            })
            .count();
        assert_eq!(
            starts, 1,
            "the escalated unit must not be restarted on resume"
        );
    }

    #[test]
    fn a_fresh_unit_with_no_branch_runs_the_full_lifecycle() {
        // The no-prior-branch path is unchanged: a unit with no deterministic branch
        // (a first run) implements, gates, reviews, and integrates - the implementer
        // and the adjudicator both spawn.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        let st = Store::open(":memory:").unwrap();
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("judge".into(), agent("judge"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                review: crate::config::ReviewPanel {
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let driver = Stub {
            write_file: Some("feature.rs".into()),
            output_by_agent: HashMap::from([(
                "judge".to_string(),
                r#"{"verdict":"approve"}"#.to_string(),
            )]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        assert!(
            driver.spawned("worker"),
            "a fresh unit with no prior branch must run the implementer"
        );
        assert!(
            driver.spawned("judge"),
            "a fresh unit must run the three-tier review"
        );
        assert_eq!(rs.units["s"].status, ledger::Status::Integrated);
    }

    #[test]
    fn agent_decision_creates_a_decided_edge() {
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            emits: vec![(
                contextgraph::TYPE_DECISION_MADE.to_string(),
                json!({"id": "d1", "summary": "x", "governs": ["f.rs"]}),
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: Some(&graph),
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let g = graph.subgraph(&["d1".to_string()], 2).unwrap();
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == contextgraph::REL_DECIDED && e.from == "a" && e.to == "d1"),
            "the acting agent 'a' must DECIDE d1 (actor stamped on the emit)"
        );
    }

    #[test]
    fn scope_creep_refuses_a_criterionless_proposed_unit() {
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                coverage: "crit".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            emits: vec![(
                TYPE_UNIT_PROPOSED.to_string(),
                json!({"id": "impl", "agent": "worker"}), // no coverage
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec!["crit".into()],
        };
        let rs = run(&cfg, &deps).unwrap();
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_SCOPE_CREEP),
            "a criterion-less proposed unit must be refused as scope creep"
        );
        assert!(
            !rs.units.contains_key("impl"),
            "the refused unit must not be added to the run"
        );
    }

    #[test]
    fn adjudicator_reject_blocks_the_stage() {
        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lens".into()],
                adjudicator: "adj".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"reject","issues":[]}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["review"].status,
            ledger::Status::Escalated,
            "a rejecting adjudicator must block integration even with no static gates"
        );
    }

    #[test]
    fn adversary_runs_between_the_lenses_and_the_adjudicator() {
        // Three-tier review order (§3.2): a stage with `agents` + `adversary` +
        // `adjudicator` must spawn the lenses FIRST (in parallel), THEN the adversary
        // (which reviews the lenses' findings), THEN the adjudicator (the neutral
        // judge). The Stub records every spawn's agent id in order; we assert the
        // adversary lands after every lens and before the adjudicator.
        let mut cfg = Config::default();
        cfg.agents.insert("lensA".into(), agent("lensA"));
        cfg.agents.insert("lensB".into(), agent("lensB"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents
            .insert("adjudicator".into(), agent("adjudicator"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lensA".into(), "lensB".into()],
                adversary: "adversary".into(),
                adjudicator: "adjudicator".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // The adjudicator is now fail-closed (item 2): approval requires an explicit
        // {"verdict":"approve"}. The shared Stub returns it for every spawn; only the
        // adjudicator's output is run through verdict_approves.
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["review"].status,
            ledger::Status::Integrated,
            "an approving three-tier review must integrate"
        );
        let order = driver.call_order.lock().unwrap().clone();
        let adv = order
            .iter()
            .position(|a| a == "adversary")
            .expect("the adversary must be spawned");
        let adj = order
            .iter()
            .position(|a| a == "adjudicator")
            .expect("the adjudicator must be spawned");
        let last_lens = order
            .iter()
            .rposition(|a| a == "lensA" || a == "lensB")
            .expect("the lenses must be spawned");
        assert!(
            last_lens < adv,
            "the adversary must run AFTER every lens (it reviews their findings); order was {order:?}"
        );
        assert!(
            adv < adj,
            "the adversary must run BEFORE the adjudicator (the adjudicator judges last); order was {order:?}"
        );
    }

    #[test]
    fn adjudicator_reject_gates_even_with_an_adversary_present() {
        // The adjudicator's verdict still gates the three-tier flow (§3.2): with an
        // adversary present and the adjudicator returning {"verdict":"reject"},
        // integration is blocked even though the adversary ran and there are no
        // static gates. (The shared Stub output is "reject" for every spawn, but only
        // the adjudicator's output is run through verdict_approves.)
        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents
            .insert("adjudicator".into(), agent("adjudicator"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lens".into()],
                adversary: "adversary".into(),
                adjudicator: "adjudicator".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"reject","issues":[]}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["review"].status,
            ledger::Status::Escalated,
            "a rejecting adjudicator must block integration even though the adversary ran"
        );
        assert!(
            driver
                .call_order
                .lock()
                .unwrap()
                .iter()
                .any(|a| a == "adversary"),
            "the adversary must have run before the adjudicator's gating verdict"
        );
    }

    #[test]
    fn unit_reviews_itself_within_its_own_lifecycle() {
        // The per-unit lifecycle (§3.2): an `agent` stage with a `defaults.review`
        // panel runs implement -> the unit's gates -> three-tier review OF THIS UNIT
        // (lenses -> adversary -> adjudicator) -> integrate, all inside ONE stage. The
        // unit must reach Integrated, and the implementer, both lenses, the adversary,
        // and the adjudicator must all have run on it - in that order.
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lensA".into(), agent("lensA"));
        cfg.agents.insert("lensB".into(), agent("lensB"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        // The review panel is declared once on defaults and applied to the unit.
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lensA".into(), "lensB".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // Fail-closed adjudicator (item 2): approval needs an explicit verdict.
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Integrated,
            "a unit that implements, passes review, and gates green must integrate"
        );
        // The implementer ran, then both lenses, then the adversary, then the
        // adjudicator - all on this one unit, inside its own lifecycle.
        let order = driver.call_order.lock().unwrap().clone();
        let worker = order
            .iter()
            .position(|a| a == "worker")
            .expect("the implementer must run");
        let last_lens = order
            .iter()
            .rposition(|a| a == "lensA" || a == "lensB")
            .expect("the lenses must run on the unit");
        let adv = order
            .iter()
            .position(|a| a == "adversary")
            .expect("the adversary must run on the unit");
        let adj = order
            .iter()
            .position(|a| a == "adj")
            .expect("the adjudicator must run on the unit");
        assert!(
            worker < last_lens,
            "the implementer runs before its own review; order was {order:?}"
        );
        assert!(
            last_lens < adv,
            "the adversary runs AFTER every lens; order was {order:?}"
        );
        assert!(
            adv < adj,
            "the adjudicator runs LAST (its verdict gates); order was {order:?}"
        );
        // The unit was marked `reviewed` after the adjudicator approved.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| {
                e.type_ == ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"reviewed\"")
            }),
            "an approved unit must be marked reviewed before it integrates"
        );
    }

    #[test]
    fn every_spawn_runs_in_a_worktree_never_the_main_repo_checkout() {
        // The worktree-isolation invariant (the headline fix): with a repo configured,
        // EVERY spawned agent in a unit's lifecycle - the implementer AND all three
        // review tiers - must run with its cwd (SpawnOpts.dir) set to the unit's
        // throwaway worktree, NEVER empty (which inherits the driver's cwd = the live
        // main checkout) and NEVER the repo root itself. This is what stops the
        // implementer's edits / remediation fixes - and any reviewer's stray Bash/Edit -
        // from landing in the checkout the gates and review never inspect.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lensA".into(), agent("lensA"));
        cfg.agents.insert("lensB".into(), agent("lensB"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lensA".into(), "lensB".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        // The canonical repo root, so a worktree dir under it is still rejected if it
        // resolves to the root (it never should - worktrees live in temp_dir()).
        let canon_repo = std::fs::canonicalize(&repo_path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or(repo_path.clone());
        for agent_id in ["worker", "lensA", "lensB", "adversary", "adj"] {
            let dirs = driver.dirs_for(agent_id);
            assert!(
                !dirs.is_empty(),
                "{agent_id} must have been spawned at least once"
            );
            for dir in &dirs {
                assert!(
                    !dir.is_empty(),
                    "{agent_id} ran with an EMPTY cwd - it would inherit the driver's \
                     cwd = the main repo checkout (the isolation bug)"
                );
                assert!(
                    !same_path(dir, &repo_path) && !same_path(dir, &canon_repo),
                    "{agent_id} ran directly in the main repo checkout ({dir:?}); \
                     it must run in its worktree"
                );
                // The cwd is a rigger worktree under the temp dir, not the repo tree.
                let name = std::path::Path::new(dir)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                assert!(
                    name.starts_with("rigger-wt-") || name.starts_with("rigger-review-"),
                    "{agent_id}'s cwd must be a rigger worktree, got {dir:?}"
                );
            }
        }
    }

    #[test]
    fn assert_isolated_cwd_refuses_empty_or_repo_root_with_a_repo() {
        // The guard that makes "run in the main repo" structurally impossible: with a
        // repo configured, an empty cwd (inherits the driver's cwd = the repo) and the
        // repo root itself are both refused; only a distinct worktree dir is allowed. A
        // repo-less run has no checkout to protect, so an empty cwd is fine there.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let cfg = Config::default();

        // With a repo set.
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let c = RunCtx::for_test(&cfg, &deps);
        assert!(
            c.assert_isolated_cwd("implementer", "x", "").is_err(),
            "an empty cwd with a repo configured must be refused (it is the main checkout)"
        );
        assert!(
            c.assert_isolated_cwd("implementer", "x", &repo_path)
                .is_err(),
            "the repo root itself must be refused as a cwd"
        );
        assert!(
            c.assert_isolated_cwd("lens", "x", "/tmp/rigger-wt-unit-abcd1234")
                .is_ok(),
            "a distinct worktree dir is the only allowed cwd"
        );

        // Repo-less: no checkout to protect, so an empty cwd is allowed.
        let st2 = Store::open(":memory:").unwrap();
        let driver2 = Stub::new();
        let deps2 = Deps {
            store: &st2,
            driver: &driver2,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let c2 = RunCtx::for_test(&cfg, &deps2);
        assert!(
            c2.assert_isolated_cwd("implementer", "x", "").is_ok(),
            "a repo-less run has no main checkout to protect; an empty cwd is fine"
        );
    }

    #[test]
    fn the_conductor_threads_each_agents_persona_to_the_driver() {
        // The agent's PERSONA (its role - the markdown body of its definition) must be
        // threaded to the driver as the system prompt on EVERY spawn path, not just the
        // implementer: the rust-engineer, the lenses, the adversary, AND the adjudicator
        // each receive THEIR OWN role. This is the single persona source both drivers
        // consume, so a workflow agent gets its role exactly as the cli path does.
        let mut cfg = Config::default();
        cfg.agents.insert(
            "worker".into(),
            agent_with_prompt("worker", "You are the rust engineer. Implement findings."),
        );
        cfg.agents.insert(
            "lensA".into(),
            agent_with_prompt("lensA", "You are the architecture lens."),
        );
        cfg.agents.insert(
            "lensB".into(),
            agent_with_prompt("lensB", "You are the technical lens."),
        );
        cfg.agents.insert(
            "adversary".into(),
            agent_with_prompt(
                "adversary",
                "You are the adversary. Prove the lenses wrong.",
            ),
        );
        cfg.agents.insert(
            "adj".into(),
            agent_with_prompt("adj", "You are the adjudicator. Render the verdict."),
        );
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lensA".into(), "lensB".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        // Every agent the conductor spawned received a system prompt that contains
        // BOTH its OWN persona AND the rigger-authored communication discipline - the
        // implementer and all three review tiers. The system prompt is persona +
        // RIGGER_COMMUNICATION, built once for both driver paths.
        for (id, persona) in [
            ("worker", "You are the rust engineer. Implement findings."),
            ("lensA", "You are the architecture lens."),
            ("lensB", "You are the technical lens."),
            (
                "adversary",
                "You are the adversary. Prove the lenses wrong.",
            ),
            ("adj", "You are the adjudicator. Render the verdict."),
        ] {
            let sys = driver
                .system_prompt_for(id)
                .unwrap_or_else(|| panic!("agent {id:?} was never spawned"));
            assert!(
                sys.contains(persona),
                "agent {id:?} must be spawned with its own persona in the system prompt; got: {sys:?}"
            );
            // The exact composition: persona, then the rigger discipline.
            assert_eq!(
                sys,
                build_system_prompt(persona),
                "agent {id:?}'s system prompt must be exactly persona + RIGGER_COMMUNICATION"
            );
        }
    }

    #[test]
    fn the_system_prompt_carries_the_rigger_communication_discipline() {
        // The conductor threads persona + RIGGER_COMMUNICATION as the system prompt:
        // every spawned agent receives BOTH its persona AND the rigger-authored
        // communication discipline (emit decisions/findings live, check peers, never
        // silently contradict a peer). Assert the discipline's key phrases reach the
        // agent's system prompt alongside its persona.
        let mut cfg = Config::default();
        cfg.agents.insert(
            "a".into(),
            agent_with_prompt("a", "You are the implementer persona."),
        );
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        let sys = driver.system_prompt_for("a").expect("agent a was spawned");
        // The persona is present.
        assert!(
            sys.contains("You are the implementer persona."),
            "the system prompt must carry the persona; got: {sys:?}"
        );
        // The rigger communication discipline's key phrases are present.
        for phrase in [
            "Rigger communication discipline",
            "DecisionMade",
            "ReviewFinding",
            "rigger_peers",
            "SUPERSEDE it explicitly",
        ] {
            assert!(
                sys.contains(phrase),
                "the system prompt must carry the discipline phrase {phrase:?}; got: {sys:?}"
            );
        }
    }

    #[test]
    fn an_agent_with_no_persona_threads_an_empty_system_prompt() {
        // An agent that declares no body threads an empty system prompt - the persona
        // source is the agent's prompt, which is empty here, so nothing is fabricated.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a")); // no prompt body
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        // An agent with no persona body still receives the rigger communication
        // discipline (every agent gets it), but no fabricated persona text - the
        // system prompt is exactly the empty persona + RIGGER_COMMUNICATION.
        let sys = driver.system_prompt_for("a").expect("agent a was spawned");
        assert_eq!(
            sys,
            build_system_prompt(""),
            "an agent with no body threads (empty persona) + RIGGER_COMMUNICATION, nothing fabricated"
        );
        assert!(
            sys.contains("Rigger communication discipline"),
            "even a persona-less agent receives the communication discipline"
        );
    }

    #[test]
    fn planner_proposed_unit_inherits_the_default_review_panel() {
        // A planner-proposed unit runs through `run_single_stage`, so it inherits the
        // per-unit three-tier review from `defaults.review` automatically (§3.2): the
        // harvested unit must be reviewed by the panel and only then integrate.
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: String::new(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // Fail-closed adjudicator (item 2): the proposed unit's review needs an
        // explicit approve verdict to integrate.
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            emits: vec![(
                TYPE_UNIT_PROPOSED.to_string(),
                json!({
                    "id": "impl-unit",
                    "agent": "worker",
                    "needs": ["plan"],
                    "gates": ["ok"],
                }),
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["impl-unit"].status,
            ledger::Status::Integrated,
            "a proposed unit must review itself via defaults.review, then integrate"
        );
        // The default panel's lens and adjudicator both ran on the proposed unit.
        let order = driver.call_order.lock().unwrap().clone();
        assert!(
            order.iter().any(|a| a == "lens") && order.iter().any(|a| a == "adj"),
            "the proposed unit must inherit the default review panel; order was {order:?}"
        );
    }

    #[test]
    fn a_producer_stage_skips_the_three_tier_review_and_unblocks_its_dependents() {
        // A `produces` (planner) stage emits a DAG, not code, so it must SKIP the
        // per-unit three-tier review and the code-integrate: lenses/adversary/
        // adjudicator reviewing an empty diff is pointless and slow (it stalled the
        // live run before the implement units could start). It still reaches
        // `Integrated` truthfully (the REVIEW_ONLY_NO_ARTIFACT marker, NOT a code
        // merge), so its dependents become ready.
        //
        // Isolation: only the PRODUCER stage carries a review panel (`lens`/`adversary`/
        // `adjudicator`); `defaults.review` is empty, so the proposed worker unit
        // inherits no panel. Therefore ANY lens/adversary/adjudicator spawn could only
        // be the producer reviewing its own (empty) diff - exactly the bug.
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents
            .insert("adjudicator".into(), agent("adjudicator"));
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                // ONLY the producer has a review panel; if the producer reviewed its
                // (empty) diff these three would spawn. Post-fix none do.
                review: config::ReviewPanel {
                    lenses: vec!["lens".into()],
                    adversary: "adversary".into(),
                    adjudicator: "adjudicator".into(),
                },
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // The planner proposes one implement unit that depends on the planner, so it
        // becomes ready only once the producer reaches Integrated (the unblock).
        let driver = Stub {
            emits: vec![(
                TYPE_UNIT_PROPOSED.to_string(),
                json!({
                    "id": "impl-unit",
                    "agent": "worker",
                    "needs": ["plan"],
                }),
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        // The producer reached the DAG-terminal Integrated TRUTHFULLY - with the
        // no-code-artifact marker, not a fabricated code commit.
        assert_eq!(
            rs.units["plan"].status,
            ledger::Status::Integrated,
            "a producer must reach Integrated to unblock its dependents"
        );
        assert_eq!(
            rs.units["plan"].commit, REVIEW_ONLY_NO_ARTIFACT,
            "a producer integrates NO code artifact; it records the review-only marker"
        );

        // The dependent unblocked: it became ready once the producer integrated and
        // ran its implementer.
        assert_eq!(
            rs.units["impl-unit"].status,
            ledger::Status::Integrated,
            "the producer must unblock its dependent so it runs"
        );

        // The CORE assertion: NO review-tier agent ran for the producer. The producer
        // is the only stage with a review panel, so a single such spawn would mean it
        // reviewed its own empty diff - the bug.
        let order = driver.call_order.lock().unwrap().clone();
        assert!(
            !order.iter().any(|a| a == "lens" || a == "adversary" || a == "adjudicator"),
            "a producer must NOT spawn any review-tier agent on its empty diff; order was {order:?}"
        );
        // The planner and the proposed worker DID run - the producer skipped review,
        // it did not skip work.
        assert!(
            order.iter().any(|a| a == "planner") && order.iter().any(|a| a == "worker"),
            "the planner and its proposed worker must both run; order was {order:?}"
        );
    }

    #[test]
    fn per_unit_adjudicator_reject_blocks_integration_and_escalates() {
        // A rejecting adjudicator on the per-unit review (§3.2) is treated like a gate
        // failure: it blocks THAT unit's integration and remediates, escalating after
        // the retry bound - EVEN THOUGH the unit's static gates pass. The shared Stub
        // returns {"verdict":"reject"} for every spawn, but only the adjudicator's
        // output gates; the implementer keeps producing a green diff each retry.
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()], // static gates pass
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"reject","issues":[]}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Escalated,
            "a rejecting per-unit adjudicator must block integration and escalate, even with green static gates"
        );
        // The reject re-looped the unit's implement -> gates -> review remediation: the
        // adversary and adjudicator both ran, and the unit never integrated.
        let order = driver.call_order.lock().unwrap().clone();
        assert!(
            order.iter().any(|a| a == "adversary"),
            "the adversary must have run before the gating verdict"
        );
        assert!(
            order.iter().any(|a| a == "adj"),
            "the adjudicator must have rendered the gating verdict"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
            "a rejected unit must emit no UnitIntegrated"
        );
        assert!(
            events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
            "a rejected unit must record a UnitFailed as it remediates"
        );
    }

    #[test]
    fn an_always_rejecting_adjudicator_escalates_after_exactly_max_retries_cycles() {
        // FIX 1 (the churn bug): an adjudicator that ALWAYS rejects must NOT loop the
        // unit forever. Each implement -> gates -> review cycle that ends in a reject
        // increments the SAME bounded attempts counter, so after exactly MAX_RETRIES
        // cycles the unit ESCALATES and the run RETURNS rather than spinning. We count
        // the cycles via the Stub's recorded worker spawns (one implement spawn per
        // cycle) and assert it equals MAX_RETRIES - not 6, not unbounded.
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true")); // static gates pass
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // Every spawn returns a reject verdict; only the adjudicator's gates, but the
        // adjudicator never relents - so the unit can only ever escalate.
        let driver = Stub {
            output: r#"{"verdict":"reject","issues":[]}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        // The run RETURNS (Ok) - it does not loop forever.
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Escalated,
            "a perpetually-rejecting adjudicator must escalate the unit, not churn"
        );
        // EXACTLY MAX_RETRIES implement cycles ran: one worker spawn per cycle. The
        // bound is applied to the review-reject path, counting the same attempts
        // counter as a gate failure would.
        let order = driver.call_order.lock().unwrap().clone();
        let worker_spawns = order.iter().filter(|a| *a == "worker").count();
        assert_eq!(
            worker_spawns as u32,
            safety::MAX_RETRIES,
            "the unit must implement exactly MAX_RETRIES times before escalating; spawns were {order:?}"
        );
        // The escalation lesson captures the final adjudicator reason (not a generic
        // placeholder), and the unit emits exactly one UnitEscalated.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let escalations = events
            .iter()
            .filter(|e| e.type_ == ledger::TYPE_UNIT_ESCALATED)
            .count();
        assert_eq!(escalations, 1, "the unit escalates exactly once");
        let lesson = events.iter().any(|e| {
            e.type_ == contextgraph::TYPE_LESSON_LEARNED
                && String::from_utf8_lossy(&e.data).contains("escalated after")
        });
        assert!(
            lesson,
            "escalation must record a lesson capturing the final adjudicator reason"
        );
        // The final UnitFailed records attempts == MAX_RETRIES (the bound), and the
        // unit never integrated.
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
            "a perpetually-rejected unit must never integrate"
        );
        assert_eq!(
            rs.units["implement"].attempts,
            safety::MAX_RETRIES,
            "the final attempts count must reach the bound"
        );
    }

    #[test]
    fn a_higher_max_retries_gives_more_attempts_before_escalation() {
        // The remediation depth is configurable via `defaults.max_retries`: it is the
        // REFINEMENT-depth knob, not a review-rigor knob. A subtle unit that the full
        // strict review keeps rejecting must be able to be given MORE attempts to
        // converge before escalation, WITHOUT weakening any review. We drive a
        // perpetually-rejecting adjudicator (so the unit can only ever escalate) and
        // assert the unit gets EXACTLY `max_retries` worker spawns before it escalates -
        // a higher value buys more attempts, a lower value escalates earlier, and the
        // review path is identical in every case.

        // Build a config whose only knob that varies is `defaults.max_retries`, run it
        // against an always-rejecting adjudicator, and return (worker spawns, final
        // status, final attempts). The review panel (lenses + adversary + adjudicator)
        // and gates are byte-identical across every bound, so this isolates the depth.
        fn escalation_run(max_retries: u32) -> (u32, ledger::Status, u32) {
            let mut cfg = Config::default();
            cfg.agents.insert("worker".into(), agent("worker"));
            cfg.agents.insert("lens".into(), agent("lens"));
            cfg.agents.insert("adversary".into(), agent("adversary"));
            cfg.agents.insert("adj".into(), agent("adj"));
            cfg.workflow.gates.insert("ok".into(), gate_def("true")); // static gates pass
            cfg.workflow.defaults.review = config::ReviewPanel {
                lenses: vec!["lens".into()],
                adversary: "adversary".into(),
                adjudicator: "adj".into(),
            };
            cfg.workflow.defaults.max_retries = max_retries;
            cfg.workflow.stages.insert(
                "implement".into(),
                Stage {
                    name: "implement".into(),
                    agent: "worker".into(),
                    gates: vec!["ok".into()],
                    on_pass: "merge".into(),
                    ..Default::default()
                },
            );
            let st = Store::open(":memory:").unwrap();
            let driver = Stub {
                output: r#"{"verdict":"reject","issues":[]}"#.into(),
                ..Stub::new()
            };
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &ExecRunner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            let rs = run(&cfg, &deps).unwrap();
            let order = driver.call_order.lock().unwrap().clone();
            let worker_spawns = order.iter().filter(|a| *a == "worker").count() as u32;
            (
                worker_spawns,
                rs.units["implement"].status,
                rs.units["implement"].attempts,
            )
        }

        // A higher bound (6) gives the unit SIX attempts to converge before escalating -
        // exactly six worker spawns, not three.
        let (spawns, status, attempts) = escalation_run(6);
        assert_eq!(
            status,
            ledger::Status::Escalated,
            "a perpetually-rejecting adjudicator still escalates - the knob loosens depth, never the bar"
        );
        assert_eq!(
            spawns, 6,
            "max_retries: 6 must give the unit SIX attempts before escalation, not the default three; spawns were {spawns}"
        );
        assert_eq!(
            attempts, 6,
            "the final folded attempt count must equal the configured bound"
        );

        // A low bound (2) escalates EARLY - exactly two attempts, fewer than the default
        // three. The same review path, a shallower depth.
        let (spawns_low, status_low, attempts_low) = escalation_run(2);
        assert_eq!(status_low, ledger::Status::Escalated);
        assert_eq!(
            spawns_low, 2,
            "max_retries: 2 must escalate after only two attempts; spawns were {spawns_low}"
        );
        assert_eq!(
            attempts_low, 2,
            "the final attempt count tracks the low bound"
        );

        // And a higher bound genuinely buys more attempts than a lower one.
        assert!(
            spawns > spawns_low,
            "a higher max_retries must give strictly more attempts before escalation than a lower one"
        );
    }

    #[test]
    fn an_absent_max_retries_preserves_the_default_bound_of_three() {
        // Back-compat: a workflow that does NOT set `defaults.max_retries` (the field is
        // 0) falls back to `safety::MAX_RETRIES` (3) exactly as before - the unit gets
        // three attempts, byte-for-byte the historical behavior. This pins that the new
        // knob is opt-in and changes nothing when absent.
        assert_eq!(
            safety::MAX_RETRIES,
            3,
            "the documented default remediation depth is 3"
        );
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        // NOTE: defaults.max_retries deliberately left unset (0 -> falls back to 3).
        assert_eq!(
            cfg.workflow.defaults.max_retries, 0,
            "this test exercises the absent (unset) case"
        );
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"reject","issues":[]}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        let order = driver.call_order.lock().unwrap().clone();
        let worker_spawns = order.iter().filter(|a| *a == "worker").count() as u32;
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Escalated,
            "an unset max_retries must escalate exactly as before"
        );
        assert_eq!(
            worker_spawns,
            safety::MAX_RETRIES,
            "an absent max_retries must give exactly the historical three attempts; spawns were {worker_spawns}"
        );
    }

    /// A driver for the Gap-16 regression (spec 06 unit 3, "approval beats the retry
    /// cap"): the implementer and every lens/adversary stay SILENT (empty output, which
    /// `verdict_approves` reads as no-approve, so only the fail-closed adjudicator's
    /// verdict gates), and the ADJUDICATOR rejects every attempt EXCEPT the one named by
    /// `approve_on_attempt`, where it approves. A test sets `approve_on_attempt` to the
    /// FINAL permitted attempt (the loop-variable `max_retries - 1`), so the approve
    /// arrives exactly on the attempt whose reject would trip `remediate` into Escalate -
    /// proving the terminal check folds the verdict BEFORE the attempt counter.
    struct AdjApprovesOnAttempt {
        adj_id: String,
        approve_on_attempt: u32,
        /// Every adjudicator spawn's attempt index, in call order, so a test can assert
        /// the unit ran the full attempt budget and the approve landed on the LAST one.
        adj_attempts: Mutex<Vec<u32>>,
    }
    impl AdjApprovesOnAttempt {
        fn new(adj_id: &str, approve_on_attempt: u32) -> Self {
            Self {
                adj_id: adj_id.to_string(),
                approve_on_attempt,
                adj_attempts: Mutex::new(Vec::new()),
            }
        }
    }
    impl AgentDriver for AdjApprovesOnAttempt {
        fn spawn(
            &self,
            a: &AgentDef,
            _prompt: &str,
            opts: &SpawnOpts,
            _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            let output = if a.id == self.adj_id {
                // The adjudicator's deterministic spawn id is `{unit}/adjudicator#{attempt}`;
                // the attempt is the trailing integer.
                let attempt = opts
                    .id
                    .rsplit('#')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                self.adj_attempts.lock().unwrap().push(attempt);
                if attempt == self.approve_on_attempt {
                    r#"{"verdict":"approve"}"#.to_string()
                } else {
                    r#"{"verdict":"reject","issues":[]}"#.to_string()
                }
            } else {
                // Non-adjudicator reviewers (lens/adversary) emit their findings to the
                // graph; their stdout is not a verdict but must be NON-degenerate, else the
                // Gap-18 respawn loop would treat this stub's empty output as an
                // infrastructure fault. A fixed narration keeps them substantive.
                "reviewed the diff".to_string()
            };
            Ok(AgentResult {
                output,
                resolved_model: String::new(),
            })
        }
    }

    #[test]
    fn approval_on_the_final_permitted_attempt_integrates_a_per_unit_stage() {
        // Gap 16 (spec 06 unit 3): an adjudicator APPROVE on a unit's FINAL permitted
        // attempt must INTEGRATE the unit - `max_retries` gates only STARTING another
        // attempt, it never overrides an approval that folded on the last attempt. The
        // adjudicator rejects attempts 0..CAP-1 and approves attempt CAP-1 (the final
        // one). A reject there would trip `remediate(CAP-1, CAP)` -> Escalate, so an
        // approve that still integrates proves the verdict is folded BEFORE the attempt
        // counter - the exact regression that recorded unit-2's approved-on-attempt-6
        // review as UnitFailed/UnitEscalated (design-intent Gap 16).
        const CAP: u32 = 3;
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true")); // static gates pass
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.defaults.max_retries = CAP;
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = AdjApprovesOnAttempt::new("adj", CAP - 1);
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Integrated,
            "an approval on the FINAL permitted attempt must integrate the unit, not escalate it"
        );
        // The adjudicator ran once per attempt up to and including the final permitted
        // one (0..CAP), so the approve genuinely landed on attempt == cap, not earlier.
        let adj_attempts = driver.adj_attempts.lock().unwrap().clone();
        assert_eq!(
            adj_attempts,
            (0..CAP).collect::<Vec<u32>>(),
            "the adjudicator must have rejected every earlier attempt and approved the last; attempts were {adj_attempts:?}"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_ESCALATED),
            "an approved unit must record NO UnitEscalated"
        );
        assert!(
            events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
            "an approved unit must record a UnitIntegrated"
        );
    }

    #[test]
    fn a_model_ladder_implementer_escalates_one_rung_per_remediation_attempt() {
        // Spec 10 unit 4 - the ladder-advance-on-retry path. An implementer on a
        // `model_ladder` resolves rung 0 on its first attempt and advances one rung on each
        // remediation attempt, and the resolved rung is VISIBLE in the logged model stamps.
        // The adjudicator rejects attempt 0 (forcing one remediation) then approves attempt 1,
        // so the unit records a `green` status for BOTH attempts; each must carry the rung
        // that attempt ran on as its META_MODEL_ALIAS. This is the same discriminating driver
        // the Gap-16 approval tests use, here proving the per-attempt rung escalation.
        const CAP: u32 = 3;
        let mut cfg = Config::default();
        let mut worker = agent("worker");
        worker.model_ladder = vec!["haiku".into(), "sonnet".into(), "opus".into()];
        cfg.agents.insert("worker".into(), worker);
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true")); // static gates pass
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.defaults.max_retries = CAP;
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // Reject attempt 0, approve attempt 1: exactly one remediation, so the ladder advances
        // exactly once (rung 0 -> rung 1).
        let driver = AdjApprovesOnAttempt::new("adj", 1);
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Integrated,
            "the unit integrates once the adjudicator approves attempt 1"
        );
        // The adjudicator ran on attempts 0 and 1 (rejected then approved), confirming exactly
        // one remediation actually occurred - so the ladder genuinely advanced.
        assert_eq!(
            driver.adj_attempts.lock().unwrap().clone(),
            vec![0, 1],
            "the adjudicator rejected attempt 0 and approved attempt 1"
        );

        // Each attempt's `green` status carries the rung that attempt ran on. Distinguish the
        // two attempts by their replay key (`implement/green#<attempt>`).
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let alias_for_attempt = |attempt: u32| -> String {
            let key = format!("implement/green#{attempt}");
            events
                .iter()
                .find(|e| {
                    e.type_ == ledger::TYPE_UNIT_STATUS
                        && e.meta.get(META_REPLAY_KEY).map(String::as_str) == Some(key.as_str())
                })
                .unwrap_or_else(|| panic!("a green status for attempt {attempt} must exist"))
                .meta
                .get(META_MODEL_ALIAS)
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(
            alias_for_attempt(0),
            "haiku",
            "attempt 0's stamp names the cheap first rung"
        );
        assert_eq!(
            alias_for_attempt(1),
            "sonnet",
            "the remediation attempt's stamp advanced exactly one rung"
        );
    }

    #[test]
    fn approval_on_the_final_permitted_attempt_integrates_a_standalone_review_stage() {
        // Gap 16 in the EXACT shape that bit unit-2-the-adjudicator-persona: a STANDALONE
        // three-tier review stage (the fan-out review path, `run_fan_out_review_loop`)
        // whose adjudicator approves on the final permitted attempt. The pre-fix conductor
        // recorded UnitFailed/UnitEscalated with the APPROVE text quoted under a "review
        // rejected:" header; the fixed conductor integrates. Same discriminating driver as
        // the per-unit test, on the standalone-review path.
        const CAP: u32 = 3;
        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true")); // static gates pass
        cfg.workflow.defaults.max_retries = CAP;
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                // No `agent` + an `agents` lens list routes this to the standalone
                // fan-out review path.
                agents: vec!["lens".into()],
                adversary: "adversary".into(),
                adjudicator: "adj".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = AdjApprovesOnAttempt::new("adj", CAP - 1);
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["review"].status,
            ledger::Status::Integrated,
            "a standalone review approved on its FINAL permitted attempt must integrate, not escalate"
        );
        let adj_attempts = driver.adj_attempts.lock().unwrap().clone();
        assert_eq!(
            adj_attempts,
            (0..CAP).collect::<Vec<u32>>(),
            "the adjudicator must have rejected every earlier attempt and approved the last; attempts were {adj_attempts:?}"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_ESCALATED),
            "an approved standalone review must record NO UnitEscalated"
        );
        assert!(
            events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
            "an approved standalone review must record a UnitIntegrated"
        );
    }

    #[test]
    fn mid_spawn_crash_escalates_without_aborting_the_run() {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            fail_spawn: true,
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        // The run completes (Ok), not aborted; the crashing unit escalates.
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["s"].status, ledger::Status::Escalated);
    }

    #[test]
    fn budget_breaker_stops_the_run_after_the_first_wave() {
        // Two stages in sequential waves (s2 needs s1). A spawn budget of 1 lets the
        // first wave run, then the pre-wave checkBudget (§4.4, §8) trips before the
        // second wave: s1 integrates, s2 never starts.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.budget = 1;
        cfg.workflow.stages.insert(
            "s1".into(),
            Stage {
                name: "s1".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "s2".into(),
            Stage {
                name: "s2".into(),
                agent: "a".into(),
                needs: vec!["s1".into()],
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["s1"].status, ledger::Status::Integrated);
        assert!(
            !rs.units.contains_key("s2"),
            "the budget breaker must stop the second wave before it starts"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_BUDGET_EXHAUSTED),
            "tripping the budget must emit a BudgetExhausted event"
        );
    }

    #[test]
    fn budget_exhaustion_aborts_the_task() {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.budget = 1;
        cfg.workflow.stages.insert(
            "s1".into(),
            Stage {
                name: "s1".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "s2".into(),
            Stage {
                name: "s2".into(),
                agent: "a".into(),
                needs: vec!["s1".into()],
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_TASK_ABORTED),
            "a tripped budget must abort the task"
        );
    }

    #[test]
    fn a_budget_halt_surfaces_its_reason_on_the_run_state() {
        // Gap 13: a budget halt is a RUNTIME condition of this run process, surfaced on the
        // returned RunState so `rigger step` can print a halt reason DISTINCT from
        // convergence (and the thin driver stops loudly on it). budget=1, two independent
        // units: one spawn is admitted, the second is refused and trips the breaker, so the
        // run halts with the spent count over the budget.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.budget = 1;
        for name in ["w1", "w2"] {
            cfg.workflow.stages.insert(
                name.into(),
                Stage {
                    name: name.into(),
                    agent: "a".into(),
                    gates: vec!["ok".into()],
                    ..Default::default()
                },
            );
        }
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.budget_halt.as_deref(),
            Some("budget exhausted: 1/1 spawns"),
            "a budget-halted run must surface its halt reason on the RunState"
        );
    }

    #[test]
    fn a_run_within_budget_surfaces_no_halt_reason() {
        // The halt signal is ABSENT on a run that did not trip the breaker: `rigger step`
        // then prints `{"wave":[],"done":true}` with no halt and the driver reports a clean
        // completion. A budget of 0 is unlimited, so the single unit never trips.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "w".into(),
            Stage {
                name: "w".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.budget_halt, None,
            "a run that never tripped the breaker reports no halt reason"
        );
    }

    #[test]
    fn the_spawn_budget_folds_from_recorded_spawn_requests_across_steps() {
        // Criterion 5 / finding adv-budget-per-step-resets: the spawn count is DERIVED
        // from the recorded spawn-request events, not a per-process in-memory counter
        // that resets every `rigger step`. Two earlier steps parked two spawn requests;
        // a fresh process building a new RunCtx folds them from the log, so with a budget
        // of 2 it already sees the budget spent - even though its own counter started at
        // zero.
        let st = Store::open(":memory:").unwrap();
        spawn::park(
            &st,
            &spawn::SpawnRequest::new("u1", "u1", ROLE_IMPLEMENTER, 0, "p"),
        )
        .unwrap();
        spawn::park(
            &st,
            &spawn::SpawnRequest::new("u2", "u2", ROLE_IMPLEMENTER, 0, "p"),
        )
        .unwrap();

        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.defaults.budget = 2;
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let c = RunCtx::for_test(&cfg, &deps);

        // The cumulative count was folded from the log, not reset to 0.
        assert_eq!(
            c.spawns.load(Ordering::SeqCst),
            2,
            "the spawn count seeds from spawn::recorded(log).len(), not 0"
        );
        // At-budget with only recorded spawns pending, the pre-wave breaker HOLDS: the
        // already-paid work must still be free to replay and integrate on this step.
        assert!(
            !c.budget_tripped(),
            "a resume whose frontier is entirely replays does not pre-wave-trip"
        );

        // A REPLAY of an already-recorded spawn is admitted for FREE (its budget was
        // spent when it was first parked) and is never counted again.
        assert!(
            c.reserve_spawn(&spawn_id("u1", ROLE_IMPLEMENTER, 0)),
            "a recorded spawn replays free, even at budget"
        );
        assert_eq!(
            c.spawns.load(Ordering::SeqCst),
            2,
            "a replay does not re-spend the budget"
        );

        // A genuinely NEW spawn is refused: the log already holds `budget` spawns.
        assert!(
            !c.reserve_spawn(&spawn_id("u3", ROLE_IMPLEMENTER, 0)),
            "a new spawn beyond the folded count is refused"
        );
        assert!(
            c.budget_broke(),
            "refusing a new over-budget spawn trips the breaker"
        );
    }

    #[test]
    fn the_pre_wave_breaker_trips_only_on_a_new_over_budget_spawn() {
        // The pre-wave breaker must not abort a resume before it can replay its recorded
        // work, but it MUST trip once this process admits a NEW spawn that reaches the
        // budget (spawns > base_spawns). One spawn recorded, budget 2.
        let st = Store::open(":memory:").unwrap();
        spawn::park(
            &st,
            &spawn::SpawnRequest::new("u1", "u1", ROLE_IMPLEMENTER, 0, "p"),
        )
        .unwrap();

        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.defaults.budget = 2;
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let c = RunCtx::for_test(&cfg, &deps);

        // Nothing new spent yet - the recorded frontier is free to replay.
        assert!(
            !c.budget_tripped(),
            "one recorded spawn under a budget of 2 does not pre-wave-trip"
        );
        // Admit one NEW spawn: there is room, and it reaches the budget.
        assert!(
            c.reserve_spawn(&spawn_id("u2", ROLE_IMPLEMENTER, 0)),
            "there is room for one new spawn"
        );
        // Now the pre-wave breaker trips: this process spent a new spawn to reach the cap.
        assert!(
            c.budget_tripped(),
            "reaching the budget via a new spawn trips the pre-wave breaker"
        );
    }

    #[test]
    fn a_review_tier_budget_refusal_aborts_with_budgetexhausted_not_a_raw_error() {
        // Criterion 5, the review-tier arm (findings budget-review-tier-no-exhausted,
        // adv-confirm-review-tier-no-budgetexhausted): a run that exceeds `defaults.budget`
        // at a REVIEW spawn must abort with BudgetExhausted, exactly like the implementer
        // path - NOT propagate a raw error out of `run` before the breaker records it.
        //
        // Budget of 1: the implementer spawn consumes the whole budget, the unit reaches
        // `verified` (empty gates pass), then the first review tier (the lens) is a NEW
        // spawn `reserve_spawn` refuses. Before the fix that refusal returned a raw Err
        // that `run_wave` collapsed and `run` propagated BEFORE the mid-wave budget_broke
        // check, so the run aborted with a raw error and emitted no BudgetExhausted.
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.workflow.defaults.budget = 1;
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            ..Default::default()
        };
        cfg.workflow.stages.insert(
            "u".into(),
            Stage {
                name: "u".into(),
                agent: "worker".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };

        // The run HALTS cleanly - the review-tier refusal must not surface as a run error.
        run(&cfg, &deps).expect("a review-tier budget refusal halts the run, it does not error");

        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_BUDGET_EXHAUSTED),
            "a review-tier budget refusal must emit BudgetExhausted, like the implementer path"
        );
        assert!(
            events.iter().any(|e| e.type_ == TYPE_TASK_ABORTED),
            "the breaker aborts the task on a review-tier refusal too"
        );
        // The implementer spent the budget and the unit reached `verified`; the lens was
        // then REFUSED before it ever spawned - the review spawn was over budget.
        assert!(
            driver.spawned("worker"),
            "the implementer runs (it is admitted under the budget)"
        );
        assert!(
            !driver.spawned("lens"),
            "the over-budget lens spawn is refused before it runs"
        );
        assert!(
            events.iter().any(|e| {
                e.type_ == ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"verified\"")
            }),
            "the unit reaches verified before the review tier is refused"
        );
    }

    #[test]
    fn coverage_gap_flags_a_spec_defect_and_errors() {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                coverage: "criterion one".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec!["criterion one".into(), "criterion two".into()],
        };
        // The gap halts the run (§4.4): flagSpecDefect, then return Err.
        assert!(run(&cfg, &deps).is_err());
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_SPEC_DEFECT),
            "an uncovered criterion must be flagged as a spec defect"
        );
    }

    #[test]
    fn planner_covering_every_criterion_passes() {
        // A `produces` planner defers coverage: it proposes a unit whose `coverage`
        // closes the only criterion, so coverage holds after planning (§3.2).
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            emits: vec![(
                TYPE_UNIT_PROPOSED.to_string(),
                json!({"id": "impl", "agent": "worker", "coverage": "crit"}),
            )],
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec!["crit".into()],
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["impl"].status, ledger::Status::Integrated);
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            !events.iter().any(|e| e.type_ == TYPE_SPEC_DEFECT),
            "a planner that covers every criterion must not flag a defect"
        );
    }

    #[test]
    fn planner_leaving_a_gap_flags_a_spec_defect() {
        // The `produces` planner proposes no unit covering "crit"; coverage is checked
        // AFTER planning and finds the gap -> SpecDefect + Err (§3.2, the coverage gate
        // is not silently disabled by the presence of a `produces` stage).
        let mut cfg = Config::default();
        cfg.agents.insert("planner".into(), agent("planner"));
        cfg.workflow.stages.insert(
            "plan".into(),
            Stage {
                name: "plan".into(),
                agent: "planner".into(),
                produces: "dag".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new(); // proposes nothing
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec!["crit".into()],
        };
        assert!(run(&cfg, &deps).is_err());
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_SPEC_DEFECT),
            "a planner that leaves a criterion uncovered must flag a spec defect"
        );
    }

    #[test]
    fn gate_only_stage_is_a_coverage_proxy_gap() {
        // A stage that "covers" a criterion but has only a gate command and no agent
        // is a mechanical proxy, not an LLM judge: it does not satisfy the criterion,
        // so the run is refused with a SpecDefect (§8 proxy-gap guard).
        let mut cfg = Config::default();
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "gateonly".into(),
            Stage {
                name: "gateonly".into(),
                coverage: "crit".into(),
                gates: vec!["ok".into()],
                ..Default::default() // no agent / agents / adjudicator
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: vec!["crit".into()],
        };
        assert!(
            run(&cfg, &deps).is_err(),
            "a criterion covered only by a gate-only stage must be an uncovered gap"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(events.iter().any(|e| e.type_ == TYPE_SPEC_DEFECT));
    }

    #[test]
    fn manual_stage_pauses_while_an_auto_stage_integrates() {
        // An explicitly-manual stage pauses (ManualReview, not integrated); an
        // independent default-autonomy stage in the same wave integrates (§4.3).
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "manual".into(),
            Stage {
                name: "manual".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                autonomy: "manual".into(),
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "auto".into(),
            Stage {
                name: "auto".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["auto"].status,
            ledger::Status::Integrated,
            "the auto stage must integrate"
        );
        assert_ne!(
            rs.units["manual"].status,
            ledger::Status::Integrated,
            "the manual stage must NOT integrate - it is awaiting review"
        );
        assert_ne!(
            rs.units["manual"].status,
            ledger::Status::Escalated,
            "the manual stage is paused, not failed"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_MANUAL_REVIEW),
            "a manual stage must emit ManualReview"
        );
    }

    #[test]
    fn isolation_none_agent_gets_no_worktree_even_with_a_repo() {
        // An agent declaring `isolation: none` runs in the current dir (no
        // worktree) even when a repo is configured (§3.1, §6). The Stub records the
        // SpawnOpts the conductor passed; isolation must be false.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert(
            "rev".into(),
            AgentDef {
                id: "rev".into(),
                isolation: "none".into(),
                ..Default::default()
            },
        );
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "rev".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let opts = driver.opts_by_agent.lock().unwrap();
        let (isolation, _parallel) = opts.get("rev").copied().unwrap();
        assert!(
            !isolation,
            "an `isolation: none` agent must run with no worktree even with a repo"
        );
    }

    #[test]
    fn spawn_opts_isolation_is_set_for_a_worktree_agent() {
        // An isolated (default) agent in a repo runs in a worktree, so SpawnOpts
        // carries isolation = true (§6).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let opts = driver.opts_by_agent.lock().unwrap();
        let (isolation, parallel) = opts.get("a").copied().unwrap();
        assert!(isolation, "a worktree-isolated agent must report isolation");
        assert!(!parallel, "a single-worker stage is not parallel");
    }

    #[test]
    fn stage_autonomy_override_seeds_the_gate() {
        // A stage with `autonomy: silent` seeds its gate's ratchet at Silent, so the
        // gate runs unattended; the default (manual) would pause. We assert via the
        // emitted GateVerdict (the stage integrates rather than pausing) and the gate
        // tracker's seeded autonomy (§3.2). A default-autonomy run would still
        // integrate too, so the discriminating check is the seeded autonomy below.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.autonomy = "manual".into();
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                autonomy: "silent".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let ctx = RunCtx {
            cfg: &cfg,
            deps: &Deps {
                store: &st,
                driver: &driver,
                gates: &ExecRunner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            },
            run_id: String::new(),
            gate_tracker: Mutex::new(HashMap::new()),
            integrate_mu: Mutex::new(()),
            spawns: AtomicU32::new(0),
            base_spawns: 0,
            recorded_spawn_ids: HashSet::new(),
            budget_broke: std::sync::atomic::AtomicBool::new(false),
            parked: std::sync::atomic::AtomicBool::new(false),
            manual_review: std::sync::atomic::AtomicBool::new(false),
            budget_halted: std::sync::atomic::AtomicBool::new(false),
            prior_status: HashMap::new(),
            prior_attempts: HashMap::new(),
            replayed_keys: Mutex::new(HashSet::new()),
            gate_verdicts: Mutex::new(HashMap::new()),
        };
        ctx.record_gate("ok", gate::Kind::Core, true, "silent");
        let seeded = ctx.gate_tracker.lock().unwrap().get("ok").unwrap().autonomy;
        assert_eq!(
            seeded,
            gate::Autonomy::Silent,
            "the stage `autonomy: silent` override must seed the gate at Silent, not the manual default"
        );
    }

    #[test]
    fn on_pass_none_runs_gates_but_does_not_integrate() {
        // A stage with `on_pass: none` and passing gates verifies but never
        // integrates: no commit, not Integrated (§3.2).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                on_pass: "none".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            write_file: Some("work.rs".into()),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_ne!(
            rs.units["s"].status,
            ledger::Status::Integrated,
            "an `on_pass: none` stage must not integrate even when its gates pass"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
            "an `on_pass: none` stage must emit no UnitIntegrated"
        );
    }

    #[test]
    fn two_units_gate_environments_never_share_a_target_dir() {
        // Gap 19 (criterion 3): a gate that runs INSIDE a unit's worktree must build into
        // a unit-keyed CARGO_TARGET_DIR, so two concurrent units' divergent trees never
        // share one incremental cache - a compile error a gate surfaces is then always
        // that unit's own, never a neighbour poisoning a shared target. Two independent
        // units each run a gate; the runner captures the target_dir handed to it, and the
        // two must be DISTINCT, both NON-EMPTY, and each the `cargo-target-<unit-slug>`
        // sibling of that unit's worktree under the run's scratch root.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        for name in ["alpha", "beta"] {
            cfg.workflow.stages.insert(
                name.into(),
                Stage {
                    name: name.into(),
                    agent: "a".into(),
                    gates: vec!["ok".into()],
                    // Verify-but-never-merge: the gate still runs per unit, but skipping
                    // the merge keeps the two independent units off a shared-repo conflict.
                    on_pass: "none".into(),
                    ..Default::default()
                },
            );
        }
        let store = Store::open(":memory:").unwrap();
        let driver = Stub {
            write_file: Some("work.rs".into()),
            ..Stub::new()
        };
        let runner = RecordingRunner::new(&[]);
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &runner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        let targets = runner.targets();
        assert_eq!(
            targets.len(),
            2,
            "each of the two units ran its one gate exactly once: {targets:?}"
        );
        assert!(
            targets.iter().all(|t| !t.is_empty()),
            "a gate inside a unit worktree must get a per-unit CARGO_TARGET_DIR, never the empty (inherit-shared) one: {targets:?}"
        );
        let unique: HashSet<&String> = targets.iter().collect();
        assert_eq!(
            unique.len(),
            2,
            "the two units' gate target dirs must DIFFER - never one shared cache: {targets:?}"
        );

        // Each target is the `cargo-target-<slug>` sibling of that unit's worktree under
        // the run's scratch root - the exact isolation the criterion requires. Derived the
        // same single-source way production does: the sibling of the unit's worktree dir.
        let scratch = crate::worktree::scratch_root_from_env(&repo_path, "");
        for name in ["alpha", "beta"] {
            let want =
                crate::worktree::unit_cache_sibling(&unit_worktree_dir(&scratch, name)).unwrap();
            assert!(
                targets.contains(&want),
                "unit {name} must build into {want}, got {targets:?}"
            );
        }
    }

    #[test]
    fn a_units_per_unit_cache_is_reclaimed_when_its_worktree_is_removed() {
        // Gap 19 (the DOMINANT graceful path): a gate that runs INSIDE a unit's worktree
        // builds into the unit-keyed `cargo-target-<slug>` cache; when the conductor tears
        // that worktree down at the end of `run_stage` (w.remove(), on integrate / park /
        // err), the cache must be reclaimed WITH the worktree - never left to leak a
        // multi-gigabyte dir on the operator's small partition. This drives the REAL
        // lifecycle: a runner MATERIALIZES the CARGO_TARGET_DIR it is handed (as a real cargo
        // build would), the unit runs end to end, and afterward the cache dir - and the
        // worktree - must both be gone from disk.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "solo".into(),
            Stage {
                name: "solo".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                // Verify-but-never-merge keeps the test off any repo-branch merge specifics;
                // the worktree teardown (w.remove) fires on this park path exactly as it does
                // on the integrate path the reject names.
                on_pass: "none".into(),
                ..Default::default()
            },
        );
        let store = Store::open(":memory:").unwrap();
        let driver = Stub {
            write_file: Some("work.rs".into()),
            ..Stub::new()
        };
        let runner = RecordingRunner::materializing();
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &runner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        let scratch = crate::worktree::scratch_root_from_env(&repo_path, "");
        let worktree = unit_worktree_dir(&scratch, "solo");
        let cache = crate::worktree::unit_cache_sibling(&worktree).unwrap();
        // The gate really built into the per-unit cache (guards against a vacuous pass where
        // the cache was never created, which would make the "is gone" assertion trivial).
        assert_eq!(
            runner.targets(),
            vec![cache.clone()],
            "the unit's one gate built into its per-unit cache: {:?}",
            runner.targets()
        );
        assert!(
            !std::path::Path::new(&cache).exists(),
            "the per-unit build cache must be reclaimed when the worktree is removed, leaked at {cache}"
        );
        assert!(
            !std::path::Path::new(&worktree).exists(),
            "the unit's worktree must be gone after run_stage tears it down: {worktree}"
        );
    }

    #[test]
    fn a_worktree_less_stage_gate_inherits_the_shared_target() {
        // Gap 19 sentinel arm: a stage whose agent declares `isolation: none` runs with NO
        // worktree (dir ""), so there is no per-unit tree to isolate and its gate must inherit
        // the ambient/shared CARGO_TARGET_DIR - the runner must be handed the empty (inherit)
        // target, never a `cargo-target-<slug>` override. (`unit_cache_sibling("")` is None.)
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert(
            "rev".into(),
            AgentDef {
                id: "rev".into(),
                isolation: "none".into(),
                ..Default::default()
            },
        );
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "rev".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let store = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let runner = RecordingRunner::new(&[]);
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &runner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        assert_eq!(
            runner.targets(),
            vec![String::new()],
            "an isolation:none stage's gate has no worktree, so it inherits the shared target (empty override): {:?}",
            runner.targets()
        );
    }

    #[test]
    fn bounded_pool_completes_every_stage_under_the_cap() {
        // Six independent stages exceed MAX_CONCURRENCY (4); the bounded pool runs
        // them in chunks, and all six must still integrate (§6).
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        for n in 0..6 {
            let name = format!("s{n}");
            cfg.workflow.stages.insert(
                name.clone(),
                Stage {
                    name,
                    agent: "a".into(),
                    gates: vec!["ok".into()],
                    ..Default::default()
                },
            );
        }
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        for n in 0..6 {
            assert_eq!(
                rs.units[&format!("s{n}")].status,
                ledger::Status::Integrated,
                "every stage must integrate even when the wave exceeds the pool cap"
            );
        }
    }

    #[test]
    fn live_run_produces_a_gated_by_edge() {
        // After a stage with a gate touches a file and integrates, the graph must
        // carry GATED_BY(file -> gate): the conductor emits a GateVerdict carrying
        // the artifact, which the projector folds (§7, Phase 2 carryover).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            write_file: Some("touched.rs".into()),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path,
            grounder: None,
            graph: Some(&graph),
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let g = graph.subgraph(&["touched.rs".to_string()], 2).unwrap();
        assert!(
            g.edges.iter().any(|e| e.rel == contextgraph::REL_GATED_BY
                && e.from == "touched.rs"
                && e.to == "ok"),
            "the live run must fold GATED_BY(touched.rs -> ok) after the stage integrates"
        );
    }

    #[test]
    fn agent_stage_runs_the_per_unit_lifecycle_not_the_fan_out_path() {
        // An implement stage names an `agent` AND `strategy: fan-out`. Under the
        // per-unit model (§3.2) it must run the SINGLE-UNIT lifecycle in
        // `run_single_stage` - implement -> gates -> the unit's own review ->
        // integrate - NOT the standalone fan-out path. `strategy: fan-out` on an
        // implementer stage means "one implementer per ready unit" (the partitioner +
        // planner-proposed units), not "run my lone agent as a parallel lens". The
        // single-worker path spawns with `parallel = false`; the fan-out path with
        // `parallel = true`.
        let st = Stage {
            name: "impl".into(),
            agent: "a".into(),
            strategy: "fan-out".into(),
            ..Default::default()
        };
        assert!(
            !is_fan_out(&st),
            "a stage that names an `agent` runs the per-unit lifecycle, not the fan-out path"
        );

        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert("impl".into(), st);
        let store = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["impl"].status, ledger::Status::Integrated);
        let opts = driver.opts_by_agent.lock().unwrap();
        let (_isolation, parallel) = opts.get("a").copied().unwrap();
        assert!(
            !parallel,
            "an `agent` stage runs the per-unit lifecycle (single-worker path), not the parallel lens path"
        );
    }

    #[test]
    fn standalone_review_stage_still_takes_the_fan_out_path() {
        // A standalone review stage - `agents` lens list, NO `agent` - keeps the
        // `run_fan_out_stage` aggregate-review path (§3.2). Asserted via SpawnOpts:
        // the lens spawns with `parallel = true`.
        let review = Stage {
            name: "review".into(),
            agents: vec!["lens".into()],
            ..Default::default()
        };
        assert!(
            is_fan_out(&review),
            "a stage with `agents` and no `agent` is a standalone fan-out review stage"
        );

        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.workflow.stages.insert("review".into(), review);
        let store = Store::open(":memory:").unwrap();
        // A substantive lens result so the standalone review proceeds (an empty result
        // would trip the Gap-18 respawn loop).
        let driver = Stub {
            output: "reviewed the diff".into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["review"].status, ledger::Status::Integrated);
        let opts = driver.opts_by_agent.lock().unwrap();
        let (_isolation, parallel) = opts.get("lens").copied().unwrap();
        assert!(
            parallel,
            "a standalone review stage spawns its lenses on the parallel fan-out path"
        );
    }

    #[test]
    fn partition_separates_overlapping_blast_radii() {
        // Overlapping file sets land in separate batches; disjoint sets share one.
        let items = vec![
            (
                "a".to_string(),
                vec!["x.rs".to_string(), "y.rs".to_string()],
            ),
            ("b".to_string(), vec!["y.rs".to_string()]), // overlaps a on y.rs
            ("c".to_string(), vec!["z.rs".to_string()]), // disjoint from a
        ];
        let batches = partition_by_blast_radius(&items);
        // a and c are disjoint -> first batch; b overlaps a -> a new batch.
        let expected: Vec<Vec<String>> = vec![
            vec!["a".to_string(), "c".to_string()],
            vec!["b".to_string()],
        ];
        assert_eq!(batches, expected);

        // Disjoint sets all share the first batch; empty radii conflict with nothing.
        let disjoint = vec![
            ("p".to_string(), vec!["p.rs".to_string()]),
            ("q".to_string(), vec!["q.rs".to_string()]),
            ("r".to_string(), Vec::new()),
        ];
        let expected_one: Vec<Vec<String>> =
            vec![vec!["p".to_string(), "q".to_string(), "r".to_string()]];
        assert_eq!(partition_by_blast_radius(&disjoint), expected_one);
    }

    #[test]
    fn partitioned_wave_still_integrates_every_stage() {
        // Correctness under partitioning: a wave with a grep grounder and
        // `partition: by-blast-radius` must still integrate EVERY ready stage, even
        // when blast radii overlap and the wave splits into sequential batches (§3.2,
        // §8). Two stages ground onto the same file (shared.rs) so they land in
        // separate batches; a third grounds elsewhere. All three must integrate.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shared.rs"), "fn shared() {}\n").unwrap();
        std::fs::write(dir.path().join("solo.rs"), "fn solo() {}\n").unwrap();
        let grep = crate::grounder::Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };

        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        for (name, coverage) in [("s1", "shared"), ("s2", "shared"), ("s3", "solo")] {
            cfg.workflow.stages.insert(
                name.into(),
                Stage {
                    name: name.into(),
                    agent: "a".into(),
                    coverage: coverage.into(),
                    gates: vec!["ok".into()],
                    partition: "by-blast-radius".into(),
                    ..Default::default()
                },
            );
        }
        let store = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: Some(&grep),
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        for name in ["s1", "s2", "s3"] {
            assert_eq!(
                rs.units[name].status,
                ledger::Status::Integrated,
                "every stage must integrate under by-blast-radius partitioning"
            );
        }
    }

    /// A gate runner that FAILS its first N runs, then passes, with a fixed evidence
    /// string on the failures. Lets a test exercise the targeted-remediation path
    /// (items 3): the conductor must thread the failing gate's evidence into the next
    /// attempt's prompt.
    struct FlakyGate {
        fail_first: u32,
        runs: AtomicU32,
        evidence: String,
    }
    impl gate::Runner for FlakyGate {
        fn run(&self, _g: &Gate, _dir: &str, _target: &str) -> gate::GateResult {
            let n = self.runs.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_first {
                gate::GateResult {
                    pass: false,
                    evidence: self.evidence.clone(),
                }
            } else {
                gate::GateResult {
                    pass: true,
                    evidence: "PASS".into(),
                }
            }
        }
    }

    #[test]
    fn lens_finding_reaches_later_tiers_through_the_graph() {
        // Item 1 + 5: the three review tiers communicate THROUGH THE CONTEXT GRAPH,
        // not via the conductor hand-threading one agent's stdout into the next
        // agent's prompt. The lens EMITS a ReviewFinding about a grounded file; the
        // projector folds it ABOUT that file live; the adversary and the adjudicator
        // GROUND on the same file AFTER the lens, so `graph_context` surfaces the
        // finding in THEIR prompts. We assert the finding text reaches the adversary's
        // and the adjudicator's prompts via the graph - and, to prove it is the graph
        // path and not threading, that the lens's STDOUT (which is no longer captured)
        // never appears in a later prompt.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("combat.rs"), "fn combat() {}\n").unwrap();
        let grep = crate::grounder::Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:").unwrap();

        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lens".into()],
                adversary: "adversary".into(),
                adjudicator: "adj".into(),
                coverage: "combat".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // Only the LENS emits the ReviewFinding. The lens's own stdout is a distinct
        // marker we expect to NEVER reach a later prompt (threading is gone).
        let mut emits_by_agent = HashMap::new();
        emits_by_agent.insert(
            "lens".to_string(),
            vec![(
                contextgraph::TYPE_REVIEW_FINDING.to_string(),
                json!({
                    "id": "f1",
                    "summary": "FINDING_SUMMARY_skips_buffer_authority",
                    "about": ["combat.rs"],
                }),
            )],
        );
        let mut output_by_agent = HashMap::new();
        output_by_agent.insert(
            "lens".to_string(),
            "LENS_STDOUT_must_not_thread".to_string(),
        );
        output_by_agent.insert("adj".to_string(), r#"{"verdict":"approve"}"#.to_string());
        // The adversary emits its finding to the graph; a substantive stdout keeps it out
        // of the Gap-18 respawn loop (empty would read as an infrastructure fault).
        output_by_agent.insert("adversary".to_string(), "ADV_reviewed".to_string());
        let driver = Stub {
            emits_by_agent,
            output_by_agent,
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: Some(&grep),
            graph: Some(&graph),
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["review"].status, ledger::Status::Integrated);

        // The finding reached the adversary and the adjudicator THROUGH THE GRAPH:
        // each grounded after the lens emitted it, so `graph_context` surfaced it.
        let adv_prompt = driver.prompts_for("adversary").pop().unwrap();
        assert!(
            adv_prompt.contains("FINDING_SUMMARY_skips_buffer_authority"),
            "the adversary must retrieve the lens's finding via the graph; prompt was:\n{adv_prompt}"
        );
        assert!(
            adv_prompt.contains("Findings other reviewers have already raised"),
            "the finding must arrive under the graph_context findings header; prompt was:\n{adv_prompt}"
        );
        let adj_prompt = driver.prompts_for("adj").pop().unwrap();
        assert!(
            adj_prompt.contains("FINDING_SUMMARY_skips_buffer_authority"),
            "the adjudicator must retrieve the lens's finding via the graph; prompt was:\n{adj_prompt}"
        );

        // Proof it is the graph path, not threading: the lens's STDOUT is no longer
        // captured, so it appears in NO later agent's prompt.
        assert!(
            !adv_prompt.contains("LENS_STDOUT_must_not_thread"),
            "the lens's stdout must NOT be threaded into the adversary's prompt; threading is replaced by the graph"
        );
        assert!(
            !adj_prompt.contains("LENS_STDOUT_must_not_thread"),
            "the lens's stdout must NOT be threaded into the adjudicator's prompt; threading is replaced by the graph"
        );

        // The finding really landed in the graph as a KIND_FINDING ABOUT combat.rs.
        let g = graph.subgraph(&["combat.rs".to_string()], 2).unwrap();
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "f1" && n.kind == contextgraph::KIND_FINDING),
            "the emitted ReviewFinding must fold into a KIND_FINDING node in the graph"
        );
    }

    #[test]
    fn review_agents_emit_findings_via_the_review_protocol() {
        // Item 3: a lens / adversary prompt must carry the REVIEW_PROTOCOL telling it
        // to record each finding as a ReviewFinding; the adjudicator's must NOT (it
        // ends with its verdict line, not a finding emit).
        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lens".into()],
                adversary: "adversary".into(),
                adjudicator: "adj".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();
        let lens_prompt = driver.prompts_for("lens").pop().unwrap();
        assert!(
            lens_prompt.contains("ReviewFinding"),
            "a lens must be told to emit findings as ReviewFindings; prompt was:\n{lens_prompt}"
        );
        let adv_prompt = driver.prompts_for("adversary").pop().unwrap();
        assert!(
            adv_prompt.contains("ReviewFinding"),
            "the adversary must be told to emit its findings as ReviewFindings; prompt was:\n{adv_prompt}"
        );
        let adj_prompt = driver.prompts_for("adj").pop().unwrap();
        assert!(
            !adj_prompt.contains("ReviewFinding"),
            "the adjudicator emits a verdict, not findings; prompt was:\n{adj_prompt}"
        );
    }

    #[test]
    fn unparseable_adjudicator_output_blocks_integration() {
        // Item 2 (fail-closed): an adjudicator whose output has no parseable verdict
        // does NOT approve - integration is blocked and the unit escalates, even with
        // no static gates. (Prose, not JSON.)
        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lens".into()],
                adjudicator: "adj".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: "the diff looks fine to me, ship it".into(), // no JSON verdict
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["review"].status,
            ledger::Status::Escalated,
            "an unparseable adjudicator verdict must NOT approve (fail-closed)"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
            "an unapproved unit must emit no UnitIntegrated"
        );
    }

    #[test]
    fn failing_gate_evidence_threaded_into_retry_prompt() {
        // Item 3 (spec 02): a gate that fails the first attempt must thread its compact
        // evidence into the SECOND attempt's prompt for that unit. The first prompt has
        // no prior-failure block; the second names the failing gate's evidence.
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow
            .gates
            .insert("flaky".into(), gate_def("unused"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["flaky".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let flaky = FlakyGate {
            fail_first: 1,
            runs: AtomicU32::new(0),
            evidence: "FAIL\nGATE_EVIDENCE_clippy_lint".into(),
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &flaky,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Integrated,
            "the second attempt's gate passes, so the unit integrates"
        );
        let prompts = driver.prompts_for("worker");
        assert!(
            prompts.len() >= 2,
            "the worker must have been retried at least once; got {} spawn(s)",
            prompts.len()
        );
        assert!(
            !prompts[0].contains("GATE_EVIDENCE_clippy_lint"),
            "the FIRST attempt's prompt must have no prior-failure block; prompt was:\n{}",
            prompts[0]
        );
        assert!(
            prompts[1].contains("GATE_EVIDENCE_clippy_lint"),
            "the SECOND attempt's prompt must carry the failing gate's evidence; prompt was:\n{}",
            prompts[1]
        );
        assert!(
            prompts[1].contains("Your previous attempt failed these gates"),
            "the retry prompt must carry an explicit prior-failed-gates block; prompt was:\n{}",
            prompts[1]
        );
    }

    #[test]
    fn unit_evidence_is_populated_after_a_run() {
        // Item 4: every TYPE_UNIT_STATUS emit used to omit `evidence`, leaving the
        // ledger's Unit.evidence always empty. After a passing run the unit's projected
        // evidence must be non-empty (gate summaries for verified, etc.).
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["ok".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(rs.units["s"].status, ledger::Status::Integrated);
        assert!(
            !rs.units["s"].evidence.is_empty(),
            "a unit's projected evidence must be non-empty after a run; was {:?}",
            rs.units["s"].evidence
        );
        assert!(
            rs.units["s"].evidence.contains_key("verified"),
            "the verified status must carry gate evidence; was {:?}",
            rs.units["s"].evidence
        );
    }

    #[test]
    fn adjudicator_rejection_reasoning_threaded_into_retry_prompt() {
        // Item 5: when the per-unit adjudicator rejects, its reasoning must reach the
        // NEXT attempt's prompt for that unit. The adjudicator always rejects (with a
        // distinctive reason), so the worker is retried; its second prompt must carry
        // the reject reason. The unit ultimately escalates (the reject never relents).
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: String::new(),
            adjudicator: "adj".into(),
        };
        cfg.workflow.stages.insert(
            "implement".into(),
            Stage {
                name: "implement".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let mut output_by_agent = HashMap::new();
        output_by_agent.insert(
            "adj".to_string(),
            r#"{"verdict":"reject","reason":"REJECT_REASON_missing_tests"}"#.to_string(),
        );
        // A substantive lens result so only the adjudicator's reject drives remediation
        // (an empty lens would trip the Gap-18 respawn loop instead).
        output_by_agent.insert("lens".to_string(), "reviewed: concerns noted".to_string());
        let driver = Stub {
            output_by_agent,
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs.units["implement"].status,
            ledger::Status::Escalated,
            "a perpetually-rejecting adjudicator escalates the unit"
        );
        let prompts = driver.prompts_for("worker");
        assert!(
            prompts.len() >= 2,
            "the worker must have been retried after the reject; got {} spawn(s)",
            prompts.len()
        );
        assert!(
            !prompts[0].contains("REJECT_REASON_missing_tests"),
            "the first prompt must have no prior-failure block; prompt was:\n{}",
            prompts[0]
        );
        assert!(
            prompts[1].contains("REJECT_REASON_missing_tests"),
            "the retry prompt must carry the adjudicator's rejection reasoning; prompt was:\n{}",
            prompts[1]
        );
        assert!(
            prompts[1].contains("Your previous attempt was rejected by review"),
            "the retry prompt must carry an explicit prior-rejection block; prompt was:\n{}",
            prompts[1]
        );
    }

    #[test]
    fn fan_out_lens_does_not_integrate() {
        // Item 6: a standalone fan-out review lens must NOT be given a worktree and
        // must NOT integrate - so even a lens that writes code could never get it
        // merged into the base repo. We run the lens on the fan-out review path against
        // a real repo; the base repo must gain NO commit from the lens, the lens must
        // have spawned with isolation = false (no worktree), and no per-lens
        // UnitIntegrated may be emitted.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let head_before = git_head(&repo_path);

        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lens".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // A substantive lens result so the review proceeds (an empty result would trip
        // the Gap-18 respawn loop).
        let driver = Stub {
            output: "reviewed the diff".into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).unwrap();

        let (isolation, _parallel) = driver
            .opts_by_agent
            .lock()
            .unwrap()
            .get("lens")
            .copied()
            .unwrap();
        assert!(
            !isolation,
            "a fan-out review lens must run with NO worktree (isolation = false)"
        );
        assert_eq!(
            head_before,
            git_head(&repo_path),
            "a review lens must not produce any commit in the base repo"
        );
        // The lens wrote into the current dir, never a worktree path: no integration
        // event ever carried a real commit for it.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            !events.iter().any(|e| {
                e.type_ == ledger::TYPE_UNIT_INTEGRATED
                    && String::from_utf8_lossy(&e.data).contains("lens")
            }),
            "no per-lens UnitIntegrated must be emitted on the fan-out review path"
        );
    }

    #[test]
    fn review_only_stage_records_no_artifact_truthfully() {
        // Item 7: a standalone review stage produces no code artifact, so it must not
        // fabricate an integration with an empty commit hash. Its terminal status is
        // `reviewed` and its UnitIntegrated commit is the explicit no-artifact marker.
        let mut cfg = Config::default();
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.stages.insert(
            "review".into(),
            Stage {
                name: "review".into(),
                agents: vec!["lens".into()],
                adjudicator: "adj".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub {
            output: r#"{"verdict":"approve"}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        // The stage reached its DAG-terminal state with the EXPLICIT no-artifact
        // marker, not an empty (dropped-looking) commit hash.
        assert_eq!(
            rs.units["review"].commit, REVIEW_ONLY_NO_ARTIFACT,
            "a review-only stage must record an explicit no-artifact marker, not an empty commit"
        );
        assert_ne!(
            rs.units["review"].commit, "",
            "a review-only stage must NOT record an empty commit hash"
        );
        // It was truthfully marked reviewed before reaching its terminal state.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| {
                e.type_ == ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"reviewed\"")
            }),
            "a review-only stage must emit a truthful `reviewed` status"
        );
    }

    #[test]
    fn two_erroring_stages_both_leave_a_record() {
        // Item 8: run_wave collapses a batch to a single returned error, dropping the
        // rest. Both erroring stages must still leave a record (a lesson) naming the
        // stage and its error. Two independent stages each reference an agent missing
        // from cfg.agents, so each errors inside run_single_stage.
        let mut cfg = Config::default();
        // NOTE: agents map is intentionally missing "ghost1"/"ghost2" so each stage's
        // run_single_stage hits the unknown-agent error.
        cfg.workflow.stages.insert(
            "s1".into(),
            Stage {
                name: "s1".into(),
                agent: "ghost1".into(),
                ..Default::default()
            },
        );
        cfg.workflow.stages.insert(
            "s2".into(),
            Stage {
                name: "s2".into(),
                agent: "ghost2".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        // The wave returns the first error (run halts), but BOTH stages must have left
        // a record before the collapse.
        let _ = run(&cfg, &deps);
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let names_with_lessons: Vec<bool> = ["s1", "s2"]
            .iter()
            .map(|name| {
                events.iter().any(|e| {
                    e.type_ == contextgraph::TYPE_LESSON_LEARNED
                        && String::from_utf8_lossy(&e.data).contains(*name)
                })
            })
            .collect();
        assert!(
            names_with_lessons[0],
            "the first erroring stage (s1) must leave a lesson record"
        );
        assert!(
            names_with_lessons[1],
            "the second erroring stage (s2) must ALSO leave a record - errors after the first must not be dropped"
        );
    }

    #[test]
    fn single_wide_wave_overruns_budget_and_is_stopped() {
        // Item 9: the budget breaker must trip at SPAWN granularity, mid-wave. Three
        // INDEPENDENT implementer stages form one wide wave; with a budget of 1, only
        // one may spawn - the other two are refused, the breaker trips, and the run is
        // actually stopped. Fewer than 3 integrate, and BudgetExhausted is recorded.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.defaults.budget = 1;
        for name in ["w1", "w2", "w3"] {
            cfg.workflow.stages.insert(
                name.into(),
                Stage {
                    name: name.into(),
                    agent: "a".into(),
                    gates: vec!["ok".into()],
                    ..Default::default()
                },
            );
        }
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        let integrated = ["w1", "w2", "w3"]
            .iter()
            .filter(|n| {
                rs.units
                    .get(**n)
                    .map(|u| u.status == ledger::Status::Integrated)
                    .unwrap_or(false)
            })
            .count();
        assert!(
            integrated < 3,
            "a budget of 1 must stop a wide wave mid-flight; {integrated} of 3 integrated"
        );
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_BUDGET_EXHAUSTED),
            "overrunning the budget mid-wave must record BudgetExhausted"
        );
        // No more than one real implementer spawn was admitted against the budget of 1.
        assert!(
            driver
                .call_order
                .lock()
                .unwrap()
                .iter()
                .filter(|a| *a == "a")
                .count()
                <= 1,
            "the budget breaker must refuse spawns beyond the budget, mid-wave"
        );
    }

    #[test]
    fn validate_acyclic_detects_a_cycle() {
        // Item 10: the function formerly named `topo_sort` computed an order nobody
        // consumed; it is now `validate_acyclic`, a pure cycle check whose name matches
        // its behavior. It returns Ok for an acyclic DAG and Err for a cycle.
        let mut acyclic: BTreeMap<String, Stage> = BTreeMap::new();
        acyclic.insert(
            "a".into(),
            Stage {
                name: "a".into(),
                ..Default::default()
            },
        );
        acyclic.insert(
            "b".into(),
            Stage {
                name: "b".into(),
                needs: vec!["a".into()],
                ..Default::default()
            },
        );
        assert!(
            validate_acyclic(&acyclic).is_ok(),
            "an acyclic DAG must validate"
        );

        let mut cyclic: BTreeMap<String, Stage> = BTreeMap::new();
        cyclic.insert(
            "x".into(),
            Stage {
                name: "x".into(),
                needs: vec!["y".into()],
                ..Default::default()
            },
        );
        cyclic.insert(
            "y".into(),
            Stage {
                name: "y".into(),
                needs: vec!["x".into()],
                ..Default::default()
            },
        );
        assert!(
            validate_acyclic(&cyclic).is_err(),
            "a dependency cycle must be rejected"
        );
    }

    /// A gate runner that records the ORDER in which gate ids were run (so a test can
    /// assert a deferred gate ran at the phase boundary, after every inline gate, and
    /// exactly once). Each named gate's pass/fail is configurable.
    struct RecordingRunner {
        /// The gate ids run, in invocation order.
        calls: Mutex<Vec<String>>,
        /// The CARGO_TARGET_DIR (`target_dir`) handed to each run, in invocation order -
        /// lets a test assert two units' gate environments never share a target (Gap 19).
        targets: Mutex<Vec<String>>,
        /// When set, a run with a non-empty `target_dir` MATERIALIZES that dir on disk (as a
        /// real cargo build would), so a graceful-lifecycle test can prove the per-unit cache
        /// is reclaimed when the unit's worktree is later removed (Gap 19).
        materialize_cache: bool,
        /// Gate ids that must FAIL; everything else passes.
        fail: HashSet<String>,
    }
    impl RecordingRunner {
        fn new(fail: &[&str]) -> Self {
            RecordingRunner {
                calls: Mutex::new(Vec::new()),
                targets: Mutex::new(Vec::new()),
                materialize_cache: false,
                fail: fail.iter().map(|s| s.to_string()).collect(),
            }
        }
        /// Like [`Self::new`] but the runner also creates each non-empty `target_dir` on disk -
        /// a stand-in for a real cargo build populating the per-unit cache (Gap 19).
        fn materializing() -> Self {
            RecordingRunner {
                materialize_cache: true,
                ..RecordingRunner::new(&[])
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn targets(&self) -> Vec<String> {
            self.targets.lock().unwrap().clone()
        }
    }
    impl gate::Runner for RecordingRunner {
        fn run(&self, g: &Gate, _dir: &str, target: &str) -> gate::GateResult {
            self.calls.lock().unwrap().push(g.id.clone());
            self.targets.lock().unwrap().push(target.to_string());
            if self.materialize_cache && !target.is_empty() {
                let _ = std::fs::create_dir_all(target);
                let _ = std::fs::write(std::path::Path::new(target).join("built.rlib"), b"x");
            }
            let pass = !self.fail.contains(&g.id);
            gate::GateResult {
                pass,
                evidence: if pass {
                    "PASS".into()
                } else {
                    format!("FAIL\ngate {} failed", g.id)
                },
            }
        }
    }

    #[test]
    fn a_deferred_gate_runs_once_at_the_phase_boundary_not_inline() {
        // A stage with both an inline (core) gate and a deferred gate. The deferred
        // gate must NOT run during the unit's inline lifecycle - the unit integrates on
        // its inline gate alone - and must run EXACTLY ONCE at the run's end-of-run
        // phase boundary, after the unit has integrated.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("inline".into(), gate_def("true"));
        cfg.workflow.gates.insert(
            "deferred".into(),
            config::Gate {
                run: "true".into(),
                kind: "deferred".into(),
            },
        );
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["inline".into(), "deferred".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        let runner = RecordingRunner::new(&[]);
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &runner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        // The unit integrated on its INLINE gate (the deferred gate played no part in
        // the unit's own lifecycle decision).
        assert_eq!(rs.units["s"].status, ledger::Status::Integrated);

        // The deferred gate ran exactly once, and it ran AFTER the inline gate - i.e.
        // at the phase boundary, never inline per unit.
        let calls = runner.calls();
        assert_eq!(
            calls.iter().filter(|c| *c == "deferred").count(),
            1,
            "the deferred gate must run exactly once (at the phase boundary), not per unit; calls: {calls:?}"
        );
        let inline_at = calls.iter().position(|c| c == "inline").unwrap();
        let deferred_at = calls.iter().position(|c| c == "deferred").unwrap();
        assert!(
            deferred_at > inline_at,
            "the deferred gate must run AFTER the inline gate (at end-of-run), not during the inline lifecycle; calls: {calls:?}"
        );

        // The deferred gate emitted a GateVerdict at the boundary, and (since it
        // passed) the run is fully done.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| {
                e.type_ == contextgraph::TYPE_GATE_VERDICT
                    && String::from_utf8_lossy(&e.data).contains("\"gate\":\"deferred\"")
            }),
            "the deferred gate must emit a GateVerdict at the phase boundary"
        );
        assert!(
            rs.done() && rs.fully_done(&[]),
            "a passing deferred gate must leave the run reported fully done"
        );
    }

    #[test]
    fn a_failing_deferred_gate_is_surfaced_and_the_run_is_not_done() {
        // A failing DEFERRED gate must be surfaced truthfully: a DeferredGateFailed
        // event is recorded AND the run is reported not-fully-done, even though the
        // unit itself integrated on its (passing) inline gate. A deferred failure must
        // NOT silently pass as success.
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.gates.insert("inline".into(), gate_def("true"));
        cfg.workflow.gates.insert(
            "deferred".into(),
            config::Gate {
                run: "false".into(),
                kind: "deferred".into(),
            },
        );
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                gates: vec!["inline".into(), "deferred".into()],
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        let driver = Stub::new();
        // The deferred gate fails; the inline gate passes.
        let runner = RecordingRunner::new(&["deferred"]);
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &runner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        // The unit still integrated - its INLINE gate passed; the deferred failure is a
        // run-level concern, not a per-unit one.
        assert_eq!(rs.units["s"].status, ledger::Status::Integrated);

        // The failure is recorded as an event naming the gate...
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.iter().any(|e| {
                e.type_ == TYPE_DEFERRED_GATE_FAILED
                    && String::from_utf8_lossy(&e.data).contains("\"gate\":\"deferred\"")
            }),
            "a failing deferred gate must emit a DeferredGateFailed event naming the gate"
        );
        // ...and the run is reported NOT fully done despite every unit integrating.
        assert!(
            !rs.done(),
            "a failing deferred gate must leave the run reported not done"
        );
        assert!(
            !rs.fully_done(&[]),
            "a failing deferred gate must leave the run reported not fully done"
        );
        assert!(
            rs.deferred_gate_failed,
            "the run state must record the deferred-gate failure"
        );
    }

    #[test]
    fn a_replayed_step_re_runs_no_recorded_gate_and_appends_no_duplicate_events() {
        // spec 04, criterion 4: a step re-running the conductor over recorded history
        // appends no unit event or gate verdict twice, and a recorded GateVerdict is
        // REPLAYED without re-running its gate command (finding adv-replay-dup-lifecycle).
        use crate::driver::replay::ReplayDriver;

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("check".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "u".into(),
            Stage {
                name: "u".into(),
                agent: "worker".into(),
                gates: vec!["check".into()],
                // No review panel and on_pass:none: the unit verifies and STAYS at
                // `verified` (never integrates), so every step re-runs it fresh over the
                // recorded implementer result - the exact shape a mid-flight unit replays.
                on_pass: "none".into(),
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        // A courier already recorded the implementer's result, so the replay driver
        // ANSWERS the implementer spawn (never parks it) and the gate is reached both
        // steps.
        crate::spawn::record_result(
            &st,
            &crate::spawn::SpawnResult::ok(spawn_id("u", ROLE_IMPLEMENTER, 0), "done"),
        )
        .unwrap();

        // Two consecutive steps replay the SAME recorded history. The gate runner
        // records every command it runs, so a re-run of an already-recorded gate would
        // show up as a second call.
        let runner = RecordingRunner::new(&[]);
        for _ in 0..2 {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
        }

        // The gate command ran EXACTLY ONCE across both steps: the second step replayed
        // the recorded verdict instead of re-running the command.
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|c| c.as_str() == "check")
                .count(),
            1,
            "a recorded GateVerdict must be replayed, its command never re-run"
        );

        let events = st.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let count_status = |status: &str| {
            events
                .iter()
                .filter(|e| {
                    e.type_ == ledger::TYPE_UNIT_STATUS
                        && String::from_utf8_lossy(&e.data)
                            .contains(&format!("\"status\":\"{status}\""))
                })
                .count()
        };
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == ledger::TYPE_UNIT_STARTED)
                .count(),
            1,
            "UnitStarted is appended once, not once per replay step"
        );
        assert_eq!(
            count_status("green"),
            1,
            "green is appended once across steps"
        );
        assert_eq!(
            count_status("verified"),
            1,
            "verified is appended once across steps"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.type_ == contextgraph::TYPE_GATE_VERDICT)
                .count(),
            1,
            "the GateVerdict is appended once - the replay re-emits none"
        );
    }

    #[test]
    fn a_re_step_replays_a_recorded_deferred_gate_without_re_running_it() {
        // spec 04, criterion 4: a deferred gate's recorded verdict is replayed on a
        // re-step, never re-running the (whole-tree) command or duplicating the verdict.
        use crate::driver::replay::ReplayDriver;

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("inline".into(), gate_def("true"));
        cfg.workflow.gates.insert(
            "deferred".into(),
            config::Gate {
                run: "true".into(),
                kind: "deferred".into(),
            },
        );
        cfg.workflow.stages.insert(
            "u".into(),
            Stage {
                name: "u".into(),
                agent: "worker".into(),
                gates: vec!["inline".into(), "deferred".into()],
                on_pass: "none".into(),
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        crate::spawn::record_result(
            &st,
            &crate::spawn::SpawnResult::ok(spawn_id("u", ROLE_IMPLEMENTER, 0), "done"),
        )
        .unwrap();

        let runner = RecordingRunner::new(&[]);
        for _ in 0..2 {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
        }

        let calls = runner.calls();
        assert_eq!(
            calls.iter().filter(|c| c.as_str() == "deferred").count(),
            1,
            "the deferred gate runs once across steps, then replays; calls: {calls:?}"
        );
        assert_eq!(
            calls.iter().filter(|c| c.as_str() == "inline").count(),
            1,
            "the inline gate runs once across steps, then replays; calls: {calls:?}"
        );

        let events = st.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let verdicts = |gate: &str| {
            events
                .iter()
                .filter(|e| {
                    e.type_ == contextgraph::TYPE_GATE_VERDICT
                        && String::from_utf8_lossy(&e.data)
                            .contains(&format!("\"gate\":\"{gate}\""))
                })
                .count()
        };
        assert_eq!(
            verdicts("deferred"),
            1,
            "no duplicate deferred GateVerdict on replay"
        );
        assert_eq!(
            verdicts("inline"),
            1,
            "no duplicate inline GateVerdict on replay"
        );
    }

    #[test]
    fn a_parked_step_never_records_a_partial_tree_deferred_verdict() {
        // BLOCKER 1 (finding adv-deferred-replay-locks-partial-tree): under stepwise
        // replay an early step empties the wave loop with the unit PARKED (its
        // implementer not yet recorded), so NOTHING has run against the tree. The
        // deferred gate must NOT run/record then - doing so would lock in a base/partial
        // -tree verdict that every later step replays, so the deferred gate would NEVER
        // validate the assembled tree. It must instead run exactly once, on the step
        // that drains the park and the unit reaches its settled state.
        use crate::driver::replay::ReplayDriver;

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("inline".into(), gate_def("true"));
        cfg.workflow.gates.insert(
            "deferred".into(),
            config::Gate {
                run: "true".into(),
                kind: "deferred".into(),
            },
        );
        cfg.workflow.stages.insert(
            "u".into(),
            Stage {
                name: "u".into(),
                agent: "worker".into(),
                gates: vec!["inline".into(), "deferred".into()],
                // on_pass:none lets the unit settle at `verified` (no repo to integrate
                // into) - the exact "unit verified but not integrated" state the
                // reviewer's probe reached on step 2.
                on_pass: "none".into(),
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        let runner = RecordingRunner::new(&[]);

        // STEP 1: the implementer is NOT recorded, so it PARKS. The unit never reaches
        // its gates, and the run has not converged.
        {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
        }
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|c| c.as_str() == "deferred")
                .count(),
            0,
            "the deferred gate must NOT run while the unit is parked (partial tree)"
        );
        let deferred_verdicts = |events: &[Event]| {
            events
                .iter()
                .filter(|e| {
                    e.type_ == contextgraph::TYPE_GATE_VERDICT
                        && String::from_utf8_lossy(&e.data).contains("\"gate\":\"deferred\"")
                })
                .count()
        };
        let events = st.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            deferred_verdicts(&events),
            0,
            "a parked step records NO deferred GateVerdict - no partial-tree verdict to lock in"
        );

        // A courier records the implementer's result: the park drains.
        crate::spawn::record_result(
            &st,
            &crate::spawn::SpawnResult::ok(spawn_id("u", ROLE_IMPLEMENTER, 0), "done"),
        )
        .unwrap();

        // STEP 2: the implementer replays, the unit reaches `verified`, and the run
        // converges - so NOW the deferred gate runs, once, against the settled tree.
        {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
        }
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|c| c.as_str() == "deferred")
                .count(),
            1,
            "the deferred gate runs exactly once, on the converged step that drained the park"
        );
        assert_eq!(
            deferred_verdicts(&st.read_stream(STREAM, 0, Direction::Forward).unwrap()),
            1,
            "the deferred verdict is recorded on the converged step, against the assembled tree"
        );

        // STEP 3: re-stepping the completed run replays the recorded verdict - it never
        // re-runs the command nor duplicates the verdict.
        {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
        }
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|c| c.as_str() == "deferred")
                .count(),
            1,
            "a re-step replays the recorded deferred verdict, never re-running its command"
        );
        assert_eq!(
            deferred_verdicts(&st.read_stream(STREAM, 0, Direction::Forward).unwrap()),
            1,
            "no duplicate deferred GateVerdict across the converged step and its replays"
        );
    }

    #[test]
    fn a_manual_review_paused_unit_defers_the_whole_tree_gate() {
        // PRIMARY BLOCKER (findings rf-converged-ignores-budget-refusal /
        // adv-confirm-converged-nonpark-partial-tree, F58/F60): a manual-review-paused
        // unit emits ManualReview and returns pending WITHOUT parking - it is
        // terminal-inserted but never integrated, and it does ZERO work this step (the
        // pause is checked BEFORE the implementer or the inline gate ever runs). The old
        // `!parked && stages.all(terminal.contains)` guard was TRUE against this
        // partial/base tree, so it ran+recorded the whole-tree deferred verdict against a
        // tree missing the paused unit - and the run-scoped key locked that partial-tree
        // result in forever, replayed on every later step even after the human approves.
        // The tree is NOT final while a unit awaits a human, so the deferred gate must be
        // DEFERRED: it must run/record NOTHING here.
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("inline".into(), gate_def("true"));
        cfg.workflow.gates.insert(
            "deferred".into(),
            config::Gate {
                run: "true".into(),
                kind: "deferred".into(),
            },
        );
        // A Manual-autonomy stage pauses on its gate awaiting a human (§4.3). It also
        // references the deferred gate, so the phase boundary would collect+run it if it
        // (wrongly) considered the tree final.
        cfg.workflow.stages.insert(
            "m".into(),
            Stage {
                name: "m".into(),
                agent: "worker".into(),
                gates: vec!["inline".into(), "deferred".into()],
                autonomy: "manual".into(),
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        let runner = RecordingRunner::new(&[]);
        // A blocking Stub driver never parks; the pause is a non-park terminal path.
        let driver = Stub::new();
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &runner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        assert_ne!(
            rs.units["m"].status,
            ledger::Status::Integrated,
            "the manual unit is paused, not integrated"
        );
        let events = st.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            events.iter().any(|e| e.type_ == TYPE_MANUAL_REVIEW),
            "the manual stage must emit ManualReview (it is paused, awaiting a human)"
        );
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|c| c.as_str() == "deferred")
                .count(),
            0,
            "the deferred gate must NOT run while a unit is manual-review-paused: the \
             tree is missing that unit's work, so it is not final"
        );
        let deferred_verdicts = events
            .iter()
            .filter(|e| {
                e.type_ == contextgraph::TYPE_GATE_VERDICT
                    && String::from_utf8_lossy(&e.data).contains("\"gate\":\"deferred\"")
            })
            .count();
        assert_eq!(
            deferred_verdicts, 0,
            "a manual-review-paused step records NO deferred GateVerdict - there is no \
             partial-tree verdict to lock in for every later step to replay"
        );
    }

    #[test]
    fn an_escalated_dep_still_runs_the_whole_tree_deferred_gate() {
        // SECONDARY BLOCKER (finding adv-converged-escalated-dep-suppresses-deferred,
        // F59): an escalated unit is terminal-forever-yet-never-integrated, and
        // `ready_stages` gates its dependents on INTEGRATED deps - so a dependent behind
        // an escalated dep NEVER becomes ready, never runs, and never enters `terminal`.
        // The old `stages.all(terminal.contains)` clause then stayed false FOREVER and
        // permanently SUPPRESSED the whole-tree deferred gate (e.g. a security scan) on
        // every run and resume - a regression from the pre-diff behavior where the
        // deferred gate ran unconditionally at run end. When a unit escalates-forever the
        // tree is as-assembled as it will ever be, so the deferred gate MUST still run
        // against it, once, rather than be suppressed.
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("lens".into(), agent("lens"));
        cfg.agents.insert("adversary".into(), agent("adversary"));
        cfg.agents.insert("adj".into(), agent("adj"));
        cfg.workflow.gates.insert("inline".into(), gate_def("true"));
        cfg.workflow.gates.insert(
            "deferred".into(),
            config::Gate {
                run: "true".into(),
                kind: "deferred".into(),
            },
        );
        cfg.workflow.defaults.review = config::ReviewPanel {
            lenses: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
        };
        // `s` passes its inline gate but the adjudicator ALWAYS rejects, so it remediates
        // to the bound and ESCALATES. It also references the deferred whole-tree gate.
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["inline".into(), "deferred".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        // `d` depends on `s`. Because `s` escalates (never integrates), `d` is
        // UNREACHABLE - it never becomes ready and never enters `terminal`.
        cfg.workflow.stages.insert(
            "d".into(),
            Stage {
                name: "d".into(),
                agent: "worker".into(),
                needs: vec!["s".into()],
                gates: vec!["inline".into()],
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        let runner = RecordingRunner::new(&[]);
        // Every review agent returns a reject verdict; only the adjudicator's gates the
        // unit, so `s` retries to the bound and escalates.
        let driver = Stub {
            output: r#"{"verdict":"reject","issues":[]}"#.into(),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &runner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Escalated,
            "the always-rejected unit must escalate, not integrate"
        );
        assert!(
            !rs.units.contains_key("d"),
            "the dependent behind the escalated unit is unreachable: it never starts"
        );
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|c| c.as_str() == "deferred")
                .count(),
            1,
            "the whole-tree deferred gate must STILL run once against the as-assembled \
             tree when a unit escalates-forever - not be suppressed permanently"
        );
        let deferred_verdicts = |events: &[Event]| {
            events
                .iter()
                .filter(|e| {
                    e.type_ == contextgraph::TYPE_GATE_VERDICT
                        && String::from_utf8_lossy(&e.data).contains("\"gate\":\"deferred\"")
                })
                .count()
        };
        assert_eq!(
            deferred_verdicts(&st.read_stream(STREAM, 0, Direction::Forward).unwrap()),
            1,
            "the deferred verdict is recorded once, against the as-assembled escalated tree"
        );

        // RESUME: `s` is folded terminal (Escalated) and skipped, `d` stays unreachable,
        // and nothing is transiently pending - so the tree is still final. The recorded
        // deferred verdict REPLAYS: the command never re-runs and no duplicate verdict is
        // appended (spec 04, criterion 4 - replay is idempotent).
        let rs2 = run(&cfg, &deps).unwrap();
        assert_eq!(
            rs2.units["s"].status,
            ledger::Status::Escalated,
            "the escalated unit stays terminal across resume"
        );
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|c| c.as_str() == "deferred")
                .count(),
            1,
            "a re-step replays the recorded deferred verdict, never re-running its command"
        );
        assert_eq!(
            deferred_verdicts(&st.read_stream(STREAM, 0, Direction::Forward).unwrap()),
            1,
            "no duplicate deferred GateVerdict across the escalated run and its replay"
        );
    }

    #[test]
    fn a_recorded_failing_deferred_verdict_re_surfaces_its_failure_on_replay() {
        // BLOCKER-adjacent (finding adv-deferred-failed-lost-on-crash / F50): the
        // GateVerdict and the DeferredGateFailed are two SEPARATE appends. Simulate a
        // crash between them - a recorded FAILING deferred verdict with NO
        // DeferredGateFailed yet - and assert the next step replays the verdict AND
        // re-surfaces the failure (keyed, so exactly once), so a red deferred gate can
        // never be reported as a finished run.
        use crate::driver::replay::ReplayDriver;

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow.gates.insert("inline".into(), gate_def("true"));
        cfg.workflow.gates.insert(
            "deferred".into(),
            config::Gate {
                run: "false".into(),
                kind: "deferred".into(),
            },
        );
        cfg.workflow.stages.insert(
            "u".into(),
            Stage {
                name: "u".into(),
                agent: "worker".into(),
                gates: vec!["inline".into(), "deferred".into()],
                on_pass: "none".into(),
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        // Run scoping (spec 06, unit 1): begin the run first so the recorded result and
        // deferred verdict below fall INSIDE the current run and are replayed on resume.
        crate::run::ensure_started(&st, &[]).unwrap();
        // The implementer result is recorded (the unit reaches `verified` and the run
        // converges), and a FAILING deferred verdict is already recorded under its replay
        // key - but the DeferredGateFailed is MISSING, exactly as a crash between the two
        // appends would leave it.
        crate::spawn::record_result(
            &st,
            &crate::spawn::SpawnResult::ok(spawn_id("u", ROLE_IMPLEMENTER, 0), "done"),
        )
        .unwrap();
        let verdict_key = deferred_gate_verdict_key("deferred");
        st.append(
            STREAM,
            ExpectedRevision::Any,
            &[Event::new(
                contextgraph::TYPE_GATE_VERDICT,
                serde_json::to_vec(&json!({"gate": "deferred", "pass": false, "evidence": "boom"}))
                    .unwrap(),
            )
            .with_meta(META_REPLAY_KEY, &verdict_key)],
        )
        .unwrap();

        // The gate runner would record any command it runs; the recorded verdict must be
        // replayed, its (whole-tree) command never re-run.
        let runner = RecordingRunner::new(&["deferred"]);
        let rs = {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap()
        };

        assert!(
            !runner.calls().iter().any(|c| c.as_str() == "deferred"),
            "the recorded failing deferred verdict is replayed, its command never re-run"
        );
        let count_failed = || {
            st.read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .iter()
                .filter(|e| e.type_ == TYPE_DEFERRED_GATE_FAILED)
                .count()
        };
        assert_eq!(
            count_failed(),
            1,
            "the lost DeferredGateFailed is re-surfaced from the replayed verdict, exactly once"
        );
        assert!(
            rs.deferred_gate_failed && !rs.done() && !rs.fully_done(&[]),
            "a red deferred gate is never reported as a finished run, even after the crash"
        );

        // Re-stepping does not append a second DeferredGateFailed (it is keyed).
        {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
        }
        assert_eq!(
            count_failed(),
            1,
            "the re-surfaced DeferredGateFailed is idempotent across further replays"
        );
    }

    #[test]
    fn a_replayed_fan_out_review_reject_appends_no_duplicate_unitfailed() {
        // BLOCKER 2 (findings rf-fanout-replay-dup-unitfailed / adv-confirm-fanout-dup-
        // unitfailed): a standalone fan-out review stage that REJECTS is `Failed` (not
        // terminal), so it is re-seeded ready every step. Before the fix it restarted at
        // attempt 0 each step, re-ran the recorded rejecting adjudicator, and re-appended
        // a duplicate UnitFailed - and the escalation bound never accumulated. Seeding
        // attempts from the log (mirroring the per-unit path) makes a replay step PARK at
        // the next unrecorded review-attempt without re-emitting the recorded failure.
        use crate::driver::replay::ReplayDriver;

        let mut cfg = Config::default();
        cfg.agents.insert("judge".into(), agent("judge"));
        cfg.workflow.stages.insert(
            "rev".into(),
            Stage {
                name: "rev".into(),
                // A fan-out review stage: empty agent + fan-out strategy, one
                // adjudicator, no lenses - so the adjudicator IS the frontier spawn.
                strategy: "fan-out".into(),
                adjudicator: "judge".into(),
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        // The adjudicator's attempt-0 verdict is recorded as a REJECT, so the replay
        // driver answers it (the review runs to a reject) instead of parking it.
        crate::spawn::record_result(
            &st,
            &crate::spawn::SpawnResult::ok(
                spawn_id("rev", ROLE_ADJUDICATOR, 0),
                "{\"verdict\":\"reject\"}",
            ),
        )
        .unwrap();

        let runner = RecordingRunner::new(&[]);
        let count_failed = || {
            st.read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .iter()
                .filter(|e| e.type_ == ledger::TYPE_UNIT_FAILED)
                .count()
        };

        // Three consecutive replay steps over the SAME recorded rejecting verdict.
        for _ in 0..3 {
            let driver = ReplayDriver::new(&st);
            let deps = Deps {
                store: &st,
                driver: &driver,
                gates: &runner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            run(&cfg, &deps).unwrap();
        }

        assert_eq!(
            count_failed(),
            1,
            "the recorded review reject yields ONE UnitFailed across replays, not one per step"
        );
        // The bound accumulates from the log: the folded attempt count is 1 (the first
        // rejected attempt), not reset to 0 every step.
        let rs = ledger::project(&st.read_stream(STREAM, 0, Direction::Forward).unwrap()).unwrap();
        assert_eq!(
            rs.units["rev"].attempts, 1,
            "the escalation bound accumulates across steps (attempts folded from the log)"
        );
        assert_eq!(
            rs.units["rev"].status,
            ledger::Status::Failed,
            "a rejected review stage stays Failed (non-terminal), resuming on the next step"
        );
    }

    /// The current HEAD commit hash of a git repo, for asserting a lens produced no
    /// commit.
    fn git_head(repo: &str) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().to_str().unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "t"],
            &["commit", "--allow-empty", "-q", "-m", "init"],
        ] {
            std::process::Command::new("git")
                .arg("-C")
                .arg(p)
                .args(args)
                .output()
                .unwrap();
        }
        dir
    }

    /// FIX 2: a gate runner that PASSES only when the worktree it is handed (`dir`)
    /// is CLEAN - `git status --porcelain` is empty. It fails on a dirty tree. So the
    /// gate passes iff the conductor committed the implementer's work BEFORE running
    /// it; if the conductor gated the dirty worktree (the false-green bug) this gate
    /// would see the uncommitted file and fail.
    struct CleanTreeGate {
        saw_dirty: std::sync::atomic::AtomicBool,
    }
    impl gate::Runner for CleanTreeGate {
        fn run(&self, _g: &Gate, dir: &str, _target: &str) -> gate::GateResult {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["status", "--porcelain"])
                .output()
                .unwrap();
            let porcelain = String::from_utf8_lossy(&out.stdout);
            let dirty = !porcelain.trim().is_empty();
            if dirty {
                self.saw_dirty.store(true, Ordering::SeqCst);
            }
            gate::GateResult {
                pass: !dirty,
                evidence: if dirty {
                    format!("worktree was dirty when gated: {}", porcelain.trim())
                } else {
                    "tree clean".into()
                },
            }
        }
    }

    #[test]
    fn gate_measures_the_committed_artifact_not_the_dirty_worktree() {
        // FIX 2 (the false-green): the implementer writes an uncommitted file; the
        // gate passes ONLY against a clean, committed tree. The conductor must commit
        // the worktree BEFORE gating, so the gate sees the committed artifact (clean
        // tree) and the unit integrates. If the conductor gated the dirty worktree -
        // the bug a real run hit, where `cargo test` passed on uncommitted tests that
        // never reached the committed artifact - this gate would fail and the unit
        // would never integrate.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.workflow
            .gates
            .insert("clean".into(), gate_def("unused"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["clean".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
        let st = Store::open(":memory:").unwrap();
        // The implementer writes a file but never commits it - exactly the dirty
        // worktree a real implementer leaves before integration.
        let driver = Stub {
            write_file: Some("feature.rs".into()),
            ..Stub::new()
        };
        let runner = CleanTreeGate {
            saw_dirty: std::sync::atomic::AtomicBool::new(false),
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &runner,
            repo: repo_path,
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();
        assert!(
            !runner.saw_dirty.load(Ordering::SeqCst),
            "the gate must run against a COMMITTED (clean) tree - the conductor commits before gating"
        );
        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Integrated,
            "with the worktree committed before gating, the clean-tree gate passes and the unit integrates"
        );
        // The committed artifact actually landed in the repo.
        assert!(
            repo.path().join("feature.rs").exists(),
            "the committed, gated artifact must merge into the base repo"
        );
    }

    #[test]
    fn an_escalated_unit_does_not_integrate_its_code() {
        // The SAFETY invariant: a unit whose three-tier review REJECTED it (here the
        // adjudicator always rejects, so it fails 3 times and ESCALATES) must NOT have
        // its code merged onto the integration branch. Escalation means "the adversarial
        // review refused this; hand it to a human" - the work-in-progress STAYS on the
        // unit's own branch (`rigger/u/<id>`), and NOTHING fast-forwards / merges onto
        // the integration branch. A real run hit exactly this: a feat(...) commit landed
        // on the run branch even though the unit escalated, and the merged-but-rejected
        // code broke the suite.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // The integration branch HEAD BEFORE the run - it must be byte-for-byte
        // unchanged after an escalation (nothing merged).
        let head_before = git_head(&repo_path);

        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), agent("worker"));
        cfg.agents.insert("judge".into(), agent("judge"));
        cfg.workflow.gates.insert("ok".into(), gate_def("true"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "worker".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                review: crate::config::ReviewPanel {
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let st = Store::open(":memory:").unwrap();
        // The implementer writes a file each attempt (the diff the review then judges);
        // the gates pass (`true`), so review IS reached - and the adjudicator ALWAYS
        // rejects, so the unit retries to the bound and escalates.
        let driver = Stub {
            write_file: Some("feature.rs".into()),
            output_by_agent: HashMap::from([(
                "judge".to_string(),
                r#"{"verdict":"reject","reason":"adversarial review refuses this"}"#.to_string(),
            )]),
            ..Stub::new()
        };
        let deps = Deps {
            store: &st,
            driver: &driver,
            gates: &ExecRunner,
            repo: repo_path.clone(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        let rs = run(&cfg, &deps).unwrap();

        // (c) The unit reaches the terminal ESCALATED state - the review refused it to
        // the bound and it was handed to a human.
        assert_eq!(
            rs.units["s"].status,
            ledger::Status::Escalated,
            "an always-rejected unit must escalate, not integrate"
        );

        // (a) No UnitIntegrated event for the escalated unit - it never landed.
        let events = st
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let integrated = events.iter().any(|e| {
            e.type_ == ledger::TYPE_UNIT_INTEGRATED
                && serde_json::from_slice::<Value>(&e.data)
                    .ok()
                    .and_then(|v| v.get("id").and_then(|x| x.as_str()).map(str::to_string))
                    .as_deref()
                    == Some("s")
        });
        assert!(
            !integrated,
            "an escalated unit must emit NO UnitIntegrated event - its code did not land"
        );

        // (b) The integration branch HEAD is UNCHANGED - the rejected code is NOT merged.
        // The unit's feat/work commit stays on `rigger/u/s` for a human, never on base.
        assert_eq!(
            git_head(&repo_path),
            head_before,
            "the integration branch HEAD must be UNCHANGED - nothing merges on escalation"
        );
        assert!(
            !repo.path().join("feature.rs").exists(),
            "the rejected artifact must NOT land in the base repo - it stays on the unit branch"
        );

        // The unit's durable checkpoint branch survives with its work, for the human.
        assert!(
            crate::worktree::Worktree::branch_has_work(&repo_path, &unit_branch("s")),
            "the escalated unit's work must remain on its own branch for a human to inspect"
        );
    }
}
