//! Periphery (contract / API / integration) tests for spec 65 criteria 1 and 4: the ONE
//! build-environment authority - `gate::BuildEnv::resolve` and its two injection sites
//! (`gate::ExecRunner::run`, `driver::cli::Driver::spawn`) - and criterion 4's JOBS CAP
//! facet (`build.jobs` -> `CARGO_BUILD_JOBS`), which rides the SAME resolver and the
//! SAME two injection sites rather than a competing seam of its own.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/gate.rs`'s own unit tests call `BuildEnv::resolve`/`apply` directly against a
//! `std::process::Command` they inspect in-process - they never observe a real OS
//! environment. `src/driver/cli.rs`'s own test spawns a real subprocess through
//! `Driver::spawn` directly, but only that one site, in isolation. `src/conductor.rs`'s
//! own integration test (`one_build_environment_authority_reaches_both_a_gate_build_and_
//! an_agent_spawn`) proves the SINGLE-RESOLVER wiring end to end, but through
//! `RecordingRunner`/`EnvRecordingDriver` test doubles that only record the Rust-level
//! `(name, value)` pairs handed to them - they can never observe what a REAL OS process
//! actually received, so a bug in the ORDER of `cmd.env(...)` calls, a var silently
//! dropped by `Command::output()`, or a real subprocess seeing a stale/ambient value
//! instead of the injected one would pass every one of those tests unnoticed.
//!
//! `config.rs`'s own unit test parses `BuildConfig` via `serde_yaml::from_str` on a
//! literal string - it never goes through the real `.rigger/workflow.yml`-on-disk
//! loading path (`config::load`), which also reads the agents directory and calls
//! `Config::validate`.
//!
//! This file closes those gaps, over the crate's PUBLIC surface only, using REAL
//! production types spawning REAL subprocesses:
//!
//! 1. `build_config_round_trips_through_the_real_on_disk_loader_and_feeds_the_resolver`:
//!    a real `.rigger/workflow.yml` with an explicit `build:` section, loaded through the
//!    real `config::load` entry point, feeds `BuildEnv::resolve` to the exact vars a
//!    configured wrapper must produce; the same loader over a workflow.yml with NO
//!    `build:` section (the back-compat case) feeds it to nothing at all.
//! 2. `build_env_resolve_falls_back_to_the_default_cache_dir_when_unset`: the one branch
//!    of the public `BuildEnv::resolve` no test anywhere else in the tree exercises - a
//!    configured wrapper with an EMPTY `cache_dir` still resolves a non-empty `<WRAPPER>_
//!    DIR`.
//! 3. `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_
//!    subprocess`: drives `conductor::run` with the REAL `gate::ExecRunner` (a real `sh
//!    -c`) and a thin spy that delegates every spawn to the REAL `driver::cli::Driver`
//!    (a real subprocess via a fixture script) - never a fake - and reads the vars each
//!    process ACTUALLY saw back out: the gate's evidence from its recorded `GateVerdict`
//!    event, the agent's own echo from its captured stdout. Proves the SAME resolved
//!    vars reach both real boundaries from one run, and that the default (no wrapper
//!    configured) injects into neither - the exact regression this authority exists to
//!    prevent (a build tool silently caching under different keys at the two sites, or a
//!    silent injection when none was asked for).
//!
//! 4. `build_config_round_trips_an_explicit_jobs_value_through_the_real_on_disk_loader`:
//!    the same real `.rigger/workflow.yml` on-disk gap test 1 closes for
//!    `wrapper`/`cache_dir`, closed for `jobs` - a real workflow.yml with an explicit
//!    `build.jobs`, loaded through the real `config::load` entry point, must feed the
//!    resolver the exact value an operator committed, with no wrapper implied.
//! 5. `jobs_cap_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess_independent_
//!    of_wrapper`: drives a full `conductor::run` exactly like test 3, but with ONLY
//!    `build.jobs` set (no wrapper) - proves `CARGO_BUILD_JOBS` reaches BOTH real
//!    boundaries (the real gate subprocess AND the real agent subprocess) from one run,
//!    and that it does so WITHOUT injecting any wrapper var. Closes the exact gap this
//!    file's own out-of-scope note used to name: the unit-level tests in `gate.rs` prove
//!    `resolve()`'s in-memory output and (via a lone `ExecRunner::run` call) ONE real
//!    subprocess, but never the agent-spawn injection site, and never through the real
//!    `conductor::run` wiring that proves it is the SAME resolved value at both sites.
//! 6. `jobs_cap_coexists_with_a_configured_wrapper_at_both_real_injection_sites`: the
//!    same proof in the other direction - `build.jobs` set ALONGSIDE a configured
//!    wrapper must reach both real subprocesses together, neither suppressing the
//!    other, matching this authority's "independent facet" design.
//!
//! Out of scope, owned by sibling units per spec 65: `auto`/named-but-absent wrapper
//! resolution (unit 2), the build-budget flock (unit 3), and the `validate`/`setup`
//! reporting surfaces (unit 5). The turn-key Agent-SDK path (`shim/shim.mjs`) carries no
//! diff in this unit (a separate, not-yet-wired injection site per the implementer's own
//! record) and so is not exercised here either.
//!
//! Nothing here is feature-gated: `BuildEnv`, `ExecRunner`, `cli::Driver`, and
//! `config::load` are all compiled and exercised in both feature lanes.

