//! Spec 62, criterion "COURIERS KEEP THE INSTANCE LIVE" (unit u62c4) - two residual boundary
//! gaps in `refresh_registry_entry`'s (`src/main.rs`) contract that
//! `tests/courier_registry_refresh_periphery.rs` proves the HEADLINE behavior for but does not
//! reach:
//!
//!   1. THE NESTED-WORKTREE EDGE CASE the function's own doc comment names as its reason for
//!      using `loc.identity()`/`loc.dir.parent()` (bound to the RESOLVED store root) instead of
//!      the ambient `project_identity()`/cwd a courier's process actually runs from: "a courier
//!      can run from a cwd that is not the store's owner (a nested worktree)". Every existing
//!      registry-refresh test runs a courier from the project root itself, so this specific
//!      choice - the one thing that makes a worker's self-report from inside its unit worktree
//!      (this suite's own `.rigger/tmp/rigger-wt-*` situation) file under the OWNING repo's
//!      entry rather than a second, worktree-scoped one - is never exercised.
//!
//!   2. THE WRITE-ERROR HALF of the degrade contract the function's doc comment promises: "a
//!      homeless environment (no resolvable state home) OR a write error never fails, slows, or
//!      warns away the courier's real work". The existing suite proves the homeless half
//!      (`a_homeless_environment_never_fails_a_courier_command`) but that test returns from
//!      `default_dir()` being `None` BEFORE `refresh_registry_entry` ever calls
//!      `rigger::registry::write` - so the `if let Err(e) = rigger::registry::write(...)` arm
//!      itself, the OTHER half of the same documented OR, has no test forcing it to actually run.

use std::path::Path;
use std::process::{Command, Output};

use rigger::registry::{self, Instance};

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::RestoreEnvVars;

/// A throwaway project the compiled binary accepts as a courier target: its own git repo with a
/// real commit (`git worktree add` needs a committed HEAD) and an INITIALIZED event log - a
/// courier refuses to fabricate one from a cwd with no existing store (spec 05).
fn courier_project_with_commit() -> tempfile::TempDir {
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
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).expect("create .rigger");
    std::fs::File::create(rigger_dir.join("events.db")).expect("seed an initialized event log");
    dir
}

/// Run `rigger <args...>` in `cwd`, with the machine-global registry redirected into the
/// CALLER-OWNED `state_home`.
fn run_rigger(cwd: &Path, state_home: &Path, args: &[&str]) -> Output {
    common::rigger_courier()
        .args(args)
        .current_dir(cwd)
        // Never let a short-lived courier spawn a real dashboard under test.
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state_home)
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
/// mirrors the read helper in `tests/courier_registry_refresh_periphery.rs` (each periphery
/// suite owns its own small fixture helpers rather than sharing test-only code across files).
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

/// GAP 1: a courier run from a REAL git-linked worktree nested under the project - exactly the
/// `<repo>/.rigger/tmp/rigger-wt-x` shape a spawned worker's self-report runs from - refreshes
/// the SAME registry entry a courier from the project root itself would, bound to the OWNING
/// root/identity, never a second entry scoped to the worktree's own path. This is the nested-
/// worktree namespace-misfile class spec 05 already closed for the EVENT STORE
/// (`result_from_a_nested_git_worktree_records_into_the_repo_stream` in `tests/cli.rs`); this
/// unit's `refresh_registry_entry` makes the identical choice (`loc.identity()`/
/// `loc.dir.parent()`, not the ambient cwd) for the REGISTRY entry, and this proves it holds
/// there too.
#[test]
fn a_courier_in_a_nested_worktree_refreshes_the_owning_roots_registry_entry() {
    let project = courier_project_with_commit();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    // Seed the entry from the OWNING ROOT, exactly as a driver or a root-run courier would.
    let seed = run_rigger(
        root,
        state.path(),
        &["progress", "u1/impl#0", "seeded from root"],
    );
    assert_ok(&seed, &["progress", "u1/impl#0", "seeded from root"]);
    let seeded = registry_entries(state.path());
    assert_eq!(
        seeded.len(),
        1,
        "the root courier writes exactly one entry: {seeded:?}"
    );
    let (seeded_path, seeded_inst) = &seeded[0];
    assert_eq!(
        seeded_inst.root,
        root.to_string_lossy(),
        "the seeded entry's root is the project root itself"
    );

    // A REAL git-linked worktree nested under the repo, exactly like the conductor's Gap-14
    // scratch root a spawned worker's own courier calls run from.
    let wt = root.join(".rigger").join("tmp").join("rigger-wt-x");
    std::fs::create_dir_all(wt.parent().unwrap()).expect("create the worktree's parent dir");
    let ok = Command::new("git")
        .args(["worktree", "add", "-q"])
        .arg(&wt)
        .current_dir(root)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(
        ok,
        "git worktree add must succeed for the nested-worktree fixture"
    );

    // The exact courier traffic a worker self-reports with, run FROM the nested worktree.
    let out = run_rigger(
        &wt,
        state.path(),
        &["progress", "u1/impl#0", "second, from the worktree"],
    );
    assert_ok(
        &out,
        &["progress", "u1/impl#0", "second, from the worktree"],
    );

    let after = registry_entries(state.path());
    assert_eq!(
        after.len(),
        1,
        "the nested-worktree courier must refresh the SAME entry, never file a second one \
         scoped to the worktree's own path: {after:?}"
    );
    assert_eq!(
        &after[0].0, seeded_path,
        "the nested-worktree call refreshed the identical entry file the root call wrote, \
         not a worktree-scoped one"
    );
    assert_eq!(
        after[0].1.root,
        root.to_string_lossy(),
        "the refreshed entry still names the OWNING root, never the nested worktree's own path \
         (got {:?})",
        after[0].1.root
    );
    assert!(
        !after[0].1.root.contains("rigger-wt-x"),
        "the entry must never be filed under the worktree's own path: {:?}",
        after[0].1.root
    );
    assert_eq!(
        after[0].1.project, seeded_inst.project,
        "the project identity is unchanged by running from the nested worktree, confirming it \
         is bound to the resolved owning root, not the process cwd"
    );
    assert!(
        after[0].1.heartbeat_ms > seeded_inst.heartbeat_ms,
        "the worktree call still bumped the heartbeat forward: {} then {}",
        seeded_inst.heartbeat_ms,
        after[0].1.heartbeat_ms
    );
}

