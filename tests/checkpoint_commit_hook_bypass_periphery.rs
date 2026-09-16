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

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{self, AgentDef, Config, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::ledger;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// A throwaway git repo with one empty commit, so a run-branch anchor (`HEAD`) resolves.
/// Mirrors `src/conductor.rs::tests::init_repo` (private to that module) and every other
/// periphery suite's identical copy (e.g. `tests/integrate_conflict_merge_periphery.rs`,
/// `tests/worktree_liveness_fence_periphery.rs`).
fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().to_str().unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        Command::new("git")
            .arg("-C")
            .arg(p)
            .args(args)
            .output()
            .unwrap();
    }
    dir
}

/// Installs an ALWAYS-REFUSING `pre-commit` hook into `repo_path`'s `.git/hooks` - git worktrees
/// created off this repo share its hooks directory (proven directly by
/// `src/worktree.rs::tests::commit_checkpoint_commits_through_a_refusing_hook_while_commit_is_refused`),
/// so a worktree spawned for a unit inherits this same refusal.
fn install_refusing_hook(repo_path: &str) {
    let hooks = Path::new(repo_path).join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'hook: refusing' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
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
        inputs: Vec::new(),
    }
}

fn review_panel() -> config::ReviewPanel {
    config::ReviewPanel {
        lenses: vec!["lens".into()],
        adjudicator: "judge".into(),
        ..Default::default()
    }
}

fn mk_stage(name: &str, gate: &str) -> Stage {
    Stage {
        name: name.into(),
        agent: "worker".into(),
        gates: vec![gate.into()],
        on_pass: "merge".into(),
        needs: vec![],
        review: review_panel(),
        ..Default::default()
    }
}

/// An ordinary, conflict-free single-unit implementer: writes `a.rs` and nothing else. The
/// non-implementer spawns (SDET-author, review lens, adjudicator) all approve outright - only
/// their fact of running (not their content) matters here.
struct SimpleWorkDriver;

impl AgentDriver for SimpleWorkDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join("a.rs"), "A_WORK\n").unwrap();
            return Ok(AgentResult::default());
        }
        if opts.id.contains("/adjudicator#") {
            return Ok(AgentResult {
                output: r#"{"verdict":"approve"}"#.into(),
                resolved_model: String::new(),
            });
        }
        Ok(AgentResult {
            output: "reviewed the diff".into(),
            resolved_model: String::new(),
        })
    }
}

/// Drives the CONDUCTOR's real pre-gate attempt commit (`RunCtx::run_single_stage`, the
/// `w.commit_checkpoint(...)` call right after the SDET-author spawn) through a genuinely
/// installed, always-refusing `pre-commit` hook - not the implementer's own same-file
/// `commit_checkpoint_commits_through_a_refusing_hook_while_commit_is_refused` unit test
/// (white-box, calls `Worktree::commit_checkpoint` directly, never through `run`).
#[test]
fn a_pre_gate_attempt_commit_bypasses_an_installed_refusing_hook() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    install_refusing_hook(&repo_path);

    // Sanity: the hook really does refuse an ordinary commit in a worktree of this same repo,
    // so a green run below is proof of a bypass, never proof the hook was toothless.
    let wt_path = std::env::temp_dir().join(format!("hook-sanity-{}", uuid::Uuid::new_v4()));
    let wt = rigger::worktree::Worktree::create(
        &repo_path,
        wt_path.to_str().unwrap(),
        "rigger/hook-sanity",
        "",
    )
    .unwrap();
    std::fs::write(wt_path.join("probe.txt"), "x\n").unwrap();
    let err = wt
        .commit("rigger: probe")
        .expect_err("the installed hook must refuse an ordinary commit in a sibling worktree");
    assert!(err.to_string().contains("hook: refusing"), "{err}");
    drop(wt);
    let _ = std::fs::remove_dir_all(&wt_path);

    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = format!("{repo_path}/.rigger-test-scratch");
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("lens".into(), agent("lens"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert("g".into(), gate_def("exit 0"));
    cfg.workflow
        .stages
        .insert("unit-a".into(), mk_stage("unit-a", "g"));

    let store = Store::open(":memory:").unwrap();
    let driver = SimpleWorkDriver;
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
