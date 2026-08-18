//! Periphery (contract / API / integration) tests for spec 70 criterion 3: the gate store
//! fence - `gate::ExecRunner::run`'s pinning of `gate::STORE_FENCE_ENV` on a gate's spawned
//! process, and `main.rs::require_store_dir`'s honoring of it.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/gate.rs`'s own unit test
//! (`exec_runner_fences_gate_store_resolution_when_target_dir_is_given`) only shell-`test`s
//! that `STORE_FENCE_ENV` carries the right VALUE on the spawned `Command` - it never runs a
//! real store-opening courier inside that command, so it cannot see whether the fenced
//! location can actually be OPENED by anything.
//!
//! `src/main.rs`'s own two unit tests
//! (`require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it`,
//! `require_store_dir_fence_is_off_by_default`) call `require_store_dir()` directly,
//! IN-PROCESS - never through a real spawned subprocess, and never through the real
//! `gate::ExecRunner` that is the ONLY production code that ever sets `STORE_FENCE_ENV`.
//! Neither unit test observes the two pieces WIRED TOGETHER end to end: a real
//! `ExecRunner::run` call spawning a real courier subprocess that itself calls
//! `require_store_dir()`.
//!
//! That gap is not cosmetic. Closing it here (test 1 below) found a genuine boundary bug the
//! disjoint unit tests could not see: `ExecRunner::run` names a fenced scratch directory but
//! never creates it, and `require_store_dir`'s fenced branch handed that unopened path
//! straight to the sqlite backend, which refuses to open a database file in a directory that
//! does not exist - so a REAL fenced courier failed outright with a database-open error
//! instead of landing in an isolated, USABLE store (the opposite of criterion 3's "resolves
//! its store to the fenced scratch location"). Reproduced against the built binary before
//! this file existed, then closed at the root (`require_store_dir` now creates the fenced
//! directory before handing it back - the one store-resolution authority every courier
//! funnels through, so the fix covers `emit`/`result`/`peers`/`reported`/`prompt` uniformly).
//!
//! Three tests, driving the REAL compiled `rigger` binary as a REAL OS subprocess through the
//! REAL `gate::ExecRunner` - never a fake Runner, never an in-process function call:
//!
//! 1. `a_real_fenced_courier_actually_succeeds_and_lands_in_an_isolated_persistent_store`:
//!    a non-empty `target_dir` (the unit-worktree gate signal) fences a real courier
//!    subprocess. Proves the fenced courier SUCCEEDS (not merely "is pointed somewhere"),
//!    that its write actually PERSISTS in that fenced location across two independently
//!    spawned gate processes (a real, working, isolated store - not a fresh one thrown away
//!    each call), and that the repo's live store stays byte-identical throughout.
//! 2. `an_unfenced_integrated_tree_gate_still_walks_up_to_the_live_store`: the SAME signal in
//!    the other direction - an empty `target_dir` (the integrated-tree/deferred-gate case)
//!    must NOT be fenced, so its real courier subprocess still reaches and writes into the
//!    repo's real live store, exactly as before this fence existed. Defensively clears
//!    `STORE_FENCE_ENV` from this test's own ambient environment first, because this exact
//!    suite is itself liable to run AS a `cargo test` gate inside a unit worktree - which
//!    would carry a real, inherited `STORE_FENCE_ENV` of its own onto every subprocess this
//!    binary spawns, silently fencing a test whose entire point is proving the unfenced path.
//! 3. `a_periphery_couriers_shared_command_ignores_an_inherited_ambient_fence`: the FAIL-SAFE
//!    direction's other edge (the u3 reject's ground (a),
//!    `adv-u3-fence-breaks-existing-store-precedence-tests-measured`) - a periphery test
//!    whose ENTIRE purpose is driving the product binary through its own store-resolution
//!    logic against a fixture of its own choosing must never observe the ambient fence this
//!    very suite's own process may itself be running under. Proves `tests/common::
//!    rigger_courier` (the fix) resolves exactly as the unfenced baseline would - refusing
//!    over a never-initialized fixture - even with a real `RIGGER_STORE_FENCE_DIR` set on
//!    this test binary's own process.
//! 4. `a_real_fenced_couriers_scratch_store_is_reclaimed_when_the_worktree_is_removed`:
//!    ground (b) of the u3 reject (`adv-u3-fence-dir-leaks-forever-uncleaned`), closed by the
//!    round-2 fix's `worktree::reclaim_cache_sibling` change and its own new unit test
//!    (`worktree_remove_also_reclaims_the_store_fence_sibling`). That unit test proves
//!    `reclaim_cache_sibling` deletes a FABRICATED fence-sibling dir built from two dummy
//!    files written by hand - it never proves the directory a REAL fenced courier (test 1
//!    above) actually leaves behind (created by `require_store_dir`, not by the test) is the
//!    one `reclaim_cache_sibling` finds, nor that the real, production teardown entry point
//!    (`Worktree::remove`) reclaims it through the exact derivation `gate.rs` and
//!    `worktree.rs` now share (`STORE_FENCE_SUFFIX`). This test wires both real paths
//!    together - a real `ExecRunner`-spawned courier creates the fence sibling, then the
//!    real `Worktree::remove` reclaims it - the integration neither unit test (one never
//!    creates the directory, the other never runs a courier) can see.
//!
//! Both cwd and target_dir are passed to `ExecRunner::run` explicitly for every call in this
//! file - never left empty to "inherit the ambient cwd" - so the only variable that ever
//! differs between tests 1 and 2 is `target_dir`, the exact signal criterion 3 is about, and
//! neither test can accidentally walk up into (or write into) the real live store of the
//! repository this suite itself runs inside.

