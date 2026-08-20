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
//! Criterion 1 (cross-episode supersede, THIS file's first two tests): the initial planning pass
//! is one episode; a plan-critique REJECT triggers `re_plan`, a SECOND, LATER episode. Both
//! episodes propose a DIFFERENT unit id for the SAME criterion - not a same-id refine and not a
//! same-episode split (criterion 2's angle) - so exactly one live owner must survive: the later
//! episode's unit, and it alone.
//!
//! Criterion 1's THIRD test (round-2 REJECT fix: sdet-c1-refine-branch-never-restamps-episode /
//! adv-u72c1-refine-staleness-order-independent-confirmed) covers the OTHER corner of THE
//! SUPERSEDE RULE - "never a stage from its OWN episode" - through the same real write path: a
//! re-plan spawn that both REFINES the earlier episode's unit (same id) and proposes a
//! genuinely-new sibling for the identical criterion, in one spawn. Before that fix, the
//! same-id fold branch never restamped the refined stage's episode, so its own episode's
//! sibling wrongly reaped it; the fix (and criterion 1's own round-1 defect, write-side
//! episode being unwired) was previously provable ONLY at the inside-out `harvest_proposed`
//! seam, which hand-appends store events with `.with_meta(META_SPAWN, ...)` directly, bypassing
//! the real `re_plan` emit closure the fix itself changed - the identical class of blind spot
//! that got criterion 1's first round-2 write-side defect through periphery testing undetected.
//!
//! Criterion 2 (same-episode siblings survive one harvest together, in any event order,
//! THIS file's FOURTH test): its done-when text names two shapes - "two serving the same
//! criterion (a real split)" and "a refine beside a new empty-id split sibling". The first is
//! already proven by criterion 1's third test above (a real split IS a same-episode-survival
//! proof; THE SUPERSEDE RULE's own-episode exclusion is exactly what lets it through). The
//! second - a same-episode refine paired with a sibling that resolves to NO criterion at all,
//! rather than a real split of the same one - is this file's uncovered corner: every
//! `RefineWithSiblingDriver` fixture above gives the sibling the SAME criterion text. The
//! fourth test drives that shape through the identical real write path via
//! `RefineWithSiblingDriver::new_unmatched_sibling`.

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

/// Drives the plan-critique reject-then-replan cycle where the RE-PLAN spawn (episode 2)
/// does two things in ONE spawn: re-emits the FIRST episode's unit under its EXACT id (a
/// refine - PLAN_PROTOCOL's own vocabulary for revising a unit, never a fresh id) AND
/// proposes a genuinely-new sibling unit for the IDENTICAL criterion (a real split
/// introduced mid-replan, spec 31's guarantee). This is the shape round-2's REJECT fix
/// (sdet-c1-refine-branch-never-restamps-episode / adv-u72c1-refine-staleness-order-
/// independent-confirmed) closes: without restamping the refined stage's episode, its own
/// episode's sibling wrongly reaps it. Neither `TwoEpisodeDriver` nor `ThreeEpisodeDriver`
/// above ever re-proposes an id under a later episode - both always mint a fresh one on
/// every spawn - so this driver is the only one in this file that reaches the same-id
/// fold branch through the real re-plan write path at all.
struct RefineWithSiblingDriver {
    planner: String,
    adjudicator: String,
    worker: String,
    criterion: String,
    calls: Mutex<Vec<String>>,
    planner_spawns: Mutex<Vec<String>>,
    /// The unit id episode 1 proposes; captured so episode 2 can refine that EXACT id
    /// rather than deriving a fresh one from its own spawn id (unlike every other driver
    /// in this file, whose whole point is proposing a DIFFERENT id per episode).
    orig_id: Mutex<Option<String>>,
    /// The genuinely-new sibling id episode 2 proposes alongside the refine.
    sibling_id: Mutex<Option<String>>,
    /// When true, episode 2 emits the genuinely-new sibling's ADD BEFORE the refine
    /// (spec 72 round-3 REJECT fix: adv-u72c1r2-restamp-order-dependent-refine-still-
    /// dropped) - the reverse of the default order, which only the round-2 fix's own
    /// tests ever drove.
    sibling_first: bool,
    /// When `Some`, episode 2's sibling proposal carries THIS criterion text instead of
    /// `self.criterion` - text that matches none of the run's acceptance criteria, so it
    /// resolves to NO criterion (the genuinely-new / empty-criterion-id sub-unit path,
    /// spec 18 §3.3) rather than a real split serving the identical criterion. Spec 72
    /// criterion 2's done-when names this as the OTHER same-episode shape ("a refine
    /// beside a new empty-id split sibling") distinct from the real-split shape every
    /// other constructor below exercises.
    unmatched_sibling_criterion: Option<String>,
}

