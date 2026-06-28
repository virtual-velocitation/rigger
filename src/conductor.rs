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
pub struct AgentResult {
    pub output: String,
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

/// Per-spawn options.
pub struct SpawnOpts {
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
    #[serde(default)]
    coverage: String,
    #[serde(default)]
    gates: Vec<String>,
}

/// Run executes the workflow and returns the final run state, projected from the
/// events it emitted. Independent stages run concurrently in waves.
pub fn run(cfg: &Config, deps: &Deps) -> Result<RunState, Error> {
    validate_acyclic(&cfg.workflow.stages)?;

    // The RunCtx is created BEFORE the coverage check so a coverage gap can be
    // flagged as a spec defect through the event log (item 2 / §4.4) instead of
    // returning a bare error with no audit trail.
    let ctx = RunCtx {
        cfg,
        deps,
        gate_tracker: Mutex::new(HashMap::new()),
        integrate_mu: Mutex::new(()),
        spawns: AtomicU32::new(0),
        budget_broke: std::sync::atomic::AtomicBool::new(false),
    };

    // Resume by replay (§4.2): seed integrated/terminal from the existing log so a
    // crashed or re-run conductor skips work that already landed instead of
    // re-spawning every agent from scratch.
    let prior = ledger::project(&deps.store.read_stream(STREAM, 0, Direction::Forward)?)
        .map_err(|e| Error(e.to_string()))?;
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

    // Coverage gate (§3.2, §8). A planner (`produces`) stage DEFERS coverage to
    // after planning: it has no units yet, so we run the planning wave + harvest
    // the proposed units FIRST, then check coverage against the extended DAG.
    // A run with no planner checks coverage up front, before any agent runs.
    if has_producer(&stages) {
        let ready = ready_stages(&stages, &integrated, &terminal);
        if !ready.is_empty() {
            ctx.run_wave(&stages, &ready, &mut integrated, &mut terminal)?;
            ctx.harvest_proposed(&mut stages, &mut proposed)?;
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
        ctx.harvest_proposed(&mut stages, &mut proposed)?;
    }

    let events = deps.store.read_stream(STREAM, 0, Direction::Forward)?;
    ledger::project(&events).map_err(|e| Error(e.to_string()))
}

/// Whether the workflow has a planner stage that produces a DAG at runtime, which
/// defers the coverage gate until after planning (§3.2).
fn has_producer(stages: &BTreeMap<String, Stage>) -> bool {
    stages.values().any(|st| !st.produces.is_empty())
}

struct RunCtx<'a> {
    cfg: &'a Config,
    deps: &'a Deps<'a>,
    gate_tracker: Mutex<HashMap<String, Gate>>,
    integrate_mu: Mutex<()>,
    /// The number of real `driver.spawn(...)` calls this run has made, for the
    /// budget circuit-breaker (§4.4, §8).
    spawns: AtomicU32,
    /// Set the moment a spawn is REFUSED because the budget is spent (item 9): the
    /// breaker now trips at spawn granularity, mid-wave, not only at wave boundaries.
    /// The run loop checks this after each wave to record the breaker and stop.
    budget_broke: std::sync::atomic::AtomicBool,
}

impl RunCtx<'_> {
    fn emit(&self, type_: &str, payload: Value) -> Result<(), Error> {
        self.emit_with_actor("", type_, payload)
    }