use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM};
use rigger::config::{self, AgentDef, BuildConfig, Config, Gate, Stage};
use rigger::contextgraph::TYPE_GATE_VERDICT;
use rigger::driver::cli;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::{BuildEnv, ExecRunner};

/// Write a minimal but real `.rigger/agents/worker.md` + `.rigger/workflow.yml` at
/// `root`, so `config::load` reaches all the way through agent parsing and
/// `Config::validate` - the real on-disk boundary, not a struct literal built in
/// memory. `build_block` is appended to the workflow verbatim: `""` omits the `build:`
/// section entirely (the back-compat case - defaults must apply), a `build:\n  ...\n`
/// block pins an explicit wrapper/cache_dir.
fn write_workflow(root: &Path, build_block: &str) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).expect("create .rigger/agents");
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .expect("write worker.md");
    let workflow = format!(
        "name: buildenvtest\n\
         defaults:\n  grounder: nop\n  budget: 60\n\
         stages:\n  a:\n    agent: worker\n    on_pass: none\n\
         {build_block}"
    );
    std::fs::write(rigger.join("workflow.yml"), workflow).expect("write workflow.yml");
}

/// The public `BuildEnv::vars()` result as a plain map, so an assertion states "these
/// exact pairs" without caring about the resolver's internal ordering.
fn as_map(env: &BuildEnv) -> std::collections::BTreeMap<String, String> {
    env.vars().iter().cloned().collect()
}

