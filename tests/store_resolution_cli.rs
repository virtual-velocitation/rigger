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
//! The cases above drive the WRITE surface (a courier's self-report). The single authority must
//! govern the READ surface just as uniformly, and there the wiring hides a second regression: a
//! read command that reports on the log short-circuits to an "empty log" sentinel when the LOCAL
//! sqlite file is absent (a never-run project), guarded by `sel.is_sqlite() && !events.db exists`.
//! The `sel.is_sqlite()` qualifier is load-bearing: for a server selection the local file is
//! ALWAYS absent, so dropping it would make every server-backed read short-circuit to that
//! sentinel - silently reporting an empty log against a live server. This file also pins that
//! qualifier, driving read commands (`prime`, `stats`) both ways over the SAME never-run project:
//!
//!   * UNCONFIGURED (sqlite default): the guard fires, the command prints its no-data sentinel
//!     and exits 0 - the control proving the sentinel path IS taken on the sqlite arm;
//!   * SERVER-configured (`KURRENTDB_CONN` set, unreachable): `sel.is_sqlite()` is false, so the
//!     command resolves the SERVER through the one authority and the eager connect fails - it
//!     errors inside the server backend and NEVER prints the local-absent sentinel.
//!
//! Every case runs unconditionally, so the single-authority wiring is regression-locked on every
//! machine, container runtime or not. These cases are the periphery of the same criterion: the
//! secret channel, the full precedence, verbatim pass-through, and the projection boundary are
//! owned by their own criteria and are not asserted here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

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
/// dashboard from starting under test. `XDG_STATE_HOME` is redirected to a per-call temp dir
/// (spec 62, "couriers count as activity"): `result` now refreshes the machine-global instance
/// registry too, so an unredirected call here would otherwise seed a phantom, since-deleted-
/// tempdir entry into the operator's real `~/.local/state/rigger/instances`.
fn run_bare_result(root: &Path, conn: Option<&str>) -> Output {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    let mut cmd = common::rigger_courier();
    cmd.args(["result", "u/impl#0", "--error", "a self-report"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
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

/// Run a READ-only command (`args`, e.g. `["prime"]`) in `root`, driving the built binary
/// exactly as an operator would. `conn` sets `KURRENTDB_CONN` (`None` removes it so the case is
/// truly unset regardless of the ambient environment); `RIGGER_NO_DASH` keeps the run's dashboard
/// from starting under test.
fn run_read(root: &Path, args: &[&str], conn: Option<&str>) -> Output {
    let mut cmd = common::rigger_courier();
    cmd.args(args)
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env_remove("KURRENTDB_CONN");
    if let Some(c) = conn {
        cmd.env("KURRENTDB_CONN", c);
    }
    cmd.output().expect("spawn rigger read command")
}

/// The single-authority discrimination for a READ command whose empty-log path is guarded by the
/// absent-local-db sentinel `sel.is_sqlite() && !events.db exists`. Drive `args` two ways over the
/// SAME never-run project (no local event log) and prove the store SELECTION - not the local
/// file's absence - decides the outcome:
///
///   * UNCONFIGURED (sqlite default): the guard fires, the command prints `sentinel` and exits 0.
///     This CONTROL proves the sentinel path is genuinely taken on the sqlite arm, so the server
///     difference below is attributable to the store SELECTION and nothing else.
///   * SERVER-configured (`KURRENTDB_CONN` set, unreachable): `sel.is_sqlite()` is false so the
///     guard must NOT fire; the command resolves the SERVER through the one authority and the
///     eager connect fails - so it errors inside the server backend and NEVER prints `sentinel`.
///
/// A regression dropping the `sel.is_sqlite()` qualifier reverts to pre-48 behavior: the server
/// case short-circuits to the empty sentinel and exits 0, silently reporting an empty log against
/// a configured server. The server-arm assertions below redden exactly then.
fn a_read_command_resolves_the_configured_store_not_the_local_absent_sentinel(
    args: &[&str],
    sentinel: &str,
) {
    let cmdline = args.join(" ");

    // CONTROL: nothing configured -> the single authority defaults to the LOCAL sqlite log, whose
    // file is absent on a never-run project, so the guard fires and the command reports empty.
    let project = empty_project();
    let root = project.path();
    let out = run_read(root, args, None);
    let ctrl_stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "unconfigured `rigger {cmdline}` on a never-run project must print its no-data sentinel \
         and exit 0; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        ctrl_stdout.contains(sentinel),
        "unconfigured `rigger {cmdline}` must print its local no-data sentinel {sentinel:?} - the \
         control proving the sqlite arm takes the absent-db guard; stdout:\n{ctrl_stdout}"
    );
    assert!(
        !local_event_log(root).exists(),
        "a read command must never fabricate a local events.db (control arm)"
    );

    // SERVER-configured, unreachable (nothing listens on this loopback port, so the eager connect
    // is refused fast): the single authority selects the server, the guard must NOT fire, and the
    // read resolves - and fails inside - the SERVER backend, never the local-absent sentinel.
    let project = empty_project();
    let root = project.path();
    let out = run_read(root, args, Some("kurrentdb://127.0.0.1:65533?tls=false"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a server-configured `rigger {cmdline}` must resolve the SERVER (unreachable) and FAIL, \
         never short-circuit to the local-absent sentinel and exit 0; stdout:\n{stdout}\n\
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("kurrentdb"),
        "a server-configured `rigger {cmdline}` must fail INSIDE the server backend (proving it \
         resolved the server the single authority selected, not the local sqlite log); \
         stderr:\n{stderr}"
    );
    assert!(
        !stdout.contains(sentinel) && !stderr.contains(sentinel),
        "a server-configured `rigger {cmdline}` must NOT emit the local-absent sentinel \
         {sentinel:?} - doing so is the dropped-`is_sqlite()` regression reporting an empty log \
         against a live server; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "a server-configured read must not fabricate a local events.db either"
    );
}

#[test]
fn prime_resolves_the_configured_server_never_the_local_absent_sentinel() {
    // `rigger prime` (cmd_prime) guards `selection.is_sqlite() && !events.db exists` before its
    // `read_all`, printing "no decisions recorded yet" on the sqlite arm.
    a_read_command_resolves_the_configured_store_not_the_local_absent_sentinel(
        &["prime"],
        "no decisions recorded yet",
    );
}

#[test]
fn stats_resolves_the_configured_server_never_the_local_absent_sentinel() {
    // `rigger stats` (cmd_stats -> stats_lines) guards `sel.is_sqlite() && !events.db exists`
    // before its namespace-scoped run-stream read, printing "no runs recorded yet" on the sqlite
    // arm. A second command through a distinct code path (the `stats_lines` helper, not cmd_prime's
    // inline guard) so the sentinel class is proven, not a single site.
    a_read_command_resolves_the_configured_store_not_the_local_absent_sentinel(
        &["stats"],
        "no runs recorded yet",
    );
}
