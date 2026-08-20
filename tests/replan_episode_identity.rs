//! Periphery (integration) tests for spec 72: replan supersede is grounded in the log, not
//! the call. These run OUTSIDE the crate, over the library's PUBLIC `conductor::run` entry
//! with a custom `AgentDriver`, so they guard the boundary the inside-out `harvest_proposed`
//! seam tests (in `src/conductor.rs`'s own test module, which can reach the private
//! `episode`/`criterion_id` `Stage` fields directly) are structurally blind to: a REAL
//! plan-critique reject feeding back into `re_plan` (spec 72's actual production trigger for
//! a second planning episode), driven end to end through gates, review, and merge.
//!
//! Criteria 1-3 of spec 72 share ONE mechanism (the `episode` identity persisted on
//! `UnitProposed`, spec 72's PLAN-EPISODE IDENTITY) tested from three distinct angles; this
//! file is the shared home for their periphery proofs, one test function per criterion, added
//! as each criterion's unit lands (spec 72's own plan-critique note d72plan-one-unit-per-criterion).
//!
//! Criterion 1 (cross-episode supersede, THIS file's first test): the initial planning pass is
//! one episode; a plan-critique REJECT triggers `re_plan`, a SECOND, LATER episode. Both
//! episodes propose a DIFFERENT unit id for the SAME criterion - not a same-id refine and not a
//! same-episode split (criterion 2's angle) - so exactly one live owner must survive: the later
//! episode's unit, and it alone.

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, TYPE_UNIT_PROPOSED,
};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger;
use serde_json::{json, Value};
use std::sync::Mutex;

/// Drives the plan-critique reject-then-replan cycle with a planner that proposes a
/// DIFFERENT unit id for the SAME criterion on each of its two spawns: the initial wave
/// spawn is one planning EPISODE, the critique-driven re-plan is a SECOND, LATER episode
/// over the exact same criterion. The emitted `UnitProposed` JSON carries NO `episode`
/// field at all - exactly the PLAN_PROTOCOL shape a real planner emits, which never
/// asks for one - so the episode identity can ONLY reach `harvest_proposed` through the
/// `emit` callback THIS test receives (the real `RunCtx`-owned closure `run_single_stage`
/// / `re_plan` builds, unchanged from what a live run wires), never through a
/// hand-authored data field. This is what makes these tests prove the REAL write path
/// (round-2 REJECT fix for f-c1-episode-writeside-unwired /
/// sdet-c1-episode-writeside-unwired-test-blindspot): before that fix, both spawns'
/// proposals deserialize `episode` as the empty string and the supersede rule can never
/// fire, exactly the production defect.
struct TwoEpisodeDriver {
    planner: String,
    adjudicator: String,
    worker: String,
    criterion: String,
    calls: Mutex<Vec<String>>,
    /// The planner's own deterministic spawn id on each of its spawns, in spawn order:
    /// the first is the initial wave spawn, each later one is a critique-driven re-plan.
    planner_spawns: Mutex<Vec<String>>,
    /// The unit id the planner proposed on each of its spawns, in the SAME order as
    /// `planner_spawns` (index-for-index), so a test can assert on the exact ids without
    /// re-deriving the sanitize rule this driver applies to a spawn id.
    proposed_ids: Mutex<Vec<String>>,
}

impl TwoEpisodeDriver {
    fn new(criterion: &str) -> Self {
        TwoEpisodeDriver {
            planner: "planner".into(),
            adjudicator: "judge".into(),
            worker: "worker".into(),
            criterion: criterion.to_string(),
            calls: Mutex::new(Vec::new()),
            planner_spawns: Mutex::new(Vec::new()),
            proposed_ids: Mutex::new(Vec::new()),
        }
    }

    fn count(&self, id: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.as_str() == id)
            .count()
    }
}

