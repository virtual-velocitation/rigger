//! Periphery (contract / API / integration) tests for spec 65 criterion 3: the
//! machine-wide BUILD BUDGET - `src/budget.rs`'s flock-based slot semaphore and its
//! ONE gating point, `gate::ExecRunner::run`.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/budget.rs`'s own unit tests exercise `BuildBudget::acquire`/`SlotGuard`
//! directly, in isolation, against a caller-supplied `slot_dir` - real flock
//! semantics, but never through the actual call site the spec names ("gate the ONE
//! compiler call site"). Every test in `src/gate.rs` that exercises
//! `ExecRunner::run` passes `&BuildBudget::default()` (unlimited), so nothing
//! anywhere proves `ExecRunner::run` actually calls `budget.acquire()` around its
//! real subprocess, that the acquired slot really blocks a second real command, or
//! that a completely different kind of call - an agent spawn - is untouched by an
//! exhausted budget. Nor does anything prove the `src/conductor.rs` wiring: that a
//! committed `build.max_concurrent` reaches `RunCtx::build_budget()` and constrains
//! REAL concurrent gate builds inside a real `conductor::run` wave, nor that
//! `build.max_concurrent` round-trips through the real on-disk `config::load` path
//! (only a bare `serde_yaml::from_str` literal is unit-tested in `config.rs`).
//!
//! This file closes those gaps, over the crate's PUBLIC surface only, using REAL
//! production types and REAL subprocesses:
//!
//! 1. `exec_runner_blocks_the_real_subprocess_on_an_externally_held_slot`: the ONE
//!    gating point (`gate.rs`'s `budget.acquire()` immediately before
//!    `cmd.output()`) is driven through the real `ExecRunner::run`, not
//!    `BuildBudget::acquire` in isolation - proves the wiring the
//!    mocked/unlimited-budget tests elsewhere cannot, and that the guard it holds
//!    is released again once the real subprocess finishes.
//! 2. `an_exhausted_budget_gates_a_real_build_but_never_a_real_agent_spawn`: the
//!    explicit "non-build agent work is never gated" clause of the done-when,
//!    proven with a real `driver::cli::Driver` subprocess spawn racing a real
//!    `ExecRunner` build against the SAME fully-exhausted budget.
//! 3. `build_config_max_concurrent_round_trips_through_the_real_on_disk_loader`:
//!    mirrors the identical gap class `build_env_authority_periphery.rs` closed for
//!    wrapper/cache_dir (unit 1), now for `max_concurrent` - the real
//!    `.rigger/workflow.yml` loader (`config::load`, agent parsing +
//!    `Config::validate` included), not a bare literal.
//! 4. `configured_max_concurrent_serializes_two_real_concurrent_stage_gate_builds`:
//!    the cross-module seam - `conductor::run` with `build.max_concurrent: 1` and
//!    two independent (`needs: []`) stages, both scheduled into the SAME
//!    concurrent wave (`run_batch`'s `std::thread::scope`), each running a REAL
//!    gate subprocess. Proves `RunCtx::build_budget()` truly threads the committed
//!    config into the real call site and forces the two real builds to serialize -
//!    and, since the redirected `TMPDIR` (see the test's own doc comment) makes
//!    this exercise `budget::default_slot_dir()` itself rather than a
//!    caller-supplied dir, closes that pub fn's own gap too.
//!
//! Out of scope, owned by sibling units per spec 65: the env resolver and its two
//! injection sites (unit 1, closed by `build_env_authority_periphery.rs`),
//! no-silent-degrade wrapper resolution (unit 2), the jobs cap (unit 4), and the
//! `validate`/`setup` reporting surfaces (unit 5).
//!
//! Nothing here is feature-gated: `budget::BuildBudget`, `gate::ExecRunner`,
//! `cli::Driver`, and `config::load` are all compiled and exercised in both
//! feature lanes.

use std::path::Path;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use rigger::budget::BuildBudget;
use rigger::conductor::{run, AgentDriver, Deps, SpawnOpts};
use rigger::config::{self, AgentDef, BuildConfig, Config, Stage};
use rigger::driver::cli;
use rigger::eventstore::sqlite::Store;
use rigger::gate::{Autonomy, BuildEnv, ExecRunner, Kind, Runner};

