//! Integration tests for `rigger reset --runs` closing a run whose driver is dead and whose
//! remaining work is already landed on the run branch.
//!
//! The problem: a run is done only when every unit folds to `UnitIntegrated`, and only the
//! conductor mints that event (`rigger emit` refuses it). A unit the operator lands by hand -
//! its branch merged onto `rigger-run` while no driver is alive - therefore never receives its
//! terminal event, so `rigger status` reports the run as still working forever and every later
//! verb treats it as the active run. `reset --runs` records the missing `UnitIntegrated` for
//! exactly such a unit: its branch carries work of its own, that work is an ancestor of the run
//! branch, and nothing is driving the run - no `rigger step` holds the step lock, no in-flight
//! spawn's liveness marker is younger than its wall-clock bound, and no registry heartbeat for
//! the store is younger than the idle window (spec 101's liveness, the one both reset modes read).
//!
//! These drive the COMPILED binary against a real git repo and `.rigger/events.db`, because the
//! observable contract is the log the operator's next `rigger status` folds.

mod common;

use common::cli::{
    init_event_log, plant_marker, read_run_events, run_rigger_envs, run_rigger_ok, seed_run_events,
};
use common::fixtures::{git_ok, git_ok_with_identity, git_out, temp_git_project_with_commit};
use std::path::Path;

