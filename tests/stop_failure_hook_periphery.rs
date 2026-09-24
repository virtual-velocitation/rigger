//! Periphery for spec 104 criterion 5 (A FAILURE HAS A CLASS): drives
//! `rigger hook stop-failure --spawn <id> --class <category>` through the REAL COMPILED
//! binary against a real `.rigger/progress.db` - the boundary
//! `src/driver/claude_code.rs`'s own `mod tests` and `progress_store.rs`'s own tests
//! structurally cannot reach: real argv parsing, real store-directory resolution
//! ([`require_store_dir`]'s walk-up), and the real CLI-level `--class` validation
//! [`cmd_hook_stop_failure`] owns.
//!
//! NOT OWNED HERE: the record's shape and the "latest wins" fold
//! ([`rigger::progress`]/[`rigger::progress_store`]'s own unit tests); the installed hook
//! command's shell-quoting (mirrors `write_guard_hook_periphery.rs`'s own carve-out for
//! `install_write_guard_hook` - `install_stop_failure_hooks`'s JSON-merge shape is proven
//! in-process by `driver::claude_code`'s own `mod tests`, and this command never builds a
//! shell command itself, so there is no shell hop to prove here); classification priority
//! (`driver::claude_code::classify_failure`'s own tests, and
//! `claude_code_stream_periphery.rs`'s real-subprocess classification tests).

mod common;

use std::path::Path;
use std::process::{Command, Output};

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
