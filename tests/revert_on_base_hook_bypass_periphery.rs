//! Periphery (real-on-disk-config, real-git, cross-module) test for spec 92's checkin-stage
//! re-enumeration over commit `7f65c40` ("rigger's own integration commits bypass git hooks"),
//! the follow-up fix `tests/checkpoint_commit_hook_bypass_periphery.rs`'s own failing test
//! (`sdet-u92-checkin-gap12-merge-into-worktree-commit-untested`) triggered: that fix switched
//! THREE of rigger's own machine-bookkeeping commit sites to bypass a repository's git hooks
//! (`--no-verify`), all under one escalation ruling, `d-checkin-rigger-own-commits-bypass-hooks`
//! (`src/worktree.rs`, `Worktree::commit`'s and `Worktree::commit_checkpoint`'s own doc
//! comments name all three by role).
//!
//! WHAT THE OTHER TWO SITES ALREADY HAVE. `a_pre_gate_attempt_commit_bypasses_an_installed_
//! refusing_hook` (the sibling periphery file above) drives a real, refusing-hook-equipped
//! `conductor::run()` for a single, conflict-free unit end to end. `Worktree::merge_into_
//! worktree`'s OWN code (`src/worktree.rs:852`) always finalizes its `git merge --no-commit
//! --no-ff` with a separate commit regardless of whether a conflict ever arose (`--no-commit`
//! leaves `MERGE_HEAD` set even on a clean merge), so that single-unit run already drives BOTH
//! of `merge_into_worktree`'s own hook-bypass sites - its pre-merge `commit_checkpoint` call
//! AND its merge-conclusion `git commit --no-edit --no-verify` - for real, through the real
//! hook, on every unit that test lands. Confirmed by reading `merge_into_worktree` directly:
//! there is no conflict-only branch guarding the finalize commit.
//!
//! WHAT THAT LEAVES UNTESTED - THIS FILE'S OWN GAP. `Worktree::revert_on_base`
//! (`src/worktree.rs:341`) is a separate, free-standing `pub fn`: it does NOT go through
//! `commit`/`commit_checkpoint`/`commit_with` at all, so neither the implementer's own
//! white-box `commit_checkpoint_commits_through_a_refusing_hook_while_commit_is_refused` test
//! nor the sibling periphery file above exercises its `--no-verify` flag even indirectly - a
//! future refactor could drop just this one inline flag and nothing anywhere would catch it.
//! Nothing in the diff's own three `revert_on_base_*` white-box tests
//! (`src/worktree.rs`'s `mod tests`, untouched by commit `7f65c40`) installs a hook either.
//! This drives the conductor's REAL compensation path (spec 12, unit 4:
//! `RunCtx::drain_compensations` -> `Worktree::revert_on_base`) end to end through a real,
//! always-refusing `pre-commit` hook installed in the run's own repository - the seeded
//! two-unit contradiction shape `src/conductor.rs`'s own
//! `a_contradiction_compensates_reverts_and_re_enters_the_integrated_unit` establishes (unit-b's
//! adjudicator proves already-integrated unit-a wrong), reproduced here black-box because the
//! SDET periphery layer never edits the implementer's own `mod tests`.
//!
//! Verified RED then GREEN by hand: reverting `revert_on_base`'s `--no-verify` flag (restoring
//! `run_git(repo, &["commit", "--no-edit", "-m", message])`) makes
//! `a_compensation_revert_bypasses_an_installed_refusing_hook` fail - `run` returns an `Err`
//! carrying the hook's own refusal text (`drain_compensations` propagates `revert_on_base`'s
//! `Err` with a bare `?`), never a silent hang or a differently-shaped failure - confirming the
//! assertion is load-bearing; restoring the fix verbatim (`git diff` on `src/` clean) returns it
//! to green.

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
/// periphery suite's identical copy (e.g. `tests/checkpoint_commit_hook_bypass_periphery.rs`).
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
/// and `revert_on_base` (`repo`, not a linked worktree) runs `git commit` in that SAME
/// repository, so it inherits this same refusal.
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

