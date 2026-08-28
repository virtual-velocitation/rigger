//! Spec 62, criterion "COURIERS KEEP THE INSTANCE LIVE" (unit u62c4) - closes a residual,
//! non-blocking gap the review round left open
//! (arch-u62c4-root-derivation-two-independent-implementations-unverified-convergence): the
//! run driver's `register_run_instance` (`src/main.rs`, pre-existing) derives the registry
//! `Instance.root` via `git_repo()` (a `git rev-parse --show-toplevel` subprocess), while the
//! courier counterpart this unit adds, `refresh_registry_entry`, derives it via
//! `require_store_dir`/`StoreLocation::dir.parent()` (a filesystem walk-up from cwd). Both feed
//! the SAME registry `Instance::id()` (root + store, hashed) - `refresh_registry_entry`'s own
//! doc comment claims it "re-stamps the SAME entry a driver heartbeat would - same file" - but
//! no suite ever drove BOTH functions for the SAME project and asserted they converge on the
//! identical file. Every existing regression proves courier-to-courier convergence only
//! (`courier_registry_refresh_periphery.rs`'s "same entry, never a second one" assertions all
//! seed AND refresh through `refresh_registry_entry`; the boundary suite's nested-worktree test
//! does the identical thing).
//!
//! This proves the driver-to-courier half directly, through the REAL compiled binary: a real
//! `rigger step` (the `register_run_instance` path) followed by a real `rigger progress` (the
//! `refresh_registry_entry` path) against the SAME project must write exactly one registry
//! entry, not two, with the courier call's own refresh landing on the identical file the step
//! created and carrying forward the SAME `project`/`root`/`store` identity.

use std::path::Path;
use std::process::{Command, Output};

use rigger::registry::{self, Instance};

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

/// A throwaway project both the driver (`register_run_instance`, via `git_repo()`) and a
/// courier (`refresh_registry_entry`, via `require_store_dir`'s walk-up) must resolve to the
/// SAME root for: a real git repo with a commit (`rigger step` anchors a run branch, which
/// needs a HEAD to branch from), and a minimal, offline, one-stage workflow (`isolation: none`,
/// a `nop` grounder) so `rigger step` completes deterministically with no model call and no
/// git worktree of its own.
fn driver_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temp project");
    let root = dir.path();
    let ok = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(ok, "git init must succeed while seeding the fixture");
    for args in [
        &["config", "user.email", "t@example.com"][..],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the fixture");
    }
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).expect("create .rigger/agents");
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .expect("write the agent prompt");
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: convergencetest
defaults:
  grounder: nop
  budget: 60
stages:
  a:
    agent: worker
    on_pass: none
"#,
    )
    .expect("write workflow.yml");
    dir
}

/// Run `rigger <args...>` in `cwd`, with the machine-global registry redirected into the
/// CALLER-OWNED `state_home` - shared across both calls in this test, so the SAME registry
/// directory is read back and re-written across the driver-then-courier sequence.
///
/// `KURRENTDB_CONN` is REMOVED from the child so this fixture's project (a throwaway git repo
/// with no committed store config) resolves the LOCAL sqlite log regardless of what the calling
/// process's own ambient environment happens to carry - mirroring every sibling courier-spawning
/// periphery test in this family (`store_precedence.rs`, `store_resolution_cli.rs`,
/// `store_secrets.rs`). Without this, a real, reachable, credentialed `KURRENTDB_CONN` in the
/// operator's shell - a documented, supported rigger configuration - would silently route this
/// test's real `rigger step`/`progress` writes into the operator's actual shared event store
/// under this fixture's fake identity: genuine test-data pollution of production
/// infrastructure, not merely a spurious local failure.
fn run_rigger(cwd: &Path, state_home: &Path, args: &[&str]) -> Output {
    common::rigger_courier()
        .args(args)
        .current_dir(cwd)
        // Never let a real driver step or courier spawn a real dashboard under test.
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state_home)
        .env_remove("KURRENTDB_CONN")
        .output()
        .expect("the rigger binary runs")
}