use std::path::Path;
use std::process::Command;

use rigger::budget::BuildBudget;
use rigger::gate::{
    Autonomy, BuildEnv, ExecRunner, Gate, Kind, Runner, STORE_FENCE_ENV, STORE_FENCE_SUFFIX,
};
use rigger::worktree::{unit_cache_sibling, Worktree};

mod common;
use common::rigger_bin;

fn git_init_quiet(root: &Path) {
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .expect("git init");
}

/// The real production topology (matching the implementer's own
/// `require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it`
/// fixture, and `find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above`
/// before it): a git repo root carrying the LIVE store, with a nested, storeless
/// `.rigger/tmp/rigger-wt-<slug>` unit worktree below it. Built under `tempfile::tempdir()`
/// (the OS temp dir, outside this repository's own git ancestry) so a courier that walks up
/// unfenced can never escape past the fixture's own root into the real repo this suite runs
/// inside - the walk-up hazard this file's real-subprocess courtiers must never risk.
struct Topology {
    _tmp: tempfile::TempDir,
    live_events: std::path::PathBuf,
    worktree: std::path::PathBuf,
    /// A unit-keyed scratch path in the same shape Gap 19 derives, NOT created here - the
    /// whole point of test 1 is that a real courier must survive an uncreated one.
    target_dir: std::path::PathBuf,
}

fn build_topology() -> Topology {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    git_init_quiet(&root);
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    let live_events = root.join(".rigger").join("events.db");
    // An empty file is a valid, openable, empty sqlite database (the exact shape
    // `find_store_dir_from`'s own precedent tests use to mark a directory as a store).
    std::fs::File::create(&live_events).unwrap();

    let worktree = root.join(".rigger").join("tmp").join("rigger-wt-probe");
    std::fs::create_dir_all(worktree.join(".rigger")).unwrap();
    std::fs::write(
        worktree.join(".rigger").join("workflow.yml"),
        "stages: []\n",
    )
    .unwrap();

    let target_dir = root.join(".rigger").join("tmp").join("cargo-target-probe");

    Topology {
        _tmp: tmp,
        live_events,
        worktree,
        target_dir,
    }
}

/// A `gate::Gate` whose command shells out to the REAL compiled `rigger` binary - never a
/// fake - so `ExecRunner::run`'s env injection is observed by an actual OS process, not a
/// Rust-level double. Single-quotes the JSON payload so its own double quotes survive `sh -c`
/// literally.
fn emit_gate(id: &str, decision_id: &str) -> Gate {
    Gate {
        id: id.into(),
        run: format!(
            "{} emit DecisionMade '{{\"id\":\"{decision_id}\",\"summary\":\"gate store fence periphery probe\"}}'",
            rigger_bin().display()
        ),
        kind: Kind::Core,
        autonomy: Autonomy::Manual,
        history: vec![],
    }
}

#[test]
fn a_real_fenced_courier_actually_succeeds_and_lands_in_an_isolated_persistent_store() {
    let topo = build_topology();
    let live_before = std::fs::read(&topo.live_events).unwrap();
    let dir = topo.worktree.to_string_lossy().into_owned();
    let target_dir = topo.target_dir.to_string_lossy().into_owned();

    let first = ExecRunner.run(
        &emit_gate("fence-emit-1", "fence-probe-1"),
        &dir,
        &target_dir,
        &BuildEnv::default(),
        &BuildBudget::default(),
    );
    assert!(
        first.pass,
        "a real fenced courier must succeed and land in the isolated scratch store, not \
         error out opening a directory nobody created: {first:?}"
    );
    assert!(
        first.evidence.contains("position 1"),
        "the first event written to a freshly fenced store must be position 1: {first:?}"
    );

    // A SECOND, independently spawned gate-fenced courier against the SAME target_dir must
    // see the FIRST one's write still there - a real, persistent, isolated store, not a
    // fresh empty one fabricated and discarded on every call.
    let second = ExecRunner.run(
        &emit_gate("fence-emit-2", "fence-probe-2"),
        &dir,
        &target_dir,
        &BuildEnv::default(),
        &BuildBudget::default(),
    );
    assert!(
        second.pass,
        "a second fenced courier against the same target_dir must also succeed: {second:?}"
    );
    assert!(
        second.evidence.contains("position 2"),
        "the second gate-spawned courier must see the first one's write still there, proving \
         a genuine persistent store rather than a fresh fence per call: {second:?}"
    );

    let live_after = std::fs::read(&topo.live_events).unwrap();
    assert_eq!(
        live_before, live_after,
        "the repo's live store must be byte-identical before and after two real fenced \
         courier subprocesses"
    );
}

