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
//! 7. `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses`
//!    (spec 65 unit 2, NO SILENT DEGRADE, added on top of the above): with
//!    `build.wrapper: auto` and a synthetic `PATH` carrying a staged fake `sccache`
//!    binary, `conductor::RunCtx::build_env` must pre-resolve `auto` to that real probed
//!    name BEFORE it ever reaches `BuildEnv::resolve` (which takes its `wrapper` argument
//!    verbatim and has no notion of `auto`) - proved by reading the SAME resolved vars
//!    back out of a real gate subprocess and a real agent subprocess, exactly like test 3
//!    proves for an explicitly-named wrapper. A literal `RUSTC_WRAPPER=auto` leaking into
//!    either subprocess (the regression this test exists to catch) would break every real
//!    build the loop runs, silently, since `auto` is not an executable - the opposite of
//!    "no silent degrade".
//!
//! 8. `auto_wrapper_finding_nothing_injects_into_neither_real_subprocess` (SDET addition,
//!    spec 65 unit 2, NO SILENT DEGRADE): test 7 above proves `auto` FINDING a wrapper
//!    reaches both real subprocesses; this proves the other half of the same contract -
//!    `auto` finding NOTHING must inject into NEITHER real subprocess, at the identical
//!    real-subprocess granularity. Nothing else in the tree proves this at this level:
//!    `resolve_wrapper_name_auto_with_nothing_on_path_resolves_to_none` (gate.rs) proves
//!    only the pure resolver's return value against a synthetic PATH, in-process, and
//!    `validate_reports_none_when_auto_finds_no_known_wrapper_on_path` (tests/cli.rs)
//!    proves only the PRINTED report line - neither observes what a real gate/agent
//!    subprocess actually receives. A regression that leaked the literal string `auto`
//!    (or any stale value) into a real subprocess on this branch specifically would pass
//!    every one of those without detection; this closes that gap.
//!
//! 9. `run_propagates_a_named_but_absent_wrappers_error_at_the_library_entry_point`
//!    (spec 65 unit 2, NO SILENT DEGRADE - closing a genuine-defect finding): every test
//!    above (and `config.rs`'s own unit tests, and `tests/cli.rs`) reaches
//!    `conductor::run` through a `Config` that went through `config::load`, whose
//!    `Config::validate` call rejects a named-but-absent wrapper before a `Config` even
//!    exists. But `conductor::run` is `pub` and `docs/architecture.md` documents "library
//!    use (embed the harness) imports the same modules from the `rigger` crate directly" as
//!    a real, first-class usage mode - a caller that builds/mutates a `Config` BY HAND (as
//!    `run_once` above already does) and passes it straight to `run` never goes through
//!    `config::load`/`Config::validate` at all. This proves `run` itself refuses to
//!    silently degrade at THAT entry point too: a hand-built `Config` naming an absent
//!    wrapper, passed directly to `run`, must return `Err` naming the binary - never a
//!    silent `Ok` that quietly built without the cache the operator asked for.
//!
//! 10. `run_propagates_a_named_wrappers_uncreatable_cache_dir_at_the_library_entry_point`
//!     (SDET addition, spec 65 unit 2, NO SILENT DEGRADE): the CACHE-DIR axis counterpart
//!     to test 9 - `RunCtx::build_env`'s `?` propagates BOTH variants of `gate::
//!     BuildLayerUnavailable` through the identical one line, but test 9 only drove the
//!     `Wrapper` variant. A hand-built `Config` naming a wrapper that IS on PATH, with a
//!     `cache_dir` that cannot be created, passed directly to `run`, must return `Err`
//!     naming the dir and the `build.cache_dir` key at this SAME bypass-`config::load`
//!     entry point.
//!
//! 11. `resolve_wrapper_name_reads_the_real_ambient_path_directly` (SDET addition): proves
//!     `gate::resolve_wrapper_name` - the wrapper-only, ambient-PATH-reading public fn -
//!     directly over the crate's public API boundary, since production code now reaches
//!     the wrapper axis through `gate::resolve_build_layer` (composing `gate::
//!     resolve_wrapper_name_from` directly) rather than through this fn.
//!
//! 12. `run_propagates_a_named_wrappers_preexisting_unwritable_cache_dir_at_the_library_
//!     entry_point` (SDET addition): the WRITABILITY sub-case counterpart to test 10. A
//!     directory that ALREADY EXISTS - the realistic steady state for a persisted, shared
//!     cache dir - makes `create_dir_all` a no-op success regardless of write permission;
//!     test 10's blocked-path-component scenario cannot reach that branch at all. Proves
//!     this failure mode reaches the identical bypass-`config::load` entry point test 10
//!     proves for the creatability failure: a hand-built `Config` naming a wrapper that IS
//!     on PATH, with a `cache_dir` that already EXISTS but cannot be WRITTEN into, passed
//!     directly to `run`, must still return `Err` naming the dir and the `build.cache_dir`
//!     key.
//!
//! Out of scope, owned by sibling units per spec 65: named-but-absent wrapper resolution
//! surfacing as a run-start CONFIG error THROUGH THE CLI (covered instead by `config.rs`'s
//! own unit tests and `tests/cli.rs`'s `validate_fails_at_run_start_when_a_named_build_
//! wrapper_is_absent_from_path` - a config-validation concern, not a real-subprocess one),
//! the build-budget flock (unit 3), and the `validate`/`setup` reporting surfaces (unit 5,
//! though `rigger validate`'s "none"/resolved-name report is already covered by
//! `tests/cli.rs`'s own spec 65 unit 2 tests). The turn-key Agent-SDK path
//! (`shim/shim.mjs`) carries no diff in this unit (a separate, not-yet-wired injection
//! site per the implementer's own record) and so is not exercised here either.
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
use rigger::gate::{resolve_wrapper_name, BuildEnv, ExecRunner};

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
/// fallback (`gate::default_cache_dir`); `one_build_environment_authority_reaches_a_
/// real_gate_subprocess_and_a_real_agent_subprocess` WRITES via `std::env::remove_var`;
/// and, as of spec 65 unit 2 (NO SILENT DEGRADE),
/// `build_config_round_trips_through_the_real_on_disk_loader_and_feeds_the_resolver`
/// BOTH READS AND WRITES PATH too - `config::load` now calls `Config::validate`, which
/// (since unit 2) calls `gate::resolve_build_layer`, an ambient-PATH read that did not
/// exist when this lock's original two holders were written; that test now also stages a
/// fake wrapper binary onto PATH so its hardcoded `wrapper: sccache` config resolves
/// deterministically regardless of what the real machine running the suite has installed
/// (see its own doc comment - this is not merely a race fix, a real machine without
/// `sccache` on PATH would otherwise fail this test unconditionally, lock or no lock).
/// `run_propagates_a_named_but_absent_wrappers_error_at_the_library_entry_point` (unit 2)
/// READS PATH too, transitively through the same `gate::resolve_build_layer` edge, reached
/// this time via `conductor::run` directly rather than `config::load` - same hazard, same
/// lock. `resolve_wrapper_name_reads_the_real_ambient_path_directly` (SDET addition) both
/// READS and WRITES PATH calling the ambient-PATH pub fn itself. `cargo test` runs every
/// test in this one binary as concurrent threads by default; a concurrent env read racing
/// a concurrent env write is a genuine hazard at the POSIX `setenv`/`getenv` level
/// regardless of which keys either side touches (the same hazard `registry::
/// state_home_from`'s own doc comment names as the reason it takes explicit values
/// instead of reading ambient env itself). Held for the duration of each test that
/// touches env, so none of them can ever interleave.
static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn build_config_round_trips_through_the_real_on_disk_loader_and_feeds_the_resolver() {
    // As of spec 65 unit 2 (NO SILENT DEGRADE), `config::load` -> `Config::validate` ->
    // `gate::resolve_build_layer` reads ambient PATH and REJECTS a named wrapper absent
    // from it - so this test's hardcoded `wrapper: sccache` below needs a real `sccache`
    // reachable on PATH regardless of what the machine actually running this suite has
    // installed. Staging a fake one (the same fixture the auto-wrapper tests below use)
    // makes this deterministic instead of an accidental pass tied to this developer's own
    // machine happening to have the real tool - see ENV_TEST_LOCK's own doc comment for
    // why this also needs the lock now.
    let _guard = env_test_lock();
    let _bindir = stage_fake_sccache_on_path();

    // The configured case: a real `.rigger/workflow.yml` on disk, loaded through the
    // real `config::load` entry point (agent parsing + `Config::validate` included,
    // not just a bare `serde_yaml::from_str`), must hand the resolver the exact
    // wrapper/cache_dir an operator committed. `cache_dir` must be a REALLY creatable
    // directory: as of spec 65 unit 2 (NO SILENT DEGRADE), `Config::validate` itself now
    // attempts to create it (never a filesystem-root path a non-root test process could
    // never create).
    let project = tempfile::tempdir().expect("create temp project");
    let cache_dir = project.path().join("shared-build-cache");
    write_workflow(
        project.path(),
        &format!(
            "build:\n  wrapper: sccache\n  cache_dir: {}\n",
            cache_dir.display()
        ),
    );
    let cfg = config::load(project.path().to_str().unwrap()).expect("load a valid workflow.yml");
    assert_eq!(cfg.workflow.build.wrapper, "sccache");
    assert_eq!(cfg.workflow.build.cache_dir, cache_dir.to_string_lossy());
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
        resolved.get("SCCACHE_DIR"),
        Some(&cache_dir.to_string_lossy().into_owned())
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

/// Every var line the resolved `BuildEnv` for a configured `sccache` wrapper writing to
/// `cache_dir` must produce, exactly as a real `sh -c` and a real subprocess would echo it
/// back. `cache_dir` must be a REAL, actually-creatable directory: as of spec 65 unit 2 (NO
/// SILENT DEGRADE), resolution now attempts to CREATE it, never just format its name - a
/// caller passes a real tempdir-backed path, never a filesystem-root literal a non-root
/// test process could never create.
fn configured_lines(cache_dir: &str) -> [String; 3] {
    [
        "RUSTC_WRAPPER=sccache".to_string(),
        format!("SCCACHE_DIR={cache_dir}"),
        "CARGO_INCREMENTAL=0".to_string(),
    ]
}

/// The same three var NAMES, unset - what a real `sh -c`/subprocess must echo when
/// nothing injected them (the default/off case).
const UNSET_LINES: [&str; 3] = ["RUSTC_WRAPPER=", "SCCACHE_DIR=", "CARGO_INCREMENTAL="];

/// `CARGO_BUILD_JOBS`, unset - what a real `sh -c`/subprocess must echo when `build.jobs`
/// is 0 (the config default). Spec 65 unit 4, JOBS CAP.
const JOBS_UNSET_LINE: &str = "CARGO_BUILD_JOBS=";

fn assert_lines_present<S: AsRef<str>>(haystack: &str, wanted: &[S], why: &str) {
    for line in wanted {
        let line = line.as_ref();
        assert!(
            haystack.lines().any(|l| l == line),
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
    // real agent subprocess spawned in the SAME run. A real, actually-creatable cache dir
    // (spec 65 unit 2, NO SILENT DEGRADE: resolution now attempts to CREATE it).
    let cache_dir = tempfile::tempdir().expect("create cache dir");
    let cache_dir_str = cache_dir.path().to_string_lossy().into_owned();
    let configured = BuildConfig {
        wrapper: "sccache".into(),
        cache_dir: cache_dir_str.clone(),
        jobs: 0,
        ..Default::default()
    };
    let lines = configured_lines(&cache_dir_str);
    let (gate_evidence, agent_outputs) = run_once(configured, &agent_bin);
    assert_lines_present(
        &gate_evidence,
        &lines,
        "a configured wrapper must reach the real gate subprocess",
    );
    assert!(!agent_outputs.is_empty(), "the agent must have spawned");
    for out in &agent_outputs {
        assert_lines_present(
            out,
            &lines,
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

    // A real, actually-creatable cache dir (spec 65 unit 2, NO SILENT DEGRADE: resolution
    // now attempts to CREATE it).
    let cache_dir = tempfile::tempdir().expect("create cache dir");
    let cache_dir_str = cache_dir.path().to_string_lossy().into_owned();
    let configured = BuildConfig {
        wrapper: "sccache".into(),
        cache_dir: cache_dir_str.clone(),
        jobs: 8,
        ..Default::default()
    };
    let lines = configured_lines(&cache_dir_str);
    let (gate_evidence, agent_outputs) = run_once(configured, &agent_bin);
    assert_lines_present(
        &gate_evidence,
        &lines,
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
            &lines,
            "a configured wrapper must still reach the real agent subprocess alongside jobs",
        );
        assert_lines_present(
            out,
            &["CARGO_BUILD_JOBS=8"],
            "a configured jobs cap must reach the real agent subprocess alongside the wrapper",
        );
    }
}

/// Stage a fake `sccache` executable in a fresh temp bin dir and prepend it to the REAL
/// process `PATH`, returning that dir (kept alive by the caller so the staged binary is
/// not cleaned up mid-test). PREPENDING (never replacing) is essential: `ExecRunner` spawns
/// `sh -c "..."` and the fixture agent's own shebang-driven body still needs the REST of
/// the real `PATH` (for `sh` and any coreutils it calls) to keep working.
fn stage_fake_sccache_on_path() -> tempfile::TempDir {
    let bindir = tempfile::tempdir().expect("create fake-wrapper bin dir");
    let bin = bindir.path().join("sccache");
    std::fs::write(&bin, "#!/bin/sh\nexit 0\n").expect("write fake sccache");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake sccache");
    }
    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    let new_path = std::env::join_paths(
        std::iter::once(bindir.path().to_path_buf()).chain(std::env::split_paths(&orig_path)),
    )
    .expect("join synthetic PATH");
    std::env::set_var("PATH", new_path);
    bindir
}

#[test]
fn auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses() {
    // Same rationale as the ENV_TEST_LOCK doc comment: this test both READS PATH
    // (transitively, through `gate::resolve_build_layer`'s ambient edge) and WRITES it
    // (staging the fake `sccache`), so it must never interleave with the other tests in
    // this binary that touch ambient env.
    let _guard = env_test_lock();
    std::env::remove_var("RUSTC_WRAPPER");
    std::env::remove_var("SCCACHE_DIR");
    std::env::remove_var("CARGO_INCREMENTAL");
    // Kept alive for the duration of the run below - dropping it would delete the staged
    // binary while `auto`'s PATH probe (inside `conductor::RunCtx::build_env`) still needs
    // to find it.
    let _bindir = stage_fake_sccache_on_path();

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    assert!(
        agent_bin.exists(),
        "the fixture agent {agent_bin:?} must exist"
    );

    // A real, actually-creatable cache dir (spec 65 unit 2, NO SILENT DEGRADE:
    // resolution now attempts to CREATE it, even for an auto-discovered wrapper).
    let cache_dir = tempfile::tempdir().expect("create cache dir");
    let cache_dir_str = cache_dir.path().to_string_lossy().into_owned();
    let auto = BuildConfig {
        wrapper: "auto".into(),
        cache_dir: cache_dir_str.clone(),
        ..Default::default()
    };
    let lines = configured_lines(&cache_dir_str);
    let (gate_evidence, agent_outputs) = run_once(auto, &agent_bin);
    assert_lines_present(
        &gate_evidence,
        &lines,
        "auto must pre-resolve to the real probed sccache binary BEFORE reaching \
         BuildEnv::resolve, so the real gate subprocess sees the SAME vars an explicit \
         `wrapper: sccache` would produce - never the literal string \"auto\"",
    );
    assert!(!agent_outputs.is_empty(), "the agent must have spawned");
    for out in &agent_outputs {
        assert_lines_present(
            out,
            &lines,
            "the SAME auto-probed resolution must reach the real agent subprocess too",
        );
    }
}

/// The real ambient `PATH` with every directory that contains an executable named
/// `sccache` or `ccache` removed - a content-filtered SUBTRACTION, not a directory-name
/// denylist and not a bare allowlist: it checks what each real directory actually holds,
/// so it cannot mistake a same-named-but-different directory for one that must go, and
/// unlike an allowlist (which would have to guess everything `sh` and the fixture agent's
/// shebang might need) it keeps every OTHER real directory exactly as the machine has it.
/// Guarantees `auto`'s probe finds neither known wrapper regardless of what is or is not
/// installed on the machine actually running this test.
fn path_with_neither_known_wrapper() -> std::ffi::OsString {
    let real = std::env::var_os("PATH").unwrap_or_default();
    let kept: Vec<_> = std::env::split_paths(&real)
        .filter(|dir| !dir.join("sccache").exists() && !dir.join("ccache").exists())
        .collect();
    std::env::join_paths(kept).expect("join filtered PATH")
}

/// SDET addition (spec 65 unit 2, NO SILENT DEGRADE): the other half of the contract test
/// `auto_wrapper_resolves_to_a_real_probed_binary_and_reaches_both_real_subprocesses` above
/// proves - `auto` finding NOTHING on PATH must inject into NEITHER a real gate subprocess
/// nor a real agent subprocess, exactly like the "off" case in
/// `one_build_environment_authority_reaches_a_real_gate_subprocess_and_a_real_agent_
/// subprocess` proves for an unconfigured wrapper, but here for the DISCOVERED-IMPLICIT
/// degrade path specifically (the code path unique to `auto`, which that existing "off"
/// case - `wrapper: ""` - never reaches at all).
#[test]
fn auto_wrapper_finding_nothing_injects_into_neither_real_subprocess() {
    // Same rationale as the sibling `auto_wrapper_resolves_...` test above: this test both
    // READS PATH (transitively, through `gate::resolve_build_layer`'s ambient edge) and
    // WRITES it (staging the filtered PATH), so it must never interleave with the other
    // tests in this binary that touch ambient env.
    let _guard = env_test_lock();
    std::env::remove_var("RUSTC_WRAPPER");
    std::env::remove_var("SCCACHE_DIR");
    std::env::remove_var("CARGO_INCREMENTAL");
    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    std::env::set_var("PATH", path_with_neither_known_wrapper());

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    assert!(
        agent_bin.exists(),
        "the fixture agent {agent_bin:?} must exist"
    );

    let auto = BuildConfig {
        wrapper: "auto".into(),
        cache_dir: "/shared/build-cache".into(),
        ..Default::default()
    };
    let (gate_evidence, agent_outputs) = run_once(auto, &agent_bin);
    std::env::set_var("PATH", orig_path);
    assert_lines_present(
        &gate_evidence,
        &UNSET_LINES,
        "auto finding no known wrapper on PATH must inject NOTHING into the real gate \
         subprocess - the discovered-implicit degrade this unit exists to prove, at the same \
         real-subprocess granularity the configured case is proven at above",
    );
    assert!(!agent_outputs.is_empty(), "the agent must have spawned");
    for out in &agent_outputs {
        assert_lines_present(
            out,
            &UNSET_LINES,
            "the SAME auto-finds-nothing degrade must reach the real agent subprocess too - a \
             literal RUSTC_WRAPPER=auto leaking here would be silently invisible until a real \
             build ran against a wrapper binary named `auto` that does not exist",
        );
    }
}

/// Spec 65 unit 2 (NO SILENT DEGRADE) - closing a genuine-defect finding:
/// `conductor::run` is a PUBLIC library entry point (`docs/architecture.md` §11: "library
/// use (embed the harness) imports the same modules from the `rigger` crate directly"),
/// reachable by a caller that builds/mutates a `Config` BY HAND - as `run_once` above
/// already does - and passes it straight to `run`, NEVER through `config::load`, which is
/// the only place a named-but-absent wrapper was previously rejected (via its
/// `Config::validate` call). A hand-built `Config` naming a wrapper that is not on PATH,
/// passed directly to the REAL `run`, must return `Err` naming the missing binary - never a
/// silent `Ok` that quietly proceeded without the cache the config asked for, and never so
/// much as a single agent spawn before that failure surfaces.
#[test]
fn run_propagates_a_named_but_absent_wrappers_error_at_the_library_entry_point() {
    // Reads ambient PATH transitively (conductor::run -> RunCtx::build_env ->
    // gate::resolve_build_layer -> std::env::var_os("PATH")) - guarded by ENV_TEST_LOCK
    // for the same reason every other PATH-touching test in this file is (see the lock's
    // own doc comment): a concurrent env::set_var/remove_var in a sibling test racing this
    // read is a real POSIX getenv/setenv hazard regardless of which keys either side names.
    let _guard = env_test_lock();
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
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    // A NAMED (non-auto, non-off) wrapper virtually certain to be absent from the real
    // ambient PATH - a hand-built `Config` never passed through `config::load`, so the ONLY
    // place this can now be caught is inside `run` itself.
    cfg.workflow.build.wrapper = "definitely-not-a-real-wrapper-rigger-u2-libtest".into();
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

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    let store = Store::open(":memory:").unwrap();
    let driver = RealDriverSpy::new(&agent_bin);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };

    // `RunState` (the `Ok` type) does not implement `Debug`, so `expect_err` cannot be used
    // here - match explicitly instead.
    let err = match run(&cfg, &deps) {
        Err(e) => e,
        Ok(_) => panic!(
            "a named-but-absent build.wrapper must fail the run, not silently degrade, even \
             when the Config reached `run` directly rather than through `config::load`"
        ),
    };
    assert!(
        err.to_string()
            .contains("definitely-not-a-real-wrapper-rigger-u2-libtest"),
        "the error must name the missing binary: {err}"
    );
    assert!(
        driver.outputs().is_empty(),
        "the build-env resolution failure must surface BEFORE any agent spawns, not after \
         wasted work: {:?}",
        driver.outputs()
    );
}

/// The CACHE-DIR axis counterpart to
/// `run_propagates_a_named_but_absent_wrappers_error_at_the_library_entry_point` above.
/// `RunCtx::build_env`'s `Err` propagation (`gate::resolve_build_layer(..).map_err(|e|
/// Error(e.to_string()))?`) is ONE `?` shared by both variants of `gate::BuildLayerUnavailable`,
/// and the sibling test above only exercises the `Wrapper` variant, so this closes the other
/// half of that enum at the SAME library-entry-point granularity: a hand-built `Config`
/// naming a wrapper binary that IS on PATH but whose `cache_dir` cannot be created, passed
/// directly to `run` (never through `config::load`/`Config::validate`), must still return
/// `Err` naming the dir and the `build.cache_dir` key - not silently proceed with a wrapper
/// that never actually caches anything.
#[test]
fn run_propagates_a_named_wrappers_uncreatable_cache_dir_at_the_library_entry_point() {
    // Same rationale as the sibling test above: reads (and here also writes, staging the
    // fake wrapper) ambient PATH transitively through gate::resolve_build_layer.
    let _guard = env_test_lock();
    let _bindir = stage_fake_sccache_on_path();

    let tmp = tempfile::tempdir().expect("tempdir");
    let blocker = tmp.path().join("blocker");
    std::fs::write(&blocker, "not a directory").expect("write blocker file");
    let cache_dir = blocker.join("nested").join("cache");

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
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.build.wrapper = "sccache".into();
    cfg.workflow.build.cache_dir = cache_dir.to_string_lossy().into_owned();
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

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    let store = Store::open(":memory:").unwrap();
    let driver = RealDriverSpy::new(&agent_bin);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };

    let err = match run(&cfg, &deps) {
        Err(e) => e,
        Ok(_) => panic!(
            "a named wrapper's uncreatable cache dir must fail the run, not silently degrade, \
             even when the Config reached `run` directly rather than through `config::load`"
        ),
    };
    let msg = err.to_string();
    assert!(
        msg.contains(&cache_dir.to_string_lossy().into_owned()),
        "the error must name the cache dir: {msg}"
    );
    assert!(
        msg.contains("build.cache_dir"),
        "the error must name the config key: {msg}"
    );
    assert!(
        driver.outputs().is_empty(),
        "the build-env resolution failure must surface BEFORE any agent spawns, not after \
         wasted work: {:?}",
        driver.outputs()
    );
}

/// The WRITABILITY sub-case counterpart to
/// `run_propagates_a_named_wrappers_uncreatable_cache_dir_at_the_library_entry_point` above.
/// `gate::ensure_cache_dir_writable` replaces `usable_with_cache_dir`'s bare `create_dir_all`
/// creatability check with a real write-probe, because a directory that ALREADY EXISTS -
/// the realistic steady state for a persisted, shared cache dir, since `default_cache_dir`
/// is a machine-wide dir every project reuses after the first one creates it - makes
/// `create_dir_all` a no-op success regardless of write permission; the sibling test's
/// blocked-path-component scenario cannot reach that branch at all. This proves the failure
/// mode reaches the identical bypass-`config::load` entry point the sibling test proves for
/// the creatability failure: a hand-built `Config` naming a wrapper that IS on PATH, with a
/// `cache_dir` that already EXISTS but cannot be WRITTEN into, passed directly to `run`
/// (never through `config::load`/`Config::validate`), must still return `Err` naming the dir
/// and the `build.cache_dir` key - not silently proceed with a wrapper that never actually
/// caches anything. Unix-only: the mode bits are a POSIX concept.
#[cfg(unix)]
#[test]
fn run_propagates_a_named_wrappers_preexisting_unwritable_cache_dir_at_the_library_entry_point() {
    // Same rationale as the sibling test above: reads (and here also writes, staging the
    // fake wrapper) ambient PATH transitively through gate::resolve_build_layer.
    let _guard = env_test_lock();
    let _bindir = stage_fake_sccache_on_path();

    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let cache_dir = tmp.path().join("preexisting-cache");
    std::fs::create_dir_all(&cache_dir).expect("pre-create cache dir");
    std::fs::set_permissions(&cache_dir, std::fs::Permissions::from_mode(0o555))
        .expect("chmod cache dir read+execute-only");
    let cache_dir_str = cache_dir.to_string_lossy().into_owned();

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
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.build.wrapper = "sccache".into();
    cfg.workflow.build.cache_dir = cache_dir_str.clone();
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

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/env-echo-agent.sh");
    let store = Store::open(":memory:").unwrap();
    let driver = RealDriverSpy::new(&agent_bin);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };

    let err = match run(&cfg, &deps) {
        Err(e) => e,
        Ok(_) => panic!(
            "a named wrapper's pre-existing-but-unwritable cache dir must fail the run, not \
             silently degrade, even when the Config reached `run` directly rather than \
             through `config::load`"
        ),
    };
    let msg = err.to_string();
    assert!(
        msg.contains(&cache_dir_str),
        "the error must name the cache dir: {msg}"
    );
    assert!(
        msg.contains("build.cache_dir"),
        "the error must name the config key: {msg}"
    );
    assert!(
        driver.outputs().is_empty(),
        "the build-env resolution failure must surface BEFORE any agent spawns, not after \
         wasted work: {:?}",
        driver.outputs()
    );
}