/// A unit id is a bare token (no `/`, `#`, `~`); a spawn id carries all three, so this
/// derives a stable, distinct unit id per spawn without colliding on illegal characters.
fn unit_id_for_spawn(spawn_id: &str) -> String {
    format!(
        "u-{}",
        spawn_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
    )
}

impl AgentDriver for TwoEpisodeDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        self.calls.lock().unwrap().push(a.id.clone());
        if a.id == self.planner {
            self.planner_spawns.lock().unwrap().push(opts.id.clone());
            let unit_id = unit_id_for_spawn(&opts.id);
            self.proposed_ids.lock().unwrap().push(unit_id.clone());
            // No `episode` key here - PLAN_PROTOCOL never asks the planner to emit one
            // (see the module doc). The episode identity must reach `harvest_proposed`
            // through `emit`'s own META_SPAWN stamp, or not at all.
            emit(
                TYPE_UNIT_PROPOSED,
                json!({
                    "id": unit_id,
                    "agent": self.worker,
                    "criterion": self.criterion,
                }),
            )?;
            return Ok(AgentResult {
                output: "proposed the DAG".into(),
                resolved_model: String::new(),
            });
        }
        if a.id == self.adjudicator {
            // Reject the FIRST DAG (drawing the re-plan that mints the second episode),
            // then approve the revision - a JUDGMENT, modelled directly, independent of
            // any blast-radius mechanics (there are none here: `grounder: None`).
            let already_rejected = self
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.as_str() == self.adjudicator)
                .count()
                > 1;
            let verdict = if already_rejected {
                "approve"
            } else {
                "reject"
            };
            return Ok(AgentResult {
                output: format!("{{\"verdict\":\"{verdict}\"}}"),
                resolved_model: String::new(),
            });
        }
        // The worker (or any other reviewer) does trivial, benign work.
        Ok(AgentResult {
            output: format!("{} ok", a.id),
            resolved_model: String::new(),
        })
    }
}

