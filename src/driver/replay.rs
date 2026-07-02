//! The replay AgentDriver: the stepwise core (§4, spec 04). `conductor::run`'s
//! imperative control flow stays intact, but every `spawn` call is answered from the
//! event log instead of running an agent in-process:
//!
//! - If the log already holds the RESULT for this spawn's deterministic id, return it
//!   immediately (replay) - so a step re-running the conductor over recorded history
//!   reconstructs the same state without re-running the agent.
//! - If not, PARK the call: persist the spawn request as a [`TYPE_SPAWN_REQUESTED`]
//!   event and signal the parked frontier. The conductor unwinds the unit CLEANLY (no
//!   `UnitFailed`, no remediation - see [`parked_spawn`]/[`is_parked`]), so when every
//!   in-flight spawn in the wave is parked the run loop finds no newly-ready unit and
//!   returns: the step's state is entirely in the log, so the process simply ends.
//!
//! This extends `ledger::RunState::apply`'s pure-fold principle to the conductor's
//! control flow. Parking is IDEMPOTENT: a step that re-runs the conductor over recorded
//! history appends no duplicate spawn request (it parks only an id that is not already
//! recorded), so the same run stream can be replayed any number of times.
//!
//! The blocking drivers (`cli`, `workflow`) are unaffected: they never park, and they
//! ignore the [`SpawnOpts`] id/unit/stage fields this driver keys on.

use serde_json::Value;

use crate::conductor::{parked_spawn, AgentDriver, AgentResult, Error, SpawnOpts, STREAM};
use crate::config::AgentDef;
use crate::eventstore::{Direction, EventStore};
use crate::spawn::{self, SpawnRequest};

/// A replay driver answers each `spawn` from the run's event log: it replays an
/// already-recorded spawn or parks an unrecorded one.
///
/// It holds the SAME event store the conductor drives (`Deps::store`), so a spawn it
/// parks is visible to the next step process (and to a concurrent sibling spawn in the
/// same wave) the moment it is appended.
pub struct ReplayDriver<'a> {
    store: &'a dyn EventStore,
}

impl<'a> ReplayDriver<'a> {
    /// Build a replay driver over `store` - the run's event log, the single source of
    /// truth for whether a spawn is already recorded (replay) or not (park).
    pub fn new(store: &'a dyn EventStore) -> ReplayDriver<'a> {
        ReplayDriver { store }
    }
}

/// Reconstruct the full [`SpawnRequest`] this call would park, from the trait's spawn
/// arguments: its deterministic id, unit, and stage come from `opts` (the conductor
/// set them from the run structure); its persona, dir, and blast-radius from `opts`;
/// its model alias and granted tools from the agent (already fan-out-stripped by
/// [`AgentDef::allowed_tools`]); and its task prompt from `prompt`.
fn spawn_request(agent: &AgentDef, prompt: &str, opts: &SpawnOpts) -> SpawnRequest {
    SpawnRequest {
        id: opts.id.clone(),
        unit: opts.unit.clone(),
        stage: opts.stage.clone(),
        prompt: prompt.to_string(),
        system_prompt: opts.system_prompt.clone(),
        model: agent.model.clone(),
        tools: agent.allowed_tools(),
        dir: opts.dir.clone(),
        blast_radius: opts.blast_radius.clone(),
    }
}

impl AgentDriver for ReplayDriver<'_> {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        // A replayed spawn's decisions were already emitted (and recorded) when the
        // agent ran out-of-process, so this driver never calls `emit`: the events it
        // would replay are already in the log.
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        // Read the run stream fresh on every spawn: the whole run's state lives in the
        // log, and a concurrent sibling spawn in the same wave may have appended a park
        // since this call started.
        let events = self
            .store
            .read_stream(STREAM, 0, Direction::Forward)
            .map_err(|e| Error(e.to_string()))?;

        // ANSWER an already-recorded spawn (replay): a recorded RESULT for this id means
        // the agent already ran, so return its outcome without re-running it. A recorded
        // failure replays AS a failure (never a fabricated success), so a step sees the
        // identical outcome the live run saw and remediates it exactly the same way.
        if let Some(res) = spawn::result_of(&events, &opts.id).map_err(|e| Error(e.to_string()))? {
            if res.is_error() {
                return Err(Error(res.error));
            }
            // Surface the RESOLVED model the worker reported through `rigger result --meta`
            // (spec 05 line 52), so the conductor can copy it onto this spawn's unit events.
            let resolved_model = res.resolved_model();
            return Ok(AgentResult {
                output: res.output,
                resolved_model,
            });
        }