/// A `gate::Gate` (the runner's own type - distinct from `config::Gate`, the
/// committed-config shape a real run converts into this) for the two tests that
/// drive `ExecRunner::run` directly rather than through a full `conductor::run`.
fn direct_gate(run_cmd: &str) -> rigger::gate::Gate {
    rigger::gate::Gate {
        id: "g".into(),
        run: run_cmd.into(),
        kind: Kind::Core,
        autonomy: Autonomy::Manual,
        history: Vec::new(),
    }
}

#[test]
fn exec_runner_blocks_the_real_subprocess_on_an_externally_held_slot() {
    // budget.rs's own unit tests prove `BuildBudget::acquire` blocks in isolation;
    // gate.rs's own unit tests always pass `&BuildBudget::default()` (unlimited),
    // so nothing anywhere proves the ACTUAL gating point - `budget.acquire()`
    // immediately before `ExecRunner::run`'s `cmd.output()` - is really wired. This
    // holds the one slot externally, drives a REAL `ExecRunner::run` through it,
    // and proves the real subprocess it spawns does not start until the slot
    // frees, then that the guard `run` itself held is released again afterward.
    let dir = tempfile::tempdir().expect("create temp dir");
    let slot_dir = dir.path().join("slots");
    let marker = dir.path().join("marker");
    let budget = BuildBudget::new(1, slot_dir);

    let held = budget.acquire();

    let gate = direct_gate(&format!("touch {}", marker.display()));
    let (tx, rx) = mpsc::channel();
    let budget2 = budget.clone();
    let handle = std::thread::spawn(move || {
        let res = ExecRunner.run(&gate, "", "", "", &BuildEnv::default(), &budget2);
        tx.send(res).unwrap();
    });

    std::thread::sleep(Duration::from_millis(250));
    assert!(
        !marker.exists(),
        "the gate subprocess must not have run yet - its one slot is externally held"
    );

    drop(held);
    let res = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the gate must run once the held slot frees");
    handle.join().unwrap();
    assert!(res.pass, "the gate command must succeed: {}", res.evidence);
    assert!(
        marker.exists(),
        "the real subprocess must have run once the slot freed"
    );

    // The guard ExecRunner::run itself acquired must have been released again by
    // the time run() returned - not merely that SOME slot eventually freed (the one
    // we dropped), but that ExecRunner's own hold on it ended with the call.
    let reacquired = budget.acquire();
    drop(reacquired);
}

#[test]
fn an_exhausted_budget_gates_a_real_build_but_never_a_real_agent_spawn() {
    // The done-when's explicit clause: "non-build agent work is never gated". The
    // one slot is held for the WHOLE test (never released) - if the agent-spawn
    // path touched the budget at all, it would block forever, not just slowly;
    // completing within a tight bound is a robust binary signal, not a timing
    // race. The paired gate-build assertion below proves the asymmetry is real:
    // the SAME exhausted budget genuinely blocks a real build.
    let dir = tempfile::tempdir().expect("create temp dir");
    let slot_dir = dir.path().join("slots");
    let budget = BuildBudget::new(1, slot_dir);
    let _held = budget.acquire(); // the one slot, held for the whole test, never released

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-agent.sh");
    assert!(agent_bin.exists(), "fixture agent {agent_bin:?} must exist");
    let driver = cli::Driver {
        bin: agent_bin.to_string_lossy().into_owned(),
    };
    let agent = AgentDef {
        id: "worker".into(),
        ..Default::default()
    };
    let opts = SpawnOpts::default();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let res = driver.spawn(&agent, "do the unit", &opts, &|_, _| Ok(()));
        tx.send(res).unwrap();
    });
    let res = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a real agent spawn must never be gated by an exhausted build budget");
    assert!(res.is_ok(), "the agent spawn must succeed: {res:?}");

    // Contrast within the same test, same exhausted budget: a real gate BUILD
    // genuinely blocks, unlike the agent spawn above.
    let gate = direct_gate("true");
    let gate_budget = budget.clone();
    let (gtx, grx) = mpsc::channel();
    std::thread::spawn(move || {
        let res = ExecRunner.run(&gate, "", "", "", &BuildEnv::default(), &gate_budget);
        gtx.send(res).unwrap();
    });
    assert!(
        grx.recv_timeout(Duration::from_millis(300)).is_err(),
        "a real gate build under the SAME exhausted budget must block, unlike the \
         agent spawn above - slots bind builds, not agents"
    );
}

