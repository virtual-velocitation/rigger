//! Periphery for spec 125 criterion 4, RIGGER EXPORTS ONLY WHAT THE WORKFLOW DECLARES: a gate run
//! through the built binary carries exactly the variables the workflow's `build.env` declares,
//! each placeholder resolved at the gate's site, and nothing else of rigger's build environment;
//! one library `conductor::run` hands its real gate and its real agent the declaration resolved
//! at each one's own site; a declaration the one check refuses stops the binary before any gate
//! runs, and stops the library's `conductor::run`, which takes a hand-built `Config` that never
//! passed `Config::validate`, before any agent spawns.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO. `gate::BuildEnv`'s own tests resolve a
//! declaration in memory and apply it to a `Command` they inspect; the conductor's own tests
//! record the pairs handed to a test runner and a test driver. None of them observes what a real
//! gate process launched by the real binary receives, or what a real agent process the real cli
//! driver spawns receives, so a variable rigger still set or cleared on its own, or a placeholder
//! resolved over the wrong site, would pass every one of them. Here the gate is `env` itself, run
//! by `rigger gate g` in a committed project, writing what it received to a file outside the
//! project (a gate that writes inside the tree is refused as a changed worktree); and through
//! `conductor::run`, a real gate and a real agent each write theirs the same way.
//!
//! NOT OWNED HERE: what a spawn at a unit or review sibling carries (the conductor's `spawn_env`
//! and spawn-site tests), every refused entry's text (the gate adapter's `BuildEnv::declared`
//! table, the config adapter's `Config::validate` table and `rigger validate`'s table in
//! `tests/cli.rs`), and the `rigger validate` report.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;

use common::cli::{committed_project, rigger_command};
use common::fixtures::{
    build_block_declaring, build_declaring, DECLARED_BJL, REFUSED_DECLARATIONS,
};

#[path = "common/real_driver_spy.rs"]
mod real_driver_spy;
#[path = "common/real_gate_run.rs"]
mod real_gate_run;
use real_gate_run::{refused_run, run_real_gate_and_agent, write_agent_running};
use rigger::eventstore::sqlite::Store;

/// The variable a build tool reads its target directory from, which rigger sets only when the
/// workflow declares it.
const TARGET_DIR: &str = "CARGO_TARGET_DIR";

/// The variables a gate row reads back from the gate's environment: [`TARGET_DIR`] and the
/// criterion's declared names.
const WATCHED: [&str; 4] = [TARGET_DIR, "B", "J", "L"];

/// The shell command that writes its whole environment to `seen`: what a gate, or an agent, runs
/// to show what it received.
fn env_to(seen: &Path) -> String {
    format!("env > '{}'", seen.display())
}

/// The workflow declaring `pairs` under `build.env` with `build.jobs` at `jobs`, and the one gate
/// `g`, which writes its whole environment to `seen`.
fn workflow_writing_env_to(pairs: &[(&str, &str)], jobs: u32, seen: &Path) -> String {
    format!(
        "{}gates:\n  g:\n    run: \"{}\"\n    kind: core\n",
        build_block_declaring(pairs, jobs),
        env_to(seen)
    )
}