        // PARK an unrecorded spawn: persist the request so a courier can drain it and the
        // next step replays its result. IDEMPOTENT (finding adv-park-not-idempotent): a
        // step re-running the conductor over recorded history must append NO duplicate
        // SpawnRequested, so park only an id that is not already recorded.
        if !spawn::is_recorded(&events, &opts.id) {
            let req = spawn_request(agent, prompt, opts);
            spawn::park(self.store, &req).map_err(|e| Error(e.to_string()))?;
        }

        // Signal the parked frontier. The conductor unwinds this unit cleanly (no
        // UnitFailed) and the step process ends once every in-flight spawn is parked.
        Err(parked_spawn(&opts.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::{is_parked, run, Deps};
    use crate::config::{Config, Stage};
    use crate::eventstore::sqlite::Store;
    use crate::gate::ExecRunner;
    use crate::spawn::{spawn_id, ROLE_IMPLEMENTER};

    /// A no-op emit sink: the replay driver never emits, so tests pass this.
    fn no_emit(_: &str, _: Value) -> Result<(), Error> {
        Ok(())
    }

    fn worker() -> AgentDef {
        AgentDef {
            id: "worker".into(),
            model: "sonnet".into(),
            tools: vec!["Read".into(), "Edit".into()],
            ..Default::default()
        }
    }

    fn opts_for(id: &str) -> SpawnOpts {
        SpawnOpts {
            id: id.to_string(),
            unit: "u".into(),
            stage: "u".into(),
            ..Default::default()
        }
    }

    #[test]
    fn answers_an_already_recorded_success_from_the_log() {
        let store = Store::open(":memory:").unwrap();
        spawn::record_result(
            &store,
            &spawn::SpawnResult::ok("u/implementer#0", "the diff"),
        )
        .unwrap();

        let driver = ReplayDriver::new(&store);
        let got = driver
            .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
            .expect("a recorded success is answered, not parked");
        assert_eq!(got.output, "the diff");

        // Answering must NOT append anything: no new spawn request is parked.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            spawn::recorded(&events).unwrap().is_empty(),
            "a replayed spawn parks nothing"
        );
    }

    #[test]
    fn answers_a_recorded_failure_as_an_error_not_a_fake_success() {
        let store = Store::open(":memory:").unwrap();
        spawn::record_result(
            &store,
            &spawn::SpawnResult::failed("u/implementer#0", "agent crashed: non-zero exit"),
        )
        .unwrap();

        let driver = ReplayDriver::new(&store);
        let err = driver
            .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
            .expect_err("a recorded failure replays AS a failure");
        assert_eq!(err.0, "agent crashed: non-zero exit");
        // A recorded failure is a real failure, NOT a park - so the conductor remediates
        // it exactly as it did live.
        assert!(!is_parked(&err));
    }

    #[test]
    fn parks_an_unrecorded_spawn_and_signals_the_frontier() {
        let store = Store::open(":memory:").unwrap();
        let driver = ReplayDriver::new(&store);

        let err = driver
            .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
            .expect_err("an unrecorded spawn parks and signals the frontier");
        assert!(is_parked(&err), "the park signal is recognizable as a park");

        // The request became a durable fact the courier (and the next step) reads back,
        // carrying everything the thin driver needs.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let parked = spawn::recorded(&events).unwrap();
        assert_eq!(parked.len(), 1);
        let req = &parked["u/implementer#0"];
        assert_eq!(req.unit, "u");
        assert_eq!(req.stage, "u");
        assert_eq!(req.prompt, "do it");
        assert_eq!(req.model, "sonnet");
        assert_eq!(req.tools, ["Read", "Edit"]);
    }

    #[test]
    fn parking_is_idempotent_across_replayed_steps() {
        // A step re-running the conductor over recorded history must append no duplicate
        // SpawnRequested for the same id (finding adv-park-not-idempotent).
        let store = Store::open(":memory:").unwrap();
        let driver = ReplayDriver::new(&store);

        for _ in 0..3 {
            let err = driver
                .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
                .expect_err("still parked until a result is recorded");
            assert!(is_parked(&err));
        }

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let spawn_requests = events
            .iter()
            .filter(|e| e.type_ == spawn::TYPE_SPAWN_REQUESTED)
            .count();
        assert_eq!(
            spawn_requests, 1,
            "re-parking the same id writes no duplicate"
        );
    }

    #[test]
    fn recording_a_result_flips_a_parked_spawn_to_replayed() {
        let store = Store::open(":memory:").unwrap();
        let driver = ReplayDriver::new(&store);

        // First: parked (no result yet).
        let first = driver.spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit);
        assert!(is_parked(&first.unwrap_err()));

        // The courier records the outcome; now the same spawn is answered from the log.
        spawn::record_result(&store, &spawn::SpawnResult::ok("u/implementer#0", "done")).unwrap();
        let answered = driver
            .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
            .expect("a recorded result replays instead of parking again");
        assert_eq!(answered.output, "done");
    }