fn two_episode_cfg() -> Config {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "planner".into(),
        AgentDef {
            id: "planner".into(),
            ..Default::default()
        },
    );
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.agents.insert(
        "judge".into(),
        AgentDef {
            id: "judge".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "ok".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
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
        "plan-critique".into(),
        Stage {
            name: "plan-critique".into(),
            needs: vec!["plan".into()],
            adjudicator: "judge".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "implement".into(),
        Stage {
            name: "implement".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            needs: vec!["plan-critique".into()],
            gates: vec!["ok".into()],
            on_pass: "merge".into(),
            ..Default::default()
        },
    );
    cfg
}

/// Spec 72 criterion 1 (cross-episode supersede): a REAL plan-critique reject feeds back
/// into `re_plan` - the actual production mechanism spec 72's Design names as the source
/// of a second planning episode - and the later episode's unit alone survives to
/// integration; the earlier episode's unit is superseded before it ever starts (it never
/// appears in the projected run state at all), exactly like a conductor-synthesized
/// baseline that a first proposal supersedes.
#[test]
fn a_replan_after_a_critique_reject_supersedes_the_initial_episodes_unit() {
    let criterion = "the periphery widget is implemented";
    let cfg = two_episode_cfg();
    let store = Store::open(":memory:").unwrap();
    let driver = TwoEpisodeDriver::new(criterion);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs = run(&cfg, &deps).unwrap();

    // The reject fed back to the planner: a SECOND spawn (a later episode) ran, with a
    // DISTINCT deterministic id from the first (the initial wave spawn and a
    // critique-driven re-plan are never the same spawn).
    let spawns = driver.planner_spawns.lock().unwrap().clone();
    assert_eq!(
        spawns.len(),
        2,
        "one reject must trigger exactly one re-plan; planner spawns: {spawns:?}"
    );
    assert_ne!(
        spawns[0], spawns[1],
        "the initial spawn and the re-plan must carry distinct deterministic ids \
         (distinct planning episodes); got {spawns:?}"
    );
    assert!(
        driver.count("judge") >= 2,
        "the adjudicator must render a verdict on both the rejected and the revised \
         DAG; judge ran {}x",
        driver.count("judge")
    );

    let ids = driver.proposed_ids.lock().unwrap().clone();
    let (episode_1_unit, episode_2_unit) = (ids[0].clone(), ids[1].clone());

    // The gate approved the revision and released the fan-out.
    assert_eq!(
        rs.units["plan-critique"].status,
        ledger::Status::Integrated,
        "the gate must approve the revised DAG and release the fan-out"
    );
    // The FIRST episode's unit never appears in the projected run state at all: it was
    // superseded (removed from the DAG) before it was ever scheduled, so no
    // UnitStarted was ever recorded for it - exactly like a conductor-synthesized
    // baseline a first proposal supersedes.
    assert!(
        !rs.units.contains_key(&episode_1_unit),
        "the FIRST episode's unit {episode_1_unit:?} must never appear in the run \
         state - superseded before it started; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    // The LATER episode's unit is the one that actually ran, and integrated.
    assert_eq!(
        rs.units[&episode_2_unit].status,
        ledger::Status::Integrated,
        "the LATER episode's unit {episode_2_unit:?} must be the one that ran and \
         integrated; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(rs.units[&episode_2_unit].spec_criterion, criterion);

    // Exactly one unit serves the criterion in the final projected state (the
    // plan-critique gate is review-only and carries no `spec_criterion` of its own).
    let serving: Vec<&str> = rs
        .units
        .values()
        .filter(|u| u.spec_criterion == criterion)
        .map(|u| u.id.as_str())
        .collect();
    assert_eq!(
        serving,
        vec![episode_2_unit.as_str()],
        "exactly one unit must serve the criterion after both episodes fold; got {serving:?}"
    );
}

/// Drives a plan-critique REJECT twice (not once) before approving, so ONE run mints
/// THREE distinct planning episodes over the same criterion instead of the minimal two
/// `TwoEpisodeDriver` above exercises. Otherwise identical shape (same-id sanitizing,
/// same worker/config conventions) - only the reject count differs.
struct ThreeEpisodeDriver {
    planner: String,
    adjudicator: String,
    worker: String,
    criterion: String,
    calls: Mutex<Vec<String>>,
    /// The planner's deterministic spawn id on each of its spawns, in spawn order: the
    /// initial wave spawn, then each critique-driven re-plan.
    planner_spawns: Mutex<Vec<String>>,
    /// The unit id proposed on each spawn, index-for-index with `planner_spawns`.
    proposed_ids: Mutex<Vec<String>>,
}

impl ThreeEpisodeDriver {
    fn new(criterion: &str) -> Self {
        ThreeEpisodeDriver {
            planner: "planner".into(),
            adjudicator: "judge".into(),
            worker: "worker".into(),
            criterion: criterion.to_string(),
            calls: Mutex::new(Vec::new()),
            planner_spawns: Mutex::new(Vec::new()),
            proposed_ids: Mutex::new(Vec::new()),
        }
    }
}

impl AgentDriver for ThreeEpisodeDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        self.calls.lock().unwrap().push(a.id.clone());
        if a.id == self.planner {
            self.planner_spawns.lock().unwrap().push(opts.id.clone());
            let unit_id = unit_id_for_spawn(&opts.id);
            self.proposed_ids.lock().unwrap().push(unit_id.clone());
            // No `episode` key here either - see `TwoEpisodeDriver::spawn`'s matching
            // comment: the identity must come from `emit`'s own META_SPAWN stamp.
            emit(
                TYPE_UNIT_PROPOSED,
                json!({
                    "id": unit_id,
                    "agent": self.worker,
                    "criterion": self.criterion,
                }),
            )?;
            return Ok(AgentResult {
                output: "proposed the DAG".into(),
                resolved_model: String::new(),
            });
        }
        if a.id == self.adjudicator {
            // Reject the first TWO DAGs (minting a second AND a third episode), then
            // approve the third revision. `self.calls` already includes THIS call.
            let adjudicator_calls = self
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.as_str() == self.adjudicator)
                .count();
            let verdict = if adjudicator_calls > 2 {
                "approve"
            } else {
                "reject"
            };
            return Ok(AgentResult {
                output: format!("{{\"verdict\":\"{verdict}\"}}"),
                resolved_model: String::new(),
            });
        }
        Ok(AgentResult {
            output: format!("{} ok", a.id),
            resolved_model: String::new(),
        })
    }
}

