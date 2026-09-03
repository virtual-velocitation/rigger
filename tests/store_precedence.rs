//! Spec 48, criterion 2 - PRECEDENCE, driven through the BUILT `rigger` binary.
//!
//! `src/main.rs`'s unit test `store_selection_precedence_flag_env_secret_file_config_then_default`
//! proves the total order exhaustively over the pure core (including server-vs-server ordering by
//! exact address). This file is the periphery layer that proves the two NEW file-backed rungs the
//! precedence criterion adds - the local secret file `.rigger/store.conn` and the committed project
//! config's `store:` selection - are genuinely WIRED into the shipped binary a worker's bare
//! self-report runs in, and that they sit at the right place in the order relative to the higher
//! rungs.
//!
//! It needs no server. Selection is observed at the sqlite-vs-server boundary, which the shipped
//! binary makes externally visible without one:
//!
//!   * a SERVER selection reaches the kurrentdb backend, whose eager connect to an unreachable
//!     address fails fast - the process exits non-zero with `kurrentdb` on stderr and never
//!     fabricates a local `.rigger/events.db`;
//!   * a SQLITE selection takes the local walk-up, which on a never-initialized project refuses
//!     with `no rigger store found` and never mentions `kurrentdb`.
//!
//! It also pins the secret rung's sentinel split: a PRESENT-but-unreadable `.rigger/store.conn`
//! (distinct from an ABSENT one, which is "no opinion") surfaces LOUDLY at the secret-file rung and
//! never collapses into the silent sqlite fallback an absent file gives - the wrong-store fracture
//! guard the config rung already carries, mirrored here for its sibling.
//!
//! Each case runs unconditionally, so the two new rungs and their ordering are regression-locked
//! on every machine. Redaction of the connection string, the `.gitignore` pattern, and verbatim
//! pass-through are owned by their own criteria and are not asserted here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

/// An unreachable but well-formed server address: nothing listens on this loopback port, so the
/// eager connect (fail-fast) is refused immediately. We prove WHICH backend the authority selected,
/// not that a server is up.
const UNREACHABLE: &str = "kurrentdb://127.0.0.1:65533?tls=false";

/// A throwaway project: its own git repo (so identity - and the owning-root anchor the store
/// resolver uses for the config and secret file - resolve exactly as a real project's do), with an
/// empty `.rigger/` and no event log yet.
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
/// precedence rungs must never fabricate this when a server is selected.
fn local_event_log(root: &Path) -> PathBuf {
    root.join(".rigger").join("events.db")
}

/// Write `<root>/.rigger/store.conn` (rung 3, the per-machine secret file).
fn write_store_conn(root: &Path, conn: &str) {
    std::fs::write(root.join(".rigger").join("store.conn"), conn).expect("write store.conn");
}

/// Place a DIRECTORY where `<root>/.rigger/store.conn` should be a file, so reading it as the secret
/// file yields an IO error DISTINCT from NotFound - the present-but-unreadable class. This mirrors
/// the config rung's parity test exactly and is more portable than `chmod 000`, which a test running
/// as root would bypass (root reads any file), silently defeating the guard under test.
fn make_store_conn_unreadable(root: &Path) {
    std::fs::create_dir(root.join(".rigger").join("store.conn"))
        .expect("place a directory where store.conn goes");
}

/// Write `<root>/.rigger/workflow.yml` pinning `store:` (rung 4, the committed project config).
fn write_store_config(root: &Path, body: &str) {
    std::fs::write(root.join(".rigger").join("workflow.yml"), body).expect("write workflow.yml");
}

