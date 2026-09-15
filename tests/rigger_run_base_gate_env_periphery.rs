//! Periphery proof for spec 91's round-2 fix (finding
//! adv-u91c2-mutation-gate-diff-base-collapses-to-empty, decision
//! u91c2-r2-run-base-tip-not-buildenv-runner-param): `$RIGGER_RUN_BASE` reaches a REAL gate
//! subprocess spawned by the REAL `conductor::run` wiring - `RunCtx::run_base_env` reading
//! `RunStarted.base_tip` off the store, then `gate::BuildEnv::with_var` appending it to the
//! SAME `build_env` every gate command already receives - and, just as load-bearing, that it
//! reaches ONLY the gate boundary, never a real agent subprocess.
//!
//! WHAT THE OTHER PERIPHERY TESTS FOR THIS FIX DO NOT COVER.
//!
//! `tests/checkin_mutation_diff_base_periphery.rs` drives the shipped shell command directly
//! with `RIGGER_RUN_BASE` set BY THE TEST (`Command::env`) - it proves the COMMAND's own
//! contract, but never that the real conductor code is the thing that actually puts the value
//! there. Nothing there (or anywhere else) drives `conductor::run` itself to prove
//! `RunCtx::run_base_env` + `gate::BuildEnv::with_var` (both entirely untested - `with_var` has
//! no unit test at all, not even in `gate.rs`'s own `mod tests`) are correctly wired at either
//! of the two call sites that layer them (`RunCtx::run_gates`'s inline path,
//! `RunCtx::run_deferred_gates`'s phase-boundary path).
//!
//! `tests/build_env_authority_periphery.rs` proves the ONE build-environment authority
//! (wrapper/cache/jobs) reaches BOTH a real gate subprocess AND a real agent subprocess.
//! `$RIGGER_RUN_BASE` deliberately does NOT follow that shape: it is layered only onto the two
//! gate-execution call sites, never onto `RunCtx::spawn_env` (the agent-spawn injection site) -
//! it exists solely for the checkin stage's own `mutation` gate command, and an agent has no
//! use for the run's start tip. Nothing in the tree proves that scope boundary at the
//! real-subprocess level; a future change that accidentally threads it into `spawn_env` too
//! (or the reverse - some agent-side need routed through this instead of its own seam) would
//! pass every existing test. This file closes that gap.
//!
//! 1. `rigger_run_base_reaches_a_real_inline_gate_subprocess_but_not_a_real_agent_subprocess`:
//!    seeds a `RunStarted` with a `base_tip` (via the real `rigger::run::start_fresh`), then
//!    drives a full `conductor::run` with ONE inline (`core`) gate and a real agent, both
//!    spawned as real subprocesses that echo their env - proves the gate sees
//!    `RIGGER_RUN_BASE=<the persisted tip>` and the agent sees it genuinely unset, from the
//!    SAME run.
//! 2. `rigger_run_base_reaches_a_real_deferred_gate_subprocess_too`: the same proof for the
//!    phase-boundary call site (`run_deferred_gates`) - a `deferred`-kind gate, run once the
//!    single-stage run converges. `run_gates` and `run_deferred_gates` are two independent call
//!    sites layering the same value; proving only one would leave the other's wiring unguarded.
//! 3. `no_persisted_base_tip_leaves_rigger_run_base_unset_in_the_real_gate_subprocess`: a run
//!    with no seeded `RunStarted` (a fresh mint, `base_tip` empty - the legacy/no-repo case)
//!    must inject nothing - the real gate subprocess sees `RIGGER_RUN_BASE` genuinely unset,
//!    matching `gate::BuildEnv::with_var`'s "empty means off" convention and the shipped gate's
//!    own `test -n "$RIGGER_RUN_BASE"` guard.
//! 4. `current_run_base_tip_reports_none_for_a_legacy_run_started_event_missing_the_field`: the
//!    event-schema half of this change (`RunStarted` gains a body field). Every existing
//!    round-trip test mints its `RunStarted` through today's own `to_event`, which always
//!    serializes `base_tip` explicitly (empty string or not) - none of them can ever exercise a
//!    truly pre-spec-91 event whose JSON body has NO `base_tip` key at all. This constructs
//!    exactly that shape (over the crate's public `Event`/`current_run_base_tip` surface, no
//!    private `RunStarted::from_event` access needed) and proves the reader degrades to `None`
//!    rather than panicking or corrupting the rest of the decode.
//!
//! Nothing here is feature-gated: `conductor::run`, `gate::ExecRunner`, `driver::cli`, and
//! `run::start_fresh`/`current_run_base_tip` are all compiled and exercised in both feature
//! lanes.