/// `rigger gate g` in `root`, its scratch and runtime directory under `scratch`, with no
/// [`WATCHED`] variable in its environment but [`TARGET_DIR`] set to `ambient` when `Some`:
/// (stdout, stderr, success).
fn gate_g(root: &Path, scratch: &Path, ambient: Option<&str>) -> (String, String, bool) {
    let state = tempfile::tempdir().expect("a state dir for the rigger run");
    let scratch = scratch.to_str().unwrap();
    let mut cmd = rigger_command(
        root,
        &["gate", "g"],
        &[("RIGGER_TMPDIR", scratch), ("XDG_RUNTIME_DIR", scratch)],
        state.path(),
    );
    for name in WATCHED {
        cmd.env_remove(name);
    }
    if let Some(dir) = ambient {
        cmd.env(TARGET_DIR, dir);
    }
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .expect("the rigger binary runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// The [`WATCHED`] variables in the `env` listing `listing`, by name.
fn watched_in(listing: &str) -> BTreeMap<String, String> {
    listing
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| WATCHED.contains(name))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// The [`WATCHED`] variables a site should see, `sees` by name with `{cache}` in a value standing
/// for the gate's build cache `cache`.
fn expected(sees: &[(&str, &str)], cache: &Path) -> BTreeMap<String, String> {
    sees.iter()
        .map(|(name, value)| {
            let value = value.replace("{cache}", cache.to_str().unwrap());
            (name.to_string(), value)
        })
        .collect()
}

/// A GATE SEES ONLY WHAT IS DECLARED: GIVEN a committed project whose workflow declares a build
/// environment (or none), WHEN `rigger gate g` runs its gate, THEN the gate sees each declared
/// variable resolved at its site - `B` over the gate's build cache, the scratch's shared
/// `cargo-target`, `J` the jobs cap only when `build.jobs` is above 0, `L` its literal - and
/// rigger neither sets nor clears an undeclared `CARGO_TARGET_DIR`.
#[test]
fn a_gate_run_through_the_binary_sees_exactly_the_declared_build_environment() {
    struct Case {
        why: &'static str,
        declared: &'static [(&'static str, &'static str)],
        jobs: u32,
        ambient: Option<&'static str>,
        /// The watched pairs the gate sees, `{cache}` standing for the gate's build cache.
        sees: &'static [(&'static str, &'static str)],
    }
    let cases = [
        Case {
            why: "nothing declared, no CARGO_TARGET_DIR in rigger's environment",
            declared: &[],
            jobs: 3,
            ambient: None,
            sees: &[],
        },
        Case {
            why: "nothing declared, an operator's CARGO_TARGET_DIR",
            declared: &[],
            jobs: 3,
            ambient: Some("/operator/target"),
            sees: &[(TARGET_DIR, "/operator/target")],
        },
        Case {
            why: "B, J and L with jobs 3",
            declared: &DECLARED_BJL,
            jobs: 3,
            ambient: None,
            sees: &[("B", "{cache}/x"), ("J", "3"), ("L", "lit")],
        },
        Case {
            why: "B, J and L with jobs 0",
            declared: &DECLARED_BJL,
            jobs: 0,
            ambient: None,
            sees: &[("B", "{cache}/x"), ("L", "lit")],
        },
    ];
    for case in cases {
        let why = case.why;
        let seen_dir = tempfile::tempdir().expect("a dir for the gate's environment");
        let seen = seen_dir.path().join("env");
        let (root, scratch) =
            committed_project(&workflow_writing_env_to(case.declared, case.jobs, &seen));
        let (out, err, ok) = gate_g(root.path(), scratch.path(), case.ambient);
        assert!(
            ok,
            "{why}: the gate is green; stdout:\n{out}\nstderr:\n{err}"
        );
        let cache = scratch.path().join("cargo-target");
        let listing = std::fs::read_to_string(&seen).expect("the gate wrote its environment");
        assert_eq!(watched_in(&listing), expected(case.sees, &cache), "{why}");
    }
}

/// ONE RUN, EACH SITE ITS OWN RESOLUTION: GIVEN a workflow declaring a build environment (or
/// none), WHEN the library's `conductor::run` drives its one repo-less unit through a real gate
/// and a real agent, THEN the gate sees each declared variable resolved over the shared gate
/// cache (`B` over the scratch root's `cargo-target`), the agent - spawned at a dir with no unit
/// sibling - sees `J` and `L` and no `B`, `J` only while `build.jobs` is above 0, and neither
/// site sees a watched variable the workflow does not declare.
#[test]
fn one_run_hands_its_real_gate_and_its_real_agent_the_declaration_resolved_at_each_site() {
    struct Case {
        why: &'static str,
        declared: &'static [(&'static str, &'static str)],
        jobs: u32,
        /// The watched pairs the gate sees, `{cache}` standing for the gate's build cache.
        gate_sees: &'static [(&'static str, &'static str)],
        /// The watched pairs the agent sees.
        agent_sees: &'static [(&'static str, &'static str)],
    }
    let cases = [
        Case {
            why: "nothing declared",
            declared: &[],
            jobs: 3,
            gate_sees: &[],
            agent_sees: &[],
        },
        Case {
            why: "B, J and L with jobs 3",
            declared: &DECLARED_BJL,
            jobs: 3,
            gate_sees: &[("B", "{cache}/x"), ("J", "3"), ("L", "lit")],
            agent_sees: &[("J", "3"), ("L", "lit")],
        },
        Case {
            why: "B, J and L with jobs 0",
            declared: &DECLARED_BJL,
            jobs: 0,
            gate_sees: &[("B", "{cache}/x"), ("L", "lit")],
            agent_sees: &[("L", "lit")],
        },
    ];
    // Both subprocesses inherit this process's environment, so every watched name leaves it, and
    // the run's scratch root is a tempdir: the gate's shared cache is then a known path, and its
    // guard lock stays out of the checkout. Held under the lock `tests/common/mod.rs` keeps for
    // every ambient-environment write, and each name written is restored to its prior value when
    // the test ends, a failing row's included.
    let _guard = common::env_test_lock();
    let written: Vec<&'static str> = WATCHED
        .into_iter()
        .chain([rigger::worktree::SCRATCH_ROOT_ENV])
        .collect();
    let _restore = common::RestoreEnvVars::capture(&written);
    let scratch = tempfile::tempdir().expect("a scratch root for the run");
    for name in WATCHED {
        std::env::remove_var(name);
    }
    std::env::set_var(rigger::worktree::SCRATCH_ROOT_ENV, scratch.path());
    let cache = scratch
        .path()
        .join(rigger::worktree::SHARED_BUILD_CACHE_NAME);
    for case in cases {
        let why = case.why;
        let seen = tempfile::tempdir().expect("a dir for what each site saw");
        let gate_seen = seen.path().join("gate-env");
        let agent_seen = seen.path().join("agent-env");
        let agent_bin = write_agent_running(seen.path(), &env_to(&agent_seen));
        let store = Store::open(":memory:").unwrap();
        let (evidence, outputs) = run_real_gate_and_agent(
            &store,
            build_declaring(case.declared, case.jobs),
            "core",
            &env_to(&gate_seen),
            &agent_bin,
        );
        assert_eq!(evidence, "PASS", "{why}: the gate is green");
        assert_eq!(
            outputs.len(),
            1,
            "{why}: the one unit spawns its agent once"
        );
        let gate = std::fs::read_to_string(&gate_seen).expect("the gate wrote its environment");
        assert_eq!(
            watched_in(&gate),
            expected(case.gate_sees, &cache),
            "{why}: the gate"
        );
        let agent = std::fs::read_to_string(&agent_seen).expect("the agent wrote its environment");
        assert_eq!(
            watched_in(&agent),
            expected(case.agent_sees, &cache),
            "{why}: the agent"
        );
    }
}

/// A REFUSED DECLARATION RUNS NO GATE: GIVEN a committed project whose workflow declares a name
/// rigger sets itself, WHEN `rigger gate g` runs, THEN it fails naming the variable, prints no
/// gate result, and its gate never runs.
#[test]
fn a_refused_declaration_stops_rigger_gate_before_its_gate_runs() {
    let refused = REFUSED_DECLARATIONS
        .iter()
        .find(|row| row.name == "RIGGER_GATE_SCRATCH")
        .expect("the gate scratch variable is a refused declaration");
    let seen_dir = tempfile::tempdir().expect("a dir for the gate's environment");
    let seen = seen_dir.path().join("env");
    let (root, scratch) = committed_project(&workflow_writing_env_to(
        &[(refused.name, refused.value)],
        3,
        &seen,
    ));
    let (out, err, ok) = gate_g(root.path(), scratch.path(), None);
    assert!(!ok, "a refused declaration fails the verb");
    assert_eq!(out, "", "no gate result is printed");
    let line = format!("rigger: config: {}", refused.message);
    assert_eq!(
        err.lines().filter(|l| *l == line).count(),
        1,
        "the verb names the refusal once: {err}"
    );
    assert!(!seen.exists(), "the gate never ran");
}

/// THE LIBRARY ENTRY POINT REFUSES TOO: GIVEN a hand-built `Config` declaring a name rigger sets
/// itself, which never passed `Config::validate`, WHEN it is handed straight to
/// `conductor::run`, THEN the run fails with the declaration's refusal before any agent spawns.
#[test]
fn conductor_run_refuses_a_hand_built_refused_declaration_before_any_spawn() {
    let refused = REFUSED_DECLARATIONS
        .iter()
        .find(|row| row.name == "RIGGER_SPAWN")
        .expect("the spawn id variable is a refused declaration");
    let err = refused_run(
        build_declaring(&[(refused.name, refused.value)], 3),
        "a declaration the check refuses must fail the run, even when the Config reached `run` \
         directly rather than through `config::load`",
    );
    assert_eq!(err, format!("conductor: {}", refused.message));
}
