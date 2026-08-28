//! Spec 62, criterion "COURIERS KEEP THE INSTANCE LIVE" (unit u62c4) - the fence-leak gap the
//! adjudicator's REJECT identified in this unit's first attempt: `refresh_registry_entry`
//! (`src/main.rs`) resolves the registry directory via `rigger::registry::default_dir()`, which
//! reads `XDG_STATE_HOME`/`HOME` DIRECTLY - entirely decoupled from the `loc`/`selection` the
//! caller already resolved through `require_store_dir`. `require_store_dir`'s own
//! `STORE_FENCE_ENV` branch (spec 70 criterion 3) exists specifically so "a fenced gate sees
//! strictly less ambient state, never more" - a unit-worktree gate's spawned test process
//! (`gate::ExecRunner::run`) pins `STORE_FENCE_ENV` around its child so an INCIDENTAL courier
//! that child's own test suite spawns (this crate's own `cargo test` runs courier-spawning
//! suites like this one) can never reach the real, machine-global state.
//!
//! Before this fix, `refresh_registry_entry` never checked the fence at all: a fenced courier
//! correctly resolved its STORE via `require_store_dir` to the pinned scratch dir, but still
//! wrote an `Instance` straight into the ambient `XDG_STATE_HOME`-derived registry - the exact
//! side channel the fence exists to close, firing on every gate run of this self-hosted project
//! (`gate_store_fence_periphery.rs` is part of the crate's own default `cargo test` gate).
//!
//! This suite proves the closed hole through the BUILT binary: a fenced courier call leaves the
//! ambient/real registry directory completely untouched, while the courier's real work (and its
//! best-effort, warn-only degrade contract) is unaffected.

use std::path::Path;
use std::process::{Command, Output};

use rigger::gate::STORE_FENCE_ENV;
use rigger::registry::{self, Instance};

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::RestoreEnvVars;

/// A throwaway project the compiled binary accepts as a courier target: its own git repo (so the
/// store's project identity resolves normally) and an INITIALIZED event log - a courier refuses
/// to fabricate one from a cwd with no existing store (spec 05).
fn courier_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temp project");
    let root = dir.path();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).expect("create .rigger");
    std::fs::File::create(rigger_dir.join("events.db")).expect("seed an initialized event log");
    dir
}

/// Every registry entry under `state_home`, decoded through `Instance`'s own (de)serialization -
/// mirrors the read helper in the sibling periphery suites (each periphery suite owns its own
/// small fixture helpers rather than sharing test-only code across files).
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

