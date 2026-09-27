//! Spec 48, criterion 2 (rung 1, the per-command FLAG) - the reshaped highest-precedence rung of
//! the store-selection order, driven through the BUILT `rigger` binary.
//!
//! `src/main.rs`'s unit test `store_selection_precedence_flag_env_secret_file_config_then_default`
//! proves the total order exhaustively over the pure core, INCLUDING the rung-1 flag arm: a bare
//! `--conn <url>` (no `--eventstore`) selects the server, outranks the environment/secret/config
//! beneath it, and is never dropped to a lower rung. `tests/store_precedence.rs` drives the two
//! file-backed rungs (the secret file and the committed config) through the shipped binary, and
//! `tests/cli.rs` drives `rigger run --eventstore kurrentdb` (with no url) to the adapter's
//! missing-connection guard. Neither drives a bare `--conn <url>` through the binary, so the
//! rung-1 flag - the HIGHEST-precedence source and the one the store-fracture footgun lived in - is
//! the one boundary the periphery layer never observed end-to-end.
//!
//! This file closes that gap. A bare `--conn <url>` must reach `store_selection` from the parsed
//! `rigger run` flags and SELECT the server it addresses - never silently drop to the local sqlite
//! default (the exact footgun: `rigger run --conn kurrentdb://prod <spec>` resolving a LOCAL
//! `.rigger/events.db` and misfiling the run off the store the operator named). Selection is
//! observed at the sqlite-vs-server boundary the shipped binary makes externally visible with no
//! server:
//!
//!   * a SERVER selection reaches the kurrentdb backend, whose eager connect to an unreachable
//!     address fails fast - the process exits non-zero with `kurrentdb` on stderr and never
//!     fabricates a local `.rigger/events.db`;
//!   * a SQLITE selection (the pre-fix drop, and what a wrong wiring would produce) instead takes
//!     the local walk-up, so its ABSENCE - no `no rigger store found`, no local log - is what pins
//!     the server selection.
//!
//! Each case runs unconditionally, so the reshaped rung-1 flag and its place at the top of the
//! order are regression-locked on every machine and on both feature lanes.

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;

use common::cli::event_log;
use common::cli::write_workflow;
use common::rigger_bin;

/// An unreachable but well-formed server address: nothing listens on this loopback port, so the
/// eager connect (fail-fast) is refused immediately. We prove WHICH backend the flag selected, not
/// that a server is up.
const UNREACHABLE: &str = "kurrentdb://127.0.0.1:65533?tls=false";

/// A throwaway project with ONE commit, so `rigger run --base HEAD` clears its base/anchor gates
/// and reaches the store-selection seam (a bare `git init` with no commit has no `HEAD` to anchor
/// against). Its own git repo makes identity and the owning-root anchor resolve exactly as a real
/// project's do. The `.rigger/` is created but carries no event log yet.
fn committed_project() -> TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let root = dir.path();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    dir
}

/// Drive `rigger run --base HEAD <extra flags>` in `root`, with `KURRENTDB_CONN` REMOVED from the
/// child so the environment rung is silent and only the flags under test drive selection.
/// `--base HEAD` resolves in the committed repo, so the run clears its base/anchor gates and reaches
/// the store-open seam. `RIGGER_NO_DASH` keeps the run from starting a real dashboard under test;
/// selection resolves (and, for a server, the eager connect fails) before that would matter.
fn run_with_flags(root: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["run", "--base", "HEAD"];
    args.extend_from_slice(extra);
    Command::new(rigger_bin())
        .args(&args)
        .current_dir(root)
        // `rigger run` registers this instance in the machine-global registry under XDG_STATE_HOME
        // (spec 50, criterion 2) BEFORE the eager connect fails. Redirect that state dir into the
        // test's own temp project so the registration lands under `root/rigger`, never the
        // operator's real ~/.local/state/rigger/instances (a `--conn` run here would otherwise seed
        // a live phantom shared instance a running dash discovers).
        .env("XDG_STATE_HOME", root)
        .env("RIGGER_NO_DASH", "1")
        .env_remove("KURRENTDB_CONN")
        .output()
        .expect("spawn rigger run")
}

/// Assert the flag selected the SERVER backend: the run failed inside the kurrentdb adapter (the
/// eager connect to the unreachable address) and fabricated no local sqlite event log. The ABSENCE
/// of the sqlite walk-up's `no rigger store found` is what distinguishes a genuine server selection
/// from the pre-fix drop.
fn assert_selected_server(out: &Output, root: &Path, why: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "{why}: an unreachable server must fail, never silently succeed against a local fallback; \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("kurrentdb"),
        "{why}: the run must fail INSIDE the server backend, proving the flag selected the server; \
         stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("no rigger store found"),
        "{why}: the flag-selected server must not drop to the local sqlite walk-up; stderr:\n{stderr}"
    );
    assert!(
        !event_log(root).exists(),
        "{why}: a server selection must NOT fabricate a local .rigger/events.db"
    );
}

/// A bare `--conn <url>` (no `--eventstore`) over a committed project whose workflow carries
/// `store_block` selects the server it addresses through the shipped binary, never dropping to
/// the local sqlite default; `why` is the assertion's reason.
fn assert_bare_conn_selects_the_server(store_block: &str, why: &str) {
    let project = committed_project();
    let root = project.path();
    write_workflow(root, "flagtest", store_block);
    let out = run_with_flags(root, &["--conn", UNREACHABLE]);
    assert_selected_server(&out, root, why);
}

rigger::test_cases! {
    /// Rung 1, the reshaped flag arm: a bare `--conn <url>` (no `--eventstore`) with NOTHING beneath
    /// it configured - no env, no secret file, a store-less committed config - must SELECT the server
    /// it addresses. Before the fix a bare `--conn` fell through the flag arm to the lower rungs and,
    /// with none set, resolved the LOCAL sqlite default - the store-fracture footgun where
    /// `rigger run --conn kurrentdb://prod <spec>` silently ran against a local log. This pins that a
    /// non-empty `--conn` is a first-class highest-precedence source, wired straight through the
    /// shipped binary.
    run_bare_conn_flag_selects_the_server_never_dropped_to_sqlite: assert_bare_conn_selects_the_server(
        "",
        "a bare --conn selects the server, never dropping to the local sqlite default",
    );
    /// Rung 1 beats rung 4, the footgun with an ACTIVE lower rung: the committed config explicitly
    /// pins `store: sqlite`, yet a bare `--conn <url>` alongside it must still select the server -
    /// the flag is never dropped to the sqlite the config names. Before the fix the bare `--conn`
    /// fell through to the config rung and resolved that sqlite; the operator's explicit `--conn`
    /// was silently discarded. This pins the flag outranking the committed config through the binary.
    run_conn_flag_beats_a_committed_sqlite_store_config: assert_bare_conn_selects_the_server(
        "store:\n  backend: sqlite\n",
        "a bare --conn outranks a committed store: sqlite config, never dropping to it",
    );
}
