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
//! Seven tests, driving the REAL compiled `rigger` binary as a REAL OS subprocess through the
//! REAL `gate::ExecRunner` - never a fake Runner (test 7 additionally drives the full
//! `conductor::run` orchestration around that same real `ExecRunner`, rather than calling it
//! directly, with only its one agent spawn faked - the gate side stays 100% real throughout):
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
//! 5. `a_real_fenced_couriers_scratch_store_is_reclaimed_for_a_review_worktree_too`: the SAME
//!    real end-to-end wiring as test 4, but for the call site u4's round-1 audit missed and
//!    the round-1 reject found unfenced (`adv-u3c70-store-fence-half-wired-review-worktree-call-site-unfenced`
//!    / `adv-u3c70-reclaim-shares-the-same-exclusion-fix-fence-alone-leaks`): a standalone
//!    review worktree's EXHAUSTIVE gate pass, which always carries an EMPTY `target_dir` (a
//!    review worktree owns no per-unit build cache), so test 4's non-empty-`target_dir`
//!    fencing signal never fires for it - `worktree::review_fence_sibling` is the new,
//!    dir-driven signal that does. Derives its expected fence path by calling
//!    `review_fence_sibling` directly rather than re-typing the suffix inline, so this test
//!    cannot silently drift from the real derivation `gate.rs` and `worktree.rs` share -
//!    exactly the failure mode the round-1 reject's addendum named.
//! 6. `a_real_fenced_couriers_scratch_store_is_reclaimed_by_discard_too`: u4 round 3's fix for
//!    `adv-u4c70r2-discard-path-leaks-review-fence-sibling` - `Worktree::discard` is the
//!    FOURTH teardown path (distinct from `remove`, which test 5 above already covers), the
//!    one `review_only_worktree` runs UNCONDITIONALLY on every standalone-review-stage
//!    attempt before `create()`, modeling a crash-resume. `worktree.rs`'s own unit test
//!    (`discard_also_reclaims_the_review_worktrees_store_fence_sibling`) only proves the
//!    reclaim against two FABRICATED dummy files written by hand; it never proves the
//!    directory a REAL fenced courier actually leaves behind is the one `discard` finds. This
//!    test wires both real paths together, exactly mirroring test 4/5's own rationale.
//! 7. `conductors_derived_store_fence_actually_reaches_a_real_exec_runner`: u4 round 3's
//!    second fix (`d-u4c70r3-store-fence-injected-not-derived`) moved the store-fence
//!    derivation from `gate::ExecRunner::run` itself into `conductor::run_gates`, which now
//!    computes it and injects it as a plain value. The conductor.rs unit test
//!    (`run_gates_derives_and_injects_the_review_worktrees_store_fence`) proves the VALUE is
//!    derived and injected correctly - but only against a `RecordingRunner` test double, never
//!    a real spawned process. This test drives the real `conductor::run` -> `run_gates` chain
//!    with the REAL `gate::ExecRunner` as its `Runner` port (never a mock) through a real
//!    standalone review stage, and proves the gate-spawned courier's write lands in the fenced
//!    scratch location rather than the repo's live store - the one observation neither the
//!    RecordingRunner-based unit test nor a hand-typed-fence unit test can make.
//!
//! Both cwd and target_dir are passed to `ExecRunner::run` explicitly for every call in this
//! file - never left empty to "inherit the ambient cwd" - so the only variable that ever
//! differs between tests 1 and 2 is `target_dir`, the exact signal criterion 3 is about, and
//! neither test can accidentally walk up into (or write into) the real live store of the
//! repository this suite itself runs inside.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use rigger::budget::BuildBudget;
use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error as ConductorError, SpawnOpts};
use rigger::config::{self, AgentDef, Config, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::{
    Autonomy, BuildEnv, ExecRunner, Gate, Kind, Runner, STORE_FENCE_ENV, STORE_FENCE_SUFFIX,
};
use rigger::worktree::{review_fence_sibling, unit_cache_sibling, Worktree};

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
        "",
        "",
        "",
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
        "",
        "",
        "",
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
        "",
        "",
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
        "",
        "",
        "",
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

#[test]
fn a_real_fenced_couriers_scratch_store_is_reclaimed_for_a_review_worktree_too() {
    // Spec 70 criterion 3, widened (u4 round 2 fix for
    // adv-u3c70-store-fence-half-wired-review-worktree-call-site-unfenced /
    // adv-u3c70-reclaim-shares-the-same-exclusion-fix-fence-alone-leaks): the SAME real,
    // end-to-end wiring as the unit-worktree test above
    // (`a_real_fenced_couriers_scratch_store_is_reclaimed_when_the_worktree_is_removed`),
    // but for the call site the u4 round-1 reject actually found unfenced -
    // `run_fan_out_stage`'s standalone review worktree (conductor.rs:4922's
    // `run_gates(st, dir, ..., GateSelection::Exhaustive)`, `dir` a `rigger-review-*`
    // worktree, `target_dir` ALWAYS empty since a review worktree owns no per-unit build
    // cache to key one off of). Proves the real courier succeeds fenced with an EMPTY
    // target_dir (the new, dir-driven signal `worktree::review_fence_sibling` adds), and
    // that the real `Worktree::remove` teardown path - the SAME one `run_fan_out_stage`
    // calls on every terminal exit - reclaims it.
    let repo = init_repo_with_head();
    let repo_path = repo.path().to_string_lossy().into_owned();
    std::fs::create_dir_all(Path::new(&repo_path).join(".rigger")).unwrap();
    let live_events = Path::new(&repo_path).join(".rigger").join("events.db");
    std::fs::File::create(&live_events).unwrap();
    let live_before = std::fs::read(&live_events).unwrap();

    // The real production derivation (conductor's `review_worktree_dir`, mirrored here): a
    // standalone review worktree lives under
    // `<repo>/.rigger/tmp/rigger-review-<stage>-<attempt>` - no per-unit cache sibling,
    // unlike a unit worktree.
    let root = rigger::worktree::scratch_root(&repo_path, "", None);
    let review_dir = format!("{root}/rigger-review-reclaim-probe-0");
    let review = Worktree::create(&repo_path, &review_dir, "rigger/review/reclaim-probe-0")
        .expect("create a real review worktree");
    std::fs::create_dir_all(Path::new(&review.dir).join(".rigger")).unwrap();
    std::fs::write(
        Path::new(&review.dir).join(".rigger").join("workflow.yml"),
        "stages: []\n",
    )
    .unwrap();

    // Derived via the real public function, not a hand-rolled format string - the SAME
    // fidelity the unit-worktree test above holds by deriving its target_dir through
    // `unit_cache_sibling`. Calling the real `review_fence_sibling` here (rather than
    // re-typing `STORE_FENCE_SUFFIX` inline) is not cosmetic: this exact widen-the-fence /
    // widen-the-reclaim pair is what the round-1 reject
    // (`adv-u3c70-reclaim-shares-the-same-exclusion-fix-fence-alone-leaks`) named as the
    // failure mode - a fence and a reclaim that quietly stop sharing one derivation. A
    // hand-rolled format string here would keep passing even if `review_fence_sibling`'s
    // formula ever drifted from what `conductor::run_gates`/`reclaim_cache_sibling` actually
    // use, silently losing the exact regression this test exists to catch.
    //
    // u4 round 3 (arch-u4c70r2-fence-signal-not-injected-into-runner-review-case):
    // `ExecRunner::run` no longer derives this fence itself from `dir` - the CALLER
    // (`conductor::run_gates`, in production) computes it and injects it as `store_fence`,
    // so this test - which drives `ExecRunner` directly rather than through a full
    // conductor run - now passes the identically-derived value production would inject,
    // exercising the real courier + reclaim integration `ExecRunner` alone can no longer
    // wire up on its own.
    let fence_dir = review_fence_sibling(&review.dir)
        .expect("a review worktree dir must derive a fence sibling");

    let result = ExecRunner.run(
        &emit_gate("review-reclaim-emit", "review-reclaim-probe"),
        &review.dir,
        "",
        "",
        "",
        &fence_dir,
        &BuildEnv::default(),
        &BuildBudget::default(),
    );
    assert!(
        result.pass,
        "a real fenced courier for a review worktree (empty target_dir) must succeed: {result:?}"
    );
    assert!(
        Path::new(&fence_dir).join("events.db").exists(),
        "a real fenced review-worktree courier must leave a real, openable events.db at the \
         derived fence sibling {fence_dir} - if this fails, the fence itself is broken, not \
         the reclaim this test targets"
    );

    review.remove().expect("remove the real review worktree");

    assert!(
        !Path::new(&fence_dir).exists(),
        "removing the review worktree via the real Worktree::remove path must reclaim the \
         real fence sibling too, leaked at {fence_dir}"
    );
    let live_after = std::fs::read(&live_events).unwrap();
    assert_eq!(
        live_before, live_after,
        "the repo's live store must stay byte-identical throughout a fenced review-worktree \
         courier that writes, then gets torn down and reclaimed"
    );
}

#[test]
fn a_real_fenced_couriers_scratch_store_is_reclaimed_by_discard_too() {
    // u4 round 3 fix for adv-u4c70r2-discard-path-leaks-review-fence-sibling: the SAME real,
    // end-to-end wiring as the two tests above, but exercising `Worktree::discard` - the
    // FOURTH teardown path (distinct from `remove`, which the test above already covers).
    // `review_only_worktree` calls `discard()` UNCONDITIONALLY before `create()` on every
    // standalone-review-stage attempt - the crash-resume path this project builds every
    // review stage around, not a rare edge case. `worktree.rs`'s own unit test
    // (`discard_also_reclaims_the_review_worktrees_store_fence_sibling`) only proves
    // `reclaim_cache_sibling` deletes a FABRICATED fence-sibling dir built from two dummy
    // files written by hand; it never proves the directory a REAL fenced courier actually
    // leaves behind (created by `require_store_dir`, not by the test) is the one `discard`
    // finds, nor that the real production entry point reclaims it - exactly the gap tests 4
    // and 5 above already close for the other three teardown paths.
    let repo = init_repo_with_head();
    let repo_path = repo.path().to_string_lossy().into_owned();
    std::fs::create_dir_all(Path::new(&repo_path).join(".rigger")).unwrap();
    let live_events = Path::new(&repo_path).join(".rigger").join("events.db");
    std::fs::File::create(&live_events).unwrap();
    let live_before = std::fs::read(&live_events).unwrap();

    let root = rigger::worktree::scratch_root(&repo_path, "", None);
    let review_dir = format!("{root}/rigger-review-discard-reclaim-probe-0");
    let branch = "rigger/review/discard-reclaim-probe-0";
    let review =
        Worktree::create(&repo_path, &review_dir, branch).expect("create a real review worktree");
    let dir = review.dir.clone();
    std::fs::create_dir_all(Path::new(&dir).join(".rigger")).unwrap();
    std::fs::write(
        Path::new(&dir).join(".rigger").join("workflow.yml"),
        "stages: []\n",
    )
    .unwrap();

    // Derived via the real public function, matching test 5's own fidelity rationale: a
    // hand-rolled format string here could keep passing even if `review_fence_sibling`'s
    // formula ever drifted from what `conductor::run_gates`/`reclaim_cache_sibling` actually
    // use.
    let fence_dir =
        review_fence_sibling(&dir).expect("a review worktree dir must derive a fence sibling");

    let result = ExecRunner.run(
        &emit_gate("discard-reclaim-emit", "discard-reclaim-probe"),
        &dir,
        "",
        "",
        "",
        &fence_dir,
        &BuildEnv::default(),
        &BuildBudget::default(),
    );
    assert!(
        result.pass,
        "a real fenced courier for a review worktree (empty target_dir) must succeed: {result:?}"
    );
    assert!(
        Path::new(&fence_dir).join("events.db").exists(),
        "a real fenced review-worktree courier must leave a real, openable events.db at the \
         derived fence sibling {fence_dir} - if this fails, the fence itself is broken, not \
         the reclaim this test targets"
    );

    // The Rust struct is gone (modeling the crash this teardown path exists for) but the
    // real git worktree registration + dir survive on disk, exactly as they would after a
    // real process crash - `discard` operates on the SAME (repo, dir, branch) a resumed
    // process would recompute, not on the dropped struct.
    drop(review);

    // `discard`, not `remove`: the crash-resume teardown path `review_only_worktree` runs
    // unconditionally before every review-stage attempt's `create()`.
    Worktree::discard(&repo_path, &dir, branch).expect("discard the real review worktree");

    assert!(
        !Path::new(&fence_dir).exists(),
        "Worktree::discard must reclaim the real fence sibling a real fenced courier left \
         behind too, leaked at {fence_dir}"
    );
    let live_after = std::fs::read(&live_events).unwrap();
    assert_eq!(
        live_before, live_after,
        "the repo's live store must stay byte-identical throughout a fenced review-worktree \
         courier that writes, then gets discarded and reclaimed"
    );
}

/// An `AgentDriver` that always succeeds with a fixed, canned output - the minimal double a
/// real `conductor::run` needs for its ONE "lens" spawn to complete a fan-out review stage.
/// Unlike `ExecRunner` (the gate side, kept 100% real below), the agent side is not this
/// test's boundary: spec 65's own precedent (`tests/build_env_authority_periphery.rs`'s
/// `RealDriverSpy`) keeps the driver real too when the AGENT injection site is under test,
/// but here the injection site under test is `conductor::run_gates` -> `gate::Runner::run`,
/// so the agent only needs to complete convincingly, not be spawned as a real subprocess.
struct FixedOutputDriver {
    output: String,
}

impl AgentDriver for FixedOutputDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        _opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), ConductorError>,
    ) -> Result<AgentResult, ConductorError> {
        Ok(AgentResult {
            output: self.output.clone(),
            resolved_model: String::new(),
        })
    }
}

