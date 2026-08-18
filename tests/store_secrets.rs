//! Spec 48, criterion 3 - SECRETS DISCIPLINE, driven through the BUILT `rigger` binary.
//!
//! This file owns the SECRET CHANNEL: the per-machine connection-string file `.rigger/store.conn`
//! (store resolver rung 3) and the guarantee that a connection string's CREDENTIALS never reach an
//! output path. It proves two things a worker's bare self-report actually observes in the shipped
//! binary:
//!
//!   * RESOLUTION FROM THE SECRET FILE - with no `--conn` flag and no `KURRENTDB_CONN`, the store
//!     resolver reads the connection string from `.rigger/store.conn` and selects the server it
//!     addresses (observed at the sqlite-vs-server boundary: the eager connect to an unreachable
//!     address fails fast with `kurrentdb` on stderr and no local `.rigger/events.db` is
//!     fabricated). It does NOT assert resolution ORDER - that total precedence is owned by
//!     criterion 2 (`store_precedence.rs`); here the secret file is the sole configured source.
//!   * REDACTION - any error that echoes the connection string scrubs the `user:password@`
//!     userinfo (the [`redact_conn`](rigger::eventstore::redact_conn) authority): the scheme and
//!     host still print (they name WHICH server failed, which is useful), but the credential is
//!     replaced by `<redacted>` and never survives to stderr. Proven for BOTH secret channels a
//!     credential rides - the environment and the secret file.
//!
//! The `.gitignore` clause of criterion 3 (setup ignores `.rigger/store.conn` by construction) is
//! pinned by the `init_project_gitignores_the_store_conn_secret_file_idempotently` unit test in
//! `src/main.rs`, at the scaffold function it directly exercises.
//!
//! It needs no server: an unreachable-but-well-formed loopback address makes the SERVER selection
//! externally visible via the fail-fast connect, so every case runs unconditionally and the secret
//! channel is regression-locked on every machine.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

/// A well-formed but unreachable server address CARRYING CREDENTIALS: nothing listens on this
/// loopback port, so the eager connect (fail-fast) is refused immediately, and the userinfo
/// (`spy:hunter2`) is the credential that must NEVER survive to any output path.
const CREDENTIALED_UNREACHABLE: &str = "kurrentdb://spy:hunter2@127.0.0.1:65533?tls=false";
/// The username half of the credential in [`CREDENTIALED_UNREACHABLE`].
const SECRET_USER: &str = "spy";
/// The password half of the credential in [`CREDENTIALED_UNREACHABLE`].
const SECRET_PASSWORD: &str = "hunter2";

/// A throwaway project: its own git repo (so identity - and the owning-root anchor the store
/// resolver uses for the secret file - resolve exactly as a real project's do), with an empty
/// `.rigger/` and no event log yet.
fn empty_project() -> TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    std::fs::create_dir_all(dir.path().join(".rigger")).expect("create .rigger");
    dir
}

/// The path where the embedded sqlite EVENT LOG would live for a project rooted at `root`. A
/// server selection - from any secret channel - must never fabricate this file.
fn local_event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

/// Write `<root>/.rigger/store.conn` (rung 3, the per-machine secret file), one line: the full
/// connection string, credentials included. Locked to owner-only (`0o600`) on Unix so the
/// permission-hygiene warning stays OUT of the resolution/redaction cases (it has its own test);
/// the default umask would otherwise make it group/other-readable and emit the nudge.
fn write_store_conn(root: &Path, conn: &str) {
    let path = root.join(".rigger").join("store.conn");
    std::fs::write(&path, conn).expect("write store.conn");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("lock store.conn to owner-only");
    }
}

