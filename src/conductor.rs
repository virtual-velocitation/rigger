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

/// Per-spawn options.
pub struct SpawnOpts {
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
    topo_sort(&cfg.workflow.stages)?;

    // The RunCtx is created BEFORE the coverage check so a coverage gap can be
    // flagged as a spec defect through the event log (item 2 / §4.4) instead of
    // returning a bare error with no audit trail.
    let ctx = RunCtx {
        cfg,
        deps,
        gate_tracker: Mutex::new(HashMap::new()),
        integrate_mu: Mutex::new(()),
        spawns: AtomicU32::new(0),
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
            ctx.emit(
                TYPE_BUDGET_EXHAUSTED,
                json!({
                    "budget": cfg.workflow.defaults.budget,
                    "spawns": ctx.spawns.load(Ordering::Relaxed),
                }),
            )?;
            ctx.abort_task("spawn budget exhausted")?;
            break;
        }
        ctx.run_wave(&stages, &ready, &mut integrated, &mut terminal)?;
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

    /// abortTask (§4.4): integrated work is already committed and every per-stage
    /// worktree is removed as its stage finishes, so there is no un-integrated
    /// worktree left to discard - abort_task records the abort so the run halts with
    /// an audit trail, and the loop stops (a pause; resume replays the ledger).
    fn abort_task(&self, reason: &str) -> Result<(), Error> {
        self.emit(TYPE_TASK_ABORTED, json!({"reason": reason}))
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
                    Err(e) if first_err.is_none() => first_err = Some(e),
                    Err(_) => {}
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

    fn run_single_stage(
        &self,
        st: &Stage,
        wt: Option<&Worktree>,
        dir: &str,
    ) -> Result<bool, Error> {
        let mut attempts = 0u32;
        loop {
            let mut spawn_err: Option<String> = None;
            if !st.agent.is_empty() {
                let agent_def = self.cfg.agents.get(&st.agent).ok_or_else(|| {
                    Error(format!(
                        "stage {:?} references unknown agent {:?}",
                        st.name, st.agent
                    ))
                })?;
                let prompt = self.build_prompt(st);
                let emit = |t: &str, v: Value| self.emit_with_actor(&st.agent, t, v);
                self.spawns.fetch_add(1, Ordering::Relaxed);
                match self.deps.driver.spawn(
                    agent_def,
                    &prompt,
                    &SpawnOpts {
                        dir: dir.to_string(),
                        isolation: wt.is_some(),
                        parallel: false,
                        blast_radius: self.grounded_seed(st),
                    },
                    &emit,
                ) {
                    Ok(_) => {
                        self.emit(
                            ledger::TYPE_UNIT_STATUS,
                            json!({"id": st.name, "status": "green"}),
                        )?;
                    }
                    // A mid-spawn crash (usage limit, non-zero exit) is remediated,
                    // not propagated: it must not abort the whole run (§8).
                    Err(e) => spawn_err = Some(format!("agent {:?}: {}", st.agent, e.0)),
                }
            }

            if spawn_err.is_none() && self.run_gates(st, dir)? {
                self.emit(
                    ledger::TYPE_UNIT_STATUS,
                    json!({"id": st.name, "status": "verified"}),
                )?;
                // on_pass governs integration (§3.2): empty or `merge` lands the
                // work; any other value (e.g. `none`) runs the gates but never
                // integrates - the verified work stays un-merged.
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

            let rem = safety::remediate(attempts, safety::MAX_RETRIES);
            attempts = rem.attempts;
            self.emit(
                ledger::TYPE_UNIT_FAILED,
                json!({"id": st.name, "attempts": attempts}),
            )?;
            if rem.decision == safety::Decision::Escalate {
                let why = spawn_err
                    .clone()
                    .unwrap_or_else(|| "its gates would not pass".to_string());
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
            // otherwise loop and retry the stage (with re-grounding via build_prompt)
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
            // Three-tier review (§3.2): the expert lenses review the diff in
            // parallel, THEN the adversary reviews the lenses' emitted findings and
            // tries to prove them wrong (a higher bar than the lenses; it reviews
            // the reviews, it is not a parallel lens), THEN the neutral adjudicator
            // weighs the lenses against the adversary and its verdict gates the
            // stage.
            self.run_agents_concurrently(st, &lenses)?;
            if !st.adversary.is_empty() {
                self.run_adversary(st, &st.adversary)?;
            }
            // The neutral adjudicator's verdict gates the stage (§3.2): an explicit
            // reject blocks integration, no matter the static gates.
            let approved = if st.adjudicator.is_empty() {
                true
            } else {
                self.emit(
                    ledger::TYPE_UNIT_STATUS,
                    json!({"id": st.name, "status": "reviewed"}),
                )?;
                self.run_adjudicator(st, &st.adjudicator)?
            };

            if approved && self.run_gates(st, "")? {
                // on_pass governs integration (§3.2): any value other than empty /
                // `merge` runs the gates but does not mark the stage integrated.
                if !integrates(st) {
                    return Ok(false);
                }
                self.emit(
                    ledger::TYPE_UNIT_INTEGRATED,
                    json!({"id": st.name, "commit": ""}),
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
                self.emit_lesson(
                    None,
                    &st.name,
                    &format!(
                        "review stage {:?} escalated after {attempts} attempts",
                        st.name
                    ),
                );
                self.emit(ledger::TYPE_UNIT_ESCALATED, json!({"id": st.name}))?;
                return Ok(false);
            }
        }
    }

    fn run_agents_concurrently(&self, st: &Stage, agent_ids: &[String]) -> Result<(), Error> {
        // Worktrees are created sequentially (git worktree add is not concurrency-safe).
        let mut jobs: Vec<(String, Option<Worktree>)> = Vec::new();
        for a in agent_ids {
            match self.agent_worktree(st, a) {
                Ok(wt) => jobs.push((a.clone(), wt)),
                Err(e) => {
                    remove_all(&jobs);
                    return Err(e);
                }
            }
        }
        // Bounded fan-out pool (§6): run the agents in chunks of at most
        // MAX_CONCURRENCY, each chunk a scoped thread group. Every agent still
        // runs; never more than MAX_CONCURRENCY at once.
        let mut results: Vec<Result<(), Error>> = Vec::with_capacity(jobs.len());
        for chunk in jobs.chunks(MAX_CONCURRENCY) {
            let chunk_results: Vec<Result<(), Error>> = std::thread::scope(|s| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|(a, wt)| s.spawn(move || self.run_agent_in_worktree(st, a, wt.as_ref())))
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            results.extend(chunk_results);
        }
        remove_all(&jobs);
        for r in results {
            r?;
        }
        Ok(())
    }

    fn run_agent_in_worktree(
        &self,
        st: &Stage,
        agent_id: &str,
        wt: Option<&Worktree>,
    ) -> Result<(), Error> {
        let agent_def = self.cfg.agents.get(agent_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown agent {:?}",
                st.name, agent_id
            ))
        })?;
        let dir = wt.map(|w| w.dir.clone()).unwrap_or_default();
        let prompt = self.build_prompt(st);
        let emit = |t: &str, v: Value| self.emit_with_actor(agent_id, t, v);
        self.spawns.fetch_add(1, Ordering::Relaxed);
        self.deps
            .driver
            .spawn(
                agent_def,
                &prompt,
                &SpawnOpts {
                    dir,
                    isolation: wt.is_some(),
                    parallel: true,
                    blast_radius: self.grounded_seed(st),
                },
                &emit,
            )
            .map_err(|e| Error(format!("stage {:?} agent {:?}: {}", st.name, agent_id, e.0)))?;
        let unit = format!("{}/{}", st.name, agent_id);
        self.integrate_and_emit(wt, agent_id, &unit, &st.gates)?;
        Ok(())
    }

    /// Run the adversary: a single agent that reviews the lenses' emitted findings
    /// and the diff and tries to prove the lenses wrong (§3.2). It runs AFTER the
    /// lenses and BEFORE the adjudicator, grounded on the same graph/log context the
    /// lenses fed (so it sees their live findings), and emits its refutations. Like
    /// the adjudicator it reviews - it produces no code to integrate, so it spawns
    /// with no worktree - and unlike the adjudicator its output does NOT gate the
    /// stage; it informs the adjudicator's judgment.
    fn run_adversary(&self, st: &Stage, adv_id: &str) -> Result<(), Error> {
        let agent_def = self.cfg.agents.get(adv_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown adversary {:?}",
                st.name, adv_id
            ))
        })?;
        let prompt = self.build_prompt(st);
        let emit = |t: &str, v: Value| self.emit_with_actor(adv_id, t, v);
        self.spawns.fetch_add(1, Ordering::Relaxed);
        self.deps
            .driver
            .spawn(
                agent_def,
                &prompt,
                &SpawnOpts {
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

    /// Run the adjudicator and return whether it approves; its verdict gates the
    /// stage. The adjudicator reviews - it produces no code to integrate.
    fn run_adjudicator(&self, st: &Stage, adj_id: &str) -> Result<bool, Error> {
        let agent_def = self.cfg.agents.get(adj_id).ok_or_else(|| {
            Error(format!(
                "stage {:?} references unknown adjudicator {:?}",
                st.name, adj_id
            ))
        })?;
        let prompt = self.build_prompt(st);
        let emit = |t: &str, v: Value| self.emit_with_actor(adj_id, t, v);
        self.spawns.fetch_add(1, Ordering::Relaxed);
        let result = self
            .deps
            .driver
            .spawn(
                agent_def,
                &prompt,
                &SpawnOpts {
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
        Ok(verdict_approves(&result.output))
    }

    fn run_gates(&self, st: &Stage, dir: &str) -> Result<bool, Error> {
        let mut all_pass = true;
        for gid in &st.gates {
            let gc = self
                .cfg
                .workflow
                .gates
                .get(gid)
                .cloned()
                .unwrap_or_default();
            let g = Gate {
                id: gid.clone(),
                run: gc.run,
                kind: gate::Kind::parse(&gc.kind),
                autonomy: gate::Autonomy::Manual,
                history: Vec::new(),
            };
            let res = self.deps.gates.run(&g, dir);
            self.emit(
                contextgraph::TYPE_GATE_VERDICT,
                json!({"gate": gid, "pass": res.pass}),
            )?;
            let (promoted, demoted, autonomy) = self.record_gate(gid, res.pass, &st.autonomy);
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
                all_pass = false;
            }
        }
        Ok(all_pass)
    }

    /// Record a gate's run on the ratchet, seeding a newly-tracked gate's starting
    /// autonomy from the stage override (`stage_autonomy`, when non-empty) and
    /// otherwise from `defaults.autonomy` (§3.2, §4.3).
    fn record_gate(
        &self,
        gid: &str,
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
        let g = tracker.entry(gid.to_string()).or_insert_with(|| Gate {
            id: gid.to_string(),
            run: String::new(),
            kind: gate::Kind::Core,
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
            // the gate at its current autonomy until a human approves.
            let proposed = gate::next_autonomy(g.autonomy);
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
        let mut b = String::new();
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

    fn agent_worktree(&self, st: &Stage, agent_id: &str) -> Result<Option<Worktree>, Error> {
        if self.deps.repo.is_empty() {
            return Ok(None);
        }
        // An agent declaring `isolation: none` runs in the current dir, no
        // worktree, even when a repo is set (§3.1, §6).
        if !self.agent_isolated(agent_id) {
            return Ok(None);
        }
        let uid = uuid::Uuid::new_v4().to_string();
        let id = &uid[..8];
        let dir = std::env::temp_dir().join(format!("rigger-wt-{id}"));
        let wt = Worktree::create(
            &self.deps.repo,
            dir.to_str().unwrap_or_default(),
            &format!("rigger/{}-{agent_id}-{id}", st.name),
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

fn remove_all(jobs: &[(String, Option<Worktree>)]) {
    for (_, w) in jobs {
        if let Some(w) = w {
            let _ = w.remove();
        }
    }
}

/// An adjudicator's verdict gates the stage: an explicit "reject" in its REVIEW
/// output blocks integration; anything else (approve, or no parseable verdict)
/// passes.
fn verdict_approves(output: &str) -> bool {
    for line in output.lines().rev() {
        if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
            if let Some(verdict) = v.get("verdict").and_then(|x| x.as_str()) {
                return !verdict.eq_ignore_ascii_case("reject");
            }
        }
    }
    true
}

const EMIT_PROTOCOL: &str = "Record each decision you make by calling the rigger_emit tool the moment you make it, with type \"DecisionMade\" and data:\n{\"id\":\"<short-id>\",\"summary\":\"<one line>\",\"governs\":[\"<file>\"],\"supersedes\":\"<prior-id-or-empty>\"}\nThis writes it to the shared event log live, so other agents see it immediately.";

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
/// single-worker path (§3.2). The decision reads `strategy`: a stage fans out when
/// `strategy == "fan-out"` (case-insensitive) OR it carries a non-empty `agents`
/// lens list. The `agents`-list branch keeps the historical behavior; the
/// `strategy` branch means the field is now consulted and honored, not ignored.
fn is_fan_out(st: &Stage) -> bool {
    st.strategy.eq_ignore_ascii_case("fan-out") || !st.agents.is_empty()
}

/// The lens set a fan-out stage runs concurrently: its `agents` list when
/// populated, else its single `agent` (a `strategy: fan-out` stage that names only
/// `agent`), else empty. This is what lets `strategy: fan-out` take the parallel
/// path even without an explicit `agents` list (§3.2).
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

/// topoSort returns the stages in dependency order; a residual cycle is a hard
/// error (the config is already validated acyclic, so this is defense in depth).
fn topo_sort(stages: &BTreeMap<String, Stage>) -> Result<Vec<String>, Error> {
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
    queue.sort();
    let mut order: Vec<String> = Vec::new();
    while !queue.is_empty() {
        let n = queue.remove(0);
        order.push(n.clone());
        let mut newly: Vec<String> = Vec::new();
        if let Some(deps) = dependents.get(&n) {
            for dep in deps {
                if let Some(d) = indeg.get_mut(dep) {
                    *d -= 1;
                    if *d == 0 {
                        newly.push(dep.clone());
                    }
                }
            }
        }
        newly.sort();
        queue.extend(newly);
    }
    if order.len() != stages.len() {
        return Err(Error("workflow has a dependency cycle".into()));
    }
    Ok(order)
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
        output: String,
        fail_spawn: bool,
        last_prompt: Mutex<String>,
        /// Per-agent (isolation, parallel) the conductor passed at each spawn.
        opts_by_agent: Mutex<HashMap<String, (bool, bool)>>,
        /// The order agents were spawned in, by id - used to assert the lenses ->
        /// adversary -> adjudicator three-tier review order.
        call_order: Mutex<Vec<String>>,
    }
    impl Stub {
        fn new() -> Self {
            Stub {
                write_file: None,
                emits: Vec::new(),
                output: String::new(),
                fail_spawn: false,
                last_prompt: Mutex::new(String::new()),
                opts_by_agent: Mutex::new(HashMap::new()),
                call_order: Mutex::new(Vec::new()),
            }
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
            Ok(AgentResult {
                output: self.output.clone(),
            })
        }
    }

    fn agent(id: &str) -> AgentDef {
        AgentDef {
            id: id.to_string(),
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
        let driver = Stub::new(); // empty output => adjudicator approves
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
        };
        ctx.record_gate("ok", true, "silent");
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
    fn strategy_fan_out_takes_the_fan_out_path() {
        // A stage with `strategy: fan-out` and a single `agent` (no `agents` list)
        // must be treated as a fan-out stage: `is_fan_out` reads `strategy`, and the
        // lens set falls back to the lone agent (§3.2). Asserting via the SpawnOpts
        // the conductor passed - the fan-out path spawns with `parallel = true`,
        // whereas the single-worker path spawns with `parallel = false`.
        let st = Stage {
            name: "impl".into(),
            agent: "a".into(),
            strategy: "fan-out".into(),
            ..Default::default()
        };
        assert!(is_fan_out(&st), "a `strategy: fan-out` stage must fan out");
        assert_eq!(
            fan_out_lenses(&st),
            vec!["a".to_string()],
            "a fan-out stage with only `agent` runs that agent as its lone lens"
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
            parallel,
            "a `strategy: fan-out` stage must spawn its agent on the parallel path"
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