#[test]
#[serial_test::serial(cwd)]
fn an_unfenced_integrated_tree_gate_still_walks_up_to_the_live_store() {
    // Defensive: this exact suite is itself liable to run AS a `cargo test` gate inside a
    // unit worktree, which would carry a real, inherited STORE_FENCE_ENV of its own onto
    // every subprocess this binary spawns - silently fencing the one test whose entire point
    // is proving the unfenced path, and reproducing (inside this very test) the class of
    // "gate red irreproducible outside the gate" defect spec 70 exists to eliminate. Shares
    // the `cwd` serial key with `src/main.rs`'s own STORE_FENCE_ENV-mutating tests' naming
    // convention (a separate OS process from this integration binary, so no real lock is
    // shared - the tag documents the same discipline, not a cross-binary dependency).
    std::env::remove_var(STORE_FENCE_ENV);

    let topo = build_topology();
    let live_before = std::fs::read(&topo.live_events).unwrap();
    assert!(
        live_before.is_empty(),
        "precondition: a fresh, empty live store"
    );
    let dir = topo.worktree.to_string_lossy().into_owned();

    // target_dir = "" - the integrated-tree/deferred-gate signal (Gap 19): never fenced,
    // exactly as it is never given a target_dir. `dir` is still explicit (never inherited
    // ambient cwd), so this test never risks running from - or writing into - wherever this
    // suite's own OS process happens to be.
    let result = ExecRunner.run(
        &emit_gate("unfenced-emit", "unfenced-probe"),
        &dir,
        "",
        &BuildEnv::default(),
        &BuildBudget::default(),
    );
    assert!(
        result.pass,
        "an unfenced courier must still succeed by walking up to the repo's real store: \
         {result:?}"
    );
    assert!(
        result.evidence.contains("position 1"),
        "the unfenced courier's write must land as the first event of the repo's real \
         store: {result:?}"
    );

    let live_after = std::fs::read(&topo.live_events).unwrap();
    assert_ne!(
        live_before, live_after,
        "an unfenced gate-spawned courier must actually write into the repo's real live \
         store - the baseline behavior this fence must never disturb for the integrated tree"
    );
}

#[test]
#[serial_test::serial(cwd)]
fn a_periphery_couriers_shared_command_ignores_an_inherited_ambient_fence() {
    // Ground (a) of the u3 reject (adv-u3-fence-breaks-existing-store-precedence-tests-
    // measured): STORE_FENCE_ENV is a process-tree-wide override, inherited by EVERY
    // descendant of a gate's spawned process - including THIS exact test binary, whenever
    // it itself runs AS a `cargo test` gate inside a unit worktree (the everyday case, per
    // `.rigger/workflow.yml`'s `test` gate). A periphery test that spawned its own courier
    // via a bare `Command::new(rigger_bin())` used to silently inherit that ambient fence
    // and observe the fenced scratch location instead of the fixture it built - exactly the
    // regression the adversary measured against tests/store_precedence.rs (8/8 -> 0/8, with
    // couriers from different test functions overwriting the same fenced store).
    // `tests/common::rigger_courier` is the fix: the ONE shared authority every periphery
    // suite now spawns the product through, defensively clearing the var before spawning so
    // a courier resolves exactly as if no ambient fence existed, regardless of how THIS
    // test binary itself was invoked. Shares the `cwd` serial key with this file's other
    // STORE_FENCE_ENV-mutating test (test 2 above) so neither observes the other's
    // in-flight env mutation.
    std::env::remove_var(STORE_FENCE_ENV);
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            std::env::remove_var(STORE_FENCE_ENV);
        }
    }
    let _restore = Restore;

    // Simulate exactly what a real fenced `cargo test` gate does to this test binary's own
    // process: an ambient RIGGER_STORE_FENCE_DIR pointing at some scratch dir this binary
    // never built and never asked for.
    let ambient_fence = tempfile::tempdir().unwrap();
    std::env::set_var(STORE_FENCE_ENV, ambient_fence.path());

    // A never-initialized project fixture - store_precedence.rs's own baseline shape - that
    // this courier must resolve AGAINST, never against the ambient fence above.
    let project = tempfile::tempdir().unwrap();
    git_init_quiet(project.path());
    std::fs::create_dir_all(project.path().join(".rigger")).unwrap();

    let out = common::rigger_courier()
        .args(["result", "u/impl#0", "--error", "a self-report"])
        .current_dir(project.path())
        .env("RIGGER_NO_DASH", "1")
        .env_remove("KURRENTDB_CONN")
        .output()
        .expect("spawn rigger result via the shared courier helper");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a courier over a never-initialized project must refuse, not silently succeed \
         against an inherited ambient fence: stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("no rigger store found"),
        "the courier must reach the REAL walk-up/refuse path for its OWN fixture, proving \
         it ignored the ambient RIGGER_STORE_FENCE_DIR this test process itself carries; \
         stderr:\n{stderr}"
    );
    assert!(
        !project.path().join(".rigger").join("events.db").exists(),
        "a refused courier must not fabricate a local events.db in its own fixture either"
    );
    assert!(
        !ambient_fence.path().join("events.db").exists(),
        "a courier that correctly ignored the ambient fence must never write into it either"
    );
}