#[test]
fn conductors_derived_store_fence_actually_reaches_a_real_exec_runner() {
    // u4 round 3 fix for d-u4c70r3-store-fence-injected-not-derived / the sharpened
    // arch-u4c70r2-fence-signal-not-injected-into-runner-review-case: `gate::Runner::run`
    // gained a caller-injected `store_fence` parameter; `conductor::run_gates` is now the ONE
    // place that derives it (`worktree::review_fence_sibling(dir)`) and threads it down. The
    // conductor.rs unit test (`run_gates_derives_and_injects_the_review_worktrees_store_fence`)
    // proves the derivation and injection - but only against a `RecordingRunner` test double
    // that never touches a real process or a real `STORE_FENCE_ENV`. This test drives the
    // SAME real `conductor::run` -> `run_gates` chain with the REAL `gate::ExecRunner` as the
    // `Runner` port (never a mock) through a real standalone review stage, and observes the
    // only thing a periphery layer can: whether the gate-spawned courier's write actually
    // lands in the fenced scratch location rather than the repo's live store, exactly
    // mirroring test 2's live-store-diff proof for the pre-existing unit-worktree case. The
    // run's own terminal disposition is not this test's concern (mirroring
    // `tests/unified_traversal_grounding.rs`'s `run_and_capture_review_prompts`) - only the
    // real subprocess side effect the wiring produced.
    let repo = init_repo_with_head();
    let repo_path = repo.path().to_string_lossy().into_owned();
    std::fs::create_dir_all(Path::new(&repo_path).join(".rigger")).unwrap();
    let live_events = Path::new(&repo_path).join(".rigger").join("events.db");
    std::fs::File::create(&live_events).unwrap();
    let live_before = std::fs::read(&live_events).unwrap();

    let mut cfg = Config::default();
    cfg.agents.insert(
        "lens".into(),
        AgentDef {
            id: "lens".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "fence-wiring-gate".into(),
        config::Gate {
            run: format!(
                "{} emit DecisionMade '{{\"id\":\"fence-wiring-probe\",\"summary\":\"gate \
                 store fence wiring probe\"}}'",
                rigger_bin().display()
            ),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        "review".into(),
        Stage {
            name: "review".into(),
            // An empty `agent` with a non-empty `agents` lens list marks this a fan-out
            // REVIEW stage (mirroring tests/unified_traversal_grounding.rs), so
            // `run_fan_out_stage` -> `review_only_worktree` mints a real `rigger-review-*`
            // worktree - the exact call site whose `target_dir` is always empty and whose
            // `store_fence` this test exists to prove reaches a real `ExecRunner`.
            agents: vec!["lens".into()],
            gates: vec!["fence-wiring-gate".into()],
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = FixedOutputDriver {
        output: "reviewed the diff".into(),
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path,
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let _ = run(&cfg, &deps);

    let live_after = std::fs::read(&live_events).unwrap();
    assert_eq!(
        live_before, live_after,
        "a real conductor::run driving the REAL ExecRunner through a standalone review stage \
         must never let the gate-spawned courier reach the repo's live store - if this fails, \
         conductor::run_gates's caller-injected store_fence is not actually reaching the real \
         Runner it wires up in production"
    );
}