/// Serializes every test in this file that touches the REAL process environment:
/// `build_env_resolve_falls_back_to_the_default_cache_dir_when_unset` transitively
/// READS `XDG_STATE_HOME`/`HOME` through `BuildEnv::resolve`'s empty-`cache_dir`
/// fallback (`gate::default_cache_dir`), and
/// `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_
/// subprocess` WRITES via `std::env::remove_var`. `cargo test` runs both as concurrent
/// threads within this one test binary by default; a concurrent env read racing a
/// concurrent env write is a genuine hazard at the POSIX `setenv`/`getenv` level
/// regardless of which keys either side touches (the same hazard `registry::
/// state_home_from`'s own doc comment names as the reason it takes explicit values
/// instead of reading ambient env itself). Held for the duration of each test that
/// touches env, so the two can never interleave.
static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn build_config_round_trips_through_the_real_on_disk_loader_and_feeds_the_resolver() {
    // The configured case: a real `.rigger/workflow.yml` on disk, loaded through the
    // real `config::load` entry point (agent parsing + `Config::validate` included,
    // not just a bare `serde_yaml::from_str`), must hand the resolver the exact
    // wrapper/cache_dir an operator committed.
    let project = tempfile::tempdir().expect("create temp project");
    write_workflow(
        project.path(),
        "build:\n  wrapper: sccache\n  cache_dir: /shared/build-cache\n",
    );
    let cfg = config::load(project.path().to_str().unwrap()).expect("load a valid workflow.yml");
    assert_eq!(cfg.workflow.build.wrapper, "sccache");
    assert_eq!(cfg.workflow.build.cache_dir, "/shared/build-cache");
    let resolved = as_map(&BuildEnv::resolve(
        &cfg.workflow.build.wrapper,
        &cfg.workflow.build.cache_dir,
        cfg.workflow.build.jobs,
    ));
    assert_eq!(
        resolved.get("RUSTC_WRAPPER").map(String::as_str),
        Some("sccache")
    );
    assert_eq!(
        resolved.get("SCCACHE_DIR").map(String::as_str),
        Some("/shared/build-cache")
    );
    assert_eq!(
        resolved.get("CARGO_INCREMENTAL").map(String::as_str),
        Some("0")
    );

    // The back-compat case: an OMITTED `build:` section - every workflow.yml committed
    // before this unit existed - must still load cleanly through the real loader and
    // resolve to nothing at all, so today's ambient-environment behavior is genuinely
    // unchanged for every pre-existing project.
    let legacy = tempfile::tempdir().expect("create temp project");
    write_workflow(legacy.path(), "");
    let cfg = config::load(legacy.path().to_str().unwrap())
        .expect("a workflow.yml with no build: section must still load");
    assert_eq!(cfg.workflow.build.wrapper, "");
    assert_eq!(cfg.workflow.build.cache_dir, "");
    assert_eq!(
        BuildEnv::resolve(
            &cfg.workflow.build.wrapper,
            &cfg.workflow.build.cache_dir,
            cfg.workflow.build.jobs,
        ),
        BuildEnv::default(),
        "an omitted build: section must resolve to the empty BuildEnv - no injection"
    );
}

#[test]
fn build_config_round_trips_an_explicit_jobs_value_through_the_real_on_disk_loader() {
    // spec 65 unit 4 (JOBS CAP): `build.jobs` is its own field of the SAME `build:`
    // block, and closes the SAME real-on-disk-loader gap the test above closes for
    // `wrapper`/`cache_dir` - `config.rs`'s own unit test parses `jobs` via a bare
    // `serde_yaml::from_str` literal, never through the real `config::load` entry point
    // (agent parsing + `Config::validate` included).
    let project = tempfile::tempdir().expect("create temp project");
    write_workflow(project.path(), "build:\n  jobs: 6\n");
    let cfg = config::load(project.path().to_str().unwrap())
        .expect("load a valid workflow.yml with an explicit build.jobs value");
    assert_eq!(cfg.workflow.build.jobs, 6);
    assert_eq!(
        cfg.workflow.build.wrapper, "",
        "an explicit jobs value with no wrapper key must not imply one"
    );
    let resolved = as_map(&BuildEnv::resolve(
        &cfg.workflow.build.wrapper,
        &cfg.workflow.build.cache_dir,
        cfg.workflow.build.jobs,
    ));
    assert_eq!(
        resolved.get("CARGO_BUILD_JOBS").map(String::as_str),
        Some("6"),
        "the real on-disk jobs value must reach the resolver: {resolved:?}"
    );
    assert!(
        !resolved.contains_key("RUSTC_WRAPPER"),
        "jobs alone must inject no wrapper vars: {resolved:?}"
    );
}

#[test]
fn build_env_resolve_falls_back_to_the_default_cache_dir_when_unset() {
    // The one branch of the public resolve() no other test in the tree exercises: a
    // configured wrapper with an EMPTY cache_dir must still resolve a non-empty
    // `<WRAPPER>_DIR` (the documented `<state home>/rigger/build-cache` default, or the
    // documented bare-relative-name fallback in a truly homeless environment) rather
    // than pointing the wrapper at nothing. This branch reads `XDG_STATE_HOME`/`HOME`
    // (real ambient env), so it holds `ENV_TEST_LOCK` for the same reason the
    // `remove_var` test below does - see that lock's doc comment.
    let _guard = env_test_lock();
    let resolved = as_map(&BuildEnv::resolve("sccache", "", 0));
    let dir = resolved
        .get("SCCACHE_DIR")
        .expect("a configured wrapper must always resolve SOME cache dir");
    assert!(
        !dir.is_empty(),
        "an unset cache_dir must not resolve to an empty cache dir"
    );
    assert!(
        dir.ends_with("rigger/build-cache") || dir == "rigger-build-cache",
        "an unset cache_dir must resolve to the documented default \
         (<state home>/rigger/build-cache, or the bare fallback rigger-build-cache): got {dir:?}"
    );
    assert_eq!(
        resolved.get("RUSTC_WRAPPER").map(String::as_str),
        Some("sccache")
    );
    assert_eq!(
        resolved.get("CARGO_INCREMENTAL").map(String::as_str),
        Some("0")
    );
}