/// Run `rigger result <id> --error <msg>` in `root` - the exact courier surface a worker's bare
/// self-report uses. `conn` sets `KURRENTDB_CONN` (`None` removes it so the case is truly unset
/// regardless of the ambient environment). `RIGGER_NO_DASH` keeps the dashboard from starting.
fn run_bare_result(root: &Path, conn: Option<&str>) -> Output {
    let mut cmd = common::rigger_courier();
    cmd.args(["result", "u/impl#0", "--error", "a self-report"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env_remove("KURRENTDB_CONN");
    if let Some(c) = conn {
        cmd.env("KURRENTDB_CONN", c);
    }
    cmd.output().expect("spawn rigger result")
}

/// Assert the courier reached the SERVER backend (so the connection string it carries WAS surfaced
/// on the failing path) yet leaked NO credential: the userinfo never prints, the scheme+host do,
/// and the redaction marker shows the credential was deliberately scrubbed rather than merely
/// absent. `stderr` is the courier's captured standard error; `why` names the channel under test.
fn assert_server_reached_and_credentials_redacted(out: &Output, root: &Path, why: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "{why}: an unreachable server must fail, not silently succeed: {stderr}"
    );
    // The server backend was constructed and its eager connect failed - proving the connection
    // string reached the backend and its error path, the only place it could leak.
    assert!(
        stderr.contains("kurrentdb"),
        "{why}: the failure must arise inside the SERVER backend (so the conn WAS on the failing \
         path): {stderr}"
    );
    // THE SECURITY PROPERTY: neither half of the credential survives to stderr.
    assert!(
        !stderr.contains(SECRET_PASSWORD),
        "{why}: the password must NEVER print on any output path: {stderr}"
    );
    assert!(
        !stderr.contains(SECRET_USER),
        "{why}: the username must NEVER print on any output path: {stderr}"
    );
    // The redaction is deliberate and visible (marker present) and the scheme+host still name WHICH
    // server failed - redaction scrubs the credential, it does not blank the whole address.
    assert!(
        stderr.contains("<redacted>"),
        "{why}: a scrubbed connection string must show the redaction marker, proving the conn was \
         named-then-redacted (not merely absent from an error that never echoed it): {stderr}"
    );
    assert!(
        stderr.contains("127.0.0.1"),
        "{why}: the host must still print - redaction removes only the userinfo: {stderr}"
    );
    // A server selection never fabricates the local sqlite event log.
    assert!(
        !local_event_log(root).exists(),
        "{why}: a server-configured courier must NOT create a local .rigger/events.db: {stderr}"
    );
}

#[test]
fn a_credentialed_conn_in_the_environment_is_redacted_in_the_error() {
    let project = empty_project();
    let root = project.path();
    let out = run_bare_result(root, Some(CREDENTIALED_UNREACHABLE));
    assert_server_reached_and_credentials_redacted(&out, root, "KURRENTDB_CONN env channel");
}

#[test]
fn the_secret_file_resolves_the_connection_string_when_env_and_flags_are_absent() {
    let project = empty_project();
    let root = project.path();
    // No --conn flag and no KURRENTDB_CONN: the ONLY configured source is the secret file. The
    // resolver must read it (rung 3) and select the server it addresses, and the credential it
    // carries must be redacted on the failing path exactly as the environment channel's is.
    write_store_conn(root, CREDENTIALED_UNREACHABLE);
    let out = run_bare_result(root, None);
    assert_server_reached_and_credentials_redacted(
        &out,
        root,
        ".rigger/store.conn secret-file channel",
    );
}

/// The permission-hygiene rung of SECRETS DISCIPLINE, end to end: a secret file exposed to other
/// users (group/world-readable) draws a warning on the resolving command's stderr, and an
/// owner-only file draws none. The warning nudges `chmod 600` without ever printing the credential
/// or blocking resolution. Unix-only - the mode bits are a POSIX concept.
#[cfg(unix)]
#[test]
fn an_exposed_secret_file_warns_and_an_owner_only_one_is_silent() {
    use std::os::unix::fs::PermissionsExt;

    // A file the store connection can safely address: an owner-only, group/world-readable, etc. The
    // conn is unreachable so the command fails after the warning, which is irrelevant to the nudge.
    let write_conn_mode = |root: &Path, mode: u32| {
        let path = root.join(".rigger").join("store.conn");
        std::fs::write(&path, CREDENTIALED_UNREACHABLE).expect("write store.conn");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
            .expect("set store.conn mode");
    };

    // Group/world-readable (0o644): the credential is exposed, so the resolver warns.
    let exposed = empty_project();
    write_conn_mode(exposed.path(), 0o644);
    let out = run_bare_result(exposed.path(), None);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("readable by other users") && stderr.contains("chmod 600"),
        "an exposed secret file must draw the permission-hygiene warning: {stderr}"
    );
    // The warning must never echo the credential it is warning about.
    assert!(
        !stderr.contains(SECRET_PASSWORD) && !stderr.contains(SECRET_USER),
        "the permission warning must not print the credential itself: {stderr}"
    );

    // Owner-only (0o600): nothing is exposed, so no nudge appears.
    let locked = empty_project();
    write_conn_mode(locked.path(), 0o600);
    let out = run_bare_result(locked.path(), None);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("readable by other users"),
        "an owner-only secret file must draw NO permission warning: {stderr}"
    );
}
