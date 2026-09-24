//! Periphery for spec 104 criterion 5 (A FAILURE HAS A CLASS): drives
//! `rigger hook stop-failure --spawn <id> --class <category>` through the REAL COMPILED
//! binary against a real `.rigger/progress.db` - the boundary
//! `src/driver/claude_code.rs`'s own `mod tests` and `progress_store.rs`'s own tests
//! structurally cannot reach: real argv parsing, real store-directory resolution
//! ([`require_store_dir`]'s walk-up), and the real CLI-level `--class` validation
//! [`cmd_hook_stop_failure`] owns.
//!
//! NOT OWNED HERE: the record's shape and the "latest wins" fold
//! ([`rigger::progress`]/[`rigger::progress_store`]'s own unit tests);
//! `install_stop_failure_hooks`'s JSON-merge shape (empty settings, composing with an
//! existing `PreToolUse` family, idempotence) - proven in-process by `driver::claude_code`'s
//! own `mod tests` against the real `hooks::install_stopfailure_hook` merge authority, so
//! that half of the seam needs no separate periphery duplicate; classification priority
//! (`driver::claude_code::classify_failure`'s own tests, and
//! `claude_code_stream_periphery.rs`'s real-subprocess classification tests).
//!
//! OWNED HERE, in the SHELL ROUND TRIP section below: the installed hook command's own
//! shell-quoting. `stop_failure_command` single-quotes `spawn_id` through the exact same
//! `shell_single_quote` helper `write_guard_command` uses for its roots, so it carries the
//! identical real-shell risk `write_guard_hook_periphery.rs` closed for THAT command - a
//! quoting bug a real shell's word-splitting would expose that no implementer test (every
//! one of which inspects the returned bytes as a string, never spawns a shell) can catch. A
//! prior draft of this module doc claimed there was "no shell hop to prove here"; that was
//! wrong - `stop_failure_command` builds a shell command line exactly like
//! `write_guard_command` does, and Claude Code will run it through `sh -c` the same way once
//! spec 105 wires this hook's installation into a live spawn.

mod common;

use std::path::Path;
use std::process::{Command, Output, Stdio};

use rigger::driver::claude_code::install_stop_failure_hooks;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::progress::{self, StopFailure};

/// The project identity the binary resolves for `root` - mirrors
/// `tests/console_status_periphery.rs`'s `run_stream_identity` (itself mirroring
/// `tests/cause_wire_periphery.rs`'s), the established derivation every periphery suite
/// that reads back a courier's namespaced write uses: the store is namespaced by project
/// identity ([`Namespaced`]), so a direct `Store::open` read against the RAW stream name
/// sees nothing until wrapped the same way.
fn run_stream_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// A throwaway project the compiled binary accepts as a courier target: its own git repo
/// (so the store's project identity resolves normally) and an INITIALIZED event log - a
/// courier refuses to fabricate one from a cwd with no existing store (spec 05). Mirrors
/// `courier_registry_refresh_periphery.rs::courier_project` exactly - the established
/// shape every courier periphery suite builds its fixture project the same way.
fn courier_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temp project");
    let root = dir.path();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).expect("create .rigger");
    Store::open(
        rigger_dir
            .join("events.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .expect("the event log initializes");
    dir
}

fn run_rigger(root: &Path, args: &[&str]) -> Output {
    common::rigger_courier()
        .args(args)
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .output()
        .expect("the rigger binary runs")
}

/// Every `StopFailure` record in `root`'s own `.rigger/progress.db`, read directly (never
/// through the command under test) so the assertion is independent of it. Wrapped in the
/// SAME [`Namespaced`] scoping the binary itself writes through - see
/// [`run_stream_identity`].
fn recorded_stop_failures(root: &Path) -> Vec<StopFailure> {
    let backend = Store::open(
        root.join(".rigger/progress.db")
            .to_str()
            .expect("a utf-8 store path"),
    )
    .expect("the progress store opens");
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(progress::STREAM, 0, Direction::Forward)
        .expect("the progress stream reads")
        .into_iter()
        .filter(|e| e.type_ == progress::TYPE_STOP_FAILURE)
        .map(|e| serde_json::from_slice(&e.data).expect("a well-formed StopFailure"))
        .collect()
}

#[test]
fn hook_stop_failure_records_the_spawn_and_class() {
    let project = courier_project();
    let out = run_rigger(
        project.path(),
        &[
            "hook",
            "stop-failure",
            "--spawn",
            "u104-fail-class/implementer#0",
            "--class",
            "rate_limit",
        ],
    );
    assert!(
        out.status.success(),
        "rigger hook stop-failure failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let recorded = recorded_stop_failures(project.path());
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].spawn, "u104-fail-class/implementer#0");
    assert_eq!(recorded[0].class, "rate_limit");
}

