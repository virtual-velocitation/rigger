//! Periphery for spec 103, criterion 1 - GATE INHERITANCE: the gates of a fan-out unit are
//! the fan-out template's, always. A proposal that supersedes a criterion's baseline gets
//! the template's gate list; a `gates` field on a proposal is accepted for compatibility and
//! UNIONED in, so a proposal can add a gate but never remove one.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. `src/conductor.rs`'s own `mod
//! tests` (`harvest_proposed_gates_every_case_with_the_templates_list_unioned`,
//! `harvest_proposed_gate_inheritance_survives_a_resumed_window`,
//! `plan_protocol_tells_the_planner_gates_come_from_the_template`) already prove the pure
//! fold arithmetic - that `stages[id].gates` ends up holding the right `Vec<String>` - over
//! a hand-built `stages` map, a `Stub` driver that never runs a real command, and a
//! `deps.repo: String::new()` that never merges anything real. None of them close the
//! periphery gap the spec's own Goal paragraph names as the actual production defect: a
//! unit whose `st.gates` was wrongly empty went "green to verified within a minute of its
//! SDET result" with `verified_evidence` `{}` - NO gate command ever ran, NO `GateVerdict`
//! was ever recorded, and NOTHING in a hand-built-map unit test can prove a REAL gate
//! command actually executes (or does not) as a result of this fold's output. That is a
//! genuine process-boundary fact: whether `st.gates` is empty or not determines whether the
//! unit's real remediation loop (`src/conductor.rs`, the gate-running loop below
//! `harvest_proposed`) invokes a real `Gate::run` command via `gate::ExecRunner` at all, and
//! `verified_evidence` (`src/conductor.rs:10935`) mechanically mirrors `st.gates` into the
//! ledger's per-unit `evidence["verified"]` - the exact field the spec's Goal cites as `{}`.
//!
//! Mechanical surface probes (new/changed pub API, trait impl, CLI flag, event type,
//! cross-module call) over this unit's diff all return EMPTY: `template_gates`,
//! `union_gates` and the modified `harvest_proposed` body are non-pub items entirely local
//! to `src/conductor.rs`, called only from within that same module. But this codebase's own
//! established convention for exactly this class of change - a private conductor fold whose
//! effect is only observable through real execution - is `tests/fanout_template_needs_and_
//! stage_retries_periphery.rs` (spec 91, the sibling fix to this same `harvest_proposed`
//! fold): it proves its own private-fold fix through the crate's PUBLIC `run`/`AgentDriver`
//! API with a REAL git repo and REAL merges, precisely because a `Stub`-driven, in-memory,
//! hand-built-map unit test cannot show the fix survives a real multi-process shape. This
//! file follows that precedent for gate inheritance: it drives the real `run()` entry point
//! with a real `gate::ExecRunner` (so a named gate's `run:` command genuinely executes) and
//! a real git repo (so `on_pass: merge` genuinely merges), and reads the fix's effect off
//! the crate's own public `ledger::RunState`/`Unit::evidence` - the same field the spec's
//! Goal paragraph names as the symptom.
//!
//! The spec's Done-when criterion names three shapes by name - "a proposal that supersedes
//! a baseline, a same-id refine, and an unmatched sub-unit" - so this file drives the real
//! boundary for the two source lines that produce a stage's `gates`: the INSERT site
//! (`gates: union_gates(&template_gates, &u.gates)`, hit identically by both the supersede
//! and the unmatched-sub-unit shapes - the same line, only `criterion_id` resolution
//! differs between them, already exhaustively distinguished at the pure-fold level by the
//! implementer) and the EXISTING-STAGE refine site (`existing.gates =
//! union_gates(&existing.gates, &u.gates)`), a genuinely different line reached only when a
//! second proposal reuses an id already in `stages`.
//!
//! NOT OWNED HERE: the pure fold arithmetic itself (`template_gates`, `union_gates`, and
//! all three of `harvest_proposed`'s insertion/mutation sites, all non-pub, exhaustively
//! covered by the implementer's own colocated tests - including resumed-window
//! re-derivation, a pure-function determinism claim with no new I/O boundary of its own);
//! `PLAN_PROTOCOL`'s prose content (a private string constant, proven by the implementer's
//! own test - its only observable effect on a REAL boundary is exactly the same
//! proposal-to-evidence path this file already drives); the runtime NO-UNGATED-FAN-OUT-UNIT
//! guard (spec 103 criterion 2, not this unit's scope) and the integration re-gate (spec 103
//! criterion 3, not this unit's scope).