/// `gate::resolve_wrapper_name` (the ambient-PATH edge of the WRAPPER-ONLY axis) has no
/// production caller - `Config::validate`, `RunCtx::build_env`, and `cmd_validate` all go
/// through `gate::resolve_build_layer` (which folds in the cache-dir axis and calls
/// `gate::resolve_wrapper_name_from` directly, never `resolve_wrapper_name` itself). It
/// remains `pub` - part of the crate's committed public surface - so this proves its one
/// distinguishing line - the real `std::env::var_os("PATH")` read - directly, over the
/// crate's public API boundary (this file compiles as a separate test crate with no access
/// to gate.rs's private items).
#[test]
fn resolve_wrapper_name_reads_the_real_ambient_path_directly() {
    // WRITES PATH (staging the fake wrapper, then filtering it back out) and READS it (the
    // function under test) - same ENV_TEST_LOCK discipline as every other PATH-touching test
    // in this file.
    let _guard = env_test_lock();
    let orig_path = std::env::var_os("PATH").unwrap_or_default();

    // auto + a known wrapper present on PATH -> Ok(Some(name)), the real probe finding it.
    {
        let _bindir = stage_fake_sccache_on_path();
        assert_eq!(
            resolve_wrapper_name("auto"),
            Ok(Some("sccache".to_string())),
            "auto must resolve the real probed wrapper it finds on the real ambient PATH"
        );
    }

    // auto + nothing known on PATH -> Ok(None), the discovered-implicit degrade.
    std::env::set_var("PATH", path_with_neither_known_wrapper());
    assert_eq!(
        resolve_wrapper_name("auto"),
        Ok(None),
        "auto finding nothing on the real ambient PATH must degrade to None, never error"
    );

    // A NAMED wrapper absent from that same real PATH -> Err naming the binary, the
    // configured-explicit failure this whole unit exists to prove never silently degrades.
    let err = resolve_wrapper_name("definitely-not-a-real-wrapper-rigger-u2-nametest")
        .expect_err("a named-but-absent wrapper must error, not silently resolve to None");
    assert!(
        err.to_string()
            .contains("definitely-not-a-real-wrapper-rigger-u2-nametest"),
        "the error must name the missing binary: {err}"
    );

    std::env::set_var("PATH", orig_path);
}
