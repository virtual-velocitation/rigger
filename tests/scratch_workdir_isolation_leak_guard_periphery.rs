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

use std::path::{Path, PathBuf};

mod common;
use common::fixtures::{fan_out_stage, workflow_cfg, NoopDriver};
use common::git::temp_git_project_with_commit;

use rigger::conductor::{run, Deps};
use rigger::config::Config;
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;

/// A single-stage, single-unit fan-out workflow (mirrors every other periphery file's
/// minimal `implement-template`-shaped fixture) whose real `ExecRunner` gate is a trivial
/// `true` - enough for `conductor::run` to create a REAL git unit worktree under
/// `cfg.workflow.defaults.workdir`, without needing a real cargo build.
fn one_unit_cfg(repo: &Path) -> Config {
    let mut cfg = workflow_cfg(
        &["worker"],
        &[("gate", "true")],
        vec![fan_out_stage("implement-template", &[], &["gate"])],
    );
    // Item 2's fix, item 3's subject: nest the scratch/worktree default back inside this
    // fixture's own repo tempdir so the real unit worktree `run` below creates never reaches
    // the real ambient `XDG_CACHE_HOME`/`HOME` cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo);
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

/// Ruling item 3, guard 1: a REAL in-process `conductor::run()` that creates a REAL git unit
/// worktree, routed through [`common::isolated_workdir`] (item 2's fix), must leave the real
/// ambient cache home exactly as it found it for THIS fixture's own repo - the class of
/// litter the operator ruling exists to close
/// (`adv-u89c2r6-empty-dir-litter-empirically-reproduced`), proven absent here rather than
/// merely argued from the code.
///
/// Scoped to the ONE entry an isolation regression would create for this fixture's own
/// (freshly minted, therefore never-before-seen) repo path - computed through
/// [`rigger::worktree::cache_scratch_root_from`], the SAME production authority
/// `real_cache_home_rigger_dir` itself reuses, never a second, independently-spelled copy of
/// the precedence - rather than a whole-directory snapshot. The real ambient cache home is
/// legitimately SHARED scratch space for every concurrently running test binary in the suite
/// (`.cargo/pidns-runner.sh` pins one `XDG_CACHE_HOME` for the whole run, by design - see its
/// own header comment): other periphery tests deliberately exercise the real ambient default
/// themselves, so a sibling creating or reclaiming its OWN, differently-keyed entry there
/// during this window is expected concurrent traffic, never a leak this guard should fail on
/// (a whole-directory `before == after` snapshot flaked exactly this way under
/// `cargo-mutants`' full-suite baseline: a concurrent sibling's own unrelated entry vanished
/// mid-window - `sdet-checkin-scratch-leak-guard-scoped-not-snapshot`).
#[test]
fn an_isolated_in_process_fan_out_run_creates_no_new_entry_under_the_real_cache_home() {
    if real_cache_home_rigger_dir().is_none() {
        return; // genuinely homeless host: nothing for this guard to check
    }

    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let cfg = one_unit_cfg(repo.path());
    let store = Store::open(":memory:").unwrap();
    let deps = Deps {
        store: &store,
        driver: &NoopDriver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: vec!["a widget exists".to_string()],
    };

    let would_leak = rigger::worktree::cache_scratch_root_from(
        &repo_path,
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
    )
    .expect("a non-empty, non-homeless fixture always resolves a cache-home scratch root");
    assert!(
        !would_leak.exists(),
        "precondition: a freshly minted fixture repo path must not already have an entry at \
         the exact path an isolation regression would create, or this test proves nothing: \
         {would_leak:?}"
    );

    let rs = run(&cfg, &deps).expect("the isolated in-process run must complete");
    assert_eq!(
        rs.units.len(),
        1,
        "exactly one fan-out unit for the one criterion"
    );

    assert!(
        !would_leak.exists(),
        "an isolated in-process conductor::run() must create no new entry under the real \
         cache home: the exact entry an isolation regression would create for this \
         fixture's own repo now exists at {would_leak:?}"
    );
}

/// Ruling item 3, guard 2: the SAME run's isolated scratch root - the real worktree it just
/// created under [`common::isolated_workdir`]'s nested path - is gone once its OWNING
/// fixture (`repo`'s own `TempDir`) drops, with no separate reclaim step. This is the
/// in-process counterpart to `tests/test_cache_home_reclaimed_periphery.rs`'s identical
/// proof for the subprocess/`XDG_CACHE_HOME` case.
#[test]
fn the_isolated_workdirs_real_worktree_is_gone_once_its_owning_repo_tempdir_drops() {
    let repo = temp_git_project_with_commit();
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
