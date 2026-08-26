//! Periphery (contract / API / integration) tests for spec 77 criterion 1: ONE BUILD
//! LOCATION - the per-unit `CARGO_TARGET_DIR` `RunCtx::spawn_env` (src/conductor.rs)
//! adds to every spawned agent's own process environment, and its real-world
//! consequence: a real `cargo build` run BY that agent, inside its worktree, must land
//! in the per-unit cache instead of embedding a `target/` dir in the worktree itself.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/conductor.rs`'s own tests
//! (`spawn_env_adds_the_per_unit_cargo_target_dir_only_for_a_real_unit_worktree`,
//! `one_build_environment_authority_reaches_both_a_gate_build_and_an_agent_spawn`) prove
//! the Rust-level `(name, value)` pair `spawn_env` derives, and that it reaches a
//! `SpawnOpts.env` a fake/recording driver captured - they never observe a REAL OS
//! process actually receiving it, and never observe what a real `cargo build` DOES with
//! it. A bug in `Command::env`'s ordering, `driver::cli::Driver` silently dropping the
//! var before `Command::output()`, or `CARGO_TARGET_DIR` reaching the subprocess with the
//! wrong VALUE would pass every one of those tests unnoticed while still leaking a
//! multi-gigabyte `target/` tree into every worktree - the exact defect spec 77 exists to
//! close.
//!
//! This file closes that gap, over the crate's PUBLIC surface only, using REAL production
//! types spawning REAL subprocesses:
//!
//! 1. `a_real_cargo_build_the_agent_runs_lands_in_the_per_unit_cache_not_the_worktree`: a
//!    real `driver::cli::Driver` spawns a real fixture-script "agent"
//!    (`tests/fixtures/cargo-build-agent.sh`) with its cwd set to a REAL git worktree
//!    (`worktree::Worktree::create`, the same entry point `rigger step` uses) and
//!    `SpawnOpts.env` carrying the SAME `CARGO_TARGET_DIR` value production code derives
//!    for that worktree (`worktree::unit_cache_sibling`). The agent runs a genuine `cargo
//!    build` against a minimal crate seeded into the worktree; the test reads the REAL
//!    filesystem back afterward: the worktree holds no `target/` dir, and the per-unit
//!    cache dir DOES hold the build's actual output - never a vacuous pass where the
//!    build silently never ran at all.
//! 2. `a_dir_with_no_per_unit_cache_never_forces_cargo_target_dir_onto_a_real_agent_
//!    subprocess`: the mirror-image negative case, at the same real-subprocess
//!    granularity - a spawn whose `dir` derives no per-unit cache (empty, the
//!    `isolation: none` / repo-less shape) must leave `CARGO_TARGET_DIR` untouched in a
//!    real subprocess's environment, exactly mirroring `gate::Runner`'s existing
//!    "target_dir always wins, else inherit" contract at the gate boundary.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use rigger::conductor::{AgentDriver, Error as ConductorError, SpawnOpts};
use rigger::config::AgentDef;
use rigger::driver::cli;
use rigger::worktree::{scratch_root, unit_cache_sibling, Worktree};

/// A real `git init` + one empty commit, so `Worktree::create` has a HEAD to branch a
/// real unit worktree off of - the shape every real `rigger step` unit worktree is
/// created against.
fn init_repo_with_head() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().to_str().unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        Command::new("git")
            .args(args)
            .current_dir(p)
            .status()
            .expect("git fixture command");
    }
    dir
}

