//! Periphery for spec 69, criterion 3 - the cause wire: a failed unit's `UnitFailed` carries a
//! closed-vocabulary `cause` (`reject` | `gate:<name>` | `integrate-conflict` | `infra:<kind>`),
//! folded onto `ledger::Unit::cause` and rendered on the `reject-recurrence` blocker line both
//! `rigger status` and the dashboard print.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. Four implementer-owned suites
//! already cover this feature exhaustively IN-PROCESS, in the SAME compilation unit, against
//! hand-built inputs:
//!   - `conductor.rs`'s own `mod tests` runs the real `RunCtx` loop and asserts the resulting
//!     `ledger::project(...)`'s `.cause` per unit, for every one of the seven emit sites.
//!   - `ledger.rs`'s own `mod tests` proves the fold: a `cause`-bearing `UnitFailed` overwrites
//!     `Unit::cause`, and a causeless one (a raw JSON string missing the key) decodes as empty
//!     rather than erroring.
//!   - `blocker.rs`'s own `mod tests` proves `classify` renders the recorded cause on the
//!     `RejectRecurrence` line, and defaults an empty one to `"unknown"`.
//!   - `main.rs`'s own `mod tests`
//!     (`status_and_dashboard_render_the_same_current_blocker_lines`) proves `status_blocker_
//!     lines` and `dash::build_state` render BYTE-IDENTICAL lines, in process, for a fixed
//!     `Vec<Event>` it builds by hand.
//!
//! None of those ever cross a real process boundary. `status_blocker_lines` and `cmd_status`
//! are PRIVATE free functions in the `rigger` BINARY crate (`src/main.rs`) - unreachable from an
//! integration-test crate under `tests/` by any means other than spawning the compiled binary
//! (mirrors `tests/watchdog_cli_periphery.rs`'s identical situation for `cmd_watch`, and
//! `tests/cli.rs`'s release-ready periphery section for `cmd_status` itself, which notes the
//! same gap for a DIFFERENT status line). And none of the implementer's fixtures ever touch a
//! REAL on-disk store: `ledger.rs`'s causeless-event test constructs its `Event` from a Rust
//! byte literal in memory, never writing it through a real `Store::open` / SQLite round trip
//! the way an actual (or a pre-criterion, LEGACY) run's history would have been persisted.
//!
//! This file drives the compiled `rigger status` binary against a real, on-disk, namespaced
//! event store - closing both gaps at once: real `argv` -> `main()` dispatch -> `cmd_status` ->
//! `status_blocker_lines` wiring, and a real SQLite round trip for the additive, serde-defaulted
//! `cause` field (spec 69, criterion 3's back-compat contract).
//!
//! NOT OWNED HERE: `gate_failure_cause`'s `"{gate}: {evidence}"` parsing, and which of the seven
//! conductor emit sites picks which cause - both are private to `conductor.rs` and exhaustively
//! covered by the implementer's own colocated tests cited above. This file only proves that
//! WHATEVER cause a `UnitFailed` carries reaches an operator's terminal correctly, exactly as
//! `blocker.rs`'s own tests already assume by hand-typing the JSON `cause` field directly - this
//! file does that same seeding through a real store and the real binary instead.

mod common;

use std::path::Path;
use std::process::Command;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};

/// A throwaway project: its own git repo (so `project_identity()` resolves deterministically),
/// with no `.rigger` dir yet. Mirrors `tests/cli.rs`'s `temp_project`.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized, empty `.rigger/events.db` under `root` - stands in for the store a
/// prior `rigger run`/`step` would have created. Mirrors `tests/cli.rs`'s `seed_store`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root` - mirrors `tests/cli.rs`'s
/// `run_stream_identity`, itself mirroring `StoreLocation::identity`'s precedence: the tracked
/// `.rigger/project.id` at the git top-level when present, else the git top-level basename,
/// else `root`'s own basename.
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
/// SQLite round trip - standing in for the conductor minting them (or, for the back-compat
/// case, for a run that predates this criterion having already minted them). Mirrors
/// `tests/cli.rs`'s `seed_run_events`.
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
/// `tests/cli.rs`'s `run_rigger`.
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

/// The headline boundary proof: all four members of the closed vocabulary
/// (`reject` | `gate:<name>` | `integrate-conflict` | `infra:<kind>`) - a distinct failed unit
/// per cause - reach the `reject-recurrence` line `rigger status` prints, through the REAL
/// compiled binary against a REAL on-disk store, exactly as an operator would see them.
#[test]
fn all_four_closed_vocabulary_causes_surface_on_reject_recurrence_lines_through_the_real_binary() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-gate"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-gate","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ("UnitStarted", r#"{"id":"u-merge"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-merge","attempts":1,"cause":"integrate-conflict"}"#,
            ),
            ("UnitStarted", r#"{"id":"u-infra"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-infra","attempts":1,"cause":"infra:spawn"}"#,
            ),
            ("UnitStarted", r#"{"id":"u-reject"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-reject","attempts":1,"cause":"reject"}"#,
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(
        ok,
        "rigger status must succeed on a store with recorded causes: {err}"
    );

    for (unit, cause) in [
        ("u-gate", "gate:fmt"),
        ("u-merge", "integrate-conflict"),
        ("u-infra", "infra:spawn"),
        ("u-reject", "reject"),
    ] {
        let want = format!("{unit}: reject-recurrence #1/3 ({cause})");
        assert!(
            out.contains(&want),
            "expected the real binary's `rigger status` to print {want:?}; got:\n{out}"
        );
    }
}

/// The back-compat proof (spec 69, criterion 3): a `UnitFailed` shaped exactly as one would
/// have been BEFORE this criterion existed - no `cause` key in its JSON at all - written
/// through a REAL `Store::open` / SQLite round trip (not an in-memory `Event` literal, unlike
/// the implementer's own `ledger.rs` fold test), still decodes without error and renders
/// `"unknown"` rather than crashing or silently dropping the unit from the blocker list.
#[test]
fn a_legacy_causeless_unit_failed_event_survives_a_real_store_round_trip_and_renders_unknown() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-legacy"}"#),
            ("UnitFailed", r#"{"id":"u-legacy","attempts":1}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(
        ok,
        "a legacy causeless UnitFailed must not break `rigger status`: {err}"
    );
    assert!(
        out.contains("u-legacy: reject-recurrence #1/3 (unknown)"),
        "a causeless prior event must default to \"unknown\" through the real binary/store, \
         never error or drop the unit; got:\n{out}"
    );
}

/// The overwrite proof, through the real binary/store this time (the implementer's own
/// `ledger.rs` test already proves it in-process against hand-built `Event`s): when a unit
/// fails twice with two DIFFERENT causes, the printed line names the cause of the LATEST
/// failure, never the first.
#[test]
fn the_reject_recurrence_line_names_the_latest_of_several_recorded_causes_through_the_real_binary()
{
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-churn"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-churn","attempts":1,"cause":"gate:fmt"}"#,
            ),
            ("UnitStarted", r#"{"id":"u-churn"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-churn","attempts":2,"cause":"reject"}"#,
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed: {err}");
    assert!(
        out.contains("u-churn: reject-recurrence #2/3 (reject)"),
        "the line must name the LATEST cause (\"reject\"), not the stale first one \
         (\"gate:fmt\"); got:\n{out}"
    );
    assert!(
        !out.contains("gate:fmt"),
        "the stale first cause must not leak into the rendered line; got:\n{out}"
    );
}
