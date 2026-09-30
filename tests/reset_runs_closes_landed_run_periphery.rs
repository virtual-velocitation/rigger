//! Integration tests for `rigger reset --runs` closing a run whose driver is dead and whose
//! remaining work is already landed on the run branch.
//!
//! The problem: a run is done only when every unit folds to `UnitIntegrated`, and only the
//! conductor mints that event (`rigger emit` refuses it). A unit the operator lands by hand -
//! its branch merged onto `rigger-run` while no driver is alive - therefore never receives its
//! terminal event, so `rigger status` reports the run as still working forever and every later
//! verb treats it as the active run. `reset --runs` records the missing `UnitIntegrated` for
//! exactly such a unit: its branch carries work of its own, that work is an ancestor of the run
//! branch, no spawn of the run awaits its result (a relaunched driver resumes every such spawn,
//! whatever its marker says), and nothing is driving the run - no `rigger step` holds the step
//! lock, no in-flight spawn's liveness marker is younger than its wall-clock bound, and no
//! registry heartbeat for the store is younger than the idle window (spec 101's liveness, the one
//! both reset modes read).
//!
//! These drive the COMPILED binary against a real git repo and `.rigger/events.db`, because the
//! observable contract is the log the operator's next `rigger status` folds.

mod common;

use common::cli::{
    configure_workdir, hold_step_lock, init_event_log, liveness_fault_body, plant_marker,
    read_run_events, real_result_body, rigger_file, run_rigger_envs, run_rigger_ok, seed_registry,
    seed_run_events,
};
use common::fixtures::{git_ok, git_ok_with_identity, git_out, temp_git_project_with_commit};
use rigger::registry;
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

/// The spawn [`IN_FLIGHT_SPAWN`] requests.
const SPAWN_ID: &str = "checkin/implementer#1";

/// A spawn requested for `checkin` with a 300 s wall-clock bound and no result yet: it awaits
/// its result, so the run stays open whatever its worker's liveness marker says.
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

/// What stands around the run while `reset --runs` reads its liveness - the three signals spec
/// 101 defines a live run by, each quiet unless a case sets it.
struct Around {
    /// The in-flight spawn's liveness marker, touched this many seconds ago (`None`: no marker).
    marker_secs_ago: Option<u64>,
    /// A `rigger step` holds the step lock for the whole reset.
    step_lock: bool,
    /// This store's registry entry last heartbeat this many ms ago (`None`: no entry).
    heartbeat_ms_ago: Option<u64>,
}

/// No signal at all: nothing is driving the run. Each case sets the one signal it stands up over
/// this.
const QUIET: Around = Around {
    marker_secs_ago: None,
    step_lock: false,
    heartbeat_ms_ago: None,
};

/// Seeds `events` into `root`, stands up the signals `around` names, runs `rigger reset --runs`,
/// and returns the run events before and after the reset.
fn reset_runs_over(
    root: &Path,
    events: &[(&str, &str)],
    around: Around,
) -> (Vec<String>, Vec<String>) {
    seed_run_events(root, events);
    let scratch = tempfile::tempdir().unwrap();
    let scratch_root = scratch.path().to_str().unwrap();
    if let Some(age) = around.marker_secs_ago {
        plant_marker(
            &rigger::liveness::marker_path(scratch_root, "r1", SPAWN_ID).unwrap(),
            age,
        );
    }
    let toplevel = git_out(root, &["rev-parse", "--show-toplevel"]);
    let seeded = around
        .heartbeat_ms_ago
        .map(|ago| seed_registry("proj", &toplevel, registry::now_ms() - ago).0);
    let mut envs = vec![("RIGGER_TMPDIR", scratch_root)];
    if let Some(home) = &seeded {
        envs.push(("XDG_STATE_HOME", home.path().to_str().unwrap()));
    }
    let before = run_log(root);
    let lock = around.step_lock.then(|| hold_step_lock(root));
    let (_out, err, ok) = run_rigger_envs(root, &["reset", "--runs"], &envs);
    drop(lock);
    assert!(ok, "rigger reset --runs must exit 0; stderr:\n{err}");
    (before, run_log(root))
}

/// Every event of `root`'s run stream, oldest first, as `<type> <body>`.
fn run_log(root: &Path) -> Vec<String> {
    read_run_events(root)
        .into_iter()
        .map(|e| format!("{} {}", e.type_, String::from_utf8_lossy(&e.data)))
        .collect()
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
    let (before, after) = reset_runs_over(root, DEAD_RUN, QUIET);
    assert_checkin_closed(root, &before, &after);
    assert_eq!(
        run_rigger_ok(root, &["status", "--line"]).trim(),
        "- . 2/2 units . done"
    );
}

/// A registry entry whose heartbeat is a minute past the idle window is what a dead driver's
/// registration leaves behind: nothing is driving the run, so the landed unit is closed.
#[test]
fn reset_runs_closes_a_dead_run_whose_registry_heartbeat_outlived_the_idle_window() {
    let dir = project(true, true);
    let root = dir.path();
    let (before, after) = reset_runs_over(
        root,
        DEAD_RUN,
        Around {
            heartbeat_ms_ago: Some(registry::DEFAULT_IDLE_MS + 60_000),
            ..QUIET
        },
    );
    assert_checkin_closed(root, &before, &after);
}

