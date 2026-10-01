//! Integration tests for the live-writer guard in front of `rigger reset --derived` (spec 71,
//! criterion 2, whose definition of "live" spec 101, criterion 5 now owns).
//!
//! The recorded incident this guard exists to prevent: `rigger reset --derived` compacts the
//! event log by keeping only each file's latest generation of the derived index, at the latest
//! event per replay key, which leaves REVISION GAPS by design. A writer whose append cursor was
//! built before that compaction ran can reissue one of those gap revisions; every later event
//! then sorts BELOW the run boundary in revision order. `rigger reset --derived` must refuse to
//! compact while run machinery is live, naming what is live, unless the operator explicitly
//! overrides with `--force-live`.
//!
//! THE GUARD READS LIVENESS (spec 101): a run is live when a `rigger step` holds the step lock,
//! when an in-flight spawn's liveness marker is younger than that spawn's wall-clock bound, or
//! when a registry heartbeat for this store is younger than the idle window. A spawn is in flight
//! until a real result is recorded for it: the step's own liveness fault does not end it. Unit
//! terminality is not a liveness signal: a run whose driver died leaves its units non-terminal
//! forever, and that run - the one whose bloat most needs the compaction - must not need
//! `--force-live` to get it.
//!
//! These tests drive the COMPILED binary against a real `.rigger/events.db`, because the
//! criterion is an operator-facing refusal whose observable effects are the command's exit
//! status, its message, and (for the guard's own writes) that the store is untouched.
//!
//! What this file OWNS and what it deliberately does not:
//!   - OWNS: the three live signals and the definition they make up, the dead-driver run that
//!     proceeds, the quiet-machinery baseline, `--force-live`, each signal judged against its own
//!     window and read where the run writes it (the scratch root read fail-closed), a liveness
//!     fault that does not end a spawn, the operator's way past an unbounded spawn, and the help
//!     and skills that state the definition.
//!   - NOT OWNED: the `--derived` prune's own selection/report/reclamation mechanics (spec 60,
//!     criterion 5 - `tests/reset_derived_compaction*.rs`), the append-time assertion (spec 71,
//!     criterion 1), and the validate advisory (spec 71, criterion 3).

mod common;

use common::cli::configure_workdir;
use common::cli::hold_step_lock;
use common::cli::liveness_fault_body;
use common::cli::plant_marker;
use common::cli::read_run_events;
use common::cli::real_result_body;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_rigger_envs;
use common::cli::seed_registry;
use common::cli::seed_run_events;
use common::cli::seed_store;
use common::cli::temp_store_project;
use common::cli::write_workflow_fixture;
use common::cli::WorkflowFixture;
use common::cli::UNISOLATED_WORKER;
use common::git::git_out;
use common::git::nested_worktree;
use common::git::temp_git_project_with_commit;
use rigger::conductor::normalize_ws;
use rigger::config::RIGGER_DIR;
use rigger::registry;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

/// The row count of the seeded event log, so a refused compaction can be proven to have pruned
/// NOTHING - the guard may only refuse, never partially act.
fn row_count(root: &Path) -> i64 {
    let conn = rusqlite::Connection::open(root.join(".rigger").join("events.db")).unwrap();
    conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap()
}

/// Runs `rigger <args>` in `root` with `envs` and asserts the derived compaction ran, `why`
/// naming the case.
fn assert_prunes(root: &Path, args: &[&str], envs: &[(&str, &str)], why: &str) {
    assert_compacted(run_rigger_envs(root, args, envs), why);
}

/// Asserts a `reset --derived` invocation's `(stdout, stderr, success)` is a compaction that ran
/// and printed its usual prune report, `why` naming the case.
fn assert_compacted(said: (String, String, bool), why: &str) {
    let (out, err, ok) = said;
    assert!(ok, "{why}; stderr: {err}");
    assert!(
        out.contains("reset --derived: pruned"),
        "{why}: must print the usual prune report; got {out:?}"
    );
}

/// Seeds `events` (none leaves the store empty) into a fresh project and asserts
/// `rigger <args>` compacts it.
fn assert_seeded_reset_prunes(events: &[(&str, &str)], args: &[&str], why: &str) {
    let dir = temp_store_project();
    let root = dir.path();
    if !events.is_empty() {
        seed_run_events(root, events);
    }
    assert_prunes(root, args, &[], why);
}

// ---------------------------------------------------------------------------------------
// Baseline: quiet machinery prunes exactly as today
// ---------------------------------------------------------------------------------------