/// Write a minimal but real `.rigger/agents/worker.md` + `.rigger/workflow.yml` at
/// `root`, so `config::load` reaches all the way through agent parsing and
/// `Config::validate` - the real on-disk boundary, not a struct literal built in
/// memory. `build_block` is appended to the workflow verbatim: `""` omits the
/// `build:` section entirely (the back-compat case - the documented default must
/// apply), a `build:\n  ...\n` block pins an explicit `max_concurrent`.
fn write_workflow(root: &Path, build_block: &str) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).expect("create .rigger/agents");
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .expect("write worker.md");
    let workflow = format!(
        "name: buildbudgettest\n\
         defaults:\n  grounder: nop\n  budget: 60\n\
         stages:\n  a:\n    agent: worker\n    on_pass: none\n\
         {build_block}"
    );
    std::fs::write(rigger.join("workflow.yml"), workflow).expect("write workflow.yml");
}

#[test]
fn build_config_max_concurrent_round_trips_through_the_real_on_disk_loader() {
    // config.rs's own unit test parses `BuildConfig` via a bare `serde_yaml::
    // from_str` literal - it never goes through the real `.rigger/workflow.yml`-
    // on-disk loading path (`config::load`), which also reads the agents
    // directory and calls `Config::validate`. Mirrors the identical gap class
    // build_env_authority_periphery.rs closed for wrapper/cache_dir (spec 65 unit
    // 1), now for `max_concurrent`.
    let configured = tempfile::tempdir().expect("create temp project");
    write_workflow(configured.path(), "build:\n  max_concurrent: 2\n");
    let cfg = config::load(configured.path().to_str().unwrap()).expect("load a valid workflow.yml");
    assert_eq!(cfg.workflow.build.max_concurrent, 2);

    // The back-compat case: an OMITTED `build:` section - every workflow.yml
    // committed before this unit existed - must still load cleanly through the
    // real loader and resolve `max_concurrent` to the documented default of 4, NOT
    // the plain `BuildConfig::default()` (0) an in-memory Rust construction gets.
    let legacy = tempfile::tempdir().expect("create temp project");
    write_workflow(legacy.path(), "");
    let cfg = config::load(legacy.path().to_str().unwrap())
        .expect("a workflow.yml with no build: section must still load");
    assert_eq!(
        cfg.workflow.build.max_concurrent, 4,
        "an omitted build: section must resolve max_concurrent to 4 through the real loader"
    );

    // An EXPLICIT 0 must still parse as 0 (unlimited) through the real loader too -
    // distinct from the omitted-key default above.
    let unlimited = tempfile::tempdir().expect("create temp project");
    write_workflow(unlimited.path(), "build:\n  max_concurrent: 0\n");
    let cfg = config::load(unlimited.path().to_str().unwrap()).expect("load a valid workflow.yml");
    assert_eq!(
        cfg.workflow.build.max_concurrent, 0,
        "an explicit max_concurrent: 0 must parse as 0 (unlimited) through the real loader"
    );
}

