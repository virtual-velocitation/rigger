//! Periphery (real-on-disk-config, real-git, cross-module) test for spec 92's checkin-stage
//! re-enumeration over the full spec diff (base `8548817`): `src/conductor.rs`'s pre-gate
//! attempt commit (`RunCtx::run_single_stage`, right after the SDET-author spawn and before the
//! gates run, so gate-green collapses to committed-green - see the module doc comment there)
//! switched from `Worktree::commit` to `Worktree::commit_checkpoint`
//! (commit `066ceaaf9ef754a4d7e0972ed262df0d85fde5d9`, "Every conductor commit is a checkpoint
//! that bypasses git hooks").
//!
//! WHAT THE IMPLEMENTER'S OWN TESTS ALREADY COVER (not re-proven here). `src/worktree.rs`'s own
//! `mod tests` proves `Worktree::commit_checkpoint` itself commits through a refusing
//! `pre-commit` hook while the ordinary `Worktree::commit` is refused by the identical hook
//! (`commit_checkpoint_commits_through_a_refusing_hook_while_commit_is_refused`, white-box,
//! calling the method directly on a bare `Worktree`).
//!
//! WHAT THAT LEAVES UNTESTED. Nothing anywhere drives the CONDUCTOR'S own pre-gate attempt
//! commit through a real, installed, refusing hook. This matters concretely: this exact
//! call site is the one the commit message's own incident names ("the docs-drift pre-commit
//! hook, in the incident") - `rigger setup` installs a real docs-drift-checking pre-commit hook
//! into every self-hosting repo (`tests/cli.rs`'s own `pre-commit hook` fixtures), so a unit
//! whose docs are not yet regenerated at attempt-commit time (gates - including any docs-drift
//! gate - run AFTER this commit, never before) would have its own implementer's and SDET's work
//! silently refused and lost if this call site ever regressed back to the hook-respecting
//! `Worktree::commit`. A grep-only regression guard (asserting the literal token
//! `commit_checkpoint` appears at the call site) would not catch a same-behavior-looking
//! refactor that still ends up invoking the hook; this drives the REAL commit through a REAL
//! refusing hook and proves the run completes instead of erroring.
//!
//! Verified RED then GREEN by hand: reverting this call site's `w.commit_checkpoint(...)` back
//! to `w.commit(...)` makes `a_pre_gate_attempt_commit_bypasses_an_installed_refusing_hook` fail
//! with the hook's own refusal text surfaced through `run`'s `Err` (never a silent hang or a
//! differently-shaped failure), confirming the assertion is load-bearing; restoring the fix
//! verbatim (`git diff` on `src/` clean) returns it to green.

mod common;

use common::fixtures::mk_stage;
use common::fixtures::A_WORK_DRIVER;
use common::fixtures::{repo_with_refusing_hook, workflow_cfg};
use rigger::conductor::{run, Deps};
use rigger::eventstore::sqlite::Store;
use rigger::ledger;
use std::path::Path;

/// Drives the CONDUCTOR's real pre-gate attempt commit (`RunCtx::run_single_stage`, the
/// `w.commit_checkpoint(...)` call right after the SDET-author spawn) through a genuinely
/// installed, always-refusing `pre-commit` hook - not the implementer's own same-file
/// `commit_checkpoint_commits_through_a_refusing_hook_while_commit_is_refused` unit test
/// (white-box, calls `Worktree::commit_checkpoint` directly, never through `run`).
#[test]
fn a_pre_gate_attempt_commit_bypasses_an_installed_refusing_hook() {
    let (repo, repo_path) = repo_with_refusing_hook();

    let mut cfg = workflow_cfg(
        &["worker", "lens", "judge"],
        &[("g", "exit 0")],
        vec![mk_stage("unit-a", "g")],
    );
    cfg.workflow.defaults.workdir = format!("{repo_path}/.rigger-test-scratch");

    let store = Store::open(":memory:").unwrap();
    let driver = A_WORK_DRIVER;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };

    let rs = run(&cfg, &deps).expect(
        "the pre-gate attempt commit must go through the installed refusing hook - a hook \
         refusal here would surface as a run Err, never a silent hang",
    );
    assert_eq!(rs.units["unit-a"].status, ledger::Status::Integrated);
    assert_eq!(
        rs.units["unit-a"].attempts, 0,
        "a clean first attempt through a bypassed hook is not a charged remediation attempt"
    );
    assert_eq!(
        std::fs::read_to_string(Path::new(&repo_path).join("a.rs")).unwrap(),
        "A_WORK\n",
        "the implementer's work must have actually landed - the commit was not merely skipped"
    );

    drop(repo);
}