use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::contextgraph::TYPE_GATE_VERDICT;
use rigger::driver::cli;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};

/// Serializes every test in this file that touches the real ambient `RIGGER_RUN_BASE` process
/// env (only the "unset" test below removes it) against a concurrent thread that might
/// otherwise race it - the same `ENV_TEST_LOCK` discipline
/// `tests/build_env_authority_periphery.rs` uses for the vars its own tests touch. Every OTHER
/// test in this file never reads or writes ambient env at all: it sets `RIGGER_RUN_BASE`
/// explicitly on the child `Command` (via the production `BuildEnv`/`apply` path), which always
/// wins over whatever the parent process's own environment holds, so only the one test that
/// relies on ambient absence needs the lock.
static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const UNIT: &str = "a";
const GATE: &str = "envgate";
const GATE_CMD: &str = "echo RIGGER_RUN_BASE=$RIGGER_RUN_BASE";

/// A fixture "agent" that echoes `RIGGER_RUN_BASE` - never configured by production code for an
/// agent spawn, see this file's own doc comment - alongside the final structured JSON line
/// every real `cli::Driver::spawn` needs to parse a result, mirroring
/// `tests/fixtures/env-echo-agent.sh`'s own shape but written per-test so this file stays
/// self-contained.
fn write_agent_fixture(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("agent.sh");
    std::fs::write(
        &path,
        "#!/bin/sh\necho RIGGER_RUN_BASE=$RIGGER_RUN_BASE\necho '{\"id\":\"final\",\"pass\":true}'\n",
    )
    .expect("write agent fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod agent fixture");
    }
    path
}

/// Delegates every spawn to the REAL `driver::cli::Driver`, recording only its stdout - the
/// identical observation-point pattern `tests/build_env_authority_periphery.rs`'s own
/// `RealDriverSpy` uses: an OBSERVATION point, not a substitute implementation.
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