/// A driver that delegates EVERY spawn to the REAL `driver::cli::Driver` (spec 65's
/// second injection site, unmodified production code) and records only the raw stdout
/// it produced. This is an OBSERVATION point, not a substitute implementation: every
/// subprocess this test drives is the actual `Command` the shipped driver builds,
/// spawned for real, with `SpawnOpts.env` applied by the real `Driver::spawn` - nothing
/// about the spawn itself is faked.
struct RealDriverSpy {
    inner: cli::Driver,
    outputs: Mutex<Vec<String>>,
}

impl RealDriverSpy {
    fn new(bin: &Path) -> Self {
        RealDriverSpy {
            inner: cli::Driver {
                bin: bin.to_string_lossy().into_owned(),
            },
            outputs: Mutex::new(Vec::new()),
        }
    }

    fn outputs(&self) -> Vec<String> {
        self.outputs.lock().unwrap().clone()
    }
}

impl AgentDriver for RealDriverSpy {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let result = self.inner.spawn(agent, prompt, opts, emit)?;
        self.outputs.lock().unwrap().push(result.output.clone());
        Ok(result)
    }
}

const UNIT: &str = "a";
const GATE: &str = "envgate";

/// A small, controlled gate command: four `echo` lines, well under the evidence
/// compactor's `MAX_LINES` (5) cap, so every line survives verbatim into the recorded
/// `GateVerdict` - unlike a raw `env` dump, whose relevant line has no guaranteed
/// position and is not guaranteed to survive the compactor's "last few lines" fallback.
/// `CARGO_BUILD_JOBS` (unit 4, JOBS CAP) rides alongside the wrapper vars as its own
/// independent facet of the same resolved `BuildEnv`.
const GATE_CMD: &str = "echo RUSTC_WRAPPER=$RUSTC_WRAPPER; echo SCCACHE_DIR=$SCCACHE_DIR; \
     echo CARGO_INCREMENTAL=$CARGO_INCREMENTAL; echo CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS";

