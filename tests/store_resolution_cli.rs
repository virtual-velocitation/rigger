//! Spec 48, criterion 1 - the SINGLE AUTHORITY, proven at the CLI boundary and ALWAYS on.
//!
//! `tests/store_resolution.rs` pins the authority two ways: structurally (the sqlite event-log
//! constructor at exactly one call site) and at runtime against a REACHABLE server booted in a
//! container. That runtime proof is the happy path, and it is skipped whenever no container
//! runtime is reachable - so on a machine without one, nothing drives the wiring end to end
//! through the shipped binary.
//!
//! This file is the periphery layer that closes that gap. It drives the BUILT `rigger` binary -
//! the exact process a worker's bare self-report runs in - and pins WHICH backend the single
//! authority selects, observably, from OUTSIDE the process and WITHOUT any server:
//!
//!   * with the server configured (`KURRENTDB_CONN` set), a bare courier resolves the SERVER
//!     backend and NEVER fabricates a local sqlite event log - the state-fracture stays closed
//!     even when the server is unreachable (the eager connect fails fast; no local `events.db`
//!     is left behind);
//!   * with nothing configured, the same courier resolves the LOCAL sqlite log;
//!   * an empty `KURRENTDB_CONN=` is treated as unset, so a stray empty value never selects a
//!     server with no address.
//!
//! Every case runs unconditionally, so the single-authority wiring is regression-locked on every
//! machine, container runtime or not. These cases are the periphery of the same criterion: the
//! secret channel, the full precedence, verbatim pass-through, and the projection boundary are
//! owned by their own criteria and are not asserted here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// The compiled `rigger` binary under test (Cargo sets this for integration tests).
fn rigger_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rigger")
}

/// A throwaway project: its own git repo (so identity resolves exactly as a real project's does)
/// with an empty `.rigger/` and no event log yet. The `TempDir` is returned so it outlives the
/// command and is removed on drop.
fn empty_project() -> TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    std::fs::create_dir_all(dir.path().join(".rigger")).expect("create .rigger");
    dir
}

/// The path where the embedded sqlite EVENT LOG would live for a project rooted at `root`. The
/// single-authority guarantee is that a server-configured courier never fabricates this file.
fn local_event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

/// Run `rigger result <id> --error <msg>` in `root` - the exact courier surface a worker's bare
/// self-report uses, and the one whose store the single authority must keep aligned with the
/// run's. `conn` sets `KURRENTDB_CONN` (`Some("")` sets it empty; `None` removes it so the case
/// is truly unset regardless of the ambient environment). `RIGGER_NO_DASH` keeps the run's
/// dashboard from starting under test.
fn run_bare_result(root: &Path, conn: Option<&str>) -> Output {
    let mut cmd = Command::new(rigger_bin());
    cmd.args(["result", "u/impl#0", "--error", "a self-report"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env_remove("KURRENTDB_CONN");
    if let Some(c) = conn {
        cmd.env("KURRENTDB_CONN", c);
    }
    cmd.output().expect("spawn rigger result")
}

#[test]
fn a_server_selected_courier_reaches_the_server_and_never_fabricates_local_sqlite() {
    let project = empty_project();
    let root = project.path();

    // A well-formed but unreachable server address: nothing listens on this loopback port, so the
    // eager connect (§8, fail-fast) is refused immediately. We are proving WHICH backend the
    // single authority selected, not that a server is up - the container test owns the
    // reachable-server proof.
    let out = run_bare_result(root, Some("kurrentdb://127.0.0.1:65533?tls=false"));
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "an unreachable server must fail, never silently succeed against a local fallback: {stderr}"
    );
    // The error carries the server backend's name because `resolve_store` constructed the server
    // store (which then failed to connect) - the courier resolved the backend the single
    // authority selected from `KURRENTDB_CONN`, not the local sqlite log.
    assert!(
        stderr.contains("kurrentdb"),
        "a server-configured courier must fail inside the SERVER backend (proving it resolved the \
         server the single authority selected), got: {stderr}"
    );
    assert!(
        !stderr.contains("no rigger store found"),
        "a server-configured courier must NOT fall back to the local sqlite walk-up: {stderr}"
    );
    // The event log is the SERVER's, so no local sqlite event log is fabricated - the
    // state-fracture stays closed even with the server unreachable.
    assert!(
        !local_event_log(root).exists(),
        "a server-configured courier must NOT create a local .rigger/events.db - that is the \
         state-fracture this criterion closes, and it must hold even when the server is down"
    );
}

#[test]
fn a_courier_with_no_server_configured_resolves_the_local_sqlite_log() {
    let project = empty_project();
    let root = project.path();

    let out = run_bare_result(root, None);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // With nothing configured, the single authority defaults to the embedded sqlite log, so the
    // courier takes the LOCAL walk-up - and, finding no initialized store, refuses to fabricate
    // one (the store-locator's own guard) rather than reaching for a server.
    assert!(
        !out.status.success(),
        "a courier with no initialized store must fail, not fabricate one: {stderr}"
    );
    assert!(
        stderr.contains("no rigger store found") && stderr.contains("events.db"),
        "an unconfigured courier must resolve the LOCAL sqlite log - its absence surfaces as a \
         local-store error, not a server connect: {stderr}"
    );
    assert!(
        !stderr.contains("kurrentdb"),
        "an unconfigured courier must not reach for a server backend: {stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "the refuse-to-fabricate guard must leave no local events.db behind: {stderr}"
    );
}

#[test]
fn an_empty_kurrentdb_conn_is_treated_as_unset_not_a_server_with_no_address() {
    let project = empty_project();
    let root = project.path();

    // A stray empty `KURRENTDB_CONN=` (e.g. an unset shell variable expanded to nothing) must NOT
    // select the server with no address - the single authority treats it as unset and defaults to
    // local sqlite, exactly as if the variable were absent.
    let out = run_bare_result(root, Some(""));
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "an empty KURRENTDB_CONN must resolve local sqlite and then refuse (no store), never error \
         on a missing server address: {stderr}"
    );
    assert!(
        stderr.contains("no rigger store found") && !stderr.contains("kurrentdb"),
        "an empty KURRENTDB_CONN must resolve the LOCAL sqlite log, identically to an unset one: \
         {stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "an empty-conn courier resolves local sqlite and fabricates nothing: {stderr}"
    );
}
