//! Periphery for spec 93, criterion 4 ("ONE FOLD"): `rigger status` (`cmd_status` in the
//! binary) prints its first line and its needs-you lines through `rigger::console`'s
//! [`console::fold`], never a second, independently-composed copy of either.
//!
//! WHY THIS FILE. `cmd_status` is a PRIVATE free function in the `rigger` BINARY crate
//! (`src/main.rs`), unreachable from an integration-test crate under `tests/` other than by
//! spawning the compiled binary (mirrors `tests/cause_wire_periphery.rs`'s identical situation
//! for the same command's blocker lines). `console.rs`'s own colocated `mod tests` already
//! proves the fold itself (unit statuses agree with `ledger::project`, blockers with
//! `blocker::from_events`, the dock's needs-you arms, the statusline's health words) against
//! hand-built in-memory `Event`s; this file proves the OTHER half - that the real compiled
//! `rigger status`, reading a real on-disk store, prints exactly what that fold computes,
//! through a real `argv` -> `main()` -> `cmd_status` dispatch and a real SQLite round trip.

mod common;

use std::path::Path;
use std::process::Command;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};

/// A throwaway project: its own git repo, no `.rigger` dir yet. Mirrors
/// `tests/cause_wire_periphery.rs`'s `temp_project`.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized, empty `.rigger/events.db` under `root`. Mirrors
/// `tests/cause_wire_periphery.rs`'s `seed_store`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root`. Mirrors
/// `tests/cause_wire_periphery.rs`'s `run_stream_identity`.
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

/// Append `events` directly to `root`'s namespaced run stream through a REAL
/// `Store::open` / SQLite round trip. Mirrors `tests/cause_wire_periphery.rs`'s
/// `seed_run_events`.
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
/// `tests/cause_wire_periphery.rs`'s `run_rigger`.
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
/// compute the SAME answer the running binary should have printed, so this test's expectation
/// is derived from the core, never hand-typed text that could drift from it.
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

/// ONE FOLD, the CLI's use of the core: for a run with an escalated unit, `rigger status`'s
/// FIRST printed line is exactly `console::fold`'s `statusline` computed independently by this
/// test from a real store round trip, and the needs-you section names that unit through
/// `console::fold`'s own dock line - not a second, hand-composed rendering.
#[test]
fn rigger_status_first_line_and_needs_you_section_are_the_consoles_own_fold() {
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

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed: {err}");

    let events = read_back_run_events(root);
    let want = rigger::console::fold(&events, 3).expect("console::fold over the real read-back");

    let first_line = out.lines().next().unwrap_or("");
    assert_eq!(
        first_line, want.statusline,
        "the first printed line must be console::fold's own statusline; got:\n{out}"
    );

    assert!(
        out.contains("needs you:"),
        "a non-empty dock must print a needs-you section; got:\n{out}"
    );
    for line in want.dock.lines() {
        assert!(
            out.contains(&line),
            "expected the needs-you section to carry {line:?}; got:\n{out}"
        );
    }

    // ConsoleState.blockers (this criterion's third fold fact, alongside the statusline and
    // the dock): the escalated unit is ALSO a current blocker (spec 19a's classifier already
    // covers `Escalated`), so the SAME real run must print a "current blockers:" section
    // carrying console::fold's own blocker line - proving the field the "ONE FOLD" Done-when
    // names ("blockers") agrees with the real binary too, not only the statusline and the dock.
    assert!(
        out.contains("current blockers:"),
        "an escalated unit is also a current blocker; got:\n{out}"
    );
    for line in &want.blockers {
        assert!(
            out.contains(line.as_str()),
            "expected the current-blockers section to carry {line:?}; got:\n{out}"
        );
    }
}