/// A minimal, dependency-free binary crate written directly into `dir` - real enough for
/// a real `cargo build` to compile in well under a second, with no network/registry
/// access.
fn seed_minimal_crate(dir: &str) {
    std::fs::write(
        Path::new(dir).join("Cargo.toml"),
        "[package]\nname = \"spawn-target-dir-probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(Path::new(dir).join("src")).unwrap();
    std::fs::write(Path::new(dir).join("src").join("main.rs"), "fn main() {}\n").unwrap();
}

fn no_emit(_t: &str, _v: Value) -> Result<(), ConductorError> {
    Ok(())
}

#[test]
fn a_real_cargo_build_the_agent_runs_lands_in_the_per_unit_cache_not_the_worktree() {
    let repo = init_repo_with_head();
    let repo_path = repo.path().to_string_lossy().into_owned();
    let root = scratch_root(&repo_path, "", None);
    let worktree_dir = format!("{root}/rigger-wt-cargo-build-probe");
    let worktree = Worktree::create(&repo_path, &worktree_dir, "rigger/u/cargo-build-probe")
        .expect("create a real unit worktree");
    seed_minimal_crate(&worktree.dir);

    // The exact per-unit cache `RunCtx::spawn_env` derives for this worktree - the SAME
    // single-source `unit_cache_sibling` production code uses, so this test can never
    // silently drift from the real derivation.
    let target_dir =
        unit_cache_sibling(&worktree.dir).expect("a unit worktree dir must derive a cache sibling");
    assert!(
        !Path::new(&target_dir).exists(),
        "the per-unit cache must not pre-exist before the real build: {target_dir}"
    );

    let agent_bin =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cargo-build-agent.sh");
    assert!(
        agent_bin.exists(),
        "the fixture agent {agent_bin:?} must exist"
    );

    let driver = cli::Driver {
        bin: agent_bin.to_string_lossy().into_owned(),
    };
    let agent = AgentDef {
        id: "worker".into(),
        ..Default::default()
    };
    let opts = SpawnOpts {
        id: "probe/implementer#0".into(),
        unit: "probe".into(),
        stage: "probe".into(),
        dir: worktree.dir.clone(),
        isolation: true,
        // The exact env `RunCtx::spawn_env` would produce for this worktree with no
        // wrapper configured: only its own per-unit CARGO_TARGET_DIR.
        env: vec![("CARGO_TARGET_DIR".to_string(), target_dir.clone())],
        ..Default::default()
    };
    let result = driver
        .spawn(&agent, "build something", &opts, &no_emit)
        .expect("the real agent subprocess must run a real cargo build");

    assert!(
        result.output.contains("CARGO_BUILD=ok"),
        "the real cargo build the agent ran must have actually succeeded, not silently \
         failed (a vacuous pass would let the target/-absence assertion below pass for the \
         wrong reason): {:?}",
        result.output
    );
    assert!(
        !Path::new(&worktree.dir).join("target").exists(),
        "a worktree whose agent ran a real cargo build must hold no embedded target/ dir - \
         CARGO_TARGET_DIR must have redirected it to the per-unit cache instead"
    );
    assert!(
        Path::new(&target_dir).join("debug").exists(),
        "the real cargo build must have actually landed in the per-unit cache at \
         {target_dir} - never a vacuous pass where it built nowhere real"
    );

    worktree.remove().expect("remove the real unit worktree");
}

#[test]
fn a_dir_with_no_per_unit_cache_never_forces_cargo_target_dir_onto_a_real_agent_subprocess() {
    // The negative mirror, at the same real-subprocess granularity as the positive test
    // above: a spawn whose `dir` is not a unit worktree (empty here - the `isolation:
    // none` / repo-less shape `unit_cache_sibling` also returns None for) gets an empty
    // `env` from `RunCtx::spawn_env` in production, so this test drives the real driver
    // with that same empty env and asserts the real subprocess sees NO CARGO_TARGET_DIR -
    // neither a stale value forced in by the driver itself nor one leftover from this test
    // process's own ambient environment.
    std::env::remove_var("CARGO_TARGET_DIR");

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    assert!(
        agent_bin.exists(),
        "the fixture agent {agent_bin:?} must exist"
    );

    let driver = cli::Driver {
        bin: agent_bin.to_string_lossy().into_owned(),
    };
    let agent = AgentDef {
        id: "worker".into(),
        ..Default::default()
    };
    let opts = SpawnOpts {
        dir: String::new(),
        isolation: false,
        env: Vec::new(),
        ..Default::default()
    };
    let result = driver
        .spawn(&agent, "no-op", &opts, &no_emit)
        .expect("the real agent subprocess must run");

    assert!(
        result.output.lines().any(|l| l == "CARGO_TARGET_DIR="),
        "a dir with no per-unit cache must leave CARGO_TARGET_DIR unset in the real \
         agent subprocess, never a forced/stale override: {:?}",
        result.output
    );
}
