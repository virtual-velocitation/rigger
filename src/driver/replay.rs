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
/// its granted tools from the agent (already fan-out-stripped by
/// [`AgentDef::allowed_tools`]); and its task prompt from `prompt`. Its model is the
/// cascade rung this attempt resolves ([`AgentDef::model_for_attempt`], spec 10 unit 4),
/// so a `model_ladder` agent parks a request naming the rung it escalated to for
/// `opts.attempt` - the same rung the conductor stamps as the requested alias. Its
/// `max_wall_clock` (resolved from `defaults.max_wall_clock` at config load) rides along
/// too, so the parked spawn also carries its per-role liveness bound (spec 10, unit 3).
fn spawn_request(agent: &AgentDef, prompt: &str, opts: &SpawnOpts) -> SpawnRequest {
    SpawnRequest {
        id: opts.id.clone(),
        unit: opts.unit.clone(),
        stage: opts.stage.clone(),
        prompt: prompt.to_string(),
        system_prompt: opts.system_prompt.clone(),
        model: agent.model_for_attempt(opts.attempt),
        tools: agent.allowed_tools(),
        dir: opts.dir.clone(),
        blast_radius: opts.blast_radius.clone(),
        max_wall_clock: agent.max_wall_clock,
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
        let all = self
            .store
            .read_stream(STREAM, 0, Direction::Forward)
            .map_err(|e| Error(e.to_string()))?;
        // Scope the spawn lookup to the CURRENT run (completes Gap 11): spawn ids for the
        // fixed stages (`plan/...`, `plan-critique/adjudicator#N`, `plan/replan#N`) are
        // spec-INDEPENDENT, so without run-scoping a fresh run REPLAYS a prior run's
        // recorded result for the same id - e.g. a stale plan-critique REJECT - and the
        // gate escalates by replaying an old verdict instead of running the new reviewer
        // (observed: a spec-12 run replayed the spec-10 plan-critique reject). Answering
        // and park-dedup must see only THIS run's events; the park itself is already
        // run-stamped (`park_in_run`).
        let events = crate::run::current_run(&all);

        // ANSWER an already-recorded spawn (replay): a recorded RESULT for this id means
        // the agent already ran, so return its outcome without re-running it. A recorded
        // failure replays AS a failure (never a fabricated success), so a step sees the
        // identical outcome the live run saw and remediates it exactly the same way.
        //
        // EXCEPT a step-synthesized LIVENESS fault (spec 10, unit 3): the agent HUNG and
        // `rigger step` recorded an infra fault on its id to make the stall visible. That
        // is NOT the unit's code failing, so it must charge no remediation attempt - we
        // fall through to RE-PARK it (idempotent, the request is already recorded), so the
        // unit unwinds cleanly like any parked spawn. A real worker result recorded later
        // (last-write-wins) is a genuine answer and supersedes it here.
        if let Some(res) = spawn::result_of(events, &opts.id).map_err(|e| Error(e.to_string()))? {
            if !res.is_liveness_fault() {
                if res.is_error() {
                    return Err(Error(res.error));
                }
                // Surface the RESOLVED model the worker reported through `rigger result
                // --meta` (spec 05 line 52), so the conductor can copy it onto this spawn's
                // unit events.
                let resolved_model = res.resolved_model();
                return Ok(AgentResult {
                    output: res.output,
                    resolved_model,
                });
            }
        }

        // PARK an unrecorded spawn: persist the request so a courier can drain it and the
        // next step replays its result. IDEMPOTENT (finding adv-park-not-idempotent): a
        // step re-running the conductor over recorded history must append NO duplicate
        // SpawnRequested, so park only an id that is not already recorded.
        if !spawn::is_recorded(events, &opts.id) {
            let req = spawn_request(agent, prompt, opts);
            // Park stamped with the run this spawn belongs to (spec 06, unit 1): the
            // conductor threaded the current run id onto `opts`, so the persisted
            // `SpawnRequested` carries the same run-id metadata as the run's other events.
            spawn::park_in_run(self.store, &req, &opts.run_id).map_err(|e| Error(e.to_string()))?;
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
    use crate::spawn::{lens_role, spawn_id, spawn_retry_id, ROLE_ADJUDICATOR, ROLE_IMPLEMENTER};

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
    fn a_parked_spawn_carries_the_model_ladder_rung_for_its_attempt() {
        // Spec 10 unit 4: the ACTUAL model a spawn runs on is the cascade rung its attempt
        // resolves, so the parked SpawnRequest names that rung - not a fixed `model`. A
        // laddered agent parked at attempt 0 carries rung 0; parked at a later remediation
        // attempt it carries the higher rung it escalated to, clamped at the last.
        let agent = AgentDef {
            id: "worker".into(),
            model_ladder: vec!["haiku".into(), "sonnet".into(), "opus".into()],
            ..Default::default()
        };
        let parked_model = |attempt: u32, id: &str| -> String {
            let store = Store::open(":memory:").unwrap();
            let driver = ReplayDriver::new(&store);
            let opts = SpawnOpts {
                id: id.to_string(),
                unit: "u".into(),
                stage: "u".into(),
                attempt,
                ..Default::default()
            };
            // An unrecorded spawn parks (and signals the parked frontier).
            let err = driver.spawn(&agent, "do it", &opts, &no_emit).unwrap_err();
            assert!(is_parked(&err), "an unrecorded spawn must park");
            let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
            spawn::recorded(&events).unwrap()[id].model.clone()
        };
        assert_eq!(
            parked_model(0, "u/implementer#0"),
            "haiku",
            "attempt 0 parks on the cheap first rung"
        );
        assert_eq!(
            parked_model(1, "u/implementer#1"),
            "sonnet",
            "the first remediation parks one rung higher"
        );
        assert_eq!(
            parked_model(9, "u/implementer#9"),
            "opus",
            "past the top clamps at the strongest rung"
        );
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
    fn re_parks_a_liveness_fault_instead_of_charging_it_as_a_failure() {
        // A step recorded a liveness fault on a hung spawn (spec 10, unit 3). Unlike a
        // worker-reported failure, this must NOT replay as a charged error - the agent's
        // process hung, not the unit's code. The driver RE-PARKS it (a clean unwind, no
        // remediation), and appends no duplicate request.
        let store = Store::open(":memory:").unwrap();
        // The spawn was parked, then a liveness fault was recorded on it.
        spawn::park(
            &store,
            &spawn::SpawnRequest::new("u", "u", ROLE_IMPLEMENTER, 0, "task"),
        )
        .unwrap();
        spawn::record_result(
            &store,
            &spawn::SpawnResult::liveness_fault("u/implementer#0", "the agent hung", "infra"),
        )
        .unwrap();

        let driver = ReplayDriver::new(&store);
        let err = driver
            .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
            .expect_err("a liveness fault re-parks (a clean unwind), never a charged error");
        assert!(
            is_parked(&err),
            "it re-parks, so the conductor charges no attempt"
        );

        // Re-parking is idempotent: no DUPLICATE spawn request is appended (the request
        // was already recorded when it first parked).
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let requested = spawn::recorded(&events).unwrap();
        assert_eq!(requested.len(), 1, "no duplicate spawn request is parked");

        // A real result recorded LATER (last-write-wins) IS a terminal answer again.
        spawn::record_result(
            &store,
            &spawn::SpawnResult::ok("u/implementer#0", "recovered output"),
        )
        .unwrap();
        let got = driver
            .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
            .expect("a real result superseding the liveness fault is answered normally");
        assert_eq!(got.output, "recovered output");
    }

    #[test]
    fn a_non_infra_labeled_liveness_fault_is_still_re_parked_no_charge() {
        // Follow-up (b) / sdet-u3-classify-hung-reclassification-cosmetic: the taxonomy class
        // on a liveness fault is a DISPLAY LABEL only; the no-charge re-park is UNIFORM. A
        // hung spawn a workflow deliberately RELABELED (here "product", not the infra
        // default) must STILL re-park (a clean unwind), never replay as a charged error - a
        // hung agent PROCESS is infrastructure regardless of the label, so the unit is never
        // charged. This pins the corrected module doc (class is a label, treatment is uniform).
        let store = Store::open(":memory:").unwrap();
        spawn::park(
            &store,
            &spawn::SpawnRequest::new("u", "u", ROLE_IMPLEMENTER, 0, "task"),
        )
        .unwrap();
        spawn::record_result(
            &store,
            &spawn::SpawnResult::liveness_fault("u/implementer#0", "the agent hung", "product"),
        )
        .unwrap();

        let driver = ReplayDriver::new(&store);
        let err = driver
            .spawn(&worker(), "do it", &opts_for("u/implementer#0"), &no_emit)
            .expect_err("a liveness fault re-parks whatever class labels it, never a charge");
        assert!(
            is_parked(&err),
            "a 'product'-labeled liveness fault re-parks no-charge, exactly like infra"
        );
        // The label still rides the recorded fault for the operator (display/audit).
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            spawn::result_of(&events, "u/implementer#0")
                .unwrap()
                .unwrap()
                .liveness_class(),
            "product",
            "the class rides the fault as a display label even though treatment ignores it"
        );
    }

    #[test]
    fn a_liveness_fault_re_parks_no_charge_across_run_boundaries_then_recovers() {
        // sdet-u3-nocharge-repark-secondstep-and-recovery-untested (follow-up c): drive the
        // no-charge re-park through conductor::run ACROSS the replay boundary (several run()
        // over one store, as successive `rigger step` processes do) AND the recovery - not the
        // isolated single-driver.spawn the other test covers. A liveness fault (as the sweep
        // records) must, on EVERY subsequent run, re-park the spawn (a clean unwind) and
        // append NO UnitFailed - a hung agent never charges the unit - and append no duplicate
        // SpawnRequested; then a real result recorded later supersedes it and the unit advances.
        let store = Store::open(":memory:").unwrap();
        let cfg = config_with(vec![stage("u", "worker")]);
        let id = spawn_id("u", ROLE_IMPLEMENTER, 0);

        // Step 1: conductor::run parks the implementer frontier.
        replay_step(&store, &cfg).expect("a parked frontier is not a run failure");
        assert!(spawn::is_recorded(
            &store.read_stream(STREAM, 0, Direction::Forward).unwrap(),
            &id
        ));

        // The sweep records a liveness fault on the hung spawn (a SpawnResult, never a
        // UnitFailed) - exactly what liveness::sweep does on a stale marker.
        spawn::record_result(
            &store,
            &spawn::SpawnResult::liveness_fault(&id, "the agent hung", "infra"),
        )
        .unwrap();

        // Steps 2 and 3: each run REPLAYS the fault, re-parks (no UnitFailed, no duplicate
        // SpawnRequested), and the unit never advances past implement to `verified`.
        for _ in 0..2 {
            replay_step(&store, &cfg).expect("re-parking a liveness fault is a clean unwind");
            let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
            assert!(
                !events
                    .iter()
                    .any(|e| e.type_ == crate::ledger::TYPE_UNIT_FAILED),
                "a hung spawn charges no remediation attempt across the boundary (no UnitFailed)"
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.type_ == spawn::TYPE_SPAWN_REQUESTED)
                    .count(),
                1,
                "re-parking the same id appends no duplicate SpawnRequested"
            );
            assert!(
                !events.iter().any(|e| {
                    e.type_ == crate::ledger::TYPE_UNIT_STATUS
                        && String::from_utf8_lossy(&e.data).contains("\"status\":\"verified\"")
                }),
                "the unit stays parked at the hung spawn, never advancing while it is hung"
            );
        }

        // Recovery: a real result recorded later (last-write-wins) supersedes the fault.
        spawn::record_result(&store, &spawn::SpawnResult::ok(&id, "implemented")).unwrap();
        replay_step(&store, &cfg).expect("a recovered spawn replays and the run advances");
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            events.iter().any(|e| {
                e.type_ == crate::ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"verified\"")
            }),
            "the real result supersedes the fault and the unit advances past implement"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == crate::ledger::TYPE_UNIT_FAILED),
            "no UnitFailed is ever appended - the hung spawn charged nothing, even on recovery"
        );
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
    fn a_prior_runs_recorded_result_never_answers_a_fresh_runs_same_id_spawn() {
        // Gap 11 completion: the fixed-stage spawn ids (`plan/...`, `plan-critique/
        // adjudicator#N`, `plan/replan#N`) are spec-INDEPENDENT, so the SAME id recurs
        // across runs over one store. The replay lookup is scoped to the CURRENT run, so a
        // fresh run NEVER replays a prior run's recorded result for that id - the observed
        // bug was a spec-12 run answering its plan-critique adjudicator with a spec-10 run's
        // stale REJECT, escalating the gate on an old verdict instead of running the new
        // reviewer. Scoping keeps the prior decision OVERTURNABLE: the new adjudicator must
        // actually run (park) and emit a NEW verdict event, not be short-circuited by the
        // old answer. (Cross-run decision CONTEXT is unaffected - it flows whole-stream
        // through grounding/peers; only this execution-replay lookup is run-scoped.)
        let store = Store::open(":memory:").unwrap();
        let id = "plan-critique/adjudicator#1";

        // Run 1 recorded a REJECT for the spec-independent adjudicator id.
        crate::run::ensure_started(&store, &["spec-10-crit".into()]).unwrap();
        spawn::record_result(&store, &spawn::SpawnResult::ok(id, "reject")).unwrap();

        // A FRESH run begins over the SAME store (distinct criteria => a new RunStarted
        // boundary, so the prior REJECT falls before the current run's slice).
        crate::run::ensure_started(&store, &["spec-12-crit".into()]).unwrap();

        // The same-id adjudicator spawn in the new run PARKS (runs its new reviewer), it
        // does NOT replay run 1's stale "reject".
        let driver = ReplayDriver::new(&store);
        let err = driver
            .spawn(&worker(), "critique the dag", &opts_for(id), &no_emit)
            .expect_err("a prior run's result must not answer a fresh run's same-id spawn");
        assert!(
            is_parked(&err),
            "the fresh run's adjudicator parks (runs anew), never replays the prior verdict"
        );

        // And once the fresh run records its OWN verdict, THAT answers within the run: the
        // new event overturns the old, and within-run replay is unbroken by the scoping.
        spawn::record_result(&store, &spawn::SpawnResult::ok(id, "approve")).unwrap();
        let answered = driver
            .spawn(&worker(), "critique the dag", &opts_for(id), &no_emit)
            .expect("the fresh run's own recorded verdict answers within the run");
        assert_eq!(
            answered.output, "approve",
            "the new run is answered by its OWN verdict, never the prior run's"
        );
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
        // Begin the run BEFORE recording the result, exactly as production does (`run`
        // calls `ensure_started` before any spawn is parked): the run-scoped replay lookup
        // only answers results INSIDE the current run's slice, so a recorded result must
        // sit after the RunStarted boundary - which it always does live.
        crate::run::ensure_started(&store, &[]).unwrap();
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
        // Begin the run before recording (production ordering): the run-scoped replay lookup
        // answers only results inside the current run's slice - see the sibling replay test.
        crate::run::ensure_started(&store, &[]).unwrap();
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
        // Run scoping (spec 06, unit 1): the earlier step's spawn belongs to THIS run, so
        // begin the run before parking it - otherwise it sits before the boundary and the
        // cross-step budget fold never counts it.
        crate::run::ensure_started(&store, &[]).unwrap();
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
    fn a_budget_halt_records_its_breaker_events_once_across_resumes() {
        // Gap 13 / finding adv-budget-exhausted-dup-across-steps: the cross-step spawn
        // fold makes a resume DETERMINISTICALLY re-reach the spent budget and RE-TRIP the
        // breaker every step. A non-keyed emit would append a DUPLICATE `BudgetExhausted`
        // and `TaskAborted` on each re-tripping step, double-reporting the one halt in the
        // audit trail. The breaker must record its events IDEMPOTENTLY (keyed, like the
        // green/verified/reviewed lifecycle), so the halt lands EXACTLY ONCE for the run.
        //
        // The finding's exact MIXED-FRONTIER trigger: budget 1, two independent units ready
        // in the same wave. Step 1 admits and PARKS one implementer (spending the budget)
        // and REFUSES the other, tripping the breaker; the parked unit is unanswered so the
        // step is not done and the driver resumes. Step 2 replays the parked spawn for FREE
        // (its id is already recorded) and, since the whole budget folds from that recorded
        // spawn across steps (all in THIS run's slice - both spawns land after this run's
        // `RunStarted`, so run-scoping keeps them in `base_spawns`), re-reaches the still-
        // unrecorded sibling and REFUSES it again - re-tripping the breaker, which must
        // append no SECOND `BudgetExhausted`/`TaskAborted`.
        let store = Store::open(":memory:").unwrap();
        let mut cfg = config_with(vec![stage("u1", "worker"), stage("u2", "worker")]);
        cfg.workflow.defaults.budget = 1;

        for _ in 0..2 {
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
        }

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        // The mixed frontier really happened: exactly ONE of the two implementers was
        // admitted+parked (its spawn recorded), the other was refused over budget - so the
        // breaker tripped on a genuine over-budget refusal, not a degenerate empty wave.
        let parked_implementers = ["u1", "u2"]
            .iter()
            .filter(|u| spawn::is_recorded(&events, &spawn_id(u, ROLE_IMPLEMENTER, 0)))
            .count();
        assert_eq!(
            parked_implementers, 1,
            "budget 1 admits exactly one implementer spawn; the sibling is refused over budget"
        );
        let budget_exhausted = events
            .iter()
            .filter(|e| e.type_ == crate::conductor::TYPE_BUDGET_EXHAUSTED)
            .count();
        let task_aborted = events
            .iter()
            .filter(|e| e.type_ == crate::conductor::TYPE_TASK_ABORTED)
            .count();
        assert_eq!(
            budget_exhausted, 1,
            "the breaker records BudgetExhausted exactly once across resumes, not once per re-trip"
        );
        assert_eq!(
            task_aborted, 1,
            "the breaker records TaskAborted exactly once across resumes, not once per re-trip"
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

    /// A read-only reviewer agent with its own id (so its spawn ids are distinct from the
    /// implementer's).
    fn named(id: &str) -> AgentDef {
        AgentDef {
            id: id.into(),
            model: "sonnet".into(),
            tools: vec!["Read".into()],
            ..Default::default()
        }
    }

    /// A single-unit config with a lens + adjudicator review panel and `on_pass: none` (so
    /// an approved review folds without a git repo to integrate into). The Gap-18 replay
    /// tests below drive its reviewers across the park/replay boundary.
    fn reviewed_unit_cfg() -> Config {
        let mut cfg = Config::default();
        cfg.agents.insert("worker".into(), worker());
        cfg.agents.insert("sdet".into(), named("sdet"));
        cfg.agents.insert("judge".into(), named("judge"));
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

    /// Run one `rigger step`: a fresh ReplayDriver over `store`, driving `conductor::run`
    /// to its parked frontier (or its loud halt).
    fn replay_step(store: &Store, cfg: &Config) -> Result<(), Error> {
        let driver = ReplayDriver::new(store);
        let deps = Deps {
            store,
            driver: &driver,
            gates: &ExecRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
        };
        run(cfg, &deps).map(|_| ())
    }

    /// A courier answering a parked spawn: record its result. An empty `output` is the
    /// degenerate infrastructure answer `build_result` records for an empty reviewer.
    fn courier_records(store: &Store, id: &str, output: &str) {
        spawn::record_result(store, &spawn::SpawnResult::ok(id, output)).unwrap();
    }

    #[test]
    fn a_degenerate_adjudicator_halts_across_replay_steps_then_recovers_when_healthy() {
        // Gap 18 / adj-u2gap18 fixes 2+3, driven on the PRODUCTION stepwise/replay driver
        // across the park/replay boundary - the untested wedge the reject named (done-when
        // line 37 requires a test INCLUDING replay):
        //  - the adjudicator's ORIGINAL spawn and both respawns are each answered EMPTY by a
        //    courier (`build_result` records an empty success), so the run replays them to
        //    the respawn bound and HALTS loudly, naming the dead adjudicator;
        //  - the retry ids are DETERMINISTIC across the separate step processes (each
        //    respawn parks under its `~retry{n}` id and is answered independently);
        //  - the halt charges the unit no attempt and emits no misattributing lesson;
        //  - RECOVERY is real, not the dead "just re-run": results are last-write-wins, so
        //    re-driving the now-healthy adjudicator and recording a SUBSTANTIVE result for a
        //    retry id lets the next step replay it and fold the review normally.
        let store = Store::open(":memory:").unwrap();
        let cfg = reviewed_unit_cfg();

        let impl_id = spawn_id("u", ROLE_IMPLEMENTER, 0);
        let lens_id = spawn_id("u", &lens_role("sdet"), 0);
        let adj0 = spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 0);
        let adj1 = spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 1);
        let adj2 = spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 2);

        let parked = |id: &str| {
            let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
            spawn::is_recorded(&events, id)
        };

        // Step 1: the implementer parks; a courier answers it.
        replay_step(&store, &cfg).expect("a parked frontier is not a run failure");
        assert!(parked(&impl_id), "step 1 parks the implementer");
        courier_records(&store, &impl_id, "implemented");

        // Step 2: the implementer replays to `verified`, then the LENS parks. A courier
        // answers it with a substantive review, so the halt below is provably the
        // ADJUDICATOR's, not the lens's.
        replay_step(&store, &cfg).unwrap();
        assert!(parked(&lens_id), "step 2 parks the lens");
        courier_records(&store, &lens_id, "lens: reviewed, no blocker");

        // Step 3: the adjudicator's ORIGINAL spawn parks; the courier answers it EMPTY.
        replay_step(&store, &cfg).unwrap();
        assert!(
            parked(&adj0),
            "step 3 parks the adjudicator's original spawn"
        );
        courier_records(&store, &adj0, "");

        // Step 4: retry0 replays EMPTY -> degenerate -> the `~retry1` respawn parks.
        replay_step(&store, &cfg).unwrap();
        assert!(
            parked(&adj1),
            "step 4 parks the deterministic ~retry1 respawn"
        );
        courier_records(&store, &adj1, "  \n ");

        // Step 5: retry0+retry1 replay EMPTY -> the `~retry2` respawn parks.
        replay_step(&store, &cfg).unwrap();
        assert!(
            parked(&adj2),
            "step 5 parks the deterministic ~retry2 respawn"
        );
        courier_records(&store, &adj2, "");

        // Step 6: retry0+retry1+retry2 ALL replay EMPTY -> the respawn bound is exhausted
        // and the run HALTS loudly, naming the dead adjudicator.
        let err = replay_step(&store, &cfg)
            .expect_err("an all-degenerate adjudicator halts the run across replay steps");
        assert!(
            err.0.contains("\"judge\"") && err.0.contains("adjudicator"),
            "the loud halt names the dead reviewer: {}",
            err.0
        );
        // The bound holds WITHIN the run: no fresh retry3 is minted this run.
        assert!(
            !parked(&spawn_retry_id("u", ROLE_ADJUDICATOR, 0, 3)),
            "the respawn bound holds - no retry3 is parked this run"
        );
        // The halt charges the unit no attempt and emits NO misattributing lesson.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == crate::ledger::TYPE_UNIT_FAILED),
            "the halt charges the unit no attempt (no UnitFailed)"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == crate::ledger::TYPE_UNIT_ESCALATED),
            "the halt does not escalate the unit (no UnitEscalated)"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.type_ == crate::contextgraph::TYPE_LESSON_LEARNED),
            "the halt emits no per-unit lesson (no misattribution of the broken reviewer)"
        );

        // RECOVERY: the operator's adjudicator is healthy now. Re-drive it and record a
        // SUBSTANTIVE approve for the latest retry id; results are last-write-wins, so this
        // supersedes the recorded empty - the honest recovery the halt message names.
        courier_records(&store, &adj2, r#"{"verdict":"approve"}"#);

        // Step 7: the run replays retry0+retry1 (empty, degenerate) then retry2 (now the
        // SUBSTANTIVE approve) -> the review folds normally, NO halt.
        replay_step(&store, &cfg).expect("a corrected retry result recovers the run - no halt");
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert!(
            events.iter().any(|e| {
                e.type_ == crate::ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"reviewed\"")
            }),
            "the corrected adjudicator verdict folds and the unit reaches reviewed"
        );
    }

    #[test]
    fn a_recorded_empty_lens_result_is_valid_on_replay_not_degenerate() {
        // adj-u2gap18 fix 1 on the PRODUCTION replay driver: the misclassification is
        // reachable WITHOUT any broken reviewer. A healthy lens emits its findings to the
        // graph and self-reports an EMPTY success; on the replay driver that empty recorded
        // result must fold as VALID (the graph is the channel), never a degenerate respawn
        // that could eventually halt. Every spawn's result is pre-recorded (as couriers
        // would across steps): the lens answered EMPTY, the adjudicator approved. The single
        // replay step must reach `reviewed` with NO lens respawn and NO halt.
        let store = Store::open(":memory:").unwrap();
        let cfg = reviewed_unit_cfg();
        // Begin the run before the couriers record (production ordering): the run-scoped
        // replay lookup answers only results inside the current run's slice.
        crate::run::ensure_started(&store, &[]).unwrap();
        courier_records(&store, &spawn_id("u", ROLE_IMPLEMENTER, 0), "implemented");
        courier_records(&store, &spawn_id("u", &lens_role("sdet"), 0), "");
        courier_records(
            &store,
            &spawn_id("u", ROLE_ADJUDICATOR, 0),
            r#"{"verdict":"approve"}"#,
        );

        replay_step(&store, &cfg)
            .expect("an empty lens result is valid on the replay driver, not a halt");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        // No lens respawn was parked - the empty stdout was not misread as degenerate.
        assert!(
            !spawn::is_recorded(&events, &spawn_retry_id("u", &lens_role("sdet"), 0, 1)),
            "a healthy empty-stdout lens is not respawned on the replay driver"
        );
        // The review folded and the unit reached `reviewed`.
        assert!(
            events.iter().any(|e| {
                e.type_ == crate::ledger::TYPE_UNIT_STATUS
                    && String::from_utf8_lossy(&e.data).contains("\"status\":\"reviewed\"")
            }),
            "the review folds and the unit reaches reviewed"
        );
    }
}
