//! Spec 62, criterion "COURIERS KEEP THE INSTANCE LIVE": the run driver's
//! `register_run_instance` derives the registry `Instance.root` through `git_repo()` (a
//! `git rev-parse --show-toplevel` subprocess), while the courier's `refresh_registry_entry`
//! derives it through `require_store_dir`/`StoreLocation::dir.parent()` (a filesystem walk-up
//! from the cwd). Both feed the same registry `Instance::id()` (root and store, hashed), so a
//! courier refresh re-stamps the entry a driver heartbeat wrote, on the same file.
//!
//! This pins the driver-to-courier convergence through the compiled binary: a real `rigger run`
//! (the `register_run_instance` path), its one spawn answered by the fake host, followed by a
//! real `rigger progress` (the `refresh_registry_entry` path) against the same project under the
//! same `XDG_STATE_HOME`, write exactly one registry entry, with the courier's refresh landing on
//! the identical file the run created and carrying the same `project`/`root`/`store` identity.

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::cli::{
    run_on_arms_beside_with, run_rigger_in_state_home, write_workflow, RECORDING_WORKER_ARM,
};
use common::fixtures::registry_entries;
use common::git::temp_git_project_with_commit;
#[path = "common/courier_registry.rs"]
mod courier_registry;
use courier_registry::assert_ok;

/// A throwaway project both the driver (`register_run_instance`, through `git_repo()`) and a
/// courier (`refresh_registry_entry`, through `require_store_dir`'s walk-up) must resolve to the
/// same root for: a git repo with a commit (`rigger run` anchors a run branch, which needs a
/// HEAD to branch from), and a minimal one-stage workflow (`isolation: none`, a `nop` grounder)
/// whose one worker [`RECORDING_WORKER_ARM`] answers, so the run completes deterministically
/// with no git worktree of its own.
fn driver_project() -> tempfile::TempDir {
    let dir = temp_git_project_with_commit();
    write_workflow(dir.path(), "");
    dir
}

/// The driver path: `rigger run` over `root` with its registry under `state`, its one worker
/// answered by [`RECORDING_WORKER_ARM`]. Asserts the run succeeds and returns the fake's
/// directory, which holds the `KURRENTDB_CONN` the spawn was started with (`conn`).
fn drive(root: &std::path::Path, state: &std::path::Path) -> tempfile::TempDir {
    let scratch = root.join("scratch");
    let (out, err, ok, fake) = run_on_arms_beside_with(
        root,
        &["run"],
        &[("XDG_STATE_HOME", state.to_str().unwrap())],
        &scratch,
        RECORDING_WORKER_ARM,
        |_| {},
    );
    assert!(ok, "the run must complete; stdout:\n{out}\nstderr:\n{err}");
    fake
}

/// THE CONVERGENCE CLAIM: a real `rigger run` (driver path, `register_run_instance`) followed
/// by a real `rigger progress` (courier path, `refresh_registry_entry`) against the identical
/// project write exactly ONE registry entry, on the SAME file, naming the SAME project/root/
/// store identity - proving the two independent root-derivation implementations genuinely
/// agree at the one thing that matters (the registry key they both feed), not merely by
/// inspection.
#[test]
fn a_driver_run_and_a_courier_progress_refresh_the_identical_registry_entry() {
    let project = driver_project();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    // The DRIVER path: a real `rigger run` registers the instance via `register_run_instance`.
    let _fake = drive(root, state.path());

    let after_run = registry_entries(state.path());
    assert_eq!(
        after_run.len(),
        1,
        "the driver's own registration must write exactly one entry: {after_run:?}"
    );
    let (run_path, run_inst) = &after_run[0];
    assert_eq!(
        run_inst.root,
        root.to_string_lossy(),
        "the driver's entry names the project root"
    );

    // The COURIER path: a real `rigger progress`, from the same root, refreshes via
    // `refresh_registry_entry`. The spawn id need not be a real, live one - `progress` records
    // unconditionally against the progress store, and the registry refresh runs regardless.
    let progress = run_rigger_in_state_home(
        root,
        state.path(),
        &["progress", "a/implementer#0", "working"],
    );
    assert_ok(&progress, &["progress", "a/implementer#0", "working"]);

    let after_progress = registry_entries(state.path());
    assert_eq!(
        after_progress.len(),
        1,
        "the courier's refresh must land on the SAME entry, never file a second one: \
         {after_progress:?}"
    );
    let (progress_path, progress_inst) = &after_progress[0];
    assert_eq!(
        progress_path, run_path,
        "the courier's refresh must be the identical file the driver's run wrote - proving \
         register_run_instance's git-derived root and refresh_registry_entry's walk-up-derived \
         root converge on the same registry key for this project"
    );
    assert_eq!(
        progress_inst.project, run_inst.project,
        "the project identity is unchanged across the driver-then-courier sequence"
    );
    assert_eq!(
        progress_inst.root, run_inst.root,
        "the root is unchanged across the driver-then-courier sequence"
    );
    assert_eq!(
        progress_inst.store, run_inst.store,
        "the store identity is unchanged across the driver-then-courier sequence"
    );
    assert!(
        progress_inst.heartbeat_ms >= run_inst.heartbeat_ms,
        "the courier's refresh still bumped (or held) the heartbeat forward: {} then {}",
        run_inst.heartbeat_ms,
        progress_inst.heartbeat_ms
    );
}

/// Given a well-formed but unreachable `KURRENTDB_CONN` in this test process's own environment,
/// when a real `rigger run` and then a real `rigger progress` drive the same project, then
/// neither child ever sees it: the shared `rigger_courier()` strips it from every child, so the
/// run's spawn is started with no `KURRENTDB_CONN` at all, both commands resolve the fixture's
/// local store and succeed, and the driver-then-courier sequence still converges on one registry
/// entry. A leak would show as a loud connection failure to the bogus address, never a silent
/// pass.
#[test]
#[serial_test::serial(kurrentdb_conn_env)]
fn an_ambient_kurrentdb_conn_never_leaks_into_the_driver_courier_calls() {
    let project = driver_project();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    // Set on this test process, not on any one Command, and restored afterwards.
    let _restore = common::RestoreEnvVars::capture(&["KURRENTDB_CONN"]);
    std::env::set_var("KURRENTDB_CONN", "kurrentdb://127.0.0.1:1/");

    let fake = drive(root, state.path());
    assert_eq!(
        std::fs::read_to_string(fake.path().join("bin/conn")).expect("the worker ran"),
        "unset",
        "the run's spawn is started with no KURRENTDB_CONN"
    );

    let progress = run_rigger_in_state_home(
        root,
        state.path(),
        &["progress", "a/implementer#0", "working"],
    );
    assert_ok(&progress, &["progress", "a/implementer#0", "working"]);

    let entries = registry_entries(state.path());
    assert_eq!(
        entries.len(),
        1,
        "the driver-then-courier sequence converges on exactly one registry entry with an \
         ambient KURRENTDB_CONN in the calling process's environment: {entries:?}"
    );
}