fn assert_ok(out: &Output, args: &[&str]) {
    assert!(
        out.status.success(),
        "rigger {args:?} failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}

/// THE REGRESSION: a courier invoked exactly as `gate::ExecRunner::run` invokes one of a unit-
/// worktree gate's own spawned test binaries - `STORE_FENCE_ENV` pinned to a scratch dir the
/// fence creates, `XDG_STATE_HOME` still pointing at the (simulated) real, ambient location -
/// must leave that ambient registry directory completely untouched. Exercised on all three
/// courier commands (`progress`, `emit`, `result`), mirroring the fence's own call-site scope
/// (`src/main.rs` lines ~6599/6655/8231).
#[test]
fn a_fenced_courier_never_writes_the_ambient_registry() {
    let project = courier_project();
    let root = project.path();
    // The simulated REAL, machine-global state home - what `default_dir()` would resolve to
    // absent any fence. Never pre-created: proves the fenced call does not even create it.
    let ambient_state = tempfile::tempdir().expect("a temp ambient XDG_STATE_HOME");
    // The scratch dir a gate's `ExecRunner` names for the fence; `require_store_dir`'s own fence
    // branch creates `<fence>/.rigger` on demand, so this need not exist beforehand either.
    let fence = tempfile::tempdir().expect("a temp fence scratch dir");
    let fence_rigger = fence.path().join(".rigger");

    for (args, marker) in [
        (
            vec!["progress", "u1/impl#0", "fenced step"],
            "progress recorded for u1/impl#0",
        ),
        (
            vec!["emit", "DecisionMade", r#"{"id":"d1","summary":"s"}"#],
            "",
        ),
        (vec!["result", "u1/impl#0", "fenced done"], ""),
    ] {
        let out = common::rigger_courier()
            .args(&args)
            .current_dir(root)
            // Never let a short-lived courier spawn a real dashboard under test.
            .env("RIGGER_NO_DASH", "1")
            .env("XDG_STATE_HOME", ambient_state.path())
            .env(STORE_FENCE_ENV, &fence_rigger)
            .output()
            .expect("the rigger binary runs");
        assert_ok(&out, &args);
        if !marker.is_empty() {
            assert!(
                String::from_utf8_lossy(&out.stdout).contains(marker),
                "the fenced courier's real work still completes normally: {}",
                String::from_utf8_lossy(&out.stdout)
            );
        }

        let ambient_entries = registry_entries(ambient_state.path());
        assert!(
            ambient_entries.is_empty(),
            "a fenced `{args:?}` must never write the ambient registry: {ambient_entries:?}"
        );
    }
}

/// The other direction, proven in the SAME run: an UNFENCED courier against the identical
/// project and ambient state home DOES write the registry entry - so the fenced test above is
/// proof of a real, checked no-op, not an accident of a fixture that never resolves the registry
/// at all (e.g. a homeless environment).
#[test]
fn an_unfenced_courier_against_the_same_fixture_does_write_the_ambient_registry() {
    let project = courier_project();
    let root = project.path();
    let ambient_state = tempfile::tempdir().expect("a temp ambient XDG_STATE_HOME");

    let out = common::rigger_courier()
        .args(["progress", "u1/impl#0", "unfenced step"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", ambient_state.path())
        .output()
        .expect("the rigger binary runs");
    assert_ok(&out, &["progress", "u1/impl#0", "unfenced step"]);

    let ambient_entries = registry_entries(ambient_state.path());
    assert_eq!(
        ambient_entries.len(),
        1,
        "an unfenced courier against the same fixture writes exactly one ambient entry: {ambient_entries:?}"
    );
}

/// Regression pin for `sdet-u62c4r3-kurrentdb-leak-not-blast-radius-audited` /
/// `adv-u62c4-r5-kurrentdb-leak-independently-reproduced-8of8`, closed at
/// `adj-u62c4-r6-verdict-reject-blast-radius-audit-incomplete`'s required fix (mirroring the
/// identically-purposed pins in the sibling `courier_registry_refresh_*_periphery.rs` files):
/// `common::rigger_courier()` now strips an ambient `KURRENTDB_CONN` from every child it
/// spawns, so an UNFENCED courier against this file's own fixture (which, unlike the fenced
/// test above, actually reaches `store_selection_at` since `require_store_dir`'s fence
/// short-circuit never fires) resolves the local sqlite store regardless of what this test
/// process's own environment carries.
///
/// Sets a well-formed but UNREACHABLE `KURRENTDB_CONN` on THIS test process before spawning -
/// never on the `Command` itself - so this is a genuine regression proof: pre-fix, the courier
/// below would crash with a real gRPC connect error instead of writing the ambient registry
/// entry the way `an_unfenced_courier_against_the_same_fixture_does_write_the_ambient_registry`
/// above proves it does under a clean environment.
#[test]
#[serial_test::serial(kurrentdb_conn_env)]
fn an_ambient_kurrentdb_conn_never_leaks_into_an_unfenced_courier() {
    let project = courier_project();
    let root = project.path();
    let ambient_state = tempfile::tempdir().expect("a temp ambient XDG_STATE_HOME");

    let _restore = RestoreEnvVars::capture(&["KURRENTDB_CONN"]);
    std::env::set_var("KURRENTDB_CONN", "kurrentdb://127.0.0.1:1/");

    let out = common::rigger_courier()
        .args(["progress", "u1/impl#0", "unfenced step"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", ambient_state.path())
        .output()
        .expect("the rigger binary runs");
    assert!(
        out.status.success(),
        "an unfenced courier spawned through the shared rigger_courier() helper must resolve \
         the fixture's local sqlite store, not attempt a real gRPC connection to whatever \
         KURRENTDB_CONN this test process's own environment carries; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let ambient_entries = registry_entries(ambient_state.path());
    assert_eq!(
        ambient_entries.len(),
        1,
        "the courier's real work (including its registry refresh) still completes normally \
         once the ambient KURRENTDB_CONN is stripped: {ambient_entries:?}"
    );
}