impl RefineWithSiblingDriver {
    fn new(criterion: &str) -> Self {
        Self::with_order(criterion, false)
    }

    /// Same shape as `new`, but episode 2 emits the sibling's ADD before the refine
    /// (spec 72 round-3 REJECT fix - see `sibling_first`).
    fn new_sibling_first(criterion: &str) -> Self {
        Self::with_order(criterion, true)
    }

    /// Same shape as `new`, but episode 2's sibling proposes `sibling_criterion` - text
    /// that resolves to NO acceptance criterion - instead of a real split of `criterion`
    /// (spec 72 criterion 2's empty-id-sibling shape; see `unmatched_sibling_criterion`).
    fn new_unmatched_sibling(criterion: &str, sibling_criterion: &str) -> Self {
        let mut d = Self::with_order(criterion, false);
        d.unmatched_sibling_criterion = Some(sibling_criterion.to_string());
        d
    }

    fn with_order(criterion: &str, sibling_first: bool) -> Self {
        RefineWithSiblingDriver {
            planner: "planner".into(),
            adjudicator: "judge".into(),
            worker: "worker".into(),
            criterion: criterion.to_string(),
            calls: Mutex::new(Vec::new()),
            planner_spawns: Mutex::new(Vec::new()),
            orig_id: Mutex::new(None),
            sibling_id: Mutex::new(None),
            sibling_first,
            unmatched_sibling_criterion: None,
        }
    }
}

impl AgentDriver for RefineWithSiblingDriver {
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
            let spawn_number = self.planner_spawns.lock().unwrap().len();
            if spawn_number == 1 {
                // Episode 1: the initial proposal, exactly like the other two drivers'
                // first spawn. No `episode` key - PLAN_PROTOCOL never asks for one.
                let unit_id = unit_id_for_spawn(&opts.id);
                *self.orig_id.lock().unwrap() = Some(unit_id.clone());
                emit(
                    TYPE_UNIT_PROPOSED,
                    json!({
                        "id": unit_id,
                        "agent": self.worker,
                        "criterion": self.criterion,
                    }),
                )?;
            } else {
                // Episode 2 (the re-plan): re-emit episode 1's EXACT id (a refine - no
                // `criterion`/`coverage` re-echoed, matching PLAN_PROTOCOL's fold-needs-
                // only shape for a revision) AND, in this SAME spawn, propose a
                // genuinely-new sibling for the IDENTICAL criterion. Both emits share
                // this spawn's own META_SPAWN stamp (the real `emit` closure `re_plan`
                // builds), so both are episode 2's proposals by construction - exactly
                // the shape that exposes a stale restamp.
                let orig_id = self
                    .orig_id
                    .lock()
                    .unwrap()
                    .clone()
                    .expect("episode 1 must have proposed first");
                let sibling_id = unit_id_for_spawn(&opts.id);
                *self.sibling_id.lock().unwrap() = Some(sibling_id.clone());
                let sibling_criterion = self
                    .unmatched_sibling_criterion
                    .clone()
                    .unwrap_or_else(|| self.criterion.clone());
                let emit_refine = |emit: &dyn Fn(&str, Value) -> Result<(), Error>| {
                    emit(
                        TYPE_UNIT_PROPOSED,
                        json!({
                            "id": orig_id,
                            "agent": self.worker,
                        }),
                    )
                };
                let emit_sibling = |emit: &dyn Fn(&str, Value) -> Result<(), Error>| {
                    emit(
                        TYPE_UNIT_PROPOSED,
                        json!({
                            "id": sibling_id,
                            "agent": self.worker,
                            "criterion": sibling_criterion,
                        }),
                    )
                };
                if self.sibling_first {
                    emit_sibling(emit)?;
                    emit_refine(emit)?;
                } else {
                    emit_refine(emit)?;
                    emit_sibling(emit)?;
                }
            }
            return Ok(AgentResult {
                output: "proposed the DAG".into(),
                resolved_model: String::new(),
            });
        }
        if a.id == self.adjudicator {
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
        Ok(AgentResult {
            output: format!("{} ok", a.id),
            resolved_model: String::new(),
        })
    }
}