/// The negative space of the rule: each case must leave the log exactly as it was.
fn assert_left_open(
    commit_work: bool,
    land: bool,
    extra: &[(&str, &str)],
    around: Around,
    why: &str,
) {
    let dir = project(commit_work, land);
    let root = dir.path();
    let events: Vec<(&str, &str)> = DEAD_RUN.iter().chain(extra).copied().collect();
    let (before, after) = reset_runs_over(root, &events, around);
    assert_eq!(after, before, "{why}: the log must be untouched");
    assert_eq!(
        run_rigger_ok(root, &["status", "--line"]).trim(),
        "checkin . 1/2 units . working",
        "{why}: the run must stay open"
    );
}

rigger::test_cases! {
    /// A live driver (an in-flight spawn, answered only by the step's liveness fault, whose
    /// resumed worker touched its marker just now) owns the run: even a landed branch is not the
    /// operator's to close. The fault answers the spawn, so the marker alone keeps the run open.
    reset_runs_leaves_a_live_run_untouched_even_when_its_branch_is_landed: {
        let fault = liveness_fault_body(SPAWN_ID);
        assert_left_open(
            true,
            true,
            &[IN_FLIGHT_SPAWN, ("SpawnResult", &fault)],
            Around {
                marker_secs_ago: Some(0),
                ..QUIET
            },
            "a live run",
        );
    };
    /// A `rigger step` holding the step lock is driving the run: a landed branch stays open.
    reset_runs_leaves_a_landed_run_open_while_a_step_holds_the_lock: assert_left_open(
        true,
        true,
        &[],
        Around {
            step_lock: true,
            ..QUIET
        },
        "a held step lock",
    );
    /// A registry heartbeat a minute inside the idle window is a live driver elsewhere on this
    /// machine: a landed branch stays open.
    reset_runs_leaves_a_landed_run_open_while_its_registry_heartbeat_is_inside_the_idle_window:
        assert_left_open(
            true,
            true,
            &[],
            Around {
                heartbeat_ms_ago: Some(registry::DEFAULT_IDLE_MS - 60_000),
                ..QUIET
            },
            "a heartbeat inside the idle window",
        );
    /// Work that never reached the run branch is not landed.
    reset_runs_leaves_a_dead_run_open_while_its_work_is_not_on_the_run_branch: assert_left_open(
        true,
        false,
        &[],
        QUIET,
        "unlanded work",
    );
    /// A unit branch that never moved off its creation point carries no work: its tip is
    /// trivially an ancestor of the run branch, yet nothing was landed.
    reset_runs_leaves_a_dead_run_open_when_its_unit_branch_carries_no_work: assert_left_open(
        false,
        false,
        &[],
        QUIET,
        "a workless branch",
    );
}

// ---------------------------------------------------------------------------------------
// A spawn that awaits its result keeps the run open, whatever its marker says
// ---------------------------------------------------------------------------------------

/// A driver that died mid-spawn leaves the spawn unanswered, and a relaunched driver resumes
/// every unanswered spawn of the run, so closing its unit would hand that driver a spawn of an
/// integrated unit. Whether its worker never touched a marker or the marker outlived the spawn's
/// 300 s bound, the landed unit stays open until the spawn's result is recorded.
#[test]
fn reset_runs_leaves_a_landed_run_open_while_a_spawn_awaits_its_result() {
    for (marker_secs_ago, why) in [
        (None, "an unanswered spawn with no marker"),
        (
            Some(301),
            "an unanswered spawn whose marker outlived its bound",
        ),
    ] {
        assert_left_open(
            true,
            true,
            &[IN_FLIGHT_SPAWN],
            Around {
                marker_secs_ago,
                ..QUIET
            },
            why,
        );
    }
}

/// A spawn result the log cannot decode leaves unknown which spawns still await theirs, so
/// `reset --runs` fails naming the undecodable result and records nothing, rather than reading
/// it as no spawn awaiting and closing the landed unit.
#[test]
fn reset_runs_fails_on_a_malformed_spawn_result_and_closes_nothing() {
    let dir = project(true, true);
    let root = dir.path();
    let events: Vec<(&str, &str)> = DEAD_RUN
        .iter()
        .copied()
        .chain([("SpawnResult", "{}")])
        .collect();
    seed_run_events(root, &events);
    let before = run_log(root);
    let scratch = tempfile::tempdir().unwrap();
    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--runs"],
        &[("RIGGER_TMPDIR", scratch.path().to_str().unwrap())],
    );
    assert!(
        !ok,
        "a malformed spawn result must fail reset --runs, never close the run; stdout: {out:?}"
    );
    assert!(
        err.contains("missing field `id`"),
        "the failure must name the undecodable result; stderr: {err:?}"
    );
    assert_eq!(
        run_log(root),
        before,
        "a reset that cannot read which spawns await their result must record nothing"
    );
}

