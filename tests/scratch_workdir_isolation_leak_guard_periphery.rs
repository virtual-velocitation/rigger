//! Periphery leak guard for spec 89 criterion 2 (SCRATCH IS OUTSIDE THE STORE TREE)'s
//! governing operator ruling, item 3: the TWO leak guards a REAL in-process
//! `conductor::run()` needs now that item 2's sweep routes it through
//! [`common::isolated_workdir`] instead of the real ambient cache-home default
//! (`op-u89c2-next-round-test-cache-isolation-is-per-test-and-leak-checked` /
//! `op-u89c2-next-round-delivers-items-2-3-4-of-the-class-ruling`).
//!
//! `tests/test_cache_home_reclaimed_periphery.rs` (round 6) already proves the SUBPROCESS
//! half of this contract: a `rigger_courier()`-spawned child's `XDG_CACHE_HOME` pin
//! (`common::test_cache_home`) is reclaimed the moment its owning test thread exits. That
//! proof is silent on the OTHER half item 2 closes - an in-process `conductor::run()` that
//! never spawns the `rigger` binary at all, so `rigger_courier`'s pin never applies, and
//! instead depends on [`common::isolated_workdir`] nesting `cfg.workflow.defaults.workdir`
//! inside the fixture's own repo tempdir so `scratch_root_path`'s SECOND precedence rung
//! wins outright and the resolver never reaches its THIRD (real ambient `XDG_CACHE_HOME`/
//! `HOME`) rung at all. This file proves that mechanism, end to end, against a REAL
//! `conductor::run()` that creates a REAL git unit worktree through a real `ExecRunner`
//! gate - not a synthetic stand-in for either half.
//!
//! Non-vacuous against the real binary, not merely argued from the code: with
//! `one_unit_cfg`'s own `cfg.workflow.defaults.workdir` line removed, guard 1 below fails
//! closed against a throwaway `XDG_CACHE_HOME` standing in for the operator's real one - the
//! exact class of litter `adv-u89c2r6-empty-dir-litter-empirically-reproduced` already
//! reproduced against an unswept file, reproduced again here as this test's own RED state
//! before landing the fix, mirroring that finding's own "isolated fake-HOME run" method so
//! this proof never has to touch the operator's actual `~/.cache/rigger` to make its point.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod common;

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;

/// A no-op `AgentDriver`: every assertion in this file is about the FILESYSTEM side effect a
/// real `ExecRunner` gate and a real unit worktree leave behind, never about agent output
/// content, so the driver itself only has to satisfy the port contract.
struct NoopDriver;

impl AgentDriver for NoopDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        _opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        Ok(AgentResult {
            output: "ok".into(),
            resolved_model: String::new(),
        })
    }
}

/// `git init` plus one real commit, so a fan-out unit worktree has a HEAD to branch off of -
/// mirrors every other periphery file's own identical copy (this codebase's established
/// per-file idiom for this fixture, e.g. `gate_store_fence_periphery.rs`'s
/// `init_repo_with_head`, `fanout_template_needs_and_stage_retries_periphery.rs`'s
/// `temp_git_project_with_commit`).
fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create the fixture repo dir");
    let p = dir.path();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        std::process::Command::new("git")
            .arg("-C")
            .arg(p)
            .args(args)
            .status()
            .expect("git fixture command");
    }
    dir
}

/// A single-stage, single-unit fan-out workflow (mirrors every other periphery file's
/// minimal `implement-template`-shaped fixture) whose real `ExecRunner` gate is a trivial
/// `true` - enough for `conductor::run` to create a REAL git unit worktree under
/// `cfg.workflow.defaults.workdir`, without needing a real cargo build.
fn one_unit_cfg(repo: &Path) -> Config {
    let mut cfg = Config::default();
    // Item 2's fix, item 3's subject: nest the scratch/worktree default back inside this
    // fixture's own repo tempdir so the real unit worktree `run` below creates never reaches
    // the real ambient `XDG_CACHE_HOME`/`HOME` cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo);
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "gate".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        "implement-template".into(),
        Stage {
            name: "implement-template".into(),
            agent: "worker".into(),
            strategy: "fan-out".into(),
            gates: vec!["gate".into()],
            on_pass: "merge".into(),
            ..Default::default()
        },
    );
    cfg
}