/// Spec 72 criterion 1 (cross-episode supersede), the CHAINED case: the single-reject
/// test above proves the minimal two-episode shape; this proves the SUPERSEDE RULE
/// holds transitively across a chain longer than the minimum - two rejects mint THREE
/// distinct planning episodes over one criterion (the initial wave plus two re-plans),
/// and only the LAST episode's unit may survive. A rank comparison that only ever
/// compared a proposal against the immediately-preceding episode (an off-by-one on the
/// "EARLIER", plural, wording in spec 72's Design) would leave the SECOND episode's
/// unit alive alongside the third; this catches that shape specifically, which neither
/// the two-episode periphery test above nor either inside-out seam test (both of which
/// also stop at two episodes) can reach.
#[test]
fn a_second_replan_supersedes_both_earlier_episodes_units() {
    let criterion = "the trinket module is implemented";
    let cfg = two_episode_cfg();
    let store = Store::open(":memory:").unwrap();
    let driver = ThreeEpisodeDriver::new(criterion);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: vec![criterion.to_string()],
    };
    let rs = run(&cfg, &deps).unwrap();

    let spawns = driver.planner_spawns.lock().unwrap().clone();
    assert_eq!(
        spawns.len(),
        3,
        "two rejects must trigger exactly two re-plans (three total episodes); planner \
         spawns: {spawns:?}"
    );

    let ids = driver.proposed_ids.lock().unwrap().clone();
    assert_eq!(ids.len(), 3, "one proposal per episode; got {ids:?}");
    let (episode_1_unit, episode_2_unit, episode_3_unit) =
        (ids[0].clone(), ids[1].clone(), ids[2].clone());
    assert!(
        episode_1_unit != episode_2_unit
            && episode_2_unit != episode_3_unit
            && episode_1_unit != episode_3_unit,
        "all three episodes must propose distinct unit ids; got {ids:?}"
    );

    assert_eq!(
        rs.units["plan-critique"].status,
        ledger::Status::Integrated,
        "the gate must approve the third revision and release the fan-out"
    );
    // Neither earlier episode's unit appears in the final run state at all - each was
    // superseded before it was ever scheduled. The SECOND episode's unit is the case
    // this test adds over the two-episode periphery test above: it must be gone too,
    // not merely "superseded by the third but still visible as started".
    assert!(
        !rs.units.contains_key(&episode_1_unit),
        "the FIRST episode's unit {episode_1_unit:?} must never appear in the run \
         state; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert!(
        !rs.units.contains_key(&episode_2_unit),
        "the SECOND episode's unit {episode_2_unit:?} must also never appear in the \
         run state - superseded in turn by the third episode, exactly like the first \
         was; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[&episode_3_unit].status,
        ledger::Status::Integrated,
        "the THIRD (latest) episode's unit {episode_3_unit:?} must be the one that ran \
         and integrated; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(rs.units[&episode_3_unit].spec_criterion, criterion);

    let serving: Vec<&str> = rs
        .units
        .values()
        .filter(|u| u.spec_criterion == criterion)
        .map(|u| u.id.as_str())
        .collect();
    assert_eq!(
        serving,
        vec![episode_3_unit.as_str()],
        "exactly one unit must serve the criterion after all three episodes fold; got \
         {serving:?}"
    );
}