/// A real `git init` + one empty commit, so `Worktree::create` has a HEAD to branch a real
/// unit worktree off of - the shape every real `rigger step` unit worktree is created
/// against, distinct from `build_topology`'s bare `git init` (which only ever needs a store
/// dir, never a worktree add).
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

#[test]
fn a_real_fenced_couriers_scratch_store_is_reclaimed_when_the_worktree_is_removed() {
    let repo = init_repo_with_head();
    let repo_path = repo.path().to_string_lossy().into_owned();
    std::fs::create_dir_all(Path::new(&repo_path).join(".rigger")).unwrap();
    let live_events = Path::new(&repo_path).join(".rigger").join("events.db");
    std::fs::File::create(&live_events).unwrap();
    let live_before = std::fs::read(&live_events).unwrap();

    // The real production derivation (conductor's `unit_worktree_dir`, mirrored here): a
    // unit worktree lives under `<repo>/.rigger/tmp/rigger-wt-<slug>`, a sibling of its own
    // `cargo-target-<slug>` cache. `Worktree::create` is the SAME entry point `rigger step`
    // uses - not a hand-built directory - so this test exercises the real `git worktree add`
    // path, not a double of it.
    let root = rigger::worktree::scratch_root(&repo_path, "", None);
    let worktree_dir = format!("{root}/rigger-wt-reclaim-probe");
    let worktree = Worktree::create(&repo_path, &worktree_dir, "rigger/u/reclaim-probe")
        .expect("create a real unit worktree");
    std::fs::create_dir_all(Path::new(&worktree.dir).join(".rigger")).unwrap();
    std::fs::write(
        Path::new(&worktree.dir)
            .join(".rigger")
            .join("workflow.yml"),
        "stages: []\n",
    )
    .unwrap();

    // The exact target_dir the conductor's own `run_gates` would pass for this worktree
    // (Gap 19) - reconstructed via the SAME single authority `reclaim_cache_sibling` uses,
    // so this test can never silently drift from the real derivation.
    let target_dir =
        unit_cache_sibling(&worktree.dir).expect("a unit worktree dir must derive a cache sibling");
    let fence_dir = format!("{target_dir}{STORE_FENCE_SUFFIX}");

    let result = ExecRunner.run(
        &emit_gate("reclaim-emit", "reclaim-probe"),
        &worktree.dir,
        &target_dir,
        &BuildEnv::default(),
        &BuildBudget::default(),
    );
    assert!(
        result.pass,
        "a real fenced courier must succeed before this test ever tears its worktree down: \
         {result:?}"
    );
    assert!(
        Path::new(&fence_dir).join("events.db").exists(),
        "a real fenced courier must leave a real, openable events.db at the derived fence \
         sibling {fence_dir} - if this fails, the fence itself (test 1) is broken, not the \
         reclaim this test targets"
    );

    // The real production teardown entry point - not a call into reclaim_cache_sibling
    // directly, which is a private fn only Worktree::remove and sweep_terminal may reach.
    worktree.remove().expect("remove the real unit worktree");

    assert!(
        !Path::new(&fence_dir).exists(),
        "removing the unit worktree via the real Worktree::remove path must reclaim the \
         real fence sibling a real fenced courier left behind, leaked at {fence_dir}"
    );
    let live_after = std::fs::read(&live_events).unwrap();
    assert_eq!(
        live_before, live_after,
        "the repo's live store must stay byte-identical throughout a fenced courier that \
         writes, then gets torn down and reclaimed"
    );
}