/// Spec 72 criterion 1, round-2 REJECT fix (sdet-c1-refine-branch-never-restamps-episode /
/// adv-u72c1-refine-staleness-order-independent-confirmed), proven through the real write
/// path instead of a hand-appended store event: episode 1 proposes `u-orig` for a
/// criterion; a plan-critique reject triggers `re_plan` (episode 2), which in ONE spawn
/// both REFINES `u-orig` (same id) and proposes a genuinely-new sibling for the IDENTICAL
/// criterion. THE SUPERSEDE RULE is unconditional on this point ("never a stage from its
/// own episode, in any event order"): both must survive to integration. Before the fix,
/// the same-id fold branch left `u-orig`'s episode stamped at episode 1 forever, so the
/// sibling's ADD-path supersede scan read it as an EARLIER episode's stale owner and
/// wrongly removed it - `u-orig` would never appear in the run state at all, and only the
/// sibling would integrate.
#[test]
fn a_same_id_refine_survives_its_own_episodes_new_sibling_through_the_real_write_path() {
    let criterion = "the sprocket assembly is implemented";
    let cfg = two_episode_cfg();
    let store = Store::open(":memory:").unwrap();
    let driver = RefineWithSiblingDriver::new(criterion);
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
        2,
        "one reject must trigger exactly one re-plan; planner spawns: {spawns:?}"
    );

    let orig_id = driver.orig_id.lock().unwrap().clone().unwrap();
    let sibling_id = driver.sibling_id.lock().unwrap().clone().unwrap();
    assert_ne!(
        orig_id, sibling_id,
        "the refine and its sibling must be distinct ids"
    );

    assert_eq!(
        rs.units["plan-critique"].status,
        ledger::Status::Integrated,
        "the gate must approve the revision and release the fan-out"
    );

    // The REFINED unit must still appear and integrate - it must never be silently
    // reaped by its own episode's new sibling.
    assert!(
        rs.units.contains_key(&orig_id),
        "the refined unit {orig_id:?} must survive its own episode's new sibling, not \
         vanish from the run state as if it were a stale earlier-episode owner; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[&orig_id].status,
        ledger::Status::Integrated,
        "the refined unit {orig_id:?} must run and integrate; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(rs.units[&orig_id].spec_criterion, criterion);

    // The genuinely-new sibling must ALSO survive (spec 31's real-split guarantee) -
    // proving the fix does not merely stop removing the refine by disabling supersession
    // outright.
    assert!(
        rs.units.contains_key(&sibling_id),
        "the genuinely-new same-episode sibling {sibling_id:?} must also survive; \
         units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[&sibling_id].status,
        ledger::Status::Integrated,
        "the sibling {sibling_id:?} must run and integrate; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(rs.units[&sibling_id].spec_criterion, criterion);

    // Both of episode 2's proposals serve the criterion in the final projected state -
    // neither reaped the other.
    let mut serving: Vec<&str> = rs
        .units
        .values()
        .filter(|u| u.spec_criterion == criterion)
        .map(|u| u.id.as_str())
        .collect();
    serving.sort_unstable();
    let mut expected = vec![orig_id.as_str(), sibling_id.as_str()];
    expected.sort_unstable();
    assert_eq!(
        serving, expected,
        "both the refine and its new sibling must serve the criterion after the fold; \
         got {serving:?}"
    );
}

/// Spec 72 criterion 1, round-3 REJECT fix (adv-u72c1r2-restamp-order-dependent-refine-
/// still-dropped): the round-2 fix above (`a_same_id_refine_survives_its_own_episodes_
/// new_sibling_through_the_real_write_path`) only ever drove ONE of the two possible
/// within-spawn emit orders - the refine before the sibling's ADD. This test drives the
/// SAME shape through `RefineWithSiblingDriver::new_sibling_first`, which reverses it:
/// episode 2's spawn emits the genuinely-new sibling's ADD FIRST, then the refine. Before
/// the round-3 fix, the sibling's ADD-path `prior_owners` scan ran while `u-orig` still
/// carried its stale episode-1 stamp (the fold branch that restamps it had not run yet),
/// so it was wrongly reaped as an earlier-episode owner - and the LATER refine event then
/// found no stage to fold onto and silently dropped, PERMANENTLY losing the unit with no
/// recovery signal. THE SUPERSEDE RULE is unconditional on event order ("never a stage
/// from its OWN episode, in any event order"), so this order must deliver the identical
/// outcome as the round-2 test above: both units survive and integrate.
#[test]
fn a_same_id_refine_survives_its_own_episodes_new_sibling_walked_first_through_the_real_write_path()
{
    let criterion = "the sprocket assembly is implemented, reversed order";
    let cfg = two_episode_cfg();
    let store = Store::open(":memory:").unwrap();
    let driver = RefineWithSiblingDriver::new_sibling_first(criterion);
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
        2,
        "one reject must trigger exactly one re-plan; planner spawns: {spawns:?}"
    );

    let orig_id = driver.orig_id.lock().unwrap().clone().unwrap();
    let sibling_id = driver.sibling_id.lock().unwrap().clone().unwrap();
    assert_ne!(
        orig_id, sibling_id,
        "the refine and its sibling must be distinct ids"
    );

    assert_eq!(
        rs.units["plan-critique"].status,
        ledger::Status::Integrated,
        "the gate must approve the revision and release the fan-out"
    );

    // The REFINED unit must still appear and integrate even though its own episode's
    // sibling ADD was walked first - it must never be silently, permanently dropped.
    assert!(
        rs.units.contains_key(&orig_id),
        "the refined unit {orig_id:?} must survive its own episode's new sibling even when \
         the sibling's ADD is walked BEFORE the refine, not vanish from the run state; \
         units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[&orig_id].status,
        ledger::Status::Integrated,
        "the refined unit {orig_id:?} must run and integrate; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(rs.units[&orig_id].spec_criterion, criterion);

    // The genuinely-new sibling must ALSO survive (spec 31's real-split guarantee).
    assert!(
        rs.units.contains_key(&sibling_id),
        "the genuinely-new same-episode sibling {sibling_id:?} must also survive; \
         units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[&sibling_id].status,
        ledger::Status::Integrated,
        "the sibling {sibling_id:?} must run and integrate; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(rs.units[&sibling_id].spec_criterion, criterion);

    // Both of episode 2's proposals serve the criterion in the final projected state,
    // regardless of the emit order within that one spawn.
    let mut serving: Vec<&str> = rs
        .units
        .values()
        .filter(|u| u.spec_criterion == criterion)
        .map(|u| u.id.as_str())
        .collect();
    serving.sort_unstable();
    let mut expected = vec![orig_id.as_str(), sibling_id.as_str()];
    expected.sort_unstable();
    assert_eq!(
        serving, expected,
        "both the refine and its new sibling must serve the criterion after the fold, \
         regardless of event order; got {serving:?}"
    );
}

/// Spec 72 criterion 2 (same-episode siblings survive one harvest together, in any event
/// order): the done-when text's SECOND named shape - "a refine beside a new empty-id split
/// sibling" - proven here through the real write path. Every `RefineWithSiblingDriver`
/// fixture above gives episode 2's sibling the SAME criterion text as the refine (a real
/// split of one criterion into two units); this test instead gives the sibling criterion
/// text that matches NONE of the run's acceptance criteria, so it resolves to no criterion
/// at all - the genuinely-new / empty-criterion-id sub-unit path (spec 18 §3.3). THE
/// SUPERSEDE RULE's prior-owners scan (`conductor.rs`'s `harvest_proposed`) runs only
/// inside the resolved-criterion branch, so an unmatched proposal can neither sweep, nor be
/// swept as, a prior owner - proven here alongside a same-episode refine so both code paths
/// (the same-id fold branch's episode restamp, and the unmatched-add branch) run together in
/// one harvest, exactly as a live replan can produce them.
#[test]
fn a_same_id_refine_survives_its_own_episodes_genuinely_new_unmatched_sibling_through_the_real_write_path(
) {
    let criterion = "the widget module is implemented";
    let cfg = two_episode_cfg();
    let store = Store::open(":memory:").unwrap();
    let driver = RefineWithSiblingDriver::new_unmatched_sibling(
        criterion,
        "an entirely separate concern the spec never lists",
    );
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
        2,
        "one reject must trigger exactly one re-plan; planner spawns: {spawns:?}"
    );

    let orig_id = driver.orig_id.lock().unwrap().clone().unwrap();
    let sibling_id = driver.sibling_id.lock().unwrap().clone().unwrap();
    assert_ne!(
        orig_id, sibling_id,
        "the refine and its sibling must be distinct ids"
    );

    assert_eq!(
        rs.units["plan-critique"].status,
        ledger::Status::Integrated,
        "the gate must approve the revision and release the fan-out"
    );

    // The REFINED unit must still appear and integrate, still serving the real criterion -
    // it must never be silently reaped by its own episode's unmatched sibling.
    assert!(
        rs.units.contains_key(&orig_id),
        "the refined unit {orig_id:?} must survive its own episode's genuinely-new \
         unmatched sibling, not vanish from the run state; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[&orig_id].status,
        ledger::Status::Integrated,
        "the refined unit {orig_id:?} must run and integrate; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(rs.units[&orig_id].spec_criterion, criterion);

    // The genuinely-new unmatched sibling must ALSO survive (spec 31's real-split
    // guarantee, extended by spec 72 criterion 2 to the empty-id shape) - proving the
    // supersede mechanism does not treat "resolves to no criterion" as "gets swept".
    assert!(
        rs.units.contains_key(&sibling_id),
        "the genuinely-new unmatched sibling {sibling_id:?} must also survive; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[&sibling_id].status,
        ledger::Status::Integrated,
        "the unmatched sibling {sibling_id:?} must run and integrate; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_ne!(
        rs.units[&sibling_id].spec_criterion, criterion,
        "an unmatched proposal must never be coerced onto the criterion it did not resolve \
         to - it keeps its own authored text, not the refine's criterion"
    );

    // Only the refine serves the criterion in the final projected state - the unmatched
    // sibling never becomes, and is never treated as, a criterion owner.
    let serving: Vec<&str> = rs
        .units
        .values()
        .filter(|u| u.spec_criterion == criterion)
        .map(|u| u.id.as_str())
        .collect();
    assert_eq!(
        serving,
        vec![orig_id.as_str()],
        "the unmatched sibling must never be counted as serving the criterion; got \
         {serving:?}"
    );
}