/// Drive one full `conductor::run` against `store` (so a caller can pre-seed its `RunStarted`
/// before this ever adopts it - matching criteria, empty here, so `ensure_started` ADOPTS
/// rather than re-mints) with ONE stage/gate of the given `kind` (`"core"` exercises
/// `run_gates`'s inline call site; `"deferred"` exercises `run_deferred_gates`'s
/// phase-boundary call site, run once this single-stage run converges), a real `ExecRunner`
/// gate, and a `RealDriverSpy` wrapping the real `cli::Driver`. Returns the gate's recorded
/// evidence and every real agent-subprocess stdout the run produced.
fn run_once(store: &Store, kind: &str, agent_bin: &Path) -> (String, Vec<String>) {
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
            kind: kind.into(),
            inputs: Vec::new(),
        },
    );
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

    let driver = RealDriverSpy::new(agent_bin);
    let deps = Deps {
        store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
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

#[test]
fn rigger_run_base_reaches_a_real_inline_gate_subprocess_but_not_a_real_agent_subprocess() {
    let store = Store::open(":memory:").unwrap();
    let tip = "deadbeefcafef00d91";
    let criteria: Vec<String> = Vec::new();
    rigger::run::start_fresh(&store, &criteria, "", "", tip, "").unwrap();

    let scratch = tempfile::tempdir().unwrap();
    let agent_bin = write_agent_fixture(scratch.path());
    let (gate_evidence, agent_outputs) = run_once(&store, "core", &agent_bin);

    assert!(
        gate_evidence
            .lines()
            .any(|l| l == format!("RIGGER_RUN_BASE={tip}")),
        "the real inline gate subprocess must see the run's persisted base_tip: {gate_evidence}"
    );
    assert!(!agent_outputs.is_empty(), "the agent must have spawned");
    for out in &agent_outputs {
        assert!(
            out.lines().any(|l| l == "RIGGER_RUN_BASE="),
            "RIGGER_RUN_BASE must never reach a real agent subprocess - it exists solely for \
             the checkin stage's own mutation gate, never RunCtx::spawn_env: {out}"
        );
    }
}

#[test]
fn rigger_run_base_reaches_a_real_deferred_gate_subprocess_too() {
    let store = Store::open(":memory:").unwrap();
    let tip = "feedfacecafebeef42";
    let criteria: Vec<String> = Vec::new();
    rigger::run::start_fresh(&store, &criteria, "", "", tip, "").unwrap();

    let scratch = tempfile::tempdir().unwrap();
    let agent_bin = write_agent_fixture(scratch.path());
    let (gate_evidence, _agent_outputs) = run_once(&store, "deferred", &agent_bin);

    assert!(
        gate_evidence
            .lines()
            .any(|l| l == format!("RIGGER_RUN_BASE={tip}")),
        "the real DEFERRED (phase-boundary) gate subprocess must see the same persisted \
         base_tip run_gates's inline call site does - run_deferred_gates layers the SAME \
         run_base_env() via the SAME with_var, never a second, divergent copy: {gate_evidence}"
    );
}

#[test]
fn no_persisted_base_tip_leaves_rigger_run_base_unset_in_the_real_gate_subprocess() {
    // A fresh store, no seeding: `run`'s own `ensure_started` mints a RunStarted with an empty
    // base_tip (the legacy/no-repo case). Guards against ambient pollution in THIS test
    // process the same way build_env_authority_periphery.rs's own tests guard RUSTC_WRAPPER
    // etc. - see ENV_TEST_LOCK's own doc comment.
    let _guard = env_test_lock();
    std::env::remove_var("RIGGER_RUN_BASE");

    let store = Store::open(":memory:").unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let agent_bin = write_agent_fixture(scratch.path());
    let (gate_evidence, _agent_outputs) = run_once(&store, "core", &agent_bin);

    assert!(
        gate_evidence.lines().any(|l| l == "RIGGER_RUN_BASE="),
        "with no persisted base_tip, the real gate subprocess must see RIGGER_RUN_BASE \
         genuinely unset (echoing empty), never a stale or ambient value: {gate_evidence}"
    );
}

#[test]
fn current_run_base_tip_reports_none_for_a_legacy_run_started_event_missing_the_field() {
    // A genuine pre-spec-91 RunStarted shape: `run`/`criteria`/`definition`/`spec` are all
    // fields that predate `base_tip` (spec 06, spec 13, spec 82) and this literal has no
    // `base_tip` key at all - never merely an empty one, which every OTHER test in the tree
    // (including the implementer's own round-trip unit test) only ever produces by going
    // through today's `to_event`, which always writes the key. `#[serde(default)]` is what
    // makes this decode at all; nothing before this test proved that specific promise for
    // `base_tip` against a body that never had the key.
    let legacy = Event::new(
        rigger::run::TYPE_RUN_STARTED,
        br#"{"run":"legacy-run-1","criteria":["c1"],"definition":"defhash","spec":"specs/1-x.md"}"#
            .to_vec(),
    );
    let events = vec![legacy];

    assert_eq!(
        rigger::run::current_run_base_tip(&events),
        None,
        "a RunStarted body with no base_tip key at all (a genuine pre-spec-91 event) must \
         decode with no persisted tip, never panic or default to Some(\"\")"
    );
    assert_eq!(
        rigger::run::current_run_id(&events).as_deref(),
        Some("legacy-run-1"),
        "the rest of the legacy body must still decode fine alongside the missing field"
    );
    assert_eq!(
        rigger::run::current_run(&events).len(),
        1,
        "run-scoping over a legacy-shaped RunStarted is unaffected by the new field"
    );
}
