//! Periphery proof that `$RIGGER_RUN_BASE` reaches a REAL gate subprocess AND a REAL agent
//! subprocess spawned by the REAL `conductor::run` wiring: `RunCtx::run_base_env` reads
//! `RunStarted.base_tip` off the store, and the one build environment (`build_env`) every gate
//! command and every spawn's environment is built from carries it.
//!
//! WHAT THE OTHER PERIPHERY TESTS FOR THIS DO NOT COVER.
//!
//! `tests/checkin_mutation_diff_base_periphery.rs` drives the shipped shell command directly
//! with `RIGGER_RUN_BASE` set BY THE TEST (`Command::env`) - it proves the COMMAND's own
//! contract, but never that the real conductor code is the thing that actually puts the value
//! there. Nothing there drives `conductor::run` itself to prove the value reaches a gate at
//! either gate path (`RunCtx::run_gates`'s inline path, `RunCtx::run_deferred_gates`'s
//! phase-boundary path) or an agent through `RunCtx::spawn_env`.
//!
//! `tests/build_env_authority_periphery.rs` proves the workflow's declared build environment
//! reaches a real gate subprocess the binary launches. `$RIGGER_RUN_BASE` rides that same build
//! environment: an agent that diffs the run's work, or runs a gate command by hand, needs the
//! same base the gate is handed, so the value lives in the one build environment
//! (`gate::BuildEnv::with_var`) rather than being layered onto the gate paths alone.
//!
//! 1. `rigger_run_base_reaches_a_real_inline_gate_subprocess_and_a_real_agent_subprocess`:
//!    seeds a `RunStarted` with a `base_tip` (via the real `rigger::run_store::start_fresh`), then
//!    drives a full `conductor::run` with ONE inline (`core`) gate and a real agent, both
//!    spawned as real subprocesses that echo their env - proves the gate and the agent both
//!    see `RIGGER_RUN_BASE=<the persisted tip>`, from the SAME run.
//! 2. `rigger_run_base_reaches_a_real_deferred_gate_subprocess_too`: the same proof for the
//!    phase-boundary call site (`run_deferred_gates`) - a `deferred`-kind gate, run once the
//!    single-stage run converges. `run_gates` and `run_deferred_gates` are two independent gate
//!    paths reading the same build environment; proving only one would leave the other unguarded.
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

use crate::common;

use std::path::Path;

use common::env_test_lock;
use rigger::config::BuildConfig;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::Event;

#[path = "common/real_driver_spy.rs"]
mod real_driver_spy;
#[path = "common/real_gate_run.rs"]
mod real_gate_run;
use real_gate_run::{run_real_gate_and_agent, write_agent_running};

// `env_test_lock()` serializes every test in this file that touches the real ambient `RIGGER_RUN_BASE` process
// env (only the "unset" test below removes it) against a concurrent thread that might
// otherwise race it - the discipline `tests/common/mod.rs`'s `env_test_lock` documents. Every
// OTHER test in this file never reads or writes ambient env at all: it sets `RIGGER_RUN_BASE`
// explicitly on the child `Command` (through the production `BuildEnv` and the `SiteEnv` it
// resolves at the gate's or the spawn's site), which always
// wins over whatever the parent process's own environment holds, so only the one test that
// relies on ambient absence needs the lock.

const GATE_CMD: &str = "echo RIGGER_RUN_BASE=$RIGGER_RUN_BASE";

/// A fixture "agent" in `dir` that echoes `RIGGER_RUN_BASE`, the run base every spawn's
/// environment carries (see this file's own doc comment), with the same [`GATE_CMD`] the gate
/// runs, written by the shared [`write_agent_running`].
fn write_agent_fixture(dir: &Path) -> std::path::PathBuf {
    write_agent_running(dir, GATE_CMD)
}

/// Drive one full `conductor::run` against `store` (so a caller can pre-seed its `RunStarted`
/// before this ever adopts it - matching criteria, empty here, so `ensure_started` ADOPTS
/// rather than re-mints) with ONE stage/gate of the given `kind` (`"core"` exercises
/// `run_gates`'s inline call site; `"deferred"` exercises `run_deferred_gates`'s
/// phase-boundary call site, run once this single-stage run converges), a real `ExecRunner`
/// gate, and a `RealDriverSpy` wrapping the real `cli::Driver`. Returns the gate's recorded
/// evidence and every real agent-subprocess stdout the run produced.
fn run_once(store: &Store, kind: &str, agent_bin: &Path) -> (String, Vec<String>) {
    run_real_gate_and_agent(store, BuildConfig::default(), kind, GATE_CMD, agent_bin)
}

#[test]
fn rigger_run_base_reaches_a_real_inline_gate_subprocess_and_a_real_agent_subprocess() {
    // Guards against ambient pollution in THIS test process (a gate exporting
    // RIGGER_RUN_BASE to a suite that runs rigger in-process): the agent subprocess inherits
    // the ambient bag, so the tip it echoes must come from spawn_env, never from a value the
    // bag already held. Under a process-per-test runner no sibling's guard runs first, so the
    // test carries its own, exactly like no_persisted_base_tip_* below.
    let _guard = env_test_lock();
    std::env::remove_var("RIGGER_RUN_BASE");

    let store = Store::open(":memory:").unwrap();
    let tip = "deadbeefcafef00d91";
    let criteria: Vec<String> = Vec::new();
    rigger::run_store::start_fresh(&store, &criteria, "", "", tip, "").unwrap();

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
            out.lines().any(|l| l == format!("RIGGER_RUN_BASE={tip}")),
            "the real agent subprocess must see the run's persisted base_tip, the same one \
             the gate sees, through RunCtx::spawn_env: {out}"
        );
    }
}

#[test]
fn rigger_run_base_reaches_a_real_deferred_gate_subprocess_too() {
    let store = Store::open(":memory:").unwrap();
    let tip = "feedfacecafebeef42";
    let criteria: Vec<String> = Vec::new();
    rigger::run_store::start_fresh(&store, &criteria, "", "", tip, "").unwrap();

    let scratch = tempfile::tempdir().unwrap();
    let agent_bin = write_agent_fixture(scratch.path());
    let (gate_evidence, _agent_outputs) = run_once(&store, "deferred", &agent_bin);

    assert!(
        gate_evidence
            .lines()
            .any(|l| l == format!("RIGGER_RUN_BASE={tip}")),
        "the real DEFERRED (phase-boundary) gate subprocess must see the same persisted \
         base_tip run_gates's inline call site does - both read the SAME build_env, never a \
         second, divergent copy: {gate_evidence}"
    );
}

#[test]
fn no_persisted_base_tip_leaves_rigger_run_base_unset_in_the_real_gate_subprocess() {
    // A fresh store, no seeding: `run`'s own `ensure_started` mints a RunStarted with an empty
    // base_tip (the legacy/no-repo case). Guards against ambient pollution in THIS test
    // process - see `env_test_lock`'s own doc comment.
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