    #[test]
    fn replaying_a_recorded_result_appends_no_duplicate_lifecycle_events() {
        // spec 04, criterion 4: once the implementer's result is recorded, re-running the
        // conductor over that history any number of times appends no UnitStarted / green
        // / verified twice - the unit-lifecycle events are replay-keyed, so the append-
        // only log stays free of the duplicates a naive re-run would manufacture every
        // step (finding adv-replay-dup-lifecycle).
        let store = Store::open(":memory:").unwrap();
        let cfg = config_with(vec![stage("u", "worker")]);
        let id = spawn_id("u", ROLE_IMPLEMENTER, 0);
        spawn::record_result(&store, &spawn::SpawnResult::ok(&id, "implemented")).unwrap();

        // Three consecutive steps replay the SAME recorded history (the implementer is
        // answered from the log every time; the unit reaches `verified` and, on_pass
        // being `none`, stays there without integrating).
        for _ in 0..3 {
            let driver = ReplayDriver::new(&store);
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
        }

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let started = events
            .iter()
            .filter(|e| e.type_ == crate::ledger::TYPE_UNIT_STARTED)
            .count();
        let count_status = |status: &str| {
            events
                .iter()
                .filter(|e| {
                    e.type_ == crate::ledger::TYPE_UNIT_STATUS
                        && String::from_utf8_lossy(&e.data)
                            .contains(&format!("\"status\":\"{status}\""))
                })
                .count()
        };
        assert_eq!(
            started, 1,
            "UnitStarted is appended once across three replay steps"
        );
        assert_eq!(
            count_status("green"),
            1,
            "green is appended once across three replay steps"
        );
        assert_eq!(
            count_status("verified"),
            1,
            "verified is appended once across three replay steps"
        );
    }

    #[test]
    fn a_replayed_spawn_stamps_the_model_alias_and_resolved_id_on_its_unit_events() {
        // spec 05 line 52: every spawn's recorded events carry the requested model ALIAS
        // and the RESOLVED model id that ran - the latter reported by the worker via
        // `rigger result --meta` and COPIED by the conductor onto the spawn's unit events.
        // Here the courier has recorded the implementer's success together with the
        // resolved id it reported through `--meta`; replaying the recorded spawn must stamp
        // BOTH the requested alias and the resolved id onto the unit events the conductor
        // emits for that spawn.
        use crate::conductor::{META_MODEL_ALIAS, META_MODEL_RESOLVED};

        let store = Store::open(":memory:").unwrap();
        let cfg = config_with(vec![stage("u", "worker")]);
        let id = spawn_id("u", ROLE_IMPLEMENTER, 0);
        spawn::record_result(
            &store,
            &spawn::SpawnResult::ok(&id, "implemented")
                .with_meta(serde_json::json!({ "resolved_model": "claude-opus-4-8-20260101" })),
        )
        .unwrap();

        let driver = ReplayDriver::new(&store);
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
        let status = |want: &str| {
            events
                .iter()
                .find(|e| {
                    e.type_ == crate::ledger::TYPE_UNIT_STATUS
                        && String::from_utf8_lossy(&e.data)
                            .contains(&format!("\"status\":\"{want}\""))
                })
                .unwrap_or_else(|| panic!("the replayed spawn emits a {want} status"))
        };

        // The green status is the implementer spawn's unit event: it carries the requested
        // alias ("sonnet", the worker's configured model) AND the worker-reported resolved id.
        let green = status("green");
        assert_eq!(
            green.meta.get(META_MODEL_ALIAS).map(String::as_str),
            Some("sonnet"),
            "green carries the requested model alias"
        );
        assert_eq!(
            green.meta.get(META_MODEL_RESOLVED).map(String::as_str),
            Some("claude-opus-4-8-20260101"),
            "green carries the resolved model id the worker reported via --meta"
        );

        // The verified status (emitted after the gates, still for the same spawn) carries
        // both too.
        let verified = status("verified");
        assert_eq!(
            verified.meta.get(META_MODEL_ALIAS).map(String::as_str),
            Some("sonnet")
        );
        assert_eq!(
            verified.meta.get(META_MODEL_RESOLVED).map(String::as_str),
            Some("claude-opus-4-8-20260101")
        );

        // UnitStarted carries the requested alias (known at spawn time, before any result).
        let started = events
            .iter()
            .find(|e| e.type_ == crate::ledger::TYPE_UNIT_STARTED)
            .expect("the unit started");
        assert_eq!(
            started.meta.get(META_MODEL_ALIAS).map(String::as_str),
            Some("sonnet"),
            "UnitStarted carries the requested model alias"
        );
    }