/// Drive one full `conductor::run` with `build` configured, a REAL `ExecRunner` for the
/// stage's one gate, and a `RealDriverSpy` wrapping the REAL `cli::Driver` (spawning
/// `agent_bin`) for the stage's one agent. Returns the gate's recorded evidence and
/// every real agent-subprocess stdout the run produced.
fn run_once(build: BuildConfig, agent_bin: &Path) -> (String, Vec<String>) {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        GATE.into(),
        Gate {
            run: GATE_CMD.into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.build = build;
    cfg.workflow.stages.insert(
        UNIT.into(),
        Stage {
            name: UNIT.into(),
            agent: "worker".into(),
            gates: vec![GATE.into()],
            on_pass: "none".into(),
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = RealDriverSpy::new(agent_bin);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    run(&cfg, &deps).expect("the run must complete: a real agent and a real gate");

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let gate_evidence = events
        .iter()
        .find(|e| e.type_ == TYPE_GATE_VERDICT)
        .map(|e| {
            let v: Value = serde_json::from_slice(&e.data).unwrap();
            v["evidence"].as_str().unwrap().to_string()
        })
        .expect("the real ExecRunner gate must have run and recorded a GateVerdict");

    (gate_evidence, driver.outputs())
}

/// Every var line the resolved `BuildEnv` for a configured `sccache` wrapper must
/// produce, exactly as a real `sh -c` and a real subprocess would echo it back.
const CONFIGURED_LINES: [&str; 3] = [
    "RUSTC_WRAPPER=sccache",
    "SCCACHE_DIR=/shared/build-cache",
    "CARGO_INCREMENTAL=0",
];

/// The same three var NAMES, unset - what a real `sh -c`/subprocess must echo when
/// nothing injected them (the default/off case).
const UNSET_LINES: [&str; 3] = ["RUSTC_WRAPPER=", "SCCACHE_DIR=", "CARGO_INCREMENTAL="];

/// `CARGO_BUILD_JOBS`, unset - what a real `sh -c`/subprocess must echo when `build.jobs`
/// is 0 (the config default). Spec 65 unit 4, JOBS CAP.
const JOBS_UNSET_LINE: &str = "CARGO_BUILD_JOBS=";

fn assert_lines_present(haystack: &str, wanted: &[&str], why: &str) {
    for line in wanted {
        assert!(
            haystack.lines().any(|l| l == *line),
            "{why}: expected the line {line:?} verbatim; got:\n{haystack}"
        );
    }
}

#[test]
fn one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess() {
    // Neutralize ambient pollution: this repo's own dev/CI environment plausibly sets
    // RUSTC_WRAPPER/CARGO_INCREMENTAL for its OWN build (this is a build-tooling repo),
    // which would let the "off" assertions below pass for the wrong reason - an
    // inherited ambient value, not the authority correctly injecting nothing. Removed
    // from THIS process only, before either real subprocess is spawned, so every child
    // this test spawns starts from a deterministic baseline regardless of the
    // operator's own shell. Guarded by ENV_TEST_LOCK (see its doc comment) so this
    // mutation never races the OTHER test in this file that reads ambient env.
    let _guard = env_test_lock();
    std::env::remove_var("RUSTC_WRAPPER");
    std::env::remove_var("SCCACHE_DIR");
    std::env::remove_var("CARGO_INCREMENTAL");

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    assert!(
        agent_bin.exists(),
        "the fixture agent {agent_bin:?} must exist"
    );

    // Configured: the SAME resolved vars must reach BOTH a real gate subprocess and a
    // real agent subprocess spawned in the SAME run.
    let configured = BuildConfig {
        wrapper: "sccache".into(),
        cache_dir: "/shared/build-cache".into(),
        jobs: 0,
        ..Default::default()
    };
    let (gate_evidence, agent_outputs) = run_once(configured, &agent_bin);
    assert_lines_present(
        &gate_evidence,
        &CONFIGURED_LINES,
        "a configured wrapper must reach the real gate subprocess",
    );
    assert!(!agent_outputs.is_empty(), "the agent must have spawned");
    for out in &agent_outputs {
        assert_lines_present(
            out,
            &CONFIGURED_LINES,
            "the SAME configured wrapper must reach the real agent subprocess",
        );
    }

    // Default (no wrapper configured): the authority must inject into NEITHER real
    // subprocess - the exact "today's ambient-environment behavior, unchanged" contract
    // this unit's own doc comments promise.
    let (off_gate_evidence, off_agent_outputs) = run_once(BuildConfig::default(), &agent_bin);
    assert_lines_present(
        &off_gate_evidence,
        &UNSET_LINES,
        "no wrapper configured must inject nothing into the real gate subprocess",
    );
    assert!(!off_agent_outputs.is_empty(), "the agent must have spawned");
    for out in &off_agent_outputs {
        assert_lines_present(
            out,
            &UNSET_LINES,
            "no wrapper configured must inject nothing into the real agent subprocess",
        );
    }
}

#[test]
fn jobs_cap_reaches_a_real_gate_subprocess_and_a_real_agent_subprocess_independent_of_wrapper() {
    // spec 65 unit 4 (JOBS CAP): `build.jobs` must reach BOTH real injection sites this
    // authority owns - a real gate subprocess AND a real agent subprocess spawned in the
    // SAME run - exactly like the test above proves for the wrapper vars. The unit-level
    // tests in `gate.rs` prove `resolve()`'s in-memory output and, via ONE direct
    // `ExecRunner::run` call, ONE real subprocess; neither proves the agent-spawn
    // injection site, and neither goes through the real `conductor::run` wiring that
    // proves the SAME resolved value reaches both boundaries together - the gap this
    // file's own scope note used to name against unit 4.
    let _guard = env_test_lock();
    std::env::remove_var("RUSTC_WRAPPER");
    std::env::remove_var("SCCACHE_DIR");
    std::env::remove_var("CARGO_INCREMENTAL");
    std::env::remove_var("CARGO_BUILD_JOBS");

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    assert!(
        agent_bin.exists(),
        "the fixture agent {agent_bin:?} must exist"
    );

    // Configured: jobs alone (no wrapper) must reach BOTH real subprocesses, and must
    // inject NO wrapper var - the "independent facet" half of the design.
    let configured = BuildConfig {
        wrapper: "".into(),
        cache_dir: "".into(),
        jobs: 6,
        ..Default::default()
    };
    let (gate_evidence, agent_outputs) = run_once(configured, &agent_bin);
    assert_lines_present(
        &gate_evidence,
        &["CARGO_BUILD_JOBS=6"],
        "a configured jobs cap must reach the real gate subprocess",
    );
    assert_lines_present(
        &gate_evidence,
        &UNSET_LINES,
        "jobs alone must inject no wrapper var into the real gate subprocess",
    );
    assert!(!agent_outputs.is_empty(), "the agent must have spawned");
    for out in &agent_outputs {
        assert_lines_present(
            out,
            &["CARGO_BUILD_JOBS=6"],
            "the SAME configured jobs cap must reach the real agent subprocess",
        );
        assert_lines_present(
            out,
            &UNSET_LINES,
            "jobs alone must inject no wrapper var into the real agent subprocess",
        );
    }

    // Unset (the config default, 0): the "unset leaves the ambient default untouched"
    // half of the criterion - CARGO_BUILD_JOBS must reach NEITHER real subprocess.
    let (off_gate_evidence, off_agent_outputs) = run_once(BuildConfig::default(), &agent_bin);
    assert_lines_present(
        &off_gate_evidence,
        &[JOBS_UNSET_LINE],
        "an unset jobs cap must inject nothing into the real gate subprocess",
    );
    assert!(!off_agent_outputs.is_empty(), "the agent must have spawned");
    for out in &off_agent_outputs {
        assert_lines_present(
            out,
            &[JOBS_UNSET_LINE],
            "an unset jobs cap must inject nothing into the real agent subprocess",
        );
    }
}

#[test]
fn jobs_cap_coexists_with_a_configured_wrapper_at_both_real_injection_sites() {
    // The other half of the "independent facet" design: a configured wrapper must NOT
    // suppress a configured jobs cap, at the SAME two real boundaries, in the SAME run -
    // proving the real-subprocess-level analog of the in-memory unit test
    // `build_env_jobs_cap_is_independent_of_the_wrapper` in `gate.rs`.
    let _guard = env_test_lock();
    std::env::remove_var("RUSTC_WRAPPER");
    std::env::remove_var("SCCACHE_DIR");
    std::env::remove_var("CARGO_INCREMENTAL");
    std::env::remove_var("CARGO_BUILD_JOBS");

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    assert!(
        agent_bin.exists(),
        "the fixture agent {agent_bin:?} must exist"
    );

    let configured = BuildConfig {
        wrapper: "sccache".into(),
        cache_dir: "/shared/build-cache".into(),
        jobs: 8,
        ..Default::default()
    };
    let (gate_evidence, agent_outputs) = run_once(configured, &agent_bin);
    assert_lines_present(
        &gate_evidence,
        &CONFIGURED_LINES,
        "a configured wrapper must still reach the real gate subprocess alongside jobs",
    );
    assert_lines_present(
        &gate_evidence,
        &["CARGO_BUILD_JOBS=8"],
        "a configured jobs cap must reach the real gate subprocess alongside the wrapper",
    );
    assert!(!agent_outputs.is_empty(), "the agent must have spawned");
    for out in &agent_outputs {
        assert_lines_present(
            out,
            &CONFIGURED_LINES,
            "a configured wrapper must still reach the real agent subprocess alongside jobs",
        );
        assert_lines_present(
            out,
            &["CARGO_BUILD_JOBS=8"],
            "a configured jobs cap must reach the real agent subprocess alongside the wrapper",
        );
    }
}