/// Run `rigger result <id> --error <msg>` in `root` with NO `--eventstore` flag and NO
/// `KURRENTDB_CONN` in the environment - the exact bare-courier surface a worker's self-report
/// uses, so the store is resolved purely from the file-backed rungs under test. `RIGGER_NO_DASH`
/// keeps the run's dashboard from starting under test. `XDG_STATE_HOME` is redirected to a
/// per-call temp dir (spec 62, "couriers count as activity"): `result` now refreshes the
/// machine-global instance registry too, so an unredirected call here would otherwise seed a
/// phantom, since-deleted-tempdir entry into the operator's real
/// `~/.local/state/rigger/instances`.
fn run_bare_courier(root: &Path) -> Output {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    common::rigger_courier()
        .args(["result", "u/impl#0", "--error", "a self-report"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .env_remove("KURRENTDB_CONN")
        .output()
        .expect("spawn rigger result")
}

/// Run the same bare courier but WITH `KURRENTDB_CONN` set (rung 2), to prove the environment
/// out-ranks a lower file-backed rung. `XDG_STATE_HOME` redirected for the same reason as
/// [`run_bare_courier`].
fn run_courier_with_env(root: &Path, conn: &str) -> Output {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    common::rigger_courier()
        .args(["result", "u/impl#0", "--error", "a self-report"])
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .env("KURRENTDB_CONN", conn)
        .output()
        .expect("spawn rigger result")
}

/// Assert the courier resolved the SERVER backend: it failed inside the kurrentdb adapter (the
/// eager connect to the unreachable address) and fabricated no local sqlite event log.
fn assert_selected_server(out: &Output, root: &Path, why: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "{why}: an unreachable server must fail, never silently succeed against a local fallback; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("kurrentdb"),
        "{why}: the courier must fail INSIDE the server backend, proving it resolved the server; \
         stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("no rigger store found"),
        "{why}: a server selection must not fall back to the local sqlite walk-up; stderr:\n{stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "{why}: a server selection must NOT fabricate a local .rigger/events.db"
    );
}

/// Assert the courier resolved the SQLITE backend: it took the local walk-up, which on a
/// never-initialized project refuses without reaching for any server.
fn assert_selected_sqlite(out: &Output, root: &Path, why: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "{why}: a courier with no initialized local store must fail, not fabricate one; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("no rigger store found") && !stderr.contains("kurrentdb"),
        "{why}: a sqlite selection must resolve the LOCAL log (surfacing as a local-store error), \
         never a server connect; stderr:\n{stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "{why}: the refuse-to-fabricate guard must leave no local events.db behind"
    );
}

#[test]
fn the_committed_store_config_selects_the_server_over_the_default() {
    // Rung 4 wired + rung 4 beats rung 5: the committed config alone pins the server (with its
    // non-secret URL), no env, no flag, no secret file -> the bare courier resolves the server.
    let project = empty_project();
    let root = project.path();
    write_store_config(
        root,
        &format!("store:\n  backend: kurrentdb\n  url: \"{UNREACHABLE}\"\n"),
    );
    let out = run_bare_courier(root);
    assert_selected_server(&out, root, "committed store: config selects the server");
}

#[test]
fn a_committed_sqlite_store_config_resolves_the_local_log() {
    // Rung 4 can also pin sqlite explicitly: the bare courier takes the local walk-up.
    let project = empty_project();
    let root = project.path();
    write_store_config(root, "store:\n  backend: sqlite\n");
    let out = run_bare_courier(root);
    assert_selected_sqlite(&out, root, "committed store: sqlite resolves the local log");
}

#[test]
fn the_secret_file_selects_the_server_and_beats_the_committed_config() {
    // Rung 3 wired + rung 3 beats rung 4: the secret file names the server while the committed
    // config pins sqlite; the secret file must win, so the courier resolves the server.
    let project = empty_project();
    let root = project.path();
    write_store_conn(root, UNREACHABLE);
    write_store_config(root, "store:\n  backend: sqlite\n");
    let out = run_bare_courier(root);
    assert_selected_server(
        &out,
        root,
        ".rigger/store.conn beats the committed store: config",
    );
}

#[test]
fn the_environment_beats_the_committed_config() {
    // Rung 2 beats rung 4: KURRENTDB_CONN names the server while the committed config pins sqlite;
    // the environment must win, so the courier resolves the server.
    let project = empty_project();
    let root = project.path();
    write_store_config(root, "store:\n  backend: sqlite\n");
    let out = run_courier_with_env(root, UNREACHABLE);
    assert_selected_server(
        &out,
        root,
        "KURRENTDB_CONN beats the committed store: config",
    );
}

#[test]
fn a_present_but_unreadable_store_conn_surfaces_loudly_not_a_silent_sqlite_fallback() {
    // Rung 3, the sentinel-inversion guard: a `.rigger/store.conn` that is PRESENT but cannot be
    // read (here a directory stands where the file goes - an IO error distinct from NotFound) must
    // surface LOUDLY, never collapse into the same silent sqlite default an ABSENT file returns.
    // Otherwise a bare courier on a server-pinning box whose secret file is unreadable (a
    // different-user / permission edge the design explicitly contemplates) would silently self-
    // report to LOCAL sqlite while the conductor uses the server - the run's state fractures across
    // two stores (the spec-05 wrong-store fracture). This is the secret-file sibling of the config
    // rung's `a_present_but_unreadable_workflow...` test (d-u2-conn-file-unreadable-loud); it pins
    // that the file-backed secret rung distinguishes absent (no opinion) from unreadable (loud), the
    // same NotFound-vs-other split `read_store_config` makes one rung down.
    let project = empty_project();
    let root = project.path();
    make_store_conn_unreadable(root);
    let out = run_bare_courier(root);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "an unreadable store.conn must fail loudly, never silently resolve a fallback; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("store connection file"),
        "the failure must name itself as a store-connection-file read error, proving it surfaced at \
         the secret-file rung rather than a downstream store miss; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("no rigger store found"),
        "an unreadable secret file must NOT fall through to the local sqlite walk-up - that silent \
         wrong-store fallback is the exact fracture this rung guards; stderr:\n{stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "a loud read failure must leave no fabricated local .rigger/events.db behind"
    );
}