    fn stage(name: &str, agent: &str) -> Stage {
        Stage {
            name: name.into(),
            agent: agent.into(),
            // A review-less stage that never merges: it exercises the park/replay of a
            // real conductor spawn without needing a git repo to integrate into.
            on_pass: "none".into(),
            ..Default::default()
        }
    }

    fn config_with(stages: Vec<Stage>) -> Config {
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), worker());
        for st in stages {
            cfg.workflow.stages.insert(st.name.clone(), st);
        }
        cfg
    }

    #[test]
    fn a_step_parks_the_first_frontier_and_the_run_ends_cleanly() {
        // Driving conductor::run with the replay driver over an EMPTY log parks the
        // frontier and returns cleanly: the step's whole state is in the log.
        let store = Store::open(":memory:").unwrap();
        let driver = ReplayDriver::new(&store);
        let cfg = config_with(vec![stage("u", "worker")]);
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };

        let rs = run(&cfg, &deps).expect("a parked frontier is not a run failure");

        // The unit's implementer spawn was parked under its deterministic id, carrying
        // the labels the conductor threaded through SpawnOpts.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let id = spawn_id("u", ROLE_IMPLEMENTER, 0);
        assert!(spawn::is_recorded(&events, &id), "the frontier is parked");
        let parked = spawn::recorded(&events).unwrap();
        assert_eq!(parked[&id].unit, "u");

        // A parked unit is neither integrated nor failed - it is waiting for its result.
        let unit = rs.units.get("u").expect("the unit started");
        assert_ne!(unit.status, crate::ledger::Status::Integrated);
    }

    #[test]
    fn a_recorded_result_replays_and_the_next_step_advances_past_it() {
        let store = Store::open(":memory:").unwrap();
        let cfg = config_with(vec![stage("u", "worker")]);

        // Step 1: park the implementer frontier.
        {
            let driver = ReplayDriver::new(&store);
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
        }
        let id = spawn_id("u", ROLE_IMPLEMENTER, 0);
        assert!(spawn::is_recorded(
            &store.read_stream(STREAM, 0, Direction::Forward).unwrap(),
            &id
        ));

        // A courier records the implementer's result.
        spawn::record_result(&store, &spawn::SpawnResult::ok(&id, "implemented")).unwrap();

        // Step 2: the same conductor run REPLAYS the recorded implementer and advances
        // past it (through gates + the empty review) to `verified`.
        {
            let driver = ReplayDriver::new(&store);
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
        }

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        // Replaying advanced the unit past implement: it reached `verified`.
        let reached_verified = events.iter().any(|e| {
            e.type_ == crate::ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains("\"status\":\"verified\"")
        });
        assert!(
            reached_verified,
            "the recorded spawn was replayed, not re-parked"
        );
        // And the implementer spawn was NOT parked a second time.
        let parks = events
            .iter()
            .filter(|e| e.type_ == spawn::TYPE_SPAWN_REQUESTED)
            .count();
        assert_eq!(parks, 1, "a replayed spawn is never re-parked");
    }

    #[test]
    fn disjoint_ready_units_park_their_spawns_in_one_step() {
        // Two independent units (no dependency between them) are ready in the same wave;
        // both park their spawns in one step, so fan-out falls out of the structure.
        let store = Store::open(":memory:").unwrap();
        let driver = ReplayDriver::new(&store);
        let cfg = config_with(vec![stage("a", "worker"), stage("b", "worker")]);
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };

        run(&cfg, &deps).expect("parking a whole wave is not a run failure");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(spawn::is_recorded(
            &events,
            &spawn_id("a", ROLE_IMPLEMENTER, 0)
        ));
        assert!(spawn::is_recorded(
            &events,
            &spawn_id("b", ROLE_IMPLEMENTER, 0)
        ));
        assert_eq!(
            spawn::recorded(&events).unwrap().len(),
            2,
            "both disjoint units parked in the same step"
        );
    }

    #[test]
    fn the_budget_breaker_binds_across_step_processes() {
        // spec 04, criterion 5 / finding adv-budget-per-step-resets: the spawn-budget
        // breaker counts spawn requests from the LOG, so it binds across the separate
        // processes a stepwise run spans. An earlier step already parked (and a courier
        // answered) one spawn, spending a budget of 1. A FRESH process - its in-memory
        // counter starting at zero - must still fold that spent spawn from the log and
        // refuse the next unit's spawn, aborting with BudgetExhausted. If the count reset
        // per process (the pre-fix bug), the new unit would park and the run would spawn
        // unboundedly across steps.
        let store = Store::open(":memory:").unwrap();
        let prior = SpawnRequest::new("earlier", "earlier", ROLE_IMPLEMENTER, 0, "prior work");
        spawn::park(&store, &prior).unwrap();
        spawn::record_result(&store, &spawn::SpawnResult::ok(&prior.id, "done")).unwrap();

        let mut cfg = config_with(vec![stage("u", "worker")]);
        cfg.workflow.defaults.budget = 1;

        let driver = ReplayDriver::new(&store);
        let deps = Deps {
            store: &store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(&cfg, &deps).expect("a tripped budget halts the run, it does not error");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.type_ == crate::conductor::TYPE_BUDGET_EXHAUSTED),
            "a fresh step folds the prior step's spawn from the log and trips the budget"
        );
        // The over-budget unit's spawn was REFUSED, never parked: the budget was already
        // fully spent by the earlier step.
        assert!(
            !spawn::is_recorded(&events, &spawn_id("u", ROLE_IMPLEMENTER, 0)),
            "the over-budget unit's spawn is refused, not parked"
        );
    }

    #[test]
    fn a_run_that_spends_its_whole_budget_still_replays_its_recorded_work() {
        // The cross-step count must not ABORT a resume before it can assemble already-paid
        // work: a run that spent exactly its budget of 1 (one parked, then answered,
        // implementer) must still replay that recorded spawn on the next step and advance
        // the unit - NOT trip a spurious BudgetExhausted because the recorded count equals
        // the budget. This is the completion case the `spawns > base_spawns` pre-wave
        // guard protects.
        let store = Store::open(":memory:").unwrap();
        let cfg = {
            let mut c = config_with(vec![stage("u", "worker")]);
            c.workflow.defaults.budget = 1;
            c
        };

        // Step 1: the single unit's implementer parks (spending the whole budget of 1).
        {
            let driver = ReplayDriver::new(&store);
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
        }
        let id = spawn_id("u", ROLE_IMPLEMENTER, 0);
        assert!(
            spawn::is_recorded(
                &store.read_stream(STREAM, 0, Direction::Forward).unwrap(),
                &id
            ),
            "step 1 parked the implementer, spending the budget"
        );
        // A courier answers it.
        spawn::record_result(&store, &spawn::SpawnResult::ok(&id, "implemented")).unwrap();

        // Step 2: a fresh process whose folded count already equals the budget. The
        // recorded spawn must still REPLAY (it is free) and the unit must advance to
        // `verified` - the breaker must not abort this replay-only step.
        {
            let driver = ReplayDriver::new(&store);
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
        }

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == crate::conductor::TYPE_BUDGET_EXHAUSTED),
            "replaying a spawn already paid for must not trip the budget"
        );
        assert!(
            events.iter().any(|e| {
                e.type_ == crate::ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"verified\"")
            }),
            "the recorded spawn replayed and the unit advanced past implement"
        );
    }

    /// A reviewer agent for the review-tier budget test - a read-only lens, distinct id
    /// from the implementer so its spawn id is genuinely NEW (not a replay).
    fn reviewer() -> AgentDef {
        AgentDef {
            id: "reviewer".into(),
            model: "sonnet".into(),
            tools: vec!["Read".into()],
            ..Default::default()
        }
    }

    #[test]
    fn a_resume_at_a_spent_budget_aborts_at_the_review_tier_with_budgetexhausted() {
        // Criterion 5, the review-tier arm the cross-step fold makes load-bearing
        // (findings adv-budget-guard-cannot-assemble-reviewed-unit,
        // budget-review-tier-no-exhausted, adv-confirm-review-tier-no-budgetexhausted).
        //
        // A run spends its whole budget of 1 on the implementer, then resumes. The
        // review-LESS completion test above passes because an empty panel needs no review
        // spawn; adding ONE lens exposes the real dogfood shape (every unit reviews itself
        // with a panel). On the resume the implementer replays FREE to `verified`, then the
        // unit's first review tier - the lens - is a genuinely NEW spawn the spent budget
        // refuses. That refusal must abort with BudgetExhausted, SYMMETRIC with the
        // implementer's Ok(false) refusal, NOT propagate a raw "spawn budget exhausted"
        // error out of run() before the breaker records it (the pre-fix behavior).
        let store = Store::open(":memory:").unwrap();
        let cfg = {
            let mut c = Config::default();
            c.agents.insert("worker".into(), worker());
            c.agents.insert("reviewer".into(), reviewer());
            c.workflow.defaults.budget = 1;
            // A one-lens panel: assembling this unit needs a NEW lens spawn, unlike the
            // review-less stage the completion test uses.
            c.workflow.defaults.review = crate::config::ReviewPanel {
                lenses: vec!["reviewer".into()],
                ..Default::default()
            };
            c.workflow.stages.insert(
                "u".into(),
                Stage {
                    name: "u".into(),
                    agent: "worker".into(),
                    on_pass: "none".into(),
                    ..Default::default()
                },
            );
            c
        };

        // Step 1: the implementer parks, spending the whole budget of 1.
        {
            let driver = ReplayDriver::new(&store);
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
        }
        let impl_id = spawn_id("u", ROLE_IMPLEMENTER, 0);
        assert!(
            spawn::is_recorded(
                &store.read_stream(STREAM, 0, Direction::Forward).unwrap(),
                &impl_id
            ),
            "step 1 parked the implementer, spending the budget"
        );
        // A courier answers the implementer.
        spawn::record_result(&store, &spawn::SpawnResult::ok(&impl_id, "implemented")).unwrap();

        // Step 2: a fresh process whose folded count already equals the budget. The
        // implementer replays free to `verified`, then the review-tier lens is refused.
        {
            let driver = ReplayDriver::new(&store);
            let deps = Deps {
                store: &store,
                driver: &driver,
                gates: &ExecRunner,
                repo: String::new(),
                grounder: None,
                graph: None,
                criteria: Vec::new(),
            };
            // The run HALTS cleanly - the review-tier refusal is not a run error.
            run(&cfg, &deps)
                .expect("a review-tier budget refusal halts the run, it does not error");
        }

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.type_ == crate::conductor::TYPE_BUDGET_EXHAUSTED),
            "the review-tier refusal trips the breaker: a run over budget at a review \
             spawn aborts with BudgetExhausted, like the implementer path"
        );
        // The implementer replayed and the unit advanced past implement to `verified`
        // before the review tier was reached.
        assert!(
            events.iter().any(|e| {
                e.type_ == crate::ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"verified\"")
            }),
            "the recorded implementer replays free and the unit reaches verified"
        );
        // The over-budget lens spawn was REFUSED, never parked: only the implementer is
        // recorded, so the resume never expanded the durable spawn set beyond the budget.
        assert_eq!(
            spawn::recorded(&events).unwrap().len(),
            1,
            "the refused lens is not parked - only the implementer spawn is recorded"
        );
    }
}
