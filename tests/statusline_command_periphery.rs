//! Periphery for spec 94, criterion 5 ("THE STATUSLINE COMMAND"): `rigger status --line`
//! prints exactly the core's `console::statusline` text - the SAME line `rigger status`
//! itself prints as its own first line - and nothing else, and `rigger setup` registers that
//! command as the editor's status line in `.claude/settings.json`.
//!
//! WHY THIS FILE. `cmd_status`/`cmd_setup` are PRIVATE free functions in the `rigger` BINARY
//! crate (`src/main.rs`), unreachable from an integration-test crate under `tests/` other than
//! by spawning the compiled binary (mirrors `tests/console_status_periphery.rs`'s identical
//! situation for the same command). `src/hooks.rs`'s own colocated `mod tests` already proves
//! the settings-merge logic (`install_status_line`) in isolation; this file proves the OTHER
//! half - that the real compiled `rigger status --line` and `rigger setup` wire it through a
//! real `argv` -> `main()` dispatch and a real on-disk store/settings file.

mod common;

use std::path::Path;
use std::process::Command;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};

/// A throwaway project: its own git repo, no `.rigger` dir yet. Mirrors
/// `tests/console_status_periphery.rs`'s `temp_project`.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized, empty `.rigger/events.db` under `root`. Mirrors
/// `tests/console_status_periphery.rs`'s `seed_store`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root`. Mirrors
/// `tests/console_status_periphery.rs`'s `run_stream_identity`.
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

/// Append `events` directly to `root`'s namespaced run stream through a REAL `Store::open` /
/// SQLite round trip. Mirrors `tests/console_status_periphery.rs`'s `seed_run_events`.
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    let db = root.join(".rigger").join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for &(ty, body) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(ty, body.as_bytes().to_vec())],
            )
            .unwrap();
    }
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success). Mirrors
/// `tests/console_status_periphery.rs`'s `run_rigger`.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// The events read back from the real store, in run order - what `console::fold` needs to
/// compute the SAME answer the running binary should have printed. Mirrors
/// `tests/console_status_periphery.rs`'s `read_back_run_events`.
fn read_back_run_events(root: &Path) -> Vec<Event> {
    let db = root.join(".rigger").join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
}

/// `rigger status --line` prints EXACTLY ONE line, and it is `console::fold`'s own
/// `statusline` computed independently by this test from a real store round trip - not a
/// second, hand-composed rendering, and no other `rigger status` furniture (needs-you,
/// current blockers, the run-id header) leaks into it.
#[test]
fn status_line_prints_exactly_the_consoles_own_statusline_and_nothing_else() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status", "--line"]);
    assert!(ok, "rigger status --line must succeed: {err}");

    let events = read_back_run_events(root);
    let want = rigger::console::fold(&events, 3).expect("console::fold over the real read-back");

    assert_eq!(
        out.trim_end_matches('\n'),
        want.statusline,
        "rigger status --line must print exactly console::fold's own statusline; got:\n{out}"
    );
    assert_eq!(
        out.lines().count(),
        1,
        "rigger status --line must print exactly one line; got:\n{out}"
    );
}

/// The line `rigger status --line` prints is IDENTICAL to `rigger status`'s own first line
/// for the same recorded stream - "the console's bottom line for the same position" - proven
/// by running both against the same seeded store rather than trusting two independent
/// derivations to agree.
#[test]
fn status_line_matches_rigger_status_first_line_for_the_same_position() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-a"}"#),
            ("UnitStarted", r#"{"id":"u-b"}"#),
            ("UnitIntegrated", r#"{"id":"u-a"}"#),
        ],
    );

    let (full_out, full_err, full_ok) = run_rigger(root, &["status"]);
    assert!(full_ok, "rigger status must succeed: {full_err}");
    let first_line = full_out.lines().next().unwrap_or("");

    let (line_out, line_err, line_ok) = run_rigger(root, &["status", "--line"]);
    assert!(line_ok, "rigger status --line must succeed: {line_err}");

    assert_eq!(
        line_out.trim_end_matches('\n'),
        first_line,
        "rigger status --line must equal rigger status's own first line; full:\n{full_out}\nline:\n{line_out}"
    );
}

/// A clean run (no units at all) prints the console's `healthy` statusline alone - the same
/// bare answer `rigger status`'s own first line gives on an empty store.
#[test]
fn status_line_on_a_clean_run() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);

    let (out, err, ok) = run_rigger(root, &["status", "--line"]);
    assert!(
        ok,
        "rigger status --line must succeed on an empty store: {err}"
    );
    assert_eq!(out.trim_end_matches('\n'), "- . 0/0 units . healthy");
}

/// `rigger status --line --json` (or any other combination with `--json`) is rejected: the
/// two are different output modes for the same command and cannot both apply to one call.
#[test]
fn status_line_and_json_are_mutually_exclusive() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);

    let (_out, err, ok) = run_rigger(root, &["status", "--line", "--json"]);
    assert!(!ok, "status --line --json must be rejected");
    assert!(
        err.contains("--line") && err.contains("--json"),
        "the error should name both conflicting flags; got:\n{err}"
    );
}

/// `rigger setup` registers `rigger status --line` as the editor's status line command in
/// `.claude/settings.json`'s `statusLine` key, drift-aware like every other `rigger setup`
/// install step: a fresh install reports it, a rerun on an up-to-date repo is silent.
#[test]
fn setup_registers_the_statusline_command() {
    let dir = temp_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["setup"]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("registered") && out.contains("status line"),
        "setup must report registering the status line command; got:\n{out}"
    );

    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".claude").join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["statusLine"]["type"], "command");
    assert_eq!(settings["statusLine"]["command"], "rigger status --line");

    // The pre-existing SessionStart hook (installed by the same setup run) must survive
    // untouched - the settings.json merges must not clobber each other.
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "rigger prime"
    );

    // A rerun on an up-to-date repo is a silent no-op for this artifact.
    let (out2, err2, ok2) = run_rigger(root, &["setup"]);
    assert!(ok2, "the rerun must succeed; stderr:\n{err2}");
    assert!(
        !out2.contains("status line"),
        "a rerun on an up-to-date repo must not re-report the status line registration; got:\n{out2}"
    );
}