fn assert_ok(out: &Output, args: &[&str]) {
    assert!(
        out.status.success(),
        "rigger {args:?} failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Every registry entry under `state_home`, decoded through `Instance`'s own (de)serialization -
/// mirrors the identically purposed helper in every sibling registry-refresh suite (each
/// periphery suite owns its own small fixture helpers rather than sharing test-only code
/// across files).
fn registry_entries(state_home: &Path) -> Vec<(std::path::PathBuf, Instance)> {
    let dir = registry::instances_dir(state_home);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(body) = std::fs::read(&path) {
            if let Ok(inst) = serde_json::from_slice::<Instance>(&body) {
                out.push((path, inst));
            }
        }
    }
    out
}

/// THE CONVERGENCE CLAIM: a real `rigger step` (driver path, `register_run_instance`) followed
/// by a real `rigger progress` (courier path, `refresh_registry_entry`) against the identical
/// project write exactly ONE registry entry, on the SAME file, naming the SAME project/root/
/// store identity - proving the two independent root-derivation implementations genuinely
/// agree at the one thing that matters (the registry key they both feed), not merely by
/// inspection.
#[test]
fn a_driver_step_and_a_courier_progress_refresh_the_identical_registry_entry() {
    let project = driver_project();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    // The DRIVER path: a real `rigger step` registers the instance via `register_run_instance`.
    let step = run_rigger(root, state.path(), &["step"]);
    assert_ok(&step, &["step"]);

    let after_step = registry_entries(state.path());
    assert_eq!(
        after_step.len(),
        1,
        "the driver's own registration must write exactly one entry: {after_step:?}"
    );
    let (step_path, step_inst) = &after_step[0];
    assert_eq!(
        step_inst.root,
        root.to_string_lossy(),
        "the driver's entry names the project root"
    );

    // The COURIER path: a real `rigger progress`, from the same root, refreshes via
    // `refresh_registry_entry`. The spawn id need not be a real, live one - `progress` records
    // unconditionally against the progress store, and the registry refresh runs regardless.
    let progress = run_rigger(
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
        progress_path, step_path,
        "the courier's refresh must be the identical file the driver's step wrote - proving \
         register_run_instance's git-derived root and refresh_registry_entry's walk-up-derived \
         root converge on the same registry key for this project"
    );
    assert_eq!(
        progress_inst.project, step_inst.project,
        "the project identity is unchanged across the driver-then-courier sequence"
    );
    assert_eq!(
        progress_inst.root, step_inst.root,
        "the root is unchanged across the driver-then-courier sequence"
    );
    assert_eq!(
        progress_inst.store, step_inst.store,
        "the store identity is unchanged across the driver-then-courier sequence"
    );
    assert!(
        progress_inst.heartbeat_ms >= step_inst.heartbeat_ms,
        "the courier's refresh still bumped (or held) the heartbeat forward: {} then {}",
        step_inst.heartbeat_ms,
        progress_inst.heartbeat_ms
    );
}

/// THE REGRESSION PIN for `adv-u62c4-r4-convergence-test-leaks-ambient-kurrentdb-conn`: before
/// `run_rigger`'s `.env_remove("KURRENTDB_CONN")` above, this suite inherited whatever
/// `KURRENTDB_CONN` the CALLING process's own environment carried, unfiltered. This test
/// simulates that ambient leak directly - setting a well-formed but UNREACHABLE
/// `KURRENTDB_CONN` in the test process's own environment, the exact shape the adjudicator
/// reproduced (`KURRENTDB_CONN=kurrentdb://127.0.0.1:1/`) - rather than depending on a real,
/// reachable server being present in CI, since a real server would make the leak look like
/// success (writing into someone's real store) rather than a loud, checkable failure here.
///
/// Pre-fix, the real `rigger step` subprocess this drives would crash immediately with a gRPC
/// connection error to the bogus address (confirmed by direct reproduction during review); with
/// the fix, the child never sees `KURRENTDB_CONN` at all, so it resolves this fixture's LOCAL
/// sqlite log exactly as the primary convergence test above does, and the driver-then-courier
/// sequence still converges on one entry.
#[test]
#[serial_test::serial(kurrentdb_conn_env)]
fn an_ambient_kurrentdb_conn_never_leaks_into_the_driver_courier_calls() {
    let project = driver_project();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    // Simulate the ambient leak: set it on THIS test process, not on any one Command - proving
    // `run_rigger` itself strips it from the child, regardless of what the calling process
    // carries. Well-formed but unreachable (matches the adjudicator's own repro), so a
    // regression manifests as a loud subprocess failure below, never a silent pass.
    //
    // Captured through the shared `RestoreEnvVars` (`tests/common`), not a bespoke Drop guard:
    // round 5's own local `struct Restore` here unconditionally `remove_var`'d on drop instead
    // of restoring this process's real prior value - a real defect (`adv-u62c4-r5-uphold-
    // remove-var-reintro-third-occurrence`, self-assessed non-blocking since no other test in
    // this file reads `KURRENTDB_CONN` afterward) that a proper capture/restore closes for
    // good rather than leaving a third divergent, still-broken copy on record.
    let _restore = common::RestoreEnvVars::capture(&["KURRENTDB_CONN"]);
    std::env::set_var("KURRENTDB_CONN", "kurrentdb://127.0.0.1:1/");

    let step = run_rigger(root, state.path(), &["step"]);
    assert_ok(&step, &["step"]);

    let progress = run_rigger(
        root,
        state.path(),
        &["progress", "a/implementer#0", "working"],
    );
    assert_ok(&progress, &["progress", "a/implementer#0", "working"]);

    let entries = registry_entries(state.path());
    assert_eq!(
        entries.len(),
        1,
        "the driver-then-courier sequence still converges on exactly one registry entry even \
         with an ambient KURRENTDB_CONN present in the calling process's environment - proving \
         run_rigger's env_remove genuinely isolates the child, not merely that this fixture \
         never happens to need a store connection: {entries:?}"
    );
}