mod common;

use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, TYPE_UNIT_PROPOSED,
};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger;
use serde_json::{json, Value};

/// A throwaway project that is its own git repo with one commit, so a base ref like `HEAD`
/// resolves and a real per-unit worktree/branch/merge can land. Mirrors
/// `tests/fanout_template_needs_and_stage_retries_periphery.rs`'s identically-named helper
/// (itself mirroring `tests/cli.rs`'s).
fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    dir
}

/// A real, single-criterion fan-out workflow: one `plan` stage feeding one `implement`
/// fan-out template that declares gate `ok` (`run: "true"`, always passes) - and a SECOND
/// gate `extra` (also always passes), never named on the template, so a proposal that adds
/// it to `gates` proves UNION rather than merely inheriting a set of one.
fn base_cfg(repo: &std::path::Path) -> Config {
    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2 (mirrored from the sibling periphery file): a
    // `Deps` driving a real repo must never reach the ambient `XDG_CACHE_HOME`/`HOME`
    // cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo);
    for id in ["planner", "worker"] {
        cfg.agents.insert(
            id.into(),
            AgentDef {
                id: id.into(),
                ..Default::default()
            },
        );
    }
    cfg.workflow.gates.insert(
        "ok".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.gates.insert(
        "extra".into(),
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

/// The fan-out criterion every planner here supersedes or refines.
const CRITERION: &str = "the auth module lands";

/// The id of the planner's own superseding (or same-id refined) unit.
const PROPOSED_ID: &str = "planner-refines-the-auth-module";

/// A planner driver whose one `UnitProposed` emit supersedes `criterion` with a unit named
/// `proposed_id`, naming exactly `proposal_gates` in its own `gates` field (omitted from
/// the wire entirely when empty, mirroring a real planner that never learned about gates -
/// `#[serde(default)] gates: Vec<String>` on the conductor's decode side, so an absent key
/// and an empty array decode identically). The `worker` role writes one real file into its
/// real worktree so `on_pass: merge` has something to merge.
struct SupersedingPlannerDriver {
    proposed_id: String,
    criterion: String,
    proposal_gates: Vec<String>,
}

impl AgentDriver for SupersedingPlannerDriver {
    fn spawn(
        &self,
        agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if agent.id == "planner" {
            let mut body = json!({
                "id": self.proposed_id,
                "agent": "worker",
                "criterion": self.criterion,
            });
            if !self.proposal_gates.is_empty() {
                body["gates"] = json!(self.proposal_gates);
            }
            emit(TYPE_UNIT_PROPOSED, body)?;
            return Ok(AgentResult {
                output: "proposed a refinement".into(),
                resolved_model: String::new(),
            });
        }
        if !opts.dir.is_empty() {
            let file = format!(
                "{}/{}.rs",
                opts.dir,
                opts.unit.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            );
            std::fs::write(file, "pub fn done() {}\n").unwrap();
        }
        Ok(AgentResult {
            output: "ok".into(),
            resolved_model: String::new(),
        })
    }
}

/// A planner driver whose one `spawn()` call emits TWO `UnitProposed` events for the SAME
/// id before returning - the first superseding the baseline with no `gates` named (hits the
/// INSERT site), the second re-emitted under the identical id naming `extra` (hits the
/// EXISTING-STAGE refine site, `existing.gates = union_gates(&existing.gates, &u.gates)` -
/// a different source line from the one `SupersedingPlannerDriver` above exercises). Both
/// land in the store before the run's next `harvest_proposed` pass ever reads it, exactly
/// mirroring the implementer's own pure-fold refine fixture's event order.
struct RefiningPlannerDriver {
    proposed_id: String,
    criterion: String,
}

impl AgentDriver for RefiningPlannerDriver {
    fn spawn(
        &self,
        agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if agent.id == "planner" {
            emit(
                TYPE_UNIT_PROPOSED,
                json!({
                    "id": self.proposed_id,
                    "agent": "worker",
                    "criterion": self.criterion,
                }),
            )?;
            emit(
                TYPE_UNIT_PROPOSED,
                json!({
                    "id": self.proposed_id,
                    "agent": "worker",
                    "criterion": self.criterion,
                    "gates": ["extra"],
                }),
            )?;
            return Ok(AgentResult {
                output: "proposed, then refined by id".into(),
                resolved_model: String::new(),
            });
        }
        if !opts.dir.is_empty() {
            let file = format!(
                "{}/{}.rs",
                opts.dir,
                opts.unit.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            );
            std::fs::write(file, "pub fn done() {}\n").unwrap();
        }
        Ok(AgentResult {
            output: "ok".into(),
            resolved_model: String::new(),
        })
    }
}

/// Drive the crate's public `run()` over [`base_cfg`]'s workflow with `driver` as the planner, a
/// real `gate::ExecRunner` (so every gate genuinely executes as a child process) and a real git
/// repo (so `on_pass: merge` genuinely merges): the planner's [`PROPOSED_ID`] unit must integrate
/// through a real merge, and its real recorded `verified` evidence - mechanically mirroring
/// `st.gates` (`verified_evidence`) - must read exactly `verified`.
fn proposal_integrates_verified_by(driver: &dyn AgentDriver, verified: &str) {
    let repo = temp_git_project_with_commit();
    let cfg = base_cfg(repo.path());

    let store = Store::open(":memory:").unwrap();
    let deps = Deps {
        store: &store,
        driver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![CRITERION.to_string()],
    };
    let rs = run(&cfg, &deps).unwrap();

    assert_eq!(
        rs.units.get(PROPOSED_ID).map(|u| u.status),
        Some(ledger::Status::Integrated),
        "the planner's unit must still integrate through a real git merge; units: {:?}",
        rs.units.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        rs.units[PROPOSED_ID]
            .evidence
            .get("verified")
            .map(String::as_str),
        Some(verified),
        "the planner's unit must carry the template's own gate list, unioned with any gate it          names itself, into a REAL gate run recorded as real evidence; got evidence: {:?}",
        rs.units[PROPOSED_ID].evidence
    );
}

rigger::test_cases! {
    /// GATE INHERITANCE's central production claim: a planner proposal that supersedes a
    /// fan-out criterion's baseline and NAMES NO `gates` field at all - the exact "a proposal
    /// naming none ran ungated" shape the spec's Goal paragraph describes - still runs the
    /// template's own gate for real. Drives the crate's public `run()` with a real
    /// `gate::ExecRunner` (so `ok`'s `run: "true"` genuinely executes as a child process) and a
    /// real git repo (so the unit's `on_pass: merge` genuinely merges), then reads the fix's
    /// effect off the same public field the spec's Goal paragraph names as the symptom:
    /// `Unit::evidence["verified"]`, mechanically mirroring `st.gates`
    /// (`verified_evidence`, `src/conductor.rs:10935`) - `{}` before this fix (RED: `gates:
    /// u.gates` with `u.gates` empty), `"gates passed: ok"` after it.
    a_gateless_supersede_of_a_fanout_baseline_still_runs_the_templates_gate_for_real:
        proposal_integrates_verified_by(
            &SupersedingPlannerDriver {
                proposed_id: PROPOSED_ID.to_string(),
                criterion: CRITERION.to_string(),
                proposal_gates: Vec::new(),
            },
            "gates passed: ok",
        );
    /// GATE INHERITANCE's union half: a proposal that DOES name its own gate gets it unioned
    /// onto the template's, never substituted for it - both gates run for real and both show up
    /// in the real recorded evidence, template's gate first (base-list order preserved).
    a_supersede_naming_its_own_gate_unions_it_onto_the_templates_gate_for_real:
        proposal_integrates_verified_by(
            &SupersedingPlannerDriver {
                proposed_id: PROPOSED_ID.to_string(),
                criterion: CRITERION.to_string(),
                proposal_gates: vec!["extra".to_string()],
            },
            "gates passed: ok, extra",
        );
    /// GATE INHERITANCE's third named shape: a SAME-ID REFINE. The refine's own `extra` gate
    /// must union onto the EXISTING stage's already-templated gate list (`existing.gates =
    /// union_gates(&existing.gates, &u.gates)`) rather than overwrite it - a source line
    /// distinct from the insert site the two tests above exercise, so this proves that line
    /// also survives real gate execution and real evidence recording, not merely the insert
    /// site.
    a_same_id_refine_unions_its_own_gate_onto_the_already_templated_list_for_real:
        proposal_integrates_verified_by(
            &RefiningPlannerDriver {
                proposed_id: PROPOSED_ID.to_string(),
                criterion: CRITERION.to_string(),
            },
            "gates passed: ok, extra",
        );
}