/// Serializes any test in this file that redirects the process `TMPDIR` (see
/// `configured_max_concurrent_serializes_two_real_concurrent_stage_gate_builds`): a
/// mutation there is a process-global env change, the same POSIX setenv/getenv
/// hazard `build_env_authority_periphery.rs`'s own `ENV_TEST_LOCK` documents. Only
/// one test in this file touches it, but the lock - and the unconditional restore
/// on every exit path, including a panic - stays as a hard discipline against a
/// future test racing it.
static TMPDIR_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn configured_max_concurrent_serializes_two_real_concurrent_stage_gate_builds() {
    let _guard = TMPDIR_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    // `budget::default_slot_dir()` is the OS temp dir - deliberately NOT
    // configurable per spec 65 ("never configurable per-project", the budget spans
    // the whole machine) - so redirecting `TMPDIR` (which `std::env::temp_dir`
    // reads first on Unix) to a private test-only directory is the only way to
    // exercise `RunCtx::build_budget()`'s real, hardcoded resolution without
    // contending with a REAL rigger process's real budget slots on this very
    // machine. Scoped to this one test and restored unconditionally afterward,
    // even on panic, via the guard below.
    let private_root = tempfile::tempdir().expect("create a private fake machine temp root");
    let prior_tmpdir = std::env::var_os("TMPDIR");
    std::env::set_var("TMPDIR", private_root.path());

    struct RestoreTmpdir(Option<std::ffi::OsString>);
    impl Drop for RestoreTmpdir {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => std::env::set_var("TMPDIR", v),
                None => std::env::remove_var("TMPDIR"),
            }
        }
    }
    let _restore = RestoreTmpdir(prior_tmpdir);

    let log = private_root.path().join("gate.log");

    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.build = BuildConfig {
        max_concurrent: 1,
        ..Default::default()
    };
    cfg.workflow.gates.insert(
        "gate_a".into(),
        config::Gate {
            run: format!(
                "echo start-a >> {p}; sleep 0.3; echo end-a >> {p}",
                p = log.display()
            ),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.gates.insert(
        "gate_b".into(),
        config::Gate {
            run: format!(
                "echo start-b >> {p}; sleep 0.3; echo end-b >> {p}",
                p = log.display()
            ),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    // Two INDEPENDENT stages (`needs` empty on both): `ready_stages` puts both in
    // the SAME wave, and `run_batch` runs a whole batch's stages concurrently via
    // `std::thread::scope` (MAX_CONCURRENCY=4, well above 2) - real OS-thread
    // concurrency, not a simulated one.
    cfg.workflow.stages.insert(
        "a".into(),
        Stage {
            name: "a".into(),
            agent: "worker".into(),
            gates: vec!["gate_a".into()],
            on_pass: "none".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "b".into(),
        Stage {
            name: "b".into(),
            agent: "worker".into(),
            gates: vec!["gate_b".into()],
            on_pass: "none".into(),
            ..Default::default()
        },
    );

    let agent_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-agent.sh");
    assert!(agent_bin.exists(), "fixture agent {agent_bin:?} must exist");
    let driver = cli::Driver {
        bin: agent_bin.to_string_lossy().into_owned(),
    };
    let store = Store::open(":memory:").unwrap();
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    run(&cfg, &deps).expect("the run must complete: two real agents and two real gate builds");

    // `default_slot_dir()`'s own composition, proven live: with TMPDIR redirected,
    // RunCtx::build_budget() must have resolved to (and actually locked) a slot
    // file under THIS private root - not some other path, and not a caller-
    // supplied one (there is none here; the conductor never exposes one).
    let expected_slot = std::env::temp_dir()
        .join("rigger-build-budget")
        .join("slot-0.lock");
    assert!(
        expected_slot.exists(),
        "conductor::build_budget() must resolve through budget::default_slot_dir() \
         under the (redirected) process temp dir: expected {expected_slot:?} to exist"
    );

    let contents = std::fs::read_to_string(&log).expect("read the gate log");
    let lines: Vec<&str> = contents.lines().collect();
    let expected: std::collections::BTreeSet<&str> = ["start-a", "end-a", "start-b", "end-b"]
        .into_iter()
        .collect();
    let got: std::collections::BTreeSet<&str> = lines.iter().copied().collect();
    assert_eq!(
        got, expected,
        "both real gate builds must have run exactly once each: got {lines:?}"
    );

    // The central claim: with max_concurrent=1, the two real gate subprocesses
    // never overlapped - one's start+end pair is fully complete before the
    // other's start appears. Any interleaving (e.g. start-a, start-b, end-a,
    // end-b) would mean both held the compiler slot at once.
    let ok = lines == ["start-a", "end-a", "start-b", "end-b"]
        || lines == ["start-b", "end-b", "start-a", "end-a"];
    assert!(
        ok,
        "the one build-budget slot must serialize the two real concurrent stage \
         gate builds (no interleaving): got {lines:?}"
    );
}