#[test]
fn nothing_configured_resolves_the_local_default() {
    // Rung 5: no flag, no env, no secret file, no committed store: - the backward-compatible
    // embedded-sqlite default.
    let project = empty_project();
    let root = project.path();
    let out = run_bare_courier(root);
    assert_selected_sqlite(
        &out,
        root,
        "no source configured resolves the sqlite default",
    );
}

#[test]
fn an_unknown_committed_backend_surfaces_loudly_not_a_silent_sqlite_fallback() {
    // Rung 4, the backend-validation guard (d-u2sdet-error-boundary-gaps): a committed
    // `store: backend:` naming neither `sqlite` nor `kurrentdb` - a typo like `kurrent`, `postgres`,
    // or here `bogus` - must surface as a LOUD configuration error naming the offending value and the
    // accepted ones, never a silent fall-through to the embedded-sqlite default that would hide the
    // typo behind today's behavior. Otherwise a project that MEANT to pin the server, but fat-fingered
    // the backend name, would self-report to LOCAL sqlite while believing it configured the shared
    // server - the spec-05 wrong-store fracture reached through a config typo instead of a missing
    // credential. `src/main.rs`'s unit test pins this over the pure core (`store: backend: bogus`
    // rejected); this proves the rejection is WIRED through the shipped binary and reaches the bare
    // courier's stderr, never swallowed between the resolver and the command handler. The sibling
    // guards `a_present_but_unreadable_store_conn...` and (config rung) `a_malformed_workflow...`
    // cover the OTHER two silent-fallback classes; this closes the invalid-value one.
    let project = empty_project();
    let root = project.path();
    write_store_config(root, "store:\n  backend: bogus\n");
    let out = run_bare_courier(root);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "an unknown store.backend must fail loudly, never silently resolve a fallback; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("store.backend") && stderr.contains("bogus"),
        "the failure must name the store.backend config rung and the offending value, proving it \
         surfaced at the committed-config rung's validation rather than a downstream store miss; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("sqlite") && stderr.contains("kurrentdb"),
        "the loud error must name the two accepted backends so the fix is unambiguous; \
         stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("no rigger store found"),
        "an invalid backend must NOT fall through to the local sqlite walk-up - that silent \
         wrong-store fallback behind a typo is the exact fracture this rung guards; stderr:\n{stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "a loud config-validation failure must leave no fabricated local .rigger/events.db behind"
    );
}

#[test]
fn a_committed_kurrentdb_backend_with_no_credential_names_all_three_sources() {
    // Rung 4 selects the server but carries NO credential (d-u2sdet-error-boundary-gaps): a committed
    // `store: backend: kurrentdb` with no `url`, and nothing beneath OR above it to supply an address
    // - no `--conn` flag, no `KURRENTDB_CONN`, no `.rigger/store.conn` - must surface the three-source
    // missing-connection error naming ALL THREE credential channels, never a silent drop to the local
    // sqlite default. The committed config deliberately holds only the CHOICE, never a secret, so the
    // address must come from a credential source; when every source is empty the operator needs the
    // full remediation, not a wrong-store fracture where the run silently resolves LOCAL sqlite while
    // the config pins the shared server. `tests/cli.rs` drives the FLAG path (`--eventstore kurrentdb`
    // no url) to this same guard and `src/main.rs`'s unit test pins the CONFIG path over the pure
    // core; neither observes the CONFIG-rung server-with-no-url arm end-to-end through the binary.
    // This closes that: the committed-config server selection reaches the missing-conn guard, wired
    // through the shipped binary, and fabricates no local log.
    let project = empty_project();
    let root = project.path();
    write_store_config(root, "store:\n  backend: kurrentdb\n");
    let out = run_bare_courier(root);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a config-selected server with no resolvable connection string must fail loudly, never \
         silently resolve the local sqlite default; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--conn")
            && stderr.contains("KURRENTDB_CONN")
            && stderr.contains("store.conn"),
        "the missing-connection error must name all three credential channels so the fix is \
         unambiguous; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("no rigger store found"),
        "a config-selected server with no credential must NOT fall through to the local sqlite \
         walk-up - that silent wrong-store fallback is the exact fracture this rung guards; \
         stderr:\n{stderr}"
    );
    assert!(
        !local_event_log(root).exists(),
        "a loud missing-connection failure must leave no fabricated local .rigger/events.db behind"
    );
}