#[test]
fn hook_stop_failure_a_second_call_for_the_same_spawn_overwrites_which_class_reads_back() {
    let project = courier_project();
    for class in ["rate_limit", "authentication_failed"] {
        let out = run_rigger(
            project.path(),
            &[
                "hook",
                "stop-failure",
                "--spawn",
                "u1/implementer#0",
                "--class",
                class,
            ],
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let recorded = recorded_stop_failures(project.path());
    assert_eq!(recorded.len(), 2, "both calls recorded their own event");
    let events: Vec<_> = recorded
        .iter()
        .map(|sf| {
            rigger::eventstore::Event::new(
                progress::TYPE_STOP_FAILURE,
                serde_json::to_vec(sf).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        progress::latest_stop_failure_class(&events, "u1/implementer#0").as_deref(),
        Some("authentication_failed"),
        "latest wins, matching AgentProgress's own per-id fold"
    );
}

#[test]
fn hook_stop_failure_rejects_an_unrecognized_class() {
    let project = courier_project();
    let out = run_rigger(
        project.path(),
        &[
            "hook",
            "stop-failure",
            "--spawn",
            "u1/implementer#0",
            "--class",
            "not_a_real_category",
        ],
    );
    assert!(
        !out.status.success(),
        "an unrecognized class must be refused"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("unrecognized"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(recorded_stop_failures(project.path()).is_empty());
}

#[test]
fn hook_stop_failure_requires_spawn_and_class() {
    let project = courier_project();

    let missing_class = run_rigger(
        project.path(),
        &["hook", "stop-failure", "--spawn", "u1/implementer#0"],
    );
    assert!(!missing_class.status.success());

    let missing_spawn = run_rigger(
        project.path(),
        &["hook", "stop-failure", "--class", "rate_limit"],
    );
    assert!(!missing_spawn.status.success());

    assert!(recorded_stop_failures(project.path()).is_empty());
}

// ---- `rigger hook`'s own dispatch (the shared namespace, not `stop-failure` itself) ----
//
// `cmd_hook`'s two error arms (no subcommand at all; an unrecognized one) have no coverage
// anywhere else - not the implementer's diff (no test landed with `cmd_hook`/`cmd_hook_stop_failure`
// in `src/main.rs`), not `tests/cli.rs`, not the SUBCOMMANDS-registry consistency check
// (`main.rs`'s own `mod tests`, which only asserts `"hook"` is a REGISTERED name, never that
// dispatching into it with a bad or missing sub-argument behaves correctly).

#[test]
fn hook_with_no_subcommand_fails_naming_stop_failure() {
    let project = courier_project();
    let out = run_rigger(project.path(), &["hook"]);
    assert!(
        !out.status.success(),
        "a bare `rigger hook` must be refused"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("stop-failure"),
        "stderr must point at the one known subcommand: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn hook_with_an_unrecognized_subcommand_fails_naming_it() {
    let project = courier_project();
    let out = run_rigger(project.path(), &["hook", "not-a-real-subcommand"]);
    assert!(
        !out.status.success(),
        "an unrecognized `rigger hook` subcommand must be refused"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not-a-real-subcommand"),
        "stderr must name the unrecognized subcommand: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---- SHELL ROUND TRIP (the installed `StopFailure` hook command, through a real shell) ----
//
// See this file's own module doc: none of the tests above, nor `driver::claude_code`'s own
// `mod tests`, ever spawn a shell - every one of them inspects
// `install_stop_failure_hooks`'/`stop_failure_command`'s returned bytes as a string, which
// cannot distinguish a quoting bug a real shell's word-splitting would expose from one only
// a hand-rolled parser would catch. This is the same class of gap
// `write_guard_hook_periphery.rs` closed for `install_write_guard_hook`'s own
// `shell_single_quote`d root values, mirrored here for `stop_failure_command`'s `spawn_id`.

/// Build ONE category's exact installed `StopFailure` hook command string
/// (`install_stop_failure_hooks`'s injection half) against the REAL compiled binary this
/// suite drives - never the bare `"rigger"` placeholder the implementer's own unit tests use
/// as a name that may not resolve on `PATH` in a sandboxed test run. Mirrors
/// `write_guard_hook_periphery.rs::write_guard_hook_command`'s identical real-binary
/// substitution.
fn stopfailure_hook_command_for(spawn_id: &str, class: &str) -> String {
    let bin = common::rigger_bin();
    let out = install_stop_failure_hooks(b"", spawn_id, bin.to_str().expect("utf-8 test path"))
        .expect("install_stop_failure_hooks must succeed against empty settings");
    let settings: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let arr = settings["hooks"]["StopFailure"]
        .as_array()
        .expect("the merged settings JSON must carry a StopFailure array");
    arr.iter()
        .find(|b| b["matcher"] == class)
        .unwrap_or_else(|| panic!("no StopFailure block for category {class:?}: {arr:?}"))["hooks"]
        [0]["command"]
        .as_str()
        .expect("the merged settings JSON must carry the hook command")
        .to_string()
}

/// Run `command` (the exact string one `StopFailure` hook entry carries) through a REAL
/// POSIX shell - `sh -c <command>`, cwd'd at `root` so `require_store_dir`'s walk-up finds
/// the courier project - exactly as Claude Code itself will once spec 105 wires this hook's
/// installation into a live spawn. `rigger hook stop-failure` reads no stdin, unlike
/// `guard-write`'s payload, so this needs no writer side.
///
/// Unlike `write_guard_hook_periphery.rs`'s identical-looking `sh -c` round trip,
/// `install_stop_failure_hooks`'s command OPENS a store (`rigger hook stop-failure` calls
/// `require_store_dir`), so this `sh` child inherits and must be defended against the same
/// fence/production-store env every direct `rigger_courier()` spawn is - see
/// `common::unfenced`'s doc comment for the false-green-outside-a-gate /
/// false-red-inside-one shape this closes.
fn run_stopfailure_command_through_a_real_shell(command: &str, root: &Path) -> Output {
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .stdin(Stdio::null());
    common::unfenced(&mut cmd)
        .output()
        .expect("spawn sh -c <installed stopfailure hook command>")
}

#[test]
fn installed_stopfailure_hook_command_records_the_spawn_id_through_a_real_shell() {
    let project = courier_project();
    let command = stopfailure_hook_command_for("u104-fail-class/implementer#0", "rate_limit");

    let out = run_stopfailure_command_through_a_real_shell(&command, project.path());
    assert!(
        out.status.success(),
        "the installed hook command must exit 0 under a real shell; stderr:\n{}\nstdout:\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );

    let recorded = recorded_stop_failures(project.path());
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].spawn, "u104-fail-class/implementer#0");
    assert_eq!(recorded[0].class, "rate_limit");
}

#[test]
fn installed_stopfailure_hook_command_survives_a_spawn_id_with_a_space_and_an_embedded_quote_through_a_real_shell(
) {
    let project = courier_project();
    let tricky_spawn = "it's a spawn id/implementer#0".to_string();
    let command = stopfailure_hook_command_for(&tricky_spawn, "overloaded");

    let out = run_stopfailure_command_through_a_real_shell(&command, project.path());
    assert!(
        out.status.success(),
        "a spawn id with a space and an embedded single quote must reach `rigger hook \
         stop-failure` as ONE intact argument once a real shell has word-split the \
         installed command; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );

    let recorded = recorded_stop_failures(project.path());
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0].spawn, tricky_spawn,
        "the recorded spawn id must survive the shell hop byte-for-byte"
    );
    assert_eq!(recorded[0].class, "overloaded");
}

#[test]
fn installed_stopfailure_hook_command_neutralizes_an_injection_shaped_spawn_id_through_a_real_shell(
) {
    let project = courier_project();
    let marker_dir = tempfile::tempdir().unwrap();
    let marker = marker_dir.path().join("injected-by-a-broken-quote");
    // An embedded `'` immediately followed by a real shell command: if `shell_single_quote`
    // ever closed the quote early instead of escaping it, a real shell would run
    // `touch <marker>` as a SEPARATE command rather than treat the whole string as one
    // `--spawn` argument value - the same injection shape
    // `write_guard_hook_periphery.rs`'s own
    // `installed_hook_command_neutralizes_an_injection_shaped_root_through_a_real_shell`
    // proves against `install_write_guard_hook`'s root values.
    let evil_spawn = format!(
        "u1/implementer#0'; touch {} ; echo '",
        marker.to_str().unwrap()
    );
    let command = stopfailure_hook_command_for(&evil_spawn, "billing_error");

    // Whatever `rigger hook stop-failure` makes of this synthetic spawn id is not the point
    // here - the two tests above already prove the record/survive behavior on plain and
    // tricky-but-legitimate spawn ids; this call's job is only to prove a real shell ran ONE
    // command and the embedded command never executed.
    let _ = run_stopfailure_command_through_a_real_shell(&command, project.path());
    assert!(
        !marker.exists(),
        "an embedded `'` in a spawn id must never let a real shell run a command that \
         followed it - the marker file must not exist"
    );
}