// ---------------------------------------------------------------------------------------
// A liveness fault is not the spawn's end; a real result is
// ---------------------------------------------------------------------------------------

rigger::test_cases! {
    /// The in-flight spawn's only result is the step's liveness fault, and a worker that resumed
    /// touched its marker a minute ago, inside its 300 s bound: the spawn is still in flight and
    /// drives the run, so even a landed branch stays open - the one live-spawn rule `reset
    /// --derived` refuses on.
    reset_runs_leaves_a_landed_run_open_while_a_liveness_faulted_spawns_marker_is_inside_its_bound: {
        let fault = liveness_fault_body(SPAWN_ID);
        assert_left_open(
            true,
            true,
            &[IN_FLIGHT_SPAWN, ("SpawnResult", &fault)],
            Around {
                marker_secs_ago: Some(60),
                ..QUIET
            },
            "a liveness-faulted spawn whose marker is inside its bound",
        );
    };
    /// The same fault with the marker a second past its bound: the worker is gone, nothing drives
    /// the run, and the landed unit is closed.
    reset_runs_closes_a_dead_run_whose_liveness_faulted_spawns_marker_outlived_its_bound: {
        let fault = liveness_fault_body(SPAWN_ID);
        closes_the_landed_checkin(&[IN_FLIGHT_SPAWN, ("SpawnResult", &fault)], 301);
    };
    /// The fault, then a real result, with the marker touched just now: the real result ended
    /// the spawn whatever its marker says, so nothing drives the run and the landed unit is
    /// closed.
    reset_runs_closes_a_dead_run_once_a_real_result_follows_the_liveness_fault_despite_a_fresh_marker: {
        let (fault, real) = (liveness_fault_body(SPAWN_ID), real_result_body(SPAWN_ID));
        closes_the_landed_checkin(
            &[IN_FLIGHT_SPAWN, ("SpawnResult", &fault), ("SpawnResult", &real)],
            0,
        );
    };
}

/// Asserts `reset --runs` over [`DEAD_RUN`] then `extra`, with the in-flight spawn's marker
/// touched `marker_secs_ago` seconds back and no other signal, closes the landed `checkin` unit.
fn closes_the_landed_checkin(extra: &[(&str, &str)], marker_secs_ago: u64) {
    let dir = project(true, true);
    let root = dir.path();
    let events: Vec<(&str, &str)> = DEAD_RUN.iter().chain(extra).copied().collect();
    let (before, after) = reset_runs_over(
        root,
        &events,
        Around {
            marker_secs_ago: Some(marker_secs_ago),
            ..QUIET
        },
    );
    assert_checkin_closed(root, &before, &after);
}

// ---------------------------------------------------------------------------------------
// The scratch root is read fail-closed
// ---------------------------------------------------------------------------------------

/// Given a landed run whose in-flight spawn - answered only by the step's liveness fault, so its
/// marker alone keeps the run open - had its resumed worker touch that marker just now under the
/// store's configured `defaults.workdir`, where a run stamps it, `reset --runs` reads it there and
/// leaves the run open. When that `defaults` block turns unparsable (a `max_retries` that is not a
/// number beside the same `workdir`), the scratch root is never degraded to the default root,
/// where the marker would read as absent and the live worker's run would be closed out from under
/// it: `reset --runs` exits non-zero naming the config, and the log is left exactly as it was.
#[test]
fn reset_runs_fails_closed_on_an_unparsable_defaults_block_and_closes_nothing() {
    let dir = project(true, true);
    let root = dir.path();
    let fault = liveness_fault_body(SPAWN_ID);
    let events: Vec<(&str, &str)> = DEAD_RUN
        .iter()
        .copied()
        .chain([IN_FLIGHT_SPAWN, ("SpawnResult", &fault)])
        .collect();
    seed_run_events(root, &events);
    let (_workdir, scratch_root) = configure_workdir(root, "");
    plant_marker(
        &rigger::liveness::marker_path(&scratch_root, "r1", SPAWN_ID).unwrap(),
        0,
    );
    let before = run_log(root);
    let reset = || run_rigger_envs(root, &["reset", "--runs"], &[("RIGGER_TMPDIR", "")]);

    let (_out, err, ok) = reset();
    assert!(
        ok,
        "a parsable defaults block resolves the workdir's scratch root; stderr:\n{err}"
    );
    assert_eq!(
        run_log(root),
        before,
        "the fresh marker under the configured workdir keeps the landed run open"
    );

    let workflow = rigger_file(root, "workflow.yml");
    let mut defaults = std::fs::read_to_string(&workflow).unwrap();
    defaults.push_str("  max_retries: three\n");
    std::fs::write(&workflow, defaults).unwrap();

    let (out, err, ok) = reset();
    assert!(
        !ok,
        "an unparsable defaults block must fail reset --runs, never close the run; stdout: \
         {out:?}"
    );
    assert!(
        err.contains("parse workflow") && err.contains("max_retries"),
        "the failure must name the unparsable config; stderr: {err:?}"
    );
    assert_eq!(
        run_log(root),
        before,
        "a reset that cannot resolve the scratch root must record nothing"
    );
}