/// The dock's OTHER two needs-you arms - the budget halt and worker-death-recurred, INCLUDING
/// its resumed guard - proven through the same real `argv` -> `cmd_status` -> real-store round
/// trip the escalated arm already gets above. Before this test, only the escalated arm was
/// wired-proven through the real binary (sdet-u93c4-dock-budget-and-recurrence-arms-not-periphery-proven);
/// the other two were proven only against hand-built in-memory `Event`s in `console.rs`'s own
/// `mod tests`. Also pins the fix for adv-u93c4-c4-dock-resumed-unit-false-needs-you (dock's
/// worker-death-recurred arm now mirrors `blocker::classify`'s resumed guard) through the real
/// binary, not only through `console.rs`'s own colocated unit test.
#[test]
fn rigger_status_needs_you_covers_budget_halt_and_worker_death_recurred_arms() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            // A unit past the recurrence threshold, never resumed: must appear.
            ("UnitStarted", r#"{"id":"u-fail"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-fail","attempts":2,"cause":"reject"}"#,
            ),
            // A unit an operator has since resumed: must NOT appear - the resumed guard
            // dock's worker-death-recurred arm now shares with blocker::classify.
            ("UnitStarted", r#"{"id":"u-resumed"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-resumed","attempts":3,"cause":"reject"}"#,
            ),
            ("UnitEscalated", r#"{"id":"u-resumed"}"#),
            (
                "UnitResumed",
                r#"{"unit":"u-resumed","attempts_granted":2,"by":"operator"}"#,
            ),
            // The run-level budget halt, seeded last so it is the CURRENT blocker
            // (`blocker::budget_halt` reports only when no later unit-lifecycle event has
            // superseded it).
            ("BudgetExhausted", r#"{"budget":10,"spawns":10}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed: {err}");

    let events = read_back_run_events(root);
    let want = rigger::console::fold(&events, 3).expect("console::fold over the real read-back");
    assert_eq!(
        want.dock.needs_you.len(),
        2,
        "fixture must exercise exactly the budget-halt and u-fail worker-death-recurred arms, \
         with u-resumed excluded: {:?}",
        want.dock.needs_you
    );

    assert!(
        out.contains("needs you:"),
        "a non-empty dock must print a needs-you section; got:\n{out}"
    );
    for line in want.dock.lines() {
        assert!(
            out.contains(&line),
            "expected the needs-you section to carry {line:?}; got:\n{out}"
        );
    }

    // Pin the two literal facts directly, independent of `want`: a regression in
    // console::fold's own dock logic (not only in cmd_status's wiring of it) would also
    // break these.
    assert!(
        out.contains("run: budget spent 10/10"),
        "expected the budget-halt line; got:\n{out}"
    );
    assert!(
        out.contains("u-fail: 2 attempts, still parked"),
        "expected the worker-death-recurred line for u-fail; got:\n{out}"
    );

    // The regression this round fixes, pinned through the real binary: a just-resumed unit
    // must not read as still-needs-you.
    assert!(
        !out.contains("u-resumed: 3 attempts, still parked"),
        "a just-resumed unit must not appear as worker-death-recurred; got:\n{out}"
    );
}

/// A clean run (no units at all) prints the console's `healthy` statusline as its first line,
/// and the needs-you section says so explicitly rather than rendering an empty header - so an
/// operator reading a quiet run's status never wonders whether the needs-you check even ran.
#[test]
fn rigger_status_reports_nothing_needs_you_on_a_clean_run() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed on an empty store: {err}");

    let first_line = out.lines().next().unwrap_or("");
    assert_eq!(first_line, "- . 0/0 units . healthy");
    assert!(
        out.contains("Nothing needs a human right now"),
        "an empty dock must say so explicitly; got:\n{out}"
    );
}

/// `--json` is unaffected by this criterion: the console's needs-you/statusline surface is
/// human-table-only, so the machine-readable array shape (spec 10/14's liveness contract) stays
/// exactly what it was.
#[test]
fn rigger_status_json_is_unaffected_by_the_console_surface() {
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

    let (out, err, ok) = run_rigger(root, &["status", "--json"]);
    assert!(ok, "rigger status --json must succeed: {err}");
    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("valid JSON");
    assert!(
        value.is_array(),
        "the top level must stay a bare array (spec 10/14's liveness contract): {out}"
    );
    assert!(
        !out.contains("needs you:"),
        "the console's human rendering must not leak into --json: {out}"
    );
}

/// `console::dock` and `console::statusline` are independently reachable PUBLIC library API,
/// not merely private helpers [`console::fold`] composes internally - any external consumer
/// (this test crate today, the WASM member crate criterion 2 builds tomorrow) can call them
/// directly and get the exact facts `fold` embeds. Proven over events a REAL SQLite
/// append/read-back round trip produced, not the hand-built in-memory `Event`s `console.rs`'s
/// own colocated `mod tests` uses - so a real store/serialization defect reaching either
/// function's inputs would show here even where it cannot show there. Also closes the two
/// `ConsoleState` fields (`units`, `blockers`) neither periphery test above independently
/// asserts, and the `UnitStatuses` type alias they are keyed by.
#[test]
fn console_dock_and_statusline_are_reachable_directly_over_a_real_store_round_trip() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
            ("UnitStarted", r#"{"id":"u-building"}"#),
        ],
    );

    let events = read_back_run_events(root);
    let run = rigger::ledger::project(&events).expect("project a real read-back stream");
    let blockers =
        rigger::blocker::from_events(&events, 3).expect("classify blockers over the real stream");

    // dock(): called directly, bypassing fold entirely.
    let dock = rigger::console::dock(&run, &events);
    assert_eq!(dock.needs_you.len(), 1, "{:?}", dock.needs_you);
    assert_eq!(dock.needs_you[0].unit, "u-esc");
    assert_eq!(
        dock.lines(),
        vec!["u-esc: escalated after exhausting remediation"]
    );

    // statusline(): likewise called directly.
    let line = rigger::console::statusline(&run, &blockers, &dock);
    assert_eq!(line, "u-building . 0/2 units . needs-you");

    // fold() over the SAME real read-back stream must equal the direct calls above,
    // byte-for-byte - the ONE FOLD guarantee, proven this time on the two fields
    // (`units`, `blockers`) this file's other tests check only through their printed text.
    let state = rigger::console::fold(&events, 3).expect("fold the same real read-back stream");
    assert_eq!(state.units.get("u-esc"), Some(&"escalated"));
    assert_eq!(state.units.get("u-building"), Some(&"grounding"));
    assert_eq!(state.blockers, rigger::blocker::lines(&blockers));
    assert_eq!(state.dock, dock);
    assert_eq!(state.statusline, line);
}
