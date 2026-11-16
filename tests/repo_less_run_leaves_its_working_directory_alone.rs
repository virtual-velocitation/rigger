//! A run with no repository never runs git against the repository its process happens to sit
//! in. A repo-less run (`Deps { repo: "" }`, as every repo-less test drives one) that resumes a
//! run whose prior log integrated `plan` reaches the integrated-branch garbage collection, and
//! an empty repository path handed to `git -C` names the process's working directory: the
//! collection then removed the real `rigger/u/plan` worktree, its directory and its branch of
//! whatever repository the test runner was started in.
//!
//! The suite changes its process's working directory, so it keeps its own executable.

mod common;

use std::path::Path;

use common::fixtures::{run_isolated, test_spawn_host, NoopDriver};
use common::git::{git_ok, temp_git_project_with_commit, trimmed_stdout};
use rigger::conductor::Deps;
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger::Status;

const UNIT: &str = "plan";
const BRANCH: &str = "rigger/u/plan";
const MARKER: &str = "live-unit-work.txt";

/// A workflow of one stage, `plan`, gated by a gate that always passes.
fn plan_workflow() -> Config {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "a".into(),
        AgentDef {
            id: "a".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "g".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
            requires: Vec::new(),
            post_merge: true,
        },
    );
    cfg.workflow.stages.insert(
        UNIT.into(),
        Stage {
            name: UNIT.into(),
            agent: "a".into(),
            gates: vec!["g".into()],
            ..Default::default()
        },
    );
    cfg
}

/// One repo-less step of the run over `store`: the same criteria each time, so a second step
/// adopts the run the first began.
fn repo_less_step(store: &Store) -> Status {
    let driver = NoopDriver;
    let deps = Deps {
        store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
        log: &|_| {},
        check_emit: &rigger::contextgraph::check_emit,
        spawn_host: test_spawn_host(),
        code_store: None,
        context: None,
    };
    run_isolated(&plan_workflow(), &deps).unwrap().units[UNIT].status
}

/// The worktrees `repo` registers, as `git worktree list --porcelain` names them.
fn registered(repo: &Path) -> String {
    trimmed_stdout(&git_ok(repo, &["worktree", "list", "--porcelain"]))
}

#[test]
fn a_resumed_repo_less_run_leaves_the_repository_it_runs_inside_alone() {
    let repo = temp_git_project_with_commit();
    let outside = tempfile::tempdir().unwrap();
    let live = outside.path().join("rigger-wt-plan");
    let live_str = live.to_str().unwrap();
    git_ok(
        repo.path(),
        &["worktree", "add", "-q", "-b", BRANCH, live_str, "HEAD"],
    );
    std::fs::write(live.join(MARKER), "a live unit's work\n").unwrap();
    std::env::set_current_dir(repo.path()).unwrap();

    let store = Store::open(":memory:").unwrap();
    assert_eq!(
        repo_less_step(&store),
        Status::Integrated,
        "sanity: the first step integrates `plan`, so the next step's collection has it to reclaim"
    );
    assert_eq!(repo_less_step(&store), Status::Integrated);

    assert!(
        registered(repo.path()).contains(live_str),
        "the repository's own `{BRANCH}` worktree must stay registered; it registers:\n{}",
        registered(repo.path())
    );
    assert!(
        live.join(MARKER).is_file(),
        "the live worktree's directory and its work must survive a repo-less run"
    );
    git_ok(
        repo.path(),
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{BRANCH}"),
        ],
    );
}