rigger::test_cases! {
    /// With no run ever started (the quietest possible machinery), `reset --derived` succeeds
    /// exactly as it did before this guard existed - the guard adds a refusal path, never a new
    /// precondition on the happy path.
    reset_derived_prunes_when_no_run_has_ever_started: assert_seeded_reset_prunes(
        &[],
        &["reset", "--derived"],
        "a quiet store must still compact",
    );
    /// A recorded spawn that already carries a result is ANSWERED, not in flight, and a unit that
    /// has INTEGRATED is terminal, not live - neither must ever block compaction (the guard would
    /// otherwise refuse forever on ordinary run history).
    reset_derived_prunes_when_every_unit_is_terminal_and_every_spawn_is_answered: assert_seeded_reset_prunes(
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
            ("UnitStarted", r#"{"id":"a","branch":"rigger/u/a"}"#),
            (
                "SpawnRequested",
                r#"{"id":"a/implementer#0","unit":"a","stage":"implement","prompt":"go"}"#,
            ),
            ("SpawnResult", r#"{"id":"a/implementer#0","output":"done"}"#),
            ("UnitIntegrated", r#"{"id":"a","commit":"deadbeef"}"#),
        ],
        &["reset", "--derived"],
        "a terminal unit with only answered spawns must not block compaction",
    );
    /// A prior, DIFFERENT run's unanswered spawn and non-terminal unit are residue, not live work in
    /// the CURRENT run's slice - the design names "in-flight spawns in the current run's slice"
    /// precisely so a zombie from an abandoned campaign can never wedge compaction forever.
    reset_derived_ignores_a_prior_runs_unanswered_spawn_and_non_terminal_unit: assert_seeded_reset_prunes(
        &[
            ("RunStarted", r#"{"run":"r0","criteria":["an older spec"]}"#),
            (
                "UnitStarted",
                r#"{"id":"zombie","branch":"rigger/u/zombie"}"#,
            ),
            (
                "SpawnRequested",
                r#"{"id":"zombie/implementer#0","unit":"zombie","stage":"zombie","prompt":"stale"}"#,
            ),
            ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
        ],
        &["reset", "--derived"],
        "a PRIOR run's unanswered spawn/non-terminal unit must not block THIS run's compaction",
    );
}

// ---------------------------------------------------------------------------------------
// The definition (spec 101, criterion 5): the guard reads liveness, never unit terminality
// ---------------------------------------------------------------------------------------

/// The run a dead driver leaves behind: every unit non-terminal. `a` is mid-spawn (requested,
/// never answered) with a 300 s wall-clock bound, `b` sits between spawn rounds (its spawn
/// answered, a marker it touched moments before answering still on disk), and `c` was parked
/// but no worker ever touched its marker.
const DEAD_DRIVER_RUN: &[(&str, &str)] = &[
    ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
    ("UnitStarted", r#"{"id":"a","branch":"rigger/u/a"}"#),
    ("UnitStarted", r#"{"id":"b","branch":"rigger/u/b"}"#),
    ("UnitStarted", r#"{"id":"c","branch":"rigger/u/c"}"#),
    (
        "SpawnRequested",
        r#"{"id":"a/implementer#0","unit":"a","stage":"implement","prompt":"go","max_wall_clock":300}"#,
    ),
    (
        "SpawnRequested",
        r#"{"id":"b/implementer#0","unit":"b","stage":"implement","prompt":"go","max_wall_clock":300}"#,
    ),
    ("SpawnResult", r#"{"id":"b/implementer#0","output":"done"}"#),
    (
        "SpawnRequested",
        r#"{"id":"c/implementer#0","unit":"c","stage":"implement","prompt":"go","max_wall_clock":300}"#,
    ),
];

/// One of the three signals that make a run live (spec 101), by what a refusal names it with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Signal(&'static str);

/// A `rigger step` holds the step lock.
const STEP_LOCK: Signal = Signal("step.lock");
/// In-flight spawn `a/implementer#0`'s liveness marker is younger than its 300 s bound.
const SPAWN_MARKER: Signal = Signal("a/implementer#0");
/// This store's registry entry has a heartbeat younger than `registry::DEFAULT_IDLE_MS`.
const HEARTBEAT: Signal = Signal("registration");
const ALL_SIGNALS: [Signal; 3] = [STEP_LOCK, SPAWN_MARKER, HEARTBEAT];

/// What one `rigger` invocation did: its stdout, stderr and success, and the event log's row
/// count before and after it.
struct Outcome {
    out: String,
    err: String,
    ok: bool,
    rows_before: i64,
    rows_after: i64,
}

impl Outcome {
    /// The invocation's `(stdout, stderr, success)`, as [`assert_compacted`] reads it.
    fn said(self) -> (String, String, bool) {
        (self.out, self.err, self.ok)
    }
}

/// Runs `rigger <args>` in `cwd` with `envs` over the seeded store at `root`, counting its event
/// log's rows before and after.
fn counted(root: &Path, cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> Outcome {
    let rows_before = row_count(root);
    let (out, err, ok) = run_rigger_envs(cwd, args, envs);
    Outcome {
        out,
        err,
        ok,
        rows_before,
        rows_after: row_count(root),
    }
}

/// Runs `rigger <args>` over [`DEAD_DRIVER_RUN`] with exactly the `fresh` signals live and every
/// other one stale or free: the step lock free, `a`'s marker touched 301 s ago (just past its
/// 300 s bound), the registry heartbeat a minute past the idle window. `b`'s marker is always
/// touched just now - a spawn with a real result has ended whatever its marker says - and `c` has
/// none.
fn reset_over_a_dead_drivers_run(fresh: &[Signal], args: &[&str]) -> Outcome {
    let dir = temp_store_project();
    let root = dir.path();
    seed_run_events(root, DEAD_DRIVER_RUN);

    let scratch = tempfile::tempdir().expect("create the scratch root the markers live under");
    let scratch_root = scratch.path().to_str().unwrap();
    let marker = |id: &str| rigger::liveness::marker_path(scratch_root, "r1", id).unwrap();
    let a_age = if fresh.contains(&SPAWN_MARKER) {
        0
    } else {
        301
    };
    plant_marker(&marker("a/implementer#0"), a_age);
    plant_marker(&marker("b/implementer#0"), 0);

    let now_ms = registry::now_ms();
    let heartbeat_ms = if fresh.contains(&HEARTBEAT) {
        now_ms
    } else {
        now_ms - registry::DEFAULT_IDLE_MS - 60_000
    };
    let toplevel = git_out(root, &["rev-parse", "--show-toplevel"]);
    let (state_home, _entry) = seed_registry("proj", &toplevel, heartbeat_ms);

    let lock = fresh.contains(&STEP_LOCK).then(|| hold_step_lock(root));

    let outcome = counted(
        root,
        root,
        args,
        &[
            ("RIGGER_TMPDIR", scratch_root),
            ("XDG_STATE_HOME", state_home.path().to_str().unwrap()),
        ],
    );
    drop(lock);
    outcome
}

/// `reset --derived` over the dead driver's run refuses while `live` alone is fresh, naming it
/// and nothing else - not the stale signals, not the answered or marker-less spawns, never a
/// non-terminal unit - and prunes NOTHING.
fn assert_refused_naming_only(live: Signal) {
    let o = reset_over_a_dead_drivers_run(&[live], &["reset", "--derived"]);
    let err = &o.err;
    assert!(!o.ok, "a fresh {live:?} must refuse; stdout: {:?}", o.out);
    assert!(
        err.contains(live.0),
        "the refusal must name the live {live:?}; stderr: {err:?}"
    );
    for quiet in ALL_SIGNALS.into_iter().filter(|s| *s != live) {
        assert!(
            !err.contains(quiet.0),
            "a stale or free {quiet:?} is not live and must not be named; stderr: {err:?}"
        );
    }
    for not_live in ["b/implementer#0", "c/implementer#0", "not yet terminal"] {
        assert!(
            !err.contains(not_live),
            "{not_live:?} is not a liveness signal and must not be named; stderr: {err:?}"
        );
    }
    assert!(
        err.contains("--force-live"),
        "the refusal must name the override; stderr: {err:?}"
    );
    assert_eq!(
        o.rows_after, o.rows_before,
        "a refused compaction must prune NOTHING - the guard may only refuse, never partially act"
    );
}

/// Every unit non-terminal, yet the step lock is free, every in-flight marker is older than its
/// bound and the registry heartbeat is older than the idle window: nothing is driving the run, so
/// `reset --derived` compacts without `--force-live`.
#[test]
fn reset_derived_proceeds_over_a_dead_drivers_non_terminal_run_without_force_live() {
    assert_compacted(
        reset_over_a_dead_drivers_run(&[], &["reset", "--derived"]).said(),
        "a run whose driver is dead must compact without --force-live",
    );
}

rigger::test_cases! {
    /// A held step lock alone makes the run live.
    reset_derived_refuses_the_dead_drivers_run_while_a_step_holds_the_lock:
        assert_refused_naming_only(STEP_LOCK);
    /// An in-flight spawn whose marker is younger than its wall-clock bound alone makes the run
    /// live.
    reset_derived_refuses_the_dead_drivers_run_while_an_in_flight_spawns_marker_is_fresh:
        assert_refused_naming_only(SPAWN_MARKER);
    /// A registry heartbeat for this store younger than the idle window alone makes the run live.
    reset_derived_refuses_the_dead_drivers_run_while_its_registry_heartbeat_is_fresh:
        assert_refused_naming_only(HEARTBEAT);
}

/// `--force-live` skips the check entirely: with all three signals live it still compacts and
/// prints the ordinary prune report.
#[test]
fn reset_derived_force_live_compacts_while_every_signal_is_live() {
    assert_compacted(
        reset_over_a_dead_drivers_run(&ALL_SIGNALS, &["reset", "--derived", "--force-live"]).said(),
        "--force-live must skip the guard",
    );
}

// ---------------------------------------------------------------------------------------
// Each signal read where the run writes it, judged against its own window
// ---------------------------------------------------------------------------------------

/// The spawn every case below leaves unanswered.
const SPAWN: &str = "a/implementer#0";

/// Run `run`, whose one unit `a` is mid-spawn: [`SPAWN`] requested with the wall-clock bound
/// `bound` (`None`: unbounded) and never answered - what a driver that died mid-spawn leaves
/// behind.
fn one_unanswered_spawn(run: &str, bound: Option<u64>) -> Vec<(&'static str, String)> {
    let bound = bound.map_or(String::new(), |secs| format!(r#","max_wall_clock":{secs}"#));
    vec![
        (
            "RunStarted",
            format!(r#"{{"run":"{run}","criteria":["crit"]}}"#),
        ),
        (
            "UnitStarted",
            r#"{"id":"a","branch":"rigger/u/a"}"#.to_string(),
        ),
        (
            "SpawnRequested",
            format!(r#"{{"id":"{SPAWN}","unit":"a","stage":"implement","prompt":"go"{bound}}}"#),
        ),
    ]
}

/// Seeds `events` into `root`'s run stream, in order.
fn seed_owned(root: &Path, events: &[(&str, String)]) {
    let borrowed: Vec<(&str, &str)> = events.iter().map(|(t, b)| (*t, b.as_str())).collect();
    seed_run_events(root, &borrowed);
}

/// Runs `rigger reset --derived` over `events` with the step lock free, [`SPAWN`]'s liveness
/// marker for each `(run id, secs ago)` in `markers` planted under the `RIGGER_TMPDIR` scratch
/// root, and this store's registry entry last heard from `heartbeat_ms_ago` ms back (`None`: no
/// entry).
fn reset_derived_around(
    events: &[(&str, String)],
    markers: &[(&str, u64)],
    heartbeat_ms_ago: Option<u64>,
) -> Outcome {
    let dir = temp_store_project();
    let root = dir.path();
    seed_owned(root, events);
    let scratch = tempfile::tempdir().expect("create the scratch root the markers live under");
    let scratch_root = scratch.path().to_str().unwrap();
    for (run, secs_ago) in markers {
        plant_marker(
            &rigger::liveness::marker_path(scratch_root, run, SPAWN).unwrap(),
            *secs_ago,
        );
    }
    let toplevel = git_out(root, &["rev-parse", "--show-toplevel"]);
    let seeded =
        heartbeat_ms_ago.map(|ago| seed_registry("proj", &toplevel, registry::now_ms() - ago).0);
    let mut envs = vec![("RIGGER_TMPDIR", scratch_root)];
    // With no entry to seed, the binary's own fresh state home holds no registration at all.
    if let Some(home) = &seeded {
        envs.push(("XDG_STATE_HOME", home.path().to_str().unwrap()));
    }
    counted(root, root, &["reset", "--derived"], &envs)
}

/// The age, in whole seconds, a refusal names for [`SPAWN`]'s liveness marker: the digits that
/// follow `<SPAWN> (marker touched `.
fn named_marker_age(err: &str) -> u64 {
    let lead = format!("{SPAWN} (marker touched ");
    let at = err
        .find(&lead)
        .unwrap_or_else(|| panic!("the refusal must name {SPAWN}'s marker age; stderr: {err:?}"))
        + lead.len();
    let digits: String = err[at..].chars().take_while(char::is_ascii_digit).collect();
    digits
        .parse()
        .unwrap_or_else(|e| panic!("{SPAWN}'s marker age {digits:?} must be whole seconds: {e}"))
}

/// The refusal's one reason for a live [`SPAWN`] - its marker's age, which must lie in `age` (a
/// range, since seconds tick while the binary starts), followed by `tail`, the bound that keeps it
/// live - naming nothing else, and the event log left exactly as it was.
fn assert_refused_naming_the_spawn(
    o: Outcome,
    age: impl std::ops::RangeBounds<u64> + std::fmt::Debug,
    tail: &str,
) {
    let err = &o.err;
    assert!(
        !o.ok,
        "a live spawn must refuse the compaction; stdout: {:?}",
        o.out
    );
    assert!(
        err.contains(&format!(
            "1 in-flight spawn(s) in the current run touched their liveness marker within their \
             wall-clock bound: {SPAWN} (marker touched "
        )),
        "the refusal must name {SPAWN} as the one live spawn; stderr: {err:?}"
    );
    let named = named_marker_age(err);
    assert!(
        age.contains(&named),
        "the refusal names {SPAWN}'s marker as touched {named}s ago, outside {age:?}; stderr: \
         {err:?}"
    );
    assert!(
        err.contains(&format!("{named}{tail}")),
        "the marker's age must be followed by {tail:?}; stderr: {err:?}"
    );
    assert!(
        !err.contains("step.lock") && !err.contains("driver registration"),
        "only the spawn is live, so only the spawn may be named; stderr: {err:?}"
    );
    assert_eq!(
        o.rows_after, o.rows_before,
        "a refused compaction must prune NOTHING - the guard may only refuse, never partially act"
    );
}

/// The idle window, in whole seconds, as the refusal states it.
fn idle_window_secs() -> u64 {
    registry::DEFAULT_IDLE_MS / 1000
}

rigger::test_cases! {
    /// A spawn is judged against its OWN recorded bound, not the registry's idle window: a marker
    /// touched half an hour ago - well past the 900 s idle window - is still inside the spawn's
    /// 3600 s bound, so the run is live and the refusal names that bound.
    reset_derived_refuses_while_a_marker_is_inside_its_own_bound_though_past_the_idle_window: {
        assert!(idle_window_secs() < 1800, "the case must sit past the idle window");
        assert_refused_naming_the_spawn(
            reset_derived_around(&one_unanswered_spawn("r1", Some(3600)), &[("r1", 1800)], None),
            1800..3600,
            "s ago, bound 3600s)",
        );
    };
    /// A minute past that same 3600 s bound the marker is stale: nothing drives the run and it
    /// compacts without `--force-live`.
    reset_derived_proceeds_once_a_marker_outlives_its_own_bound: assert_compacted(
        reset_derived_around(&one_unanswered_spawn("r1", Some(3600)), &[("r1", 3660)], None)
            .said(),
        "a marker a minute past its spawn's own bound is not live",
    );
}

// ---------------------------------------------------------------------------------------
// A liveness fault is not the spawn's end; a real result is
// ---------------------------------------------------------------------------------------

/// [`one_unanswered_spawn`] in run `r1`, bounded at 300 s, then each of `results` recorded on
/// [`SPAWN`] in order: the step's liveness fault ([`liveness_fault_body`]) - a diagnosis of a
/// silent worker, not its end: the replay driver re-parks the spawn, and a worker that resumes
/// touches its marker again - or a real result ([`real_result_body`]), which ends the spawn.
fn one_spawn_answered_by(results: &[String]) -> Vec<(&'static str, String)> {
    let mut events = one_unanswered_spawn("r1", Some(300));
    events.extend(results.iter().map(|r| ("SpawnResult", r.clone())));
    events
}

rigger::test_cases! {
    /// The spawn's only result is the step's liveness fault, and a worker that resumed touched
    /// its marker a minute ago, inside its 300 s bound: the spawn is still in flight, so the run
    /// is live - the refusal names it with its marker's age and bound, and prunes nothing.
    reset_derived_refuses_while_a_liveness_faulted_spawns_marker_is_inside_its_bound:
        assert_refused_naming_the_spawn(
            reset_derived_around(
                &one_spawn_answered_by(&[liveness_fault_body(SPAWN)]),
                &[("r1", 60)],
                None,
            ),
            60..300,
            "s ago, bound 300s)",
        );
    /// The same fault with the marker a second past its bound: the worker is gone, nothing
    /// drives the run, and it compacts without `--force-live`.
    reset_derived_proceeds_once_a_liveness_faulted_spawns_marker_outlives_its_bound:
        assert_compacted(
            reset_derived_around(
                &one_spawn_answered_by(&[liveness_fault_body(SPAWN)]),
                &[("r1", 301)],
                None,
            )
            .said(),
            "a liveness-faulted spawn whose marker outlived its bound is not live",
        );
    /// The fault, then a real result, with the marker touched just now: the real result ended
    /// the spawn whatever its marker says, so the run compacts.
    reset_derived_proceeds_once_a_real_result_follows_the_liveness_fault_despite_a_fresh_marker:
        assert_compacted(
            reset_derived_around(
                &one_spawn_answered_by(&[liveness_fault_body(SPAWN), real_result_body(SPAWN)]),
                &[("r1", 0)],
                None,
            )
            .said(),
            "a real result after the liveness fault ends the spawn, however fresh its marker",
        );
}

/// A PRIOR run's marker is never read for the current run: run `r0` and run `r1` each left
/// [`SPAWN`] unanswered, and only the marker filed under the CURRENT run's id decides whether it
/// is live - a fresh `r0` marker beside a stale `r1` one compacts, and the mirror refuses.
fn prior_and_current_run(prior_marker_secs_ago: u64, current_marker_secs_ago: u64) -> Outcome {
    let events: Vec<(&str, String)> = one_unanswered_spawn("r0", Some(300))
        .into_iter()
        .chain(one_unanswered_spawn("r1", Some(300)))
        .collect();
    reset_derived_around(
        &events,
        &[
            ("r0", prior_marker_secs_ago),
            ("r1", current_marker_secs_ago),
        ],
        None,
    )
}

rigger::test_cases! {
    /// A fresh marker a PRIOR run filed for the same spawn id never makes the current run live.
    reset_derived_ignores_a_prior_runs_fresh_marker_for_the_same_spawn_id: assert_compacted(
        prior_and_current_run(0, 301).said(),
        "only the current run's marker decides, and it is past its bound",
    );
    /// The current run's own fresh marker makes it live, whatever the prior run's says.
    reset_derived_reads_the_current_runs_marker_beside_a_prior_runs_stale_one:
        assert_refused_naming_the_spawn(prior_and_current_run(301, 0), 0..300, "s ago, bound 300s)");
}

/// A registry heartbeat a minute INSIDE the idle window keeps the run live - the window, not
/// "just now", is what the guard judges it against - while the lock is free and no spawn marker is
/// on disk; the refusal names the registration and the window, and nothing else.
#[test]
fn reset_derived_refuses_while_the_registry_heartbeat_is_a_minute_inside_the_idle_window() {
    let (out, err, ok) = reset_derived_around(
        &one_unanswered_spawn("r1", Some(300)),
        &[],
        Some(registry::DEFAULT_IDLE_MS - 60_000),
    )
    .said();
    assert!(
        !ok,
        "a heartbeat inside the idle window must refuse; stdout: {out:?}"
    );
    let reason = format!(
        "1 driver registration(s) for this project's store in the machine-global instance \
         registry (spec 50) heartbeat within the last {}s",
        idle_window_secs()
    );
    assert!(
        err.contains(&reason),
        "the refusal must name the registration and the idle window; stderr: {err:?}"
    );
    assert!(
        !err.contains("step.lock") && !err.contains("in-flight spawn(s)"),
        "only the registration is live, so only it may be named; stderr: {err:?}"
    );
}

// ---------------------------------------------------------------------------------------
// The spawn signal is read where the writer stamps it
// ---------------------------------------------------------------------------------------

/// `rigger reset --derived` run from `cwd` with `RIGGER_TMPDIR` set to `rigger_tmpdir` - blank
/// for no override, which the scratch-root resolver reads as unset whatever the ambient
/// environment carries - so the binary resolves the scratch root exactly as the run that stamped
/// the marker did, over [`one_unanswered_spawn`] (bound 300 s) in `root`'s store, whose worker
/// touched its marker just now under `scratch_root`.
fn reset_from_with_a_fresh_marker_under(
    root: &Path,
    cwd: &Path,
    rigger_tmpdir: &str,
    scratch_root: &str,
) -> Outcome {
    seed_owned(root, &one_unanswered_spawn("r1", Some(300)));
    plant_marker(
        &rigger::liveness::marker_path(scratch_root, "r1", SPAWN).unwrap(),
        0,
    );
    counted(
        root,
        cwd,
        &["reset", "--derived"],
        &[("RIGGER_TMPDIR", rigger_tmpdir)],
    )
}

/// `root`'s subdirectory `sub`, created, for a command run from below the store's owning root.
fn subdirectory_of(root: &Path) -> PathBuf {
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).expect("create the subdirectory the command runs from");
    sub
}

/// With no `defaults.workdir` configured, a run stamps its markers under the store owner's
/// default cache scratch root, and the guard reads them there.
#[test]
fn reset_derived_reads_a_marker_stamped_under_the_default_scratch_root() {
    let dir = temp_store_project();
    let root = dir.path();
    let scratch_root = common::default_scratch_root(root);
    assert_refused_naming_the_spawn(
        reset_from_with_a_fresh_marker_under(root, root, "", scratch_root.to_str().unwrap()),
        0..300,
        "s ago, bound 300s)",
    );
}

/// With `defaults.workdir` configured in the store's `workflow.yml`, a run stamps its markers
/// under that workdir - not the default cache root - and the guard reads them there.
#[test]
fn reset_derived_reads_a_marker_stamped_under_a_configured_workdir() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_workdir, scratch_root) = configure_workdir(root, "");
    assert_refused_naming_the_spawn(
        reset_from_with_a_fresh_marker_under(root, root, "", &scratch_root),
        0..300,
        "s ago, bound 300s)",
    );
}

/// The scratch root is read FAIL-CLOSED: a `defaults` block that cannot be parsed - `workdir`
/// configured beside a `max_retries` that is not a number - never degrades to the default root,
/// where the fresh marker under the configured workdir would read as absent. `reset --derived`
/// exits non-zero naming the unparsable config and prunes nothing: the guard may only refuse.
#[test]
fn reset_derived_fails_closed_on_an_unparsable_defaults_block_beside_a_configured_workdir() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_workdir, scratch_root) = configure_workdir(root, "  max_retries: three\n");
    let o = reset_from_with_a_fresh_marker_under(root, root, "", &scratch_root);
    assert!(
        !o.ok,
        "an unparsable defaults block must fail the command, never compact; stdout: {:?}",
        o.out
    );
    assert!(
        o.err.contains("parse workflow") && o.err.contains("max_retries"),
        "the failure must name the unparsable config; stderr: {:?}",
        o.err
    );
    assert_eq!(
        o.rows_after, o.rows_before,
        "a command that cannot resolve the scratch root must prune NOTHING"
    );
}

/// A store whose OWNING root has no UTF-8 path resolves no scratch root to read its spawns'
/// markers or its build caches under, and every reset mode that reads that root refuses it rather
/// than reading the unresolved root as "no spawn marker, so nothing is live": `reset --derived`,
/// `reset --runs` and `reset --build-cache` each exit non-zero naming why, and none so much as
/// opens the store first - its `events.db` is still the empty file it was seeded as, never given
/// a schema, migrated or pruned.
#[cfg(unix)]
#[test]
fn reset_refuses_a_store_whose_owning_root_has_no_utf8_path_and_opens_nothing() {
    use std::os::unix::ffi::OsStrExt;
    let parent = tempfile::tempdir().expect("create the parent of the non-UTF-8 root");
    let root = parent
        .path()
        .join(std::ffi::OsStr::from_bytes(b"owning-\xff-root"));
    seed_store(&root);
    let events_db = rigger_file(&root, "events.db");
    for mode in ["--derived", "--runs", "--build-cache"] {
        let (out, err, ok) = run_rigger_envs(&root, &["reset", mode], &[]);
        assert!(
            !ok,
            "reset {mode} must refuse a store whose scratch root it cannot resolve; stdout: \
             {out:?}"
        );
        assert!(
            err.contains(&format!(
                "reset: the store at {} has no UTF-8 owning root, so the scratch root its runs \
                 write their liveness markers and build caches under cannot be resolved; \
                 refusing rather than reading that as no live spawn",
                root.join(RIGGER_DIR).display()
            )),
            "reset {mode} must name the unresolvable owning root; stderr: {err:?}"
        );
        assert_eq!(
            std::fs::metadata(&events_db)
                .expect("the seeded store is still there")
                .len(),
            0,
            "reset {mode} must refuse before it opens the store"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Only a mode that reads the scratch root resolves it
// ---------------------------------------------------------------------------------------

/// Configures `root`'s store with a `defaults` block that cannot be parsed - `workdir` beside a
/// `max_retries` that is not a number - and returns the configured workdir.
fn configure_unparsable_defaults(root: &Path) -> tempfile::TempDir {
    configure_workdir(root, "  max_retries: three\n").0
}

/// Runs `rigger <args>` from `root` over a private cache home holding one scratch root keyed on
/// a checkout that no longer exists - exactly what `reset --scratch-orphans` reclaims - and
/// returns the invocation's `(stdout, stderr, success)` with whether that root still stands.
fn reset_beside_an_orphan_root(root: &Path, args: &[&str]) -> ((String, String, bool), bool) {
    let cache = tempfile::tempdir().expect("create the private cache home");
    let gone = cache.path().join("deleted-checkout");
    let orphan = cache
        .path()
        .join("rigger")
        .join(rigger::liveness::marker_filename(gone.to_str().unwrap()).unwrap());
    std::fs::create_dir_all(&orphan).expect("plant the orphan scratch root");
    let said = run_rigger_envs(
        root,
        args,
        &[("XDG_CACHE_HOME", cache.path().to_str().unwrap())],
    );
    (said, orphan.exists())
}

/// `rigger <args>` - a selection that includes `--scratch-orphans` - from `root` exits 0, reports
/// its sweep and reclaims the orphan root, `why` naming the case; returns the invocation's
/// `(stdout, stderr, success)`.
fn assert_sweeps_the_orphan_root(root: &Path, args: &[&str], why: &str) -> (String, String, bool) {
    let ((out, err, ok), orphan_stands) = reset_beside_an_orphan_root(root, args);
    assert!(ok, "{why}: {args:?} must succeed; stderr: {err:?}");
    assert!(
        out.contains("--scratch-orphans: reclaimed 1 scratch root(s) whose repo no longer exists"),
        "{why}: {args:?} must report its sweep; stdout: {out:?}"
    );
    assert!(
        !orphan_stands,
        "{why}: the orphan scratch root must be reclaimed"
    );
    (out, err, ok)
}

/// `reset --scratch-orphans` reads no configuration, so a `defaults` block the live-writer guard
/// fails closed on never stops its sweep.
#[test]
fn reset_scratch_orphans_sweeps_over_an_unparsable_defaults_block() {
    let dir = temp_store_project();
    let root = dir.path();
    let _workdir = configure_unparsable_defaults(root);
    assert_sweeps_the_orphan_root(
        root,
        &["reset", "--scratch-orphans"],
        "an unparsable defaults block",
    );
}

/// `reset --scratch-orphans` reads no scratch root, so a store whose owning root has no UTF-8
/// path - which resolves none - never stops its sweep with the refusal the reading modes give.
#[cfg(unix)]
#[test]
fn reset_scratch_orphans_sweeps_over_a_store_whose_owning_root_has_no_utf8_path() {
    use std::os::unix::ffi::OsStrExt;
    let parent = tempfile::tempdir().expect("create the parent of the non-UTF-8 root");
    let root = parent
        .path()
        .join(std::ffi::OsStr::from_bytes(b"owning-\xff-root"));
    seed_store(&root);
    assert_sweeps_the_orphan_root(
        &root,
        &["reset", "--scratch-orphans"],
        "a non-UTF-8 owning root",
    );
}

/// `--force-live` skips the live-writer guard entirely, its scratch-root read included, so over a
/// `defaults` block the guard could not parse `reset --derived --force-live` still compacts.
#[test]
fn reset_derived_force_live_compacts_over_an_unparsable_defaults_block() {
    let dir = temp_store_project();
    let root = dir.path();
    let _workdir = configure_unparsable_defaults(root);
    assert_prunes(
        root,
        &["reset", "--derived", "--force-live"],
        &[],
        "--force-live must skip the guard's scratch-root read",
    );
}

/// A composed invocation that includes a mode reading the scratch root resolves it before the
/// first prune: `reset --scratch-orphans --derived` over an unparsable `defaults` block fails
/// naming the config, and the sweep it lists first never runs.
#[test]
fn reset_scratch_orphans_composed_with_derived_sweeps_nothing_over_an_unparsable_defaults_block() {
    let dir = temp_store_project();
    let root = dir.path();
    let _workdir = configure_unparsable_defaults(root);
    let ((out, err, ok), orphan_stands) =
        reset_beside_an_orphan_root(root, &["reset", "--scratch-orphans", "--derived"]);
    assert!(
        !ok,
        "the guard's unresolvable scratch root must fail the command; stdout: {out:?}"
    );
    assert!(
        err.contains("parse workflow") && err.contains("max_retries"),
        "the failure must name the unparsable config; stderr: {err:?}"
    );
    assert!(
        !out.contains("--scratch-orphans:"),
        "the sweep must not run before the failing precheck; stdout: {out:?}"
    );
    assert!(
        orphan_stands,
        "the orphan scratch root must stand: no prune runs before the precheck fails"
    );
}

/// Given the disk reclaim an operator reaches for while a `workflow.yml` typo leaves the
/// `defaults` block unparsable, when they compose the two modes that read none of it - `reset
/// --scratch-orphans --derived --force-live` - then the sweep reclaims the orphan root and the
/// compaction runs: no mode in the selection reads the scratch root, so nothing resolves it.
#[test]
fn reset_scratch_orphans_with_derived_force_live_sweeps_and_compacts_over_an_unparsable_defaults_block(
) {
    let dir = temp_store_project();
    let root = dir.path();
    let _workdir = configure_unparsable_defaults(root);
    let why = "a selection of modes that read no scratch root, over an unparsable defaults block";
    assert_compacted(
        assert_sweeps_the_orphan_root(
            root,
            &["reset", "--scratch-orphans", "--derived", "--force-live"],
            why,
        ),
        why,
    );
}

/// Every mode that reads the scratch root resolves it BEFORE the one-time identity migration, so
/// over an unparsable `defaults` block each of `reset --runs`, `reset --build-cache` and `reset
/// --derived` fails naming the config without first migrating a legacy store: the migration's
/// stream rename and the decision it records never happen. `reset --derived --force-live`, which
/// reads no scratch root, then migrates that same store - the fixture does have history to move.
#[test]
fn every_mode_reading_the_scratch_root_fails_on_it_before_the_identity_migration_writes() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"legacy-decision","summary":"pre-mint history","governs":["src/legacy.rs"]}"#,
        ],
    );
    assert!(ok, "seeding legacy history must succeed; stderr: {err}");
    std::fs::write(rigger_file(root, "project.id"), "durablemint\n")
        .expect("mint an identity distinct from the basename");
    let _workdir = configure_unparsable_defaults(root);
    let rows = row_count(root);
    for mode in ["--runs", "--build-cache", "--derived"] {
        let (out, err, ok) = run_rigger(root, &["reset", mode]);
        assert!(
            !ok,
            "reset {mode} must fail on the unparsable config; stdout: {out:?}"
        );
        assert!(
            err.contains("parse workflow") && err.contains("max_retries"),
            "reset {mode} must name the unparsable config; stderr: {err:?}"
        );
        assert!(
            !err.contains("migrated project identity"),
            "reset {mode} must fail before the identity migration runs; stderr: {err:?}"
        );
        assert_eq!(
            row_count(root),
            rows,
            "reset {mode} must record nothing before its precheck fails"
        );
    }
    let (_out, err, ok) = run_rigger(root, &["reset", "--derived", "--force-live"]);
    assert!(
        ok && err.contains("migrated project identity") && err.contains("durablemint"),
        "fixture: a mode that reads no scratch root migrates the legacy history; stderr: {err:?}"
    );
}

/// `reset --build-cache` reads the scratch root its build caches live under FAIL-CLOSED, as the
/// live-writer guard does: over a `defaults` block that cannot be parsed it exits non-zero naming
/// the config and reclaims nothing - neither the cache under the configured workdir nor the one
/// under the default scratch root a degraded read would have fallen back to.
#[test]
fn reset_build_cache_fails_closed_on_an_unparsable_defaults_block_and_reclaims_nothing() {
    let dir = temp_store_project();
    let root = dir.path();
    let (_workdir, scratch_root) = configure_workdir(root, "  max_retries: three\n");
    let built = [
        Path::new(&scratch_root)
            .join("cargo-target")
            .join("built.rlib"),
        common::default_scratch_root(root)
            .join("cargo-target")
            .join("built.rlib"),
    ];
    for artifact in &built {
        std::fs::create_dir_all(artifact.parent().unwrap()).expect("create a build cache");
        std::fs::write(artifact, [0u8; 64]).expect("populate the build cache");
    }
    let (out, err, ok) = run_rigger(root, &["reset", "--build-cache"]);
    assert!(
        !ok,
        "an unparsable defaults block must fail reset --build-cache; stdout: {out:?}"
    );
    assert!(
        err.contains("parse workflow") && err.contains("max_retries"),
        "the failure must name the unparsable config; stderr: {err:?}"
    );
    assert!(
        !out.contains("--build-cache:"),
        "no reclaim may be reported; stdout: {out:?}"
    );
    for artifact in &built {
        assert!(
            artifact.exists(),
            "{} must stand: a scratch root the command cannot resolve reclaims nothing",
            artifact.display()
        );
    }
}

/// Run from a nested unit worktree, the guard still reads the marker the run stamped under the
/// store OWNER's scratch root, never one derived from the process cwd.
#[test]
fn reset_derived_from_a_nested_worktree_reads_the_marker_under_the_owning_roots_scratch_root() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    seed_store(root);
    let nested = nested_worktree(root, "wt");
    let scratch_root = common::default_scratch_root(root);
    assert_ne!(
        common::default_scratch_root(&nested),
        scratch_root,
        "fixture bug: the cwd's own scratch root must differ from the owning root's"
    );
    assert_refused_naming_the_spawn(
        reset_from_with_a_fresh_marker_under(root, &nested, "", scratch_root.to_str().unwrap()),
        0..300,
        "s ago, bound 300s)",
    );
}

/// A relative `defaults.workdir` names a directory under the store's owning root - where a run
/// stamps its markers - never under the directory `reset` runs from: run from a subdirectory,
/// the guard still reads the fresh marker under `<root>/<workdir>` and refuses.
#[test]
fn reset_derived_from_a_subdirectory_reads_the_marker_under_a_relative_workdir_on_the_owning_root()
{
    let dir = temp_store_project();
    let root = dir.path();
    std::fs::write(
        rigger_file(root, "workflow.yml"),
        "defaults:\n  workdir: rel-scratch\n",
    )
    .expect("configure a relative defaults.workdir");
    let scratch_root = root.join("rel-scratch");
    assert_refused_naming_the_spawn(
        reset_from_with_a_fresh_marker_under(
            root,
            &subdirectory_of(root),
            "",
            scratch_root.to_str().unwrap(),
        ),
        0..300,
        "s ago, bound 300s)",
    );
}

/// A relative `RIGGER_TMPDIR` is anchored on the store's owning root the same way: with the
/// override `rel-override` and no workdir configured, the guard run from a subdirectory reads the
/// fresh marker under `<root>/rel-override` - where a run stamps it - and refuses.
#[test]
fn reset_derived_from_a_subdirectory_reads_the_marker_under_a_relative_rigger_tmpdir_on_the_owning_root(
) {
    let dir = temp_store_project();
    let root = dir.path();
    let scratch_root = root.join("rel-override");
    assert_refused_naming_the_spawn(
        reset_from_with_a_fresh_marker_under(
            root,
            &subdirectory_of(root),
            "rel-override",
            scratch_root.to_str().unwrap(),
        ),
        0..300,
        "s ago, bound 300s)",
    );
}

/// The workflow a real `rigger step` parks [`SPAWN`] under: one stage `a` whose worker runs in the
/// checkout (no worktree), every spawn bounded at 300 s, and the scratch root placed by the
/// RELATIVE `defaults.workdir` `rel-scratch`.
const RELATIVE_WORKDIR_WORKFLOW: WorkflowFixture = WorkflowFixture {
    worker: UNISOLATED_WORKER,
    body:
        "defaults:\n  grounder: nop\n  budget: 60\n  max_wall_clock: 300\n  workdir: rel-scratch\n\
           stages:\n  a:\n    agent: worker\n    on_pass: none\n",
};

/// The workflow a real `rigger step` parks [`SPAWN`] under, as [`RELATIVE_WORKDIR_WORKFLOW`] but
/// with the scratch root placed by the `~/` `defaults.workdir` `~/tilde-scratch`.
const TILDE_WORKDIR_WORKFLOW: WorkflowFixture = WorkflowFixture {
    worker: UNISOLATED_WORKER,
    body:
        "defaults:\n  grounder: nop\n  budget: 60\n  max_wall_clock: 300\n  workdir: \"~/tilde-scratch\"\n\
           stages:\n  a:\n    agent: worker\n    on_pass: none\n",
};

/// Given `fixture` scaffolded in a fresh git project, when a real `rigger step` at its root (under
/// `envs`) parks [`SPAWN`], then the liveness marker path it hands the worker is the one under
/// `scratch_root(<git toplevel>)`, and when the worker touches exactly that path, `reset --derived`
/// run from a subdirectory (under the same `envs`) reads it there and refuses naming the spawn:
/// the path the run writes is the path the guard reads. `why` names the case.
fn assert_the_guard_reads_the_marker_rigger_step_stamps(
    fixture: &WorkflowFixture,
    envs: &[(&str, &str)],
    scratch_root: impl FnOnce(&str) -> String,
    why: &str,
) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_workflow_fixture(root, fixture);
    let (out, err, ok) = run_rigger_envs(root, &["step"], envs);
    assert!(ok, "{why}: rigger step must park the spawn; stderr: {err}");
    let step: serde_json::Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("rigger step prints one JSON line: {e}; got {out:?}"));
    let stamped = step["wave"]
        .as_array()
        .and_then(|wave| wave.iter().find(|item| item["id"] == SPAWN))
        .and_then(|item| item["marker_path"].as_str())
        .unwrap_or_else(|| panic!("the wave must carry {SPAWN} with its marker path; got {out:?}"))
        .to_string();
    let run_id =
        rigger::run::current_run_id(&read_run_events(root)).expect("the step started a run");
    let toplevel = git_out(root, &["rev-parse", "--show-toplevel"]);
    let expected = rigger::liveness::marker_path(&scratch_root(&toplevel), &run_id, SPAWN)
        .expect("a real spawn id resolves a marker path");
    assert_eq!(
        Path::new(&stamped),
        expected,
        "{why}: the marker path handed to the worker must be the absolute one under the \
         configured scratch root"
    );

    plant_marker(&expected, 0);
    assert_refused_naming_the_spawn(
        counted(root, &subdirectory_of(root), &["reset", "--derived"], envs),
        0..300,
        "s ago, bound 300s)",
    );
}

/// Given a store whose `defaults.workdir` is relative, when a real `rigger step` at the repository
/// root parks [`SPAWN`], then the liveness marker path it hands the worker is the ABSOLUTE path
/// under `<root>/rel-scratch` - a relative one would land under whatever directory the worker
/// runs in - and when the worker touches exactly that path, `reset --derived` run from a
/// subdirectory reads it there and refuses naming the spawn: the path the run writes is the path
/// the guard reads.
#[test]
fn the_marker_rigger_step_stamps_under_a_relative_workdir_is_the_one_the_guard_reads_from_a_subdirectory(
) {
    assert_the_guard_reads_the_marker_rigger_step_stamps(
        &RELATIVE_WORKDIR_WORKFLOW,
        &[("RIGGER_TMPDIR", "")],
        |toplevel| format!("{toplevel}/rel-scratch"),
        "a relative workdir anchored on the repository root",
    );
}

/// Given a store whose `defaults.workdir` is `~/tilde-scratch`, when a real `rigger step` parks
/// [`SPAWN`] under the process `HOME`, then the marker path it hands the worker is under
/// `$HOME/tilde-scratch` - the home the command runs under, never a literal `~` directory and
/// never the repository - and `reset --derived` under the same `HOME` reads that marker and
/// refuses naming the spawn: a `~/` scratch root never hides a live spawn from the guard.
#[test]
fn the_marker_rigger_step_stamps_under_a_tilde_workdir_is_the_one_the_guard_reads_under_the_same_home(
) {
    let home_dir = tempfile::tempdir().expect("create the HOME a ~/ workdir expands under");
    let home = home_dir.path().to_str().unwrap();
    assert_the_guard_reads_the_marker_rigger_step_stamps(
        &TILDE_WORKDIR_WORKFLOW,
        &[("RIGGER_TMPDIR", ""), ("HOME", home)],
        |_| format!("{home}/tilde-scratch"),
        "a ~/ workdir expanded under the process HOME",
    );
}

// ---------------------------------------------------------------------------------------
// The operator's way past an unbounded spawn
// ---------------------------------------------------------------------------------------

/// Given a dead driver's run whose one in-flight spawn is UNBOUNDED, its worker having touched
/// the liveness marker every host keeps for every spawn a day before it died: when the operator
/// runs `reset --derived`, then it refuses naming the `rigger result` that ends the spawn; when the
/// operator records that result as told, then the spawn no longer holds the run live and the one
/// signal left is the registry heartbeat that `rigger result` courier itself just stamped for
/// this store; and once that heartbeat is past the idle window the compaction runs - never
/// needing `--force-live`.
#[test]
fn an_unbounded_spawn_holds_the_run_live_until_the_operator_records_its_result() {
    let dir = temp_store_project();
    let root = dir.path();
    seed_owned(root, &one_unanswered_spawn("r1", None));
    let scratch = tempfile::tempdir().expect("create the scratch root the markers live under");
    let scratch_root = scratch.path().to_str().unwrap();
    plant_marker(
        &rigger::liveness::marker_path(scratch_root, "r1", SPAWN).unwrap(),
        86_400,
    );
    // ONE state home across every invocation, as on the operator's machine: the registry the
    // courier writes is the registry the guard reads.
    let state_home = tempfile::tempdir().expect("create XDG_STATE_HOME");
    let envs = [
        ("RIGGER_TMPDIR", scratch_root),
        ("XDG_STATE_HOME", state_home.path().to_str().unwrap()),
    ];
    let reset = || counted(root, root, &["reset", "--derived"], &envs);

    assert_refused_naming_the_spawn(
        reset(),
        86_400..,
        "s ago; unbounded, so it stays live until its result is recorded - `rigger result \
         a/implementer#0 --error <why>` once its worker is gone)",
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["result", SPAWN, "--error", "its worker is gone"],
        &envs,
    );
    assert!(
        ok,
        "the operator records the spawn's result; stdout: {out} stderr: {err}"
    );

    let (out, err, ok) = reset().said();
    assert!(
        !ok,
        "the result courier's own registry heartbeat is inside the idle window; stdout: {out:?}"
    );
    assert!(
        !err.contains(SPAWN),
        "a spawn with a recorded result has ended and must not be named; stderr: {err:?}"
    );
    assert!(
        err.contains(&format!(
            "1 driver registration(s) for this project's store in the machine-global instance \
             registry (spec 50) heartbeat within the last {}s",
            idle_window_secs()
        )),
        "the one signal left is the heartbeat the courier stamped; stderr: {err:?}"
    );

    let instances = registry::instances_dir(state_home.path());
    let stamped =
        registry::read_live_no_prune(&instances, registry::now_ms(), registry::DEFAULT_IDLE_MS);
    assert_eq!(
        stamped.len(),
        1,
        "the courier stamped exactly one entry, this store's"
    );
    for mut inst in stamped {
        inst.heartbeat_ms = registry::now_ms() - registry::DEFAULT_IDLE_MS - 60_000;
        registry::write(&instances, &inst).expect("age the courier's heartbeat");
    }
    assert_compacted(
        reset().said(),
        "with the result recorded and the heartbeat past the idle window nothing is live",
    );
}

// ---------------------------------------------------------------------------------------
// The operator's documents state the definition
// ---------------------------------------------------------------------------------------

/// Given the operator reads the binary's help or rigger's rendered skills, then each states the
/// liveness definition this criterion owns - a held step lock, an in-flight spawn's marker inside
/// its wall-clock bound, a registry heartbeat inside the idle window - and that a dead driver's
/// run is not live; none still reads unit terminality or a bare unanswered spawn as live.
#[test]
fn the_help_and_the_rendered_skills_state_that_the_guard_reads_liveness() {
    let dir = temp_store_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["--help"]);
    assert!(ok, "--help must succeed; stdout: {out}");
    // Whitespace collapsed, so the wrapped help reads as prose.
    let help = normalize_ws(&format!("{err}{out}"));
    assert!(
        help.contains(
            "Refuses while the run is live (a held step lock, an in-flight spawn's marker inside \
             its wall-clock bound, or a registry heartbeat inside the idle window), naming what is \
             live (a dead driver's run is not)"
        ),
        "the --derived help must state the liveness definition; got {help:?}"
    );
    assert!(
        !help.contains("a non-terminal unit"),
        "the help must not name unit terminality as a live signal; got {help:?}"
    );
    assert!(
        help.contains(
            "When no driver is alive and no spawn of the run awaits its result, it also closes \
             the current run's units whose branch work is landed on rigger-run"
        ),
        "the --runs help must state that the close waits for every spawn's result; got {help:?}"
    );

    let (out, err, ok) = run_rigger(root, &["docs"]);
    assert!(
        ok,
        "rigger docs renders the skills; stdout: {out} stderr: {err}"
    );
    let skill = |name: &str| -> String {
        std::fs::read_to_string(root.join("skills").join(name).join("SKILL.md"))
            .unwrap_or_else(|e| panic!("rigger docs writes the {name} skill: {e}; {out}"))
    };

    let reset_store = skill("rigger-reset-store");
    assert!(
        reset_store.contains(
            "When no driver is alive (no `rigger step` holds the lock, no in-flight spawn's \
             liveness marker is younger than its wall-clock bound, no registration for the store \
             has a heartbeat inside the idle window) and no spawn of the run awaits its result, \
             it also closes the current run's units"
        ),
        "the reset-store skill must state the liveness `--runs` closes a dead run on, and that \
         the close waits for every spawn's result; got {reset_store:?}"
    );
    assert!(
        !reset_store.contains("no spawn awaits a result"),
        "a bare unanswered spawn is not a live signal; got {reset_store:?}"
    );

    let using = skill("using-rigger");
    assert!(
        using.contains(
            "it refuses while the run is live - a `rigger step` holds its lock, an in-flight \
             spawn (one with no recorded result, or only the step's liveness fault) has a \
             liveness marker younger than its wall-clock bound, or a driver registration for this \
             store has a heartbeat inside the idle window - naming what it found. A run whose \
             driver died is not live: units it left non-terminal never block the compaction, a \
             spawn with no marker never does, and an in-flight spawn stops blocking once its \
             marker outlives the spawn's bound or a real result is recorded for it. An unbounded \
             spawn's marker never outlives its bound, so record that spawn's result to end it."
        ),
        "the using-rigger skill must state the liveness `--derived` refuses on; got {using:?}"
    );
    assert!(
        !using.contains("not yet terminal"),
        "unit terminality is not a live signal; got {using:?}"
    );
    // Every host keeps a liveness marker for every spawn, bounded or not - the thin workflow
    // driver stamps one on each wave item and tells its worker to keep it fresh - so no host is
    // one under which an unbounded spawn has no marker and never blocks.
    assert!(
        !using.contains("frames no heartbeat") && !using.contains("under that driver it has none"),
        "no host leaves an unbounded spawn without a marker; got {using:?}"
    );
}

// ---------------------------------------------------------------------------------------
// Signal 1: a held step lock
// ---------------------------------------------------------------------------------------

/// While another `rigger step` holds `.rigger/step.lock`, `reset --derived` refuses, naming the
/// lock; once released, the identical command succeeds.
#[test]
fn reset_derived_refuses_a_held_step_lock_and_succeeds_once_released() {
    use fs2::FileExt;
    let dir = temp_store_project();
    let root = dir.path();

    let lock_file = hold_step_lock(root);

    let (out, err, ok) = run_rigger_envs(root, &["reset", "--derived"], &[]);
    assert!(
        !ok,
        "reset --derived must refuse while a step holds step.lock; stdout: {out:?}"
    );
    assert!(
        err.contains("step.lock") || err.contains("rigger step"),
        "the refusal must name the held step lock; stderr: {err:?}"
    );
    assert!(
        err.contains("--force-live"),
        "the refusal must name the override; stderr: {err:?}"
    );

    FileExt::unlock(&lock_file).unwrap();
    drop(lock_file);

    assert_prunes(
        root,
        &["reset", "--derived"],
        &[],
        "once released, reset --derived must succeed",
    );
}

/// The step-lock probe is resolved at the STORE's own directory, never the process cwd - so a
/// `reset --derived` invoked from a NESTED WORKTREE (the exact geometry rigger runs its own
/// units in) still peeks the real, resolved store's lock rather than a nonexistent `.rigger`
/// under the worktree's own path.
#[test]
fn reset_derived_from_a_nested_worktree_still_refuses_the_resolved_stores_held_lock() {
    use fs2::FileExt;
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    seed_store(root);
    let nested = nested_worktree(root, "wt");
    assert!(
        !nested.join(".rigger").exists(),
        "the nested worktree must carry no store of its own"
    );

    let lock_file = hold_step_lock(root);

    let (out, err, ok) = run_rigger_envs(&nested, &["reset", "--derived"], &[]);
    assert!(
        !ok,
        "reset --derived from the nested worktree must refuse the resolved store's held lock; \
         stdout: {out:?}"
    );
    assert!(
        err.contains("step.lock") || err.contains("rigger step"),
        "the refusal must name the held step lock; stderr: {err:?}"
    );

    FileExt::unlock(&lock_file).unwrap();
    drop(lock_file);
}

// ---------------------------------------------------------------------------------------
// Fail-safe: an unreadable signal must refuse, never read as quiet
// ---------------------------------------------------------------------------------------

/// A malformed `SpawnRequested` body in the current run's slice makes the in-flight-spawn read
/// fail to decode. The pure-composition unit test already proves the internal function returns
/// `Err`; this periphery test proves the FULL wire end to end - that `cmd_reset`'s own error
/// propagation actually surfaces that as a failed compiled-binary invocation (a non-zero exit and
/// a non-empty message on stderr), not a swallowed error read as "nothing in flight" by some
/// layer between the guard and the process exit code. The refusal must be TOTAL: the malformed
/// event, and everything else in the log, survives untouched.
#[test]
fn reset_derived_fails_the_cli_on_a_malformed_current_run_spawn_event_and_prunes_nothing() {
    let dir = temp_store_project();
    let root = dir.path();
    // A `SpawnRequested` body missing every field `spawn::recorded` needs to decode it - valid
    // JSON, but not a valid request, so the current-run in-flight-spawn read fails outright
    // rather than seeing "no spawns in flight".
    seed_run_events(root, &[("SpawnRequested", "{}")]);
    let before = row_count(root);

    let (out, err, ok) = run_rigger_envs(root, &["reset", "--derived"], &[]);
    assert!(
        !ok,
        "a malformed current-run spawn event must fail the CLI, never read as quiet; stdout: \
         {out:?}"
    );
    assert!(
        !err.is_empty(),
        "the CLI failure must carry a message an operator can act on"
    );
    assert_eq!(
        row_count(root),
        before,
        "a refused compaction must prune NOTHING - the guard may only refuse, never partially act"
    );
}

/// A spawn result the log cannot decode, recorded against a spawn whose worker touched its marker
/// a minute ago - inside its 300 s bound - leaves unknown whether that result ended the spawn, so
/// the guard cannot read the run as dead: `reset --derived` exits non-zero naming the undecodable
/// result and prunes nothing, rather than reading it as the spawn's end and compacting.
#[test]
fn reset_derived_fails_on_a_malformed_result_for_a_spawn_whose_marker_is_inside_its_bound() {
    let o = reset_derived_around(
        &one_spawn_answered_by(&["{}".to_string()]),
        &[("r1", 60)],
        None,
    );
    assert!(
        !o.ok,
        "a malformed result a live marker needs read must fail the CLI, never compact; stdout: \
         {:?}",
        o.out
    );
    assert!(
        o.err.contains("missing field `id`"),
        "the failure must name the undecodable result; stderr: {:?}",
        o.err
    );
    assert_eq!(
        o.rows_after, o.rows_before,
        "a refused compaction must prune NOTHING - the guard may only refuse, never partially act"
    );
}

// ---------------------------------------------------------------------------------------
// Fail-safe: a step-lock probe fault (not a genuinely held lock) must refuse, never proceed
// ---------------------------------------------------------------------------------------

/// `acquire_step_lock`'s probe can fail for a reason that is NOT another `rigger step` holding
/// the lock (a permission fault, a read-only filesystem) - the guard's own docs say that fault
/// must propagate as a command error rather than being misread either way: not as "a step is
/// running" (a wrong diagnosis) and never swallowed into "quiet" (which would let compaction
/// proceed past a signal it never actually verified). Standing `.rigger/step.lock` up as a
/// DIRECTORY rather than a file faults the probe's own `open()` with an unrelated IO error (not a
/// lock conflict), proving end to end that this SECOND fail-safe direction reaches the compiled
/// binary's own exit code - exactly as the malformed-spawn-event case above proves for the first.
#[test]
fn reset_derived_fails_on_an_unreadable_step_lock_probe_and_prunes_nothing() {
    let dir = temp_store_project();
    let root = dir.path();
    // `temp_store_project` seeds an empty `events.db` FILE with no schema yet (the schema is created on
    // the first real `Store::open`, as every other `row_count` caller below arranges via
    // `seed_run_events`) - an empty seed establishes it without recording any event.
    seed_run_events(root, &[]);
    std::fs::create_dir_all(root.join(".rigger").join("step.lock"))
        .expect("stand up step.lock as a directory so the probe's open() faults");
    let before = row_count(root);

    let (out, err, ok) = run_rigger_envs(root, &["reset", "--derived"], &[]);
    assert!(
        !ok,
        "an unreadable step-lock probe must fail the CLI, never proceed to compact; \
         stdout: {out:?}"
    );
    assert!(
        !err.is_empty(),
        "the CLI failure must carry a message an operator can act on"
    );
    assert!(
        !err.contains("another `rigger step` is already running"),
        "an unrelated probe fault must never be misdiagnosed as a genuinely held lock; \
         stderr: {err:?}"
    );
    assert_eq!(
        row_count(root),
        before,
        "a refusal on an unreadable probe must prune NOTHING"
    );
}

// ---------------------------------------------------------------------------------------
// Signal 4: a fresh driver registration
// ---------------------------------------------------------------------------------------

/// A live entry in the machine-global instance registry (spec 50) for THIS project's exact
/// store refuses compaction naming the registration - even with an empty run stream (an
/// in-process `rigger run`/`serve` may not have parked its first spawn yet).
#[test]
fn reset_derived_refuses_a_live_driver_registration_naming_it() {
    let dir = temp_store_project();
    let root = dir.path();
    let toplevel = git_out(root, &["rev-parse", "--show-toplevel"]);

    let (state_home, _entry) = seed_registry("proj", &toplevel, registry::now_ms());

    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--derived"],
        &[("XDG_STATE_HOME", state_home.path().to_str().unwrap())],
    );
    assert!(
        !ok,
        "reset --derived must refuse while a driver registration is live; stdout: {out:?}"
    );
    assert!(
        err.contains("registration"),
        "the refusal must name the live driver registration; stderr: {err:?}"
    );
}

/// A registration for a DIFFERENT project's store must never block THIS one's compaction - the
/// guard is scoped to this store's exact identity, not "any instance is running somewhere".
#[test]
fn reset_derived_ignores_a_registration_for_a_different_store() {
    let dir = temp_store_project();
    let root = dir.path();

    let (state_home, _entry) =
        seed_registry("other-proj", "/tmp/some-other-project", registry::now_ms());

    assert_prunes(
        root,
        &["reset", "--derived"],
        &[("XDG_STATE_HOME", state_home.path().to_str().unwrap())],
        "an unrelated project's registration must never block this store's compaction",
    );
}

/// Spec 62, criterion 5 round 4 (`adv-u62c5r4-known-roots-prune-race-with-instances-provider`):
/// this guard's own driver-registration probe reads the WHOLE machine-global registry (every
/// project's entry, not just this store's own) to count live registrations that match `expected`,
/// so before the fix, checking THIS store's liveness had the side effect of PRUNING any OTHER
/// project's entry whose heartbeat had already gone stale, via `registry::read_live`'s delete.
/// That is the identical race class the dash's own three call sites were hardened against in this
/// same round (`registry::read_live_no_prune`): a foreign project's registry entry is the only
/// route the machine-wide self-reap watcher ever learns that project's root from, so an unrelated
/// `rigger reset --derived` invocation deleting it out from under that project is exactly the
/// permanent, silent loss criterion 5's cross-project guarantee exists to prevent, just reached
/// through a different call site than the dash's landing poll or attach resolve.
///
/// A stale (heartbeat well in the past) entry for an unrelated project's store never blocks this
/// store's compaction (matching `reset_derived_ignores_a_registration_for_a_different_store`'s
/// live-entry case) AND its registry FILE must still exist on disk after the command runs -
/// `refuse_derived_reset_if_live`'s own probe now filters staleness without ever deleting.
#[test]
fn reset_derived_never_deletes_a_stale_foreign_registry_entrys_file() {
    let dir = temp_store_project();
    let root = dir.path();

    // Well past `DEFAULT_IDLE_MS`: the pre-fix `read_live` would have pruned this outright as
    // a side effect of the guard's own liveness probe.
    let (state_home, entry_path) = seed_registry("other-proj", "/tmp/some-other-project", 0);
    assert!(
        entry_path.exists(),
        "the entry must exist before the command runs"
    );

    assert_prunes(
        root,
        &["reset", "--derived"],
        &[("XDG_STATE_HOME", state_home.path().to_str().unwrap())],
        "a stale, unrelated project's registration must never block this store's compaction",
    );
    assert!(
        entry_path.exists(),
        "reset --derived's own live-writer probe must never delete a foreign project's stale \
         registry entry from disk as a side effect of checking THIS store's liveness - deletion \
         is reserved exclusively to the dash's self-reap watcher's own tick: {entry_path:?}"
    );
}

// ---------------------------------------------------------------------------------------
// The override: --force-live
// ---------------------------------------------------------------------------------------

/// `--force-live` composed with `--runs` alone (no `--derived`) is inert - there is nothing for
/// it to override, so `reset --runs --force-live` behaves byte-identically to plain
/// `reset --runs`.
#[test]
fn force_live_with_runs_alone_is_inert() {
    let with_force = temp_store_project();
    let plain = temp_store_project();

    let (out_f, err_f, ok_f) =
        run_rigger_envs(with_force.path(), &["reset", "--runs", "--force-live"], &[]);
    let (out_p, err_p, ok_p) = run_rigger_envs(plain.path(), &["reset", "--runs"], &[]);
    assert!(
        ok_f,
        "reset --runs --force-live must succeed; stderr: {err_f}"
    );
    assert!(ok_p, "reset --runs must succeed; stderr: {err_p}");
    assert_eq!(
        out_f, out_p,
        "--force-live must not change --runs's own output when --derived was never requested"
    );
}

/// Composing `--runs` with a REFUSED `--derived` still completes `--runs`'s own prune - each
/// mode sheds only its own accumulation, and one mode's live-writer refusal must never silently
/// cancel the other's independent, already-safe work.
#[test]
fn runs_composed_with_a_refused_derived_still_completes_its_own_prune() {
    let dir = temp_store_project();
    let root = dir.path();
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
            (
                "SpawnRequested",
                r#"{"id":"a/implementer#0","unit":"a","stage":"implement","prompt":"go","max_wall_clock":300}"#,
            ),
        ],
    );
    // The spawn's worker touched its marker just now: the run is live.
    let scratch = tempfile::tempdir().unwrap();
    let scratch_root = scratch.path().to_str().unwrap();
    plant_marker(
        &rigger::liveness::marker_path(scratch_root, "r1", "a/implementer#0").unwrap(),
        0,
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--runs", "--derived"],
        &[("RIGGER_TMPDIR", scratch_root)],
    );
    assert!(
        !ok,
        "the composed command must still fail overall (the --derived guard refuses); \
         stdout: {out:?}"
    );
    assert!(
        out.contains("reset --runs: pruned"),
        "--runs's own prune must have completed and printed before the --derived refusal \
         aborted the composition; got stdout {out:?} stderr {err:?}"
    );
    assert!(
        err.contains("a/implementer#0"),
        "the --derived refusal must still name the live spawn; stderr: {err:?}"
    );
}

// ---------------------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------------------

/// A bare `reset --force-live` (no mode) is refused exactly as a bare `reset` is -
/// `--force-live` never implies a mode of its own.
#[test]
fn reset_force_live_alone_is_refused_as_no_mode() {
    let dir = temp_store_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger_envs(root, &["reset", "--force-live"], &[]);
    assert!(!ok, "a bare --force-live must not imply a mode");
    assert!(
        err.contains("at least one mode"),
        "must give the same 'at least one mode' refusal as a bare reset; stderr: {err:?}"
    );
}

/// The `rigger --help` entry for `reset --derived` documents `--force-live` AND owns the risk it
/// overrides (spec 71: "an explicit override flag whose help text owns the risk") - not merely
/// naming the flag, which a future edit could water down without this test noticing.
#[test]
fn the_derived_help_entry_documents_force_live_and_owns_the_risk() {
    let dir = temp_store_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger_envs(root, &["--help"], &[]);
    assert!(ok, "--help must succeed; stdout: {out}");
    assert!(
        err.contains("--force-live"),
        "help must document the override flag; got {err:?}"
    );
    assert!(
        err.contains("corruption"),
        "help must OWN the risk (name the corruption it forces past), not just the flag token; \
         got {err:?}"
    );
}