    /// Emit an event, optionally stamping the acting agent in its metadata (the
    /// DECIDED-edge source), appending to the log and folding it into the live
    /// graph so later agents can read it.
    fn emit_with_actor(&self, actor: &str, type_: &str, payload: Value) -> Result<(), Error> {
        let mut ev = Event::new(type_, serde_json::to_vec(&payload)?);
        if !actor.is_empty() {
            ev = ev.with_meta(contextgraph::META_ACTOR, actor);
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

    /// Whether the spawn budget circuit-breaker has tripped (§4.4, §8): a positive
    /// `defaults.budget` and at least that many real spawns already made.
    fn budget_tripped(&self) -> bool {
        let budget = self.cfg.workflow.defaults.budget;
        budget > 0
            && safety::budget_exhausted(budget as i64, self.spawns.load(Ordering::Relaxed) as i64)
    }

    /// Atomically reserve one spawn against the budget at SPAWN granularity (item 9):
    /// admit a spawn (incrementing the counter) only while the budget has room, so a
    /// single wide wave that overruns the budget is stopped mid-wave rather than only
    /// at the next wave boundary. Returns `true` when the spawn is admitted and
    /// `false` when it is refused (the budget is spent); on refusal it sets
    /// `budget_broke` so the run loop records the breaker and halts. A zero budget
    /// means unlimited - every spawn is admitted. The `fetch_update` makes the
    /// check-and-increment atomic, so concurrent lenses in one wave never overshoot.
    fn reserve_spawn(&self) -> bool {
        let budget = self.cfg.workflow.defaults.budget;
        if budget == 0 {
            self.spawns.fetch_add(1, Ordering::Relaxed);
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

    /// abortTask (§4.4): integrated work is already committed and every per-stage
    /// worktree is removed as its stage finishes, so there is no un-integrated
    /// worktree left to discard - abort_task records the abort so the run halts with
    /// an audit trail, and the loop stops (a pause; resume replays the ledger).
    fn abort_task(&self, reason: &str) -> Result<(), Error> {
        self.emit(TYPE_TASK_ABORTED, json!({"reason": reason}))
    }

    /// Trip the spawn-budget circuit-breaker (§4.4, §8): record BudgetExhausted with
    /// the budget and the spawns made, then abort the task. Shared by the pre-wave
    /// check and the mid-wave (spawn-granularity, item 9) trip so both halt the run
    /// the same way, with one audit trail.
    fn trip_budget_breaker(&self) -> Result<(), Error> {
        self.emit(
            TYPE_BUDGET_EXHAUSTED,
            json!({
                "budget": self.cfg.workflow.defaults.budget,
                "spawns": self.spawns.load(Ordering::Relaxed),
            }),
        )?;
        self.abort_task("spawn budget exhausted")
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
        // UnitStarted carries the assigned agent and its dependencies, so the graph
        // can project ASSIGNED_TO (unit->agent) and BLOCKS (need->unit).
        self.emit(
            ledger::TYPE_UNIT_STARTED,
            json!({
                "id": name,
                "unit": name,
                "spec_criterion": st.coverage,
                "criterion": st.coverage,
                "agent": st.agent,
                "needs": st.needs,
            }),
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
            self.emit(TYPE_MANUAL_REVIEW, json!({"id": st.name, "unit": st.name}))?;
            return Ok(false);
        }
        if is_fan_out(st) {
            return self.run_fan_out_stage(st);
        }
        let wt = self.stage_worktree(st)?;
        let dir = wt.as_ref().map(|w| w.dir.clone()).unwrap_or_default();
        let result = self.run_single_stage(st, wt.as_ref(), &dir);
        if let Some(w) = &wt {
            let _ = w.remove();
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
    /// review the unit (they produce no code), so they run with no worktree of their
    /// own. After the adjudicator approves, the unit is marked `reviewed` and its
    /// evidence carries the verdict reason (item 4). An empty panel runs no review and
    /// approves trivially (the historical behavior).
    fn review_unit(&self, st: &Stage) -> Result<ReviewOutcome, Error> {
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
            self.run_review_agents_concurrently(st, &lenses)?;
        }
        // TIER 2: the adversary grounds AFTER the lenses, so `graph_context` surfaces
        // their findings; it tries to prove them wrong and emits its own findings.
        if !adversary.is_empty() {
            self.run_adversary(st, &adversary)?;
        }
        if adjudicator.is_empty() {
            return Ok(ReviewOutcome::approved(String::new()));
        }
        // TIER 3: the adjudicator grounds last, reads the lenses' and adversary's
        // findings from the graph, and renders the gating verdict.
        let (approved, reason) = self.run_adjudicator(st, &adjudicator)?;
        if approved {
            // The adjudicator's verdict reason is folded into the unit's `reviewed`
            // evidence (item 4).
            self.emit(
                ledger::TYPE_UNIT_STATUS,
                json!({
                    "id": st.name,
                    "status": "reviewed",
                    "evidence": review_evidence(&reason),
                }),
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
    ) -> Result<bool, Error> {
        let mut attempts = 0u32;
        // The last attempt's concrete failure, threaded into the NEXT attempt's
        // prompt (item 3 + 5 / spec 02). Empty on the first attempt, so that prompt
        // is unchanged.
        let mut prior = PriorFailure::default();
        loop {
            let mut spawn_err: Option<String> = None;
            if !st.agent.is_empty() {
                let agent_def = self.cfg.agents.get(&st.agent).ok_or_else(|| {
                    Error(format!(
                        "stage {:?} references unknown agent {:?}",
                        st.name, st.agent
                    ))
                })?;
                // Budget breaker at spawn granularity (item 9): refuse this spawn if
                // the budget is spent. A refused implementer spawn stops the unit
                // (Ok(false), not escalated); the run loop records BudgetExhausted.
                if !self.reserve_spawn() {
                    return Ok(false);
                }
                let prompt = self.build_prompt_with_failure(st, &prior);
                let emit = |t: &str, v: Value| self.emit_with_actor(&st.agent, t, v);
                match self.deps.driver.spawn(
                    agent_def,
                    &prompt,
                    &SpawnOpts {
                        system_prompt: agent_def.prompt.clone(),
                        dir: dir.to_string(),
                        isolation: wt.is_some(),
                        parallel: false,
                        blast_radius: self.grounded_seed(st),
                    },
                    &emit,
                ) {
                    Ok(_) => {
                        // The green status records that the implementer produced a
                        // diff (item 4): the per-unit evidence names the agent that
                        // implemented it.
                        let mut green = BTreeMap::new();
                        green.insert("green".to_string(), format!("implemented by {}", st.agent));
                        self.emit(
                            ledger::TYPE_UNIT_STATUS,
                            json!({"id": st.name, "status": "green", "evidence": green}),
                        )?;
                    }
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
                let gate_outcome = self.run_gates(st, dir)?;
                if gate_outcome.pass {
                    // The verified status carries the gate evidence (item 4): each
                    // gate that ran summarized for the ledger's per-unit evidence.
                    self.emit(
                        ledger::TYPE_UNIT_STATUS,
                        json!({
                            "id": st.name,
                            "status": "verified",
                            "evidence": verified_evidence(&st.gates),
                        }),
                    )?;
                    let review = self.review_unit(st)?;
                    if review.approved {
                        // on_pass governs integration (§3.2): empty or `merge` lands
                        // the work; any other value (e.g. `none`) runs the gates but
                        // never integrates - the verified, reviewed work stays
                        // un-merged.
                        if !integrates(st) {
                            return Ok(false);
                        }
                        let commit = self.integrate_and_emit(wt, &st.agent, &st.name, &st.gates)?;
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

            let rem = safety::remediate(attempts, safety::MAX_RETRIES);
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
        // The fan-out lens set is `agents` when populated; a `strategy: fan-out`
        // stage that names a single `agent` (and no `agents`) runs that one agent as
        // its lone lens on the parallel path, so `strategy` is honored even without an
        // explicit lens list (§3.2).
        let lenses = fan_out_lenses(st);
        let mut attempts = 0u32;
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
            self.run_review_agents_concurrently(st, &lenses)?;
            if !st.adversary.is_empty() {
                self.run_adversary(st, &st.adversary)?;
            }
            // The neutral adjudicator's verdict gates the stage (§3.2), fail-closed:
            // it approves ONLY on an explicit `approve`, blocking integration
            // otherwise, no matter the static gates.
            let (approved, reason) = if st.adjudicator.is_empty() {
                (true, String::new())
            } else {
                self.run_adjudicator(st, &st.adjudicator)?
            };

            let gates_pass = approved && self.run_gates(st, "")?.pass;
            if gates_pass {
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
                self.emit(
                    ledger::TYPE_UNIT_STATUS,
                    json!({
                        "id": st.name,
                        "status": "reviewed",
                        "evidence": review_evidence(&reason),
                    }),
                )?;
                self.emit(
                    ledger::TYPE_UNIT_INTEGRATED,
                    json!({"id": st.name, "commit": REVIEW_ONLY_NO_ARTIFACT}),
                )?;
                return Ok(true);
            }

            let rem = safety::remediate(attempts, safety::MAX_RETRIES);
            attempts = rem.attempts;
            self.emit(
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
    /// they spawn with NO worktree and never integrate (item 6: a reviewing lens must
    /// not get its writes merged into the base repo).
    fn run_review_agents_concurrently(
        &self,
        st: &Stage,
        agent_ids: &[String],
    ) -> Result<(), Error> {
        // Bounded fan-out pool (§6): run the lenses in chunks of at most
        // MAX_CONCURRENCY, each chunk a scoped thread group. Every lens still runs;
        // never more than MAX_CONCURRENCY at once.
        for chunk in agent_ids.chunks(MAX_CONCURRENCY) {
            let chunk_results: Vec<Result<(), Error>> = std::thread::scope(|s| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|a| s.spawn(move || self.run_lens(st, a)))
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for r in chunk_results {
                r?;
            }
        }
        Ok(())
    }

    /// Run a single review lens. A lens reviews - it writes no code - so it spawns
    /// with NO worktree and its output is never integrated (item 6). It is prompted
    /// with the grounded base prompt plus the REVIEW_PROTOCOL, so it EMITS each
    /// finding it raises to the shared context graph (the cross-agent memory), where
    /// the adversary, the adjudicator, and its fellow lenses retrieve it. Its stdout
    /// is no longer captured to thread into another agent's prompt - the graph is the
    /// channel. Budget-refused spawns (item 9) surface as an error so the run halts.
    fn run_lens(&self, st: &Stage, agent_id: &str) -> Result<(), Error> {
        let agent_def = self.cfg.agents.get(agent_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown agent {:?}",
                st.name, agent_id
            ))
        })?;
        if !self.reserve_spawn() {
            return Err(Error(format!(
                "stage {:?} lens {:?}: spawn budget exhausted",
                st.name, agent_id
            )));
        }
        let prompt = self.build_review_prompt(st);
        let emit = |t: &str, v: Value| self.emit_with_actor(agent_id, t, v);
        self.deps
            .driver
            .spawn(
                agent_def,
                &prompt,
                &SpawnOpts {
                    system_prompt: agent_def.prompt.clone(),
                    dir: String::new(),
                    isolation: false,
                    parallel: true,
                    blast_radius: self.grounded_seed(st),
                },
                &emit,
            )
            .map_err(|e| Error(format!("stage {:?} agent {:?}: {}", st.name, agent_id, e.0)))?;
        Ok(())
    }

    /// Run the adversary: a single agent that reviews the lenses' findings and the
    /// diff and tries to prove the lenses wrong (§3.2). It runs AFTER the lenses, so
    /// the lenses' ReviewFindings are already folded into the graph and its grounded
    /// prompt (via `graph_context`) surfaces them - it retrieves the lenses' findings
    /// through the graph, not from a hand-threaded block. It then EMITS its own
    /// findings (the REVIEW_PROTOCOL) so the adjudicator reads them the same way. Like
    /// the adjudicator it reviews - it produces no code to integrate, so it spawns
    /// with no worktree - and unlike the adjudicator its output does NOT gate the
    /// stage; it informs the adjudicator's judgment via the graph.
    fn run_adversary(&self, st: &Stage, adv_id: &str) -> Result<(), Error> {
        let agent_def = self.cfg.agents.get(adv_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown adversary {:?}",
                st.name, adv_id
            ))
        })?;
        if !self.reserve_spawn() {
            return Err(Error(format!(
                "stage {:?} adversary {:?}: spawn budget exhausted",
                st.name, adv_id
            )));
        }
        let prompt = self.build_review_prompt(st);
        let emit = |t: &str, v: Value| self.emit_with_actor(adv_id, t, v);
        self.deps
            .driver
            .spawn(
                agent_def,
                &prompt,
                &SpawnOpts {
                    system_prompt: agent_def.prompt.clone(),
                    dir: String::new(),
                    isolation: false,
                    parallel: false,
                    blast_radius: self.grounded_seed(st),
                },
                &emit,
            )
            .map_err(|e| {
                Error(format!(
                    "stage {:?} adversary {:?}: {}",
                    st.name, adv_id, e.0
                ))
            })?;
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
    fn run_adjudicator(&self, st: &Stage, adj_id: &str) -> Result<(bool, String), Error> {
        let agent_def = self.cfg.agents.get(adj_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown adjudicator {:?}",
                st.name, adj_id
            ))
        })?;
        if !self.reserve_spawn() {
            return Err(Error(format!(
                "stage {:?} adjudicator {:?}: spawn budget exhausted",
                st.name, adj_id
            )));
        }
        let prompt = self.build_prompt(st);
        let emit = |t: &str, v: Value| self.emit_with_actor(adj_id, t, v);
        let result = self
            .deps
            .driver
            .spawn(
                agent_def,
                &prompt,
                &SpawnOpts {
                    system_prompt: agent_def.prompt.clone(),
                    dir: String::new(),
                    isolation: false,
                    parallel: false,
                    blast_radius: self.grounded_seed(st),
                },
                &emit,
            )
            .map_err(|e| {
                Error(format!(
                    "stage {:?} adjudicator {:?}: {}",
                    st.name, adj_id, e.0
                ))
            })?;
        Ok((verdict_approves(&result.output), result.output))
    }

    fn run_gates(&self, st: &Stage, dir: &str) -> Result<GateOutcome, Error> {
        let mut outcome = GateOutcome {
            pass: true,
            evidence: Vec::new(),
        };
        for gid in &st.gates {
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
            let res = self.deps.gates.run(&g, dir);
            // The compact gate evidence is threaded into the GateVerdict event
            // payload (item 3): a real run otherwise discarded it, so neither the
            // ledger nor the workflow driver ever saw WHY a gate passed or failed.
            self.emit(
                contextgraph::TYPE_GATE_VERDICT,
                json!({"gate": gid, "pass": res.pass, "evidence": res.evidence}),
            )?;
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
    ) -> Result<String, Error> {
        let wt = match wt {
            Some(w) => w,
            None => return Ok(String::new()),
        };
        let files = wt.changed_files()?;
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

    /// The stage's blast-radius: the distinct files the grounder surfaces for the
    /// stage's `coverage` (or its name when `coverage` is empty), in ground order
    /// (§5.3). This is the same grounding `build_prompt` seeds the graph context from
    /// and `partition_wave` partitions by, so the blast-radius the side-car filters
    /// peer decisions against is exactly the files the agent was grounded on. Empty
    /// when no grounder is configured (best-effort but real, not always empty).
    fn grounded_seed(&self, st: &Stage) -> Vec<String> {
        let gr = match self.deps.grounder {
            Some(g) => g,
            None => return Vec::new(),
        };
        let query = if st.coverage.is_empty() {
            &st.name
        } else {
            &st.coverage
        };
        let mut seed: Vec<String> = Vec::new();
        for r in gr.ground(query, 8) {
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
            let query = if st.coverage.is_empty() {
                &st.name
            } else {
                &st.coverage
            };
            let refs = gr.ground(query, 8);
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
        write_nodes(
            &mut b,
            &g,
            contextgraph::KIND_DECISION,
            "Decisions that govern these files (do not contradict them; supersede explicitly if you must):",
        );
        write_nodes(
            &mut b,
            &g,
            contextgraph::KIND_LESSON,
            "Lessons already learned about these files (do not repeat these mistakes):",
        );
        // Findings other reviewers already raised about these files. The subgraph is
        // seeded on the unit's files and a ReviewFinding folds ABOUT those files, so
        // the same traversal that returns the GOVERNING decisions returns the findings
        // too: this is the graph path by which the adversary and adjudicator (which
        // ground AFTER the lenses) retrieve the lenses' findings, replacing the
        // conductor hand-threading one agent's stdout into another's prompt. Each line
        // names the reviewer (`by`) and the finding summary.
        write_findings(&mut b, &g);
        b
    }

    fn emit_lesson(&self, wt: Option<&Worktree>, unit_name: &str, summary: &str) {
        let about: Vec<String> = wt.and_then(|w| w.changed_files().ok()).unwrap_or_default();
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

    fn stage_worktree(&self, st: &Stage) -> Result<Option<Worktree>, Error> {
        if self.deps.repo.is_empty() || st.agent.is_empty() {
            return Ok(None);
        }
        // An agent declaring `isolation: none` runs in the current dir, no
        // worktree, even when a repo is set (§3.1, §6).
        if !self.agent_isolated(&st.agent) {
            return Ok(None);
        }
        let uid = uuid::Uuid::new_v4().to_string();
        let id = &uid[..8];
        let dir = std::env::temp_dir().join(format!("rigger-wt-{id}"));
        let wt = Worktree::create(
            &self.deps.repo,
            dir.to_str().unwrap_or_default(),
            &format!("rigger/{}-{id}", st.name),
        )?;
        Ok(Some(wt))
    }

    fn harvest_proposed(
        &self,
        stages: &mut BTreeMap<String, Stage>,
        proposed: &mut HashSet<String>,
    ) -> Result<(), Error> {
        let events = self.deps.store.read_stream(STREAM, 0, Direction::Forward)?;
        for e in &events {
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

/// The protocol a REVIEW agent (a lens or the adversary) follows so its findings
/// reach the shared context graph - the cross-agent memory the three tiers
/// communicate THROUGH. A reviewer records each finding by calling rigger_emit the
/// moment it raises it; the projector folds it ABOUT the files it concerns, and the
/// later tiers (and concurrent lenses) retrieve it via grounding + rigger_peers,
/// never via the conductor splicing one agent's stdout into another's prompt.
const REVIEW_PROTOCOL: &str = "Record each review finding you raise by calling the rigger_emit tool the moment you raise it, with type \"ReviewFinding\" and data:\n{\"id\":\"<short-id>\",\"summary\":\"<one line>\",\"about\":[\"<file>\"]}\nThis writes it to the shared context graph live, so the adversary, the adjudicator, and your fellow reviewers see it immediately (via grounding and rigger_peers) and address or refute it.";

fn write_nodes(b: &mut String, g: &Graph, kind: &str, header: &str) {
    let mut first = true;
    for n in &g.nodes {
        if n.kind != kind {
            continue;
        }
        let summary = match n.attrs.get("summary") {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        if first {
            b.push_str(header);
            b.push('\n');
            first = false;
        }
        b.push_str(&format!("- {}: {}\n", n.id, summary));
    }
    if !first {
        b.push('\n');
    }
}

/// Surface the KIND_FINDING nodes a prior reviewer raised about the seeded files
/// (item 2): the graph path by which a later review agent retrieves the findings the
/// lenses already emitted. Each line names the raising reviewer (`by`) and the
/// finding summary so the agent can address or refute it. A finding with no summary
/// is skipped (nothing actionable to surface).
fn write_findings(b: &mut String, g: &Graph) {
    let header =
        "Findings other reviewers have already raised about these files (address or refute them):";
    let mut first = true;
    for n in &g.nodes {
        if n.kind != contextgraph::KIND_FINDING {
            continue;
        }
        let summary = match n.attrs.get("summary") {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        if first {
            b.push_str(header);
            b.push('\n');
            first = false;
        }
        let by = n.attrs.get("by").map(String::as_str).unwrap_or("");
        if by.is_empty() {
            b.push_str(&format!("- {}: {}\n", n.id, summary));
        } else {
            b.push_str(&format!("- {by} ({}): {summary}\n", n.id));
        }
    }
    if !first {
        b.push('\n');
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
        fail_spawn: bool,
        last_prompt: Mutex<String>,
        /// Per-agent (isolation, parallel) the conductor passed at each spawn.
        opts_by_agent: Mutex<HashMap<String, (bool, bool)>>,
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
                fail_spawn: false,
                last_prompt: Mutex::new(String::new()),
                opts_by_agent: Mutex::new(HashMap::new()),
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

        /// The persona (system prompt) the conductor threaded to the driver for the
        /// named agent, or None if it was never spawned.
        fn system_prompt_for(&self, agent_id: &str) -> Option<String> {
            self.system_prompt_by_agent
                .lock()
                .unwrap()
                .get(agent_id)
                .cloned()
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
            if self.fail_spawn {
                return Err(Error("simulated mid-spawn crash".into()));
            }
            if let Some(f) = &self.write_file {
                let _ = std::fs::write(Path::new(&opts.dir).join(f), "work\n");
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
                .output_by_agent
                .get(&a.id)
                .cloned()
                .unwrap_or_else(|| self.output.clone());
            Ok(AgentResult { output })
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

        // Every agent the conductor spawned received ITS OWN persona as the system
        // prompt - the implementer and all three review tiers.
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
            assert_eq!(
                driver.system_prompt_for(id).as_deref(),
                Some(persona),
                "agent {id:?} must be spawned with its own persona as the system prompt"
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
        assert_eq!(
            driver.system_prompt_for("a").as_deref(),
            Some(""),
            "an agent with no body threads an empty (not fabricated) system prompt"
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
            gate_tracker: Mutex::new(HashMap::new()),
            integrate_mu: Mutex::new(()),
            spawns: AtomicU32::new(0),
            budget_broke: std::sync::atomic::AtomicBool::new(false),
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
        fn run(&self, _g: &Gate, _dir: &str) -> gate::GateResult {
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
}