/// The real ambient cache-home `rigger` directory THIS process would resolve scratch under
/// by DEFAULT (spec 89 criterion 2's THIRD `scratch_root_path` precedence rung) - read-only,
/// mirroring `src/worktree.rs`'s own
/// `scratch_root_resolves_env_then_config_then_repo_default` unit test, never mutated, so
/// this never races a concurrently-running test over process-global environment state.
/// Reuses [`rigger::driver::replay::cache_home_from`] - the SAME XDG-then-`$HOME/.cache`
/// authority [`rigger::worktree::cache_scratch_root_from`] itself calls - rather than a
/// second, independently-spelled copy of that precedence. `None` on a genuinely homeless
/// host (neither `XDG_CACHE_HOME` nor `HOME` set): nothing to guard there, since
/// `scratch_root_path` itself has nowhere ambient to degrade to either.
fn real_cache_home_rigger_dir() -> Option<PathBuf> {
    let xdg = std::env::var_os("XDG_CACHE_HOME");
    let home = std::env::var_os("HOME");
    Some(rigger::driver::replay::cache_home_from(xdg, home)?.join("rigger"))
}

/// Every direct child entry currently under `dir` (non-recursive: a NEW top-level entry is
/// exactly what a leaked worktree/scratch root would be) - `dir` not yet existing reads as
/// empty, matching a machine that has never run a fixture against this cache home before.
fn snapshot(dir: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect()
}

/// Ruling item 3, guard 1: a REAL in-process `conductor::run()` that creates a REAL git unit
/// worktree, routed through [`common::isolated_workdir`] (item 2's fix), must leave the real
/// ambient cache home exactly as it found it - the class of litter the operator ruling
/// exists to close (`adv-u89c2r6-empty-dir-litter-empirically-reproduced`), proven absent
/// here rather than merely argued from the code.
#[test]
fn an_isolated_in_process_fan_out_run_creates_no_new_entry_under_the_real_cache_home() {
    let Some(real_dir) = real_cache_home_rigger_dir() else {
        return; // genuinely homeless host: nothing for this guard to check
    };
    let before = snapshot(&real_dir);

    let repo = init_repo();
    let cfg = one_unit_cfg(repo.path());
    let store = Store::open(":memory:").unwrap();
    let deps = Deps {
        store: &store,
        driver: &NoopDriver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec!["a widget exists".to_string()],
    };
    let rs = run(&cfg, &deps).expect("the isolated in-process run must complete");
    assert_eq!(
        rs.units.len(),
        1,
        "exactly one fan-out unit for the one criterion"
    );

    let after = snapshot(&real_dir);
    assert_eq!(
        before, after,
        "an isolated in-process conductor::run() must create no new entry under the real \
         cache home {real_dir:?}: before {before:?} after {after:?}"
    );
}

/// Ruling item 3, guard 2: the SAME run's isolated scratch root - the real worktree it just
/// created under [`common::isolated_workdir`]'s nested path - is gone once its OWNING
/// fixture (`repo`'s own `TempDir`) drops, with no separate reclaim step. This is the
/// in-process counterpart to `tests/test_cache_home_reclaimed_periphery.rs`'s identical
/// proof for the subprocess/`XDG_CACHE_HOME` case.
#[test]
fn the_isolated_workdirs_real_worktree_is_gone_once_its_owning_repo_tempdir_drops() {
    let repo = init_repo();
    let cfg = one_unit_cfg(repo.path());
    let scratch_root = PathBuf::from(&cfg.workflow.defaults.workdir);
    let store = Store::open(":memory:").unwrap();
    let deps = Deps {
        store: &store,
        driver: &NoopDriver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: None,
        criteria: vec!["a widget exists".to_string()],
    };
    run(&cfg, &deps).expect("the isolated in-process run must complete");
    assert!(
        scratch_root.exists(),
        "the run must actually have created content under the isolated workdir \
         {scratch_root:?}, or this test proves nothing"
    );

    drop(repo);

    assert!(
        !scratch_root.exists(),
        "the isolated scratch root must be reclaimed the moment its owning repo tempdir \
         drops, with no separate reclaim step: {scratch_root:?}"
    );
}