/// The run's history up to the hand landing: unit `a` integrated by the conductor, then the
/// `checkin` unit started and failed an attempt before its driver died.
const DEAD_RUN: &[(&str, &str)] = &[
    ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
    ("UnitStarted", r#"{"id":"a","branch":"rigger/u/a"}"#),
    ("UnitIntegrated", r#"{"id":"a","commit":"deadbeef"}"#),
    (
        "UnitStarted",
        r#"{"id":"checkin","branch":"rigger/u/checkin","needs":["a"]}"#,
    ),
    ("UnitFailed", r#"{"id":"checkin","attempts":1}"#),
];

/// A spawn requested for `checkin` with a 300 s wall-clock bound and no result yet: live while
/// its worker's liveness marker is younger than that bound, dead once it is older.
const IN_FLIGHT_SPAWN: (&str, &str) = (
    "SpawnRequested",
    r#"{"id":"checkin/implementer#1","unit":"checkin","stage":"implement","prompt":"go","max_wall_clock":300}"#,
);

/// A git project checked out on a `rigger-run` branch with a `rigger/u/checkin` unit branch cut
/// from it; `commit_work` adds one commit of the unit's own, and `land` (only meaningful with
/// it) fast-forwards the run branch onto the unit's tip - the operator's hand landing.
fn project(commit_work: bool, land: bool) -> tempfile::TempDir {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    git_ok(root, &["checkout", "-q", "-b", "rigger-run"]);
    git_ok(root, &["branch", "rigger/u/checkin"]);
    if commit_work {
        git_ok(root, &["checkout", "-q", "rigger/u/checkin"]);
        std::fs::write(root.join("work.rs"), "fn work() {}\n").unwrap();
        git_ok(root, &["add", "work.rs"]);
        git_ok_with_identity(root, &["commit", "-q", "-m", "checkin work"]);
        git_ok(root, &["checkout", "-q", "rigger-run"]);
    }
    if land {
        git_ok(root, &["merge", "-q", "--ff-only", "rigger/u/checkin"]);
    }
    init_event_log(root);
    dir
}

/// Seeds `events` into `root`, plants the in-flight spawn's liveness marker touched
/// `marker_secs_ago` seconds ago (none when `None`), runs `rigger reset --runs`, and returns the
/// run events before and after the reset.
fn reset_runs_over(
    root: &Path,
    events: &[(&str, &str)],
    marker_secs_ago: Option<u64>,
) -> (Vec<String>, Vec<String>) {
    seed_run_events(root, events);
    let scratch = tempfile::tempdir().unwrap();
    let scratch_root = scratch.path().to_str().unwrap();
    if let Some(age) = marker_secs_ago {
        let marker =
            rigger::liveness::marker_path(scratch_root, "r1", "checkin/implementer#1").unwrap();
        plant_marker(&marker, age);
    }
    let types = |root: &Path| -> Vec<String> {
        read_run_events(root)
            .into_iter()
            .map(|e| format!("{} {}", e.type_, String::from_utf8_lossy(&e.data)))
            .collect()
    };
    let before = types(root);
    let (_out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--runs"],
        &[("RIGGER_TMPDIR", scratch_root)],
    );
    assert!(ok, "rigger reset --runs must exit 0; stderr:\n{err}");
    (before, types(root))
}

/// Asserts `after` still holds every event of `before`, in order, as its prefix - the reset
/// deletes and rewrites nothing.
fn assert_prefix_kept(before: &[String], after: &[String]) {
    assert_eq!(
        &after[..before.len()],
        before,
        "reset --runs must delete or alter no event of the log"
    );
}

/// Asserts the reset over `root` recorded exactly one terminal event after `before`: the landed
/// `checkin` unit's `UnitIntegrated`, naming its landed tip and the operator.
fn assert_checkin_closed(root: &Path, before: &[String], after: &[String]) {
    let tip = git_out(root, &["rev-parse", "rigger/u/checkin"]);
    assert_prefix_kept(before, after);
    assert_eq!(
        after.len(),
        before.len() + 1,
        "exactly one terminal event is recorded; got {after:#?}"
    );
    let closed = after.last().unwrap();
    assert!(
        closed.starts_with("UnitIntegrated ")
            && closed.contains(r#""id":"checkin""#)
            && closed.contains(&tip)
            && closed.contains(r#""by":"operator""#),
        "the landed unit's terminal event names it, its landed tip and the operator; got {closed}"
    );
}

#[test]
fn reset_runs_closes_a_dead_run_whose_checkin_is_landed_on_the_run_branch() {
    let dir = project(true, true);
    let root = dir.path();
    let (before, after) = reset_runs_over(root, DEAD_RUN, None);
    assert_checkin_closed(root, &before, &after);
    assert_eq!(
        run_rigger_ok(root, &["status", "--line"]).trim(),
        "- . 2/2 units . done"
    );
}

/// A driver that died mid-spawn leaves the spawn unanswered forever; once its worker's marker
/// is older than the spawn's wall-clock bound nothing is driving the run, so the landed unit is
/// closed exactly as if no spawn had been in flight.
#[test]
fn reset_runs_closes_a_dead_run_whose_in_flight_spawns_marker_outlived_its_bound() {
    let dir = project(true, true);
    let root = dir.path();
    let events: Vec<(&str, &str)> = DEAD_RUN.iter().copied().chain([IN_FLIGHT_SPAWN]).collect();
    let (before, after) = reset_runs_over(root, &events, Some(301));
    assert_checkin_closed(root, &before, &after);
}

/// The negative space of the rule: each case must leave the log exactly as it was.
fn assert_left_open(
    commit_work: bool,
    land: bool,
    extra: &[(&str, &str)],
    marker_secs_ago: Option<u64>,
    why: &str,
) {
    let dir = project(commit_work, land);
    let root = dir.path();
    let events: Vec<(&str, &str)> = DEAD_RUN.iter().chain(extra).copied().collect();
    let (before, after) = reset_runs_over(root, &events, marker_secs_ago);
    assert_eq!(after, before, "{why}: the log must be untouched");
    assert_eq!(
        run_rigger_ok(root, &["status", "--line"]).trim(),
        "checkin . 1/2 units . working",
        "{why}: the run must stay open"
    );
}

rigger::test_cases! {
    /// A live driver (an in-flight spawn whose worker touched its marker just now) owns the
    /// run: even a landed branch is not the operator's to close.
    reset_runs_leaves_a_live_run_untouched_even_when_its_branch_is_landed: assert_left_open(
        true,
        true,
        &[IN_FLIGHT_SPAWN],
        Some(0),
        "a live run",
    );
    /// Work that never reached the run branch is not landed.
    reset_runs_leaves_a_dead_run_open_while_its_work_is_not_on_the_run_branch: assert_left_open(
        true,
        false,
        &[],
        None,
        "unlanded work",
    );
    /// A unit branch that never moved off its creation point carries no work: its tip is
    /// trivially an ancestor of the run branch, yet nothing was landed.
    reset_runs_leaves_a_dead_run_open_when_its_unit_branch_carries_no_work: assert_left_open(
        false,
        false,
        &[],
        None,
        "a workless branch",
    );
}
