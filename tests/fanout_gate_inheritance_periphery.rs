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
use common::fixtures::{fan_out_stage, plan_stage, workflow_cfg};
use common::fixtures::{unit_proposal, ProposingPlannerDriver};
use common::git::temp_git_project_with_commit;

use rigger::conductor::{run, Deps};
use rigger::config::Config;
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger;

/// A real, single-criterion fan-out workflow: one `plan` stage feeding one `implement`
/// fan-out template that declares gate `ok` (`run: "true"`, always passes) - and a SECOND
/// gate `extra` (also always passes), never named on the template, so a proposal that adds
/// it to `gates` proves UNION rather than merely inheriting a set of one.
fn base_cfg(repo: &std::path::Path) -> Config {
    let mut cfg = workflow_cfg(
        &["planner", "worker"],
        &[("ok", "true"), ("extra", "true")],
        vec![plan_stage(), fan_out_stage("implement", &["plan"], &["ok"])],
    );
    // Spec 89 criterion 2 ruling item 2 (mirrored from the sibling periphery file): a
    // `Deps` driving a real repo must never reach the ambient `XDG_CACHE_HOME`/`HOME`
    // cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo);
    cfg
}

/// The fan-out criterion every planner here supersedes or refines.
const CRITERION: &str = "the auth module lands";

/// The id of the planner's own superseding (or same-id refined) unit.
const PROPOSED_ID: &str = "planner-refines-the-auth-module";

/// Drive the crate's public `run()` over [`base_cfg`]'s workflow with a planner emitting
/// `proposals` (each a [`unit_proposal`] for [`PROPOSED_ID`] under [`CRITERION`]), a real
/// `gate::ExecRunner` (so every gate genuinely executes as a child process) and a real git
/// repo (so `on_pass: merge` genuinely merges): the planner's [`PROPOSED_ID`] unit must
/// integrate through a real merge, and its real recorded `verified` evidence - mechanically
/// mirroring `st.gates` (`verified_evidence`) - must read exactly `verified`.
fn proposal_integrates_verified_by(proposals: &[&[&str]], output: &'static str, verified: &str) {
    let repo = temp_git_project_with_commit();
    let cfg = base_cfg(repo.path());

    let store = Store::open(":memory:").unwrap();
    let driver = ProposingPlannerDriver {
        proposals: proposals
            .iter()
            .map(|gates| unit_proposal(PROPOSED_ID, CRITERION, gates))
            .collect(),
        output,
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec![CRITERION.to_string()],
        log: &|_| {},
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
        "the planner's unit must carry the template's own gate list, unioned with any gate it \
         names itself, into a REAL gate run recorded as real evidence; got evidence: {:?}",
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
        proposal_integrates_verified_by(&[&[]], "proposed a refinement", "gates passed: ok");
    /// GATE INHERITANCE's union half: a proposal that DOES name its own gate gets it unioned
    /// onto the template's, never substituted for it - both gates run for real and both show up
    /// in the real recorded evidence, template's gate first (base-list order preserved).
    a_supersede_naming_its_own_gate_unions_it_onto_the_templates_gate_for_real:
        proposal_integrates_verified_by(
            &[&["extra"]],
            "proposed a refinement",
            "gates passed: ok, extra",
        );
    /// GATE INHERITANCE's third named shape: a SAME-ID REFINE. Two proposals under the same
    /// id - the first naming no gates (the INSERT site), the second naming `extra` (the
    /// EXISTING-STAGE refine site). The refine's own `extra` gate must union onto the EXISTING
    /// stage's already-templated gate list (`existing.gates = union_gates(&existing.gates,
    /// &u.gates)`) rather than overwrite it - a source line distinct from the insert site the
    /// two tests above exercise, so this proves that line also survives real gate execution
    /// and real evidence recording, not merely the insert site.
    a_same_id_refine_unions_its_own_gate_onto_the_already_templated_list_for_real:
        proposal_integrates_verified_by(
            &[&[], &["extra"]],
            "proposed, then refined by id",
            "gates passed: ok, extra",
        );
}