fn mk_stage(name: &str, needs: Vec<String>) -> Stage {
    Stage {
        name: name.into(),
        agent: "worker".into(),
        gates: vec!["g".into()],
        on_pass: "merge".into(),
        needs,
        review: review_panel(),
        ..Default::default()
    }
}

/// Mirrors `src/conductor.rs::tests::CompDriver` (private to that module) exactly in shape:
/// each unit's implementer writes its OWN disjoint file (so the two land with no merge
/// conflict), unit-a's adjudicator always approves outright, and unit-b's adjudicator approves
/// its OWN work but names already-integrated unit-a as the compensation target - the seeded
/// two-unit contradiction that drives `RunCtx::drain_compensations` -> `Worktree::revert_on_base`
/// for real.
struct CompDriver;

impl AgentDriver for CompDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let unit = opts.id.split('/').next().unwrap_or_default();
        if opts.id.contains("/implementer#") {
            if !opts.dir.is_empty() {
                let (file, content) = match unit {
                    "unit-a" => ("a.rs", "fn a() {}\n"),
                    _ => ("b.rs", "fn b() {}\n"),
                };
                std::fs::write(Path::new(&opts.dir).join(file), content).unwrap();
            }
            return Ok(AgentResult::default());
        }
        if opts.id.contains("/adjudicator#") {
            let out = if unit == "unit-b" {
                r#"{"verdict":"approve","compensate":"unit-a"}"#
            } else {
                r#"{"verdict":"approve"}"#
            };
            return Ok(AgentResult {
                output: out.into(),
                resolved_model: String::new(),
            });
        }
        Ok(AgentResult {
            output: "reviewed the diff".into(),
            resolved_model: String::new(),
        })
    }
}

#[test]
fn a_compensation_revert_bypasses_an_installed_refusing_hook() {
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
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("lens".into(), agent("lens"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert("g".into(), gate_def("exit 0"));
    // unit-b needs unit-a, so unit-a integrates FIRST and unit-b's review can then prove that
    // already-integrated unit-a wrong (the ordering `drain_compensations` requires).
    cfg.workflow
        .stages
        .insert("unit-a".into(), mk_stage("unit-a", vec![]));
    cfg.workflow
        .stages
        .insert("unit-b".into(), mk_stage("unit-b", vec!["unit-a".into()]));

    let store = Store::open(":memory:").unwrap();
    let driver = CompDriver;
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
        "the compensation revert's own commit must go through the installed refusing hook - a \
         hook refusal here would surface as a run Err (drain_compensations propagates \
         revert_on_base's Err with a bare ?), never a silent hang",
    );

    // CONVERGENCE: unit-a re-integrated after its compensation rollback; unit-b integrated.
    assert_eq!(
        rs.units["unit-a"].status,
        ledger::Status::Integrated,
        "unit-a re-integrates after its compensation rollback"
    );
    assert_eq!(
        rs.units["unit-b"].status,
        ledger::Status::Integrated,
        "unit-b integrates"
    );

    // The revert actually landed as a real, evented commit on the run branch - not merely
    // skipped because the hook silently swallowed it into a no-op.
    let log = Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["log", "--pretty=%s"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(
        log.lines().any(|l| l.contains("compensate unit-a")),
        "the run branch must carry the evented revert of unit-a's integrating commit; log:\n{log}"
    );

    // And unit-a's final content is the RE-implemented tree, not merely the pre-revert one -
    // confirming the revert genuinely removed the original commit's effect before unit-a
    // re-landed it (the same file, re-created by the second implementer spawn).
    assert_eq!(
        std::fs::read_to_string(Path::new(&repo_path).join("a.rs")).unwrap(),
        "fn a() {}\n",
        "unit-a's re-integrated work must have actually landed"
    );

    drop(repo);
}
