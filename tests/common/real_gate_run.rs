//! One full `conductor::run` through a REAL gate and a REAL agent, shared by the suites that
//! observe what environment those two subprocesses actually receive. Included by path
//! (`#[path = "common/real_gate_run.rs"] mod real_gate_run;`) alongside
//! `common/real_driver_spy.rs`, whose `RealDriverSpy` it drives. A suite uses the subset it
//! needs (hence the module-wide `dead_code` allowance, the same convention
//! `tests/common/mod.rs` keeps).

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde_json::Value;

use rigger::conductor::{run, Deps, STREAM};
use rigger::config::{AgentDef, BuildConfig, Config, Gate, Stage};
use rigger::contextgraph::TYPE_GATE_VERDICT;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::ExecRunner;

use crate::real_driver_spy::RealDriverSpy;

const UNIT: &str = "a";
const GATE: &str = "envgate";

/// Drive one full `conductor::run` onto `store` with `build` configured: ONE stage whose one
/// gate of kind `gate_kind` runs `gate_cmd` through a REAL `ExecRunner`, and whose one agent
/// is a `RealDriverSpy` wrapping the REAL `cli::Driver` (spawning `agent_bin`). Returns the
/// gate's recorded evidence and every real agent-subprocess stdout the run produced.
pub fn run_real_gate_and_agent(
    store: &Store,
    build: BuildConfig,
    gate_kind: &str,
    gate_cmd: &str,
    agent_bin: &Path,
) -> (String, Vec<String>) {
    let cfg = one_gate_workflow(build, gate_kind, gate_cmd);
    let driver = RealDriverSpy::new(agent_bin);
    run(&cfg, &deps(store, &driver)).expect("the run must complete: a real agent and a real gate");

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

/// Hand `run` - the PUBLIC library entry point, never through `config::load` - a one-gate
/// workflow configured with `build` whose build environment cannot be resolved: the run must
/// return `Err` (`must_fail` says why a silent `Ok` is wrong) before a single agent spawns.
/// Returns the error's message.
pub fn refused_run(build: BuildConfig, must_fail: &str) -> String {
    let cfg = one_gate_workflow(build, "core", "true");
    let agent_bin = echo_agent_path();
    let store = Store::open(":memory:").unwrap();
    let driver = RealDriverSpy::new(&agent_bin);

    // `RunState` (the `Ok` type) does not implement `Debug`, so `expect_err` cannot be used
    // here - match explicitly instead.
    let err = match run(&cfg, &deps(&store, &driver)) {
        Err(e) => e,
        Ok(_) => panic!("{must_fail}"),
    };
    assert!(
        driver.outputs().is_empty(),
        "the build-env resolution failure must surface BEFORE any agent spawns, not after \
         wasted work: {:?}",
        driver.outputs()
    );
    err.to_string()
}

/// The fixture agent that echoes the build variables it was spawned with.
pub fn echo_agent_path() -> PathBuf {
    crate::common::repo::repo_root().join("tests/fixtures/env-echo-agent.sh")
}

/// A workflow of ONE `worker` stage whose one gate of kind `gate_kind` runs `gate_cmd`, with
/// `build` configured.
fn one_gate_workflow(build: BuildConfig, gate_kind: &str, gate_cmd: &str) -> Config {
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
            run: gate_cmd.into(),
            kind: gate_kind.into(),
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
    cfg
}

/// The run's dependencies: `store`, `driver` for every agent and the REAL `ExecRunner` for
/// every gate.
fn deps<'a>(store: &'a Store, driver: &'a RealDriverSpy) -> Deps<'a> {
    Deps {
        store,
        driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
        log: &|_| {},
    }
}