/// GAP 2: a registry WRITE ERROR (not merely a homeless environment) must never fail a
/// courier's real work either - the OTHER half of `refresh_registry_entry`'s documented OR.
/// Forced deterministically by pre-creating `<state>/rigger` as a plain FILE, so
/// `create_dir_all(<state>/rigger/instances)` fails on that path component and
/// `rigger::registry::write` returns `Err` - never by touching filesystem permissions, which
/// would not discriminate a process run as root.
#[test]
fn a_registry_write_error_never_fails_a_couriers_real_work() {
    let project = courier_project_with_commit();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    std::fs::write(state.path().join("rigger"), b"not a directory")
        .expect("block the registry's own directory with a same-named file");

    let out = run_rigger(
        root,
        state.path(),
        &["progress", "u1/impl#0", "did a thing"],
    );
    assert!(
        out.status.success(),
        "a registry write error must never fail the courier's real work: stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("progress recorded for u1/impl#0"),
        "the courier's own output is unaffected by the registry write error: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("instance registry refresh skipped"),
        "the write-error degrade is still visible on stderr (warn-only, never fatal), distinct \
         from the homeless-environment message: {stderr}"
    );
}

/// Regression pin for `sdet-u62c4r3-kurrentdb-leak-not-blast-radius-audited` /
/// `adv-u62c4-r5-kurrentdb-leak-independently-reproduced-8of8`, closed at
/// `adj-u62c4-r6-verdict-reject-blast-radius-audit-incomplete`'s required fix (mirroring the
/// identically-purposed pin in `courier_registry_refresh_periphery.rs`, each periphery suite
/// owning its own): `run_rigger`'s `common::rigger_courier()` now strips an ambient
/// `KURRENTDB_CONN` from every child it spawns, so this file's own courier calls (including
/// the nested-worktree one above, whose fixture commits no store config either) resolve the
/// local sqlite store regardless of what this test process's own environment carries.
///
/// Sets a well-formed but UNREACHABLE `KURRENTDB_CONN` on THIS test process before spawning -
/// never on the `Command` itself - so this is a genuine regression proof rather than a
/// trivially-passing fixture: pre-fix, the courier below would crash with a real gRPC connect
/// error instead of resolving this fixture's local sqlite store.
#[test]
#[serial_test::serial(kurrentdb_conn_env)]
fn an_ambient_kurrentdb_conn_never_leaks_into_a_boundary_courier() {
    let project = courier_project_with_commit();
    let root = project.path();
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME");

    let _restore = RestoreEnvVars::capture(&["KURRENTDB_CONN"]);
    std::env::set_var("KURRENTDB_CONN", "kurrentdb://127.0.0.1:1/");

    let out = run_rigger(
        root,
        state.path(),
        &["progress", "u1/impl#0", "did a thing"],
    );
    assert!(
        out.status.success(),
        "a courier spawned through the shared rigger_courier() helper must resolve the \
         fixture's local sqlite store, not attempt a real gRPC connection to whatever \
         KURRENTDB_CONN this test process's own environment carries; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
