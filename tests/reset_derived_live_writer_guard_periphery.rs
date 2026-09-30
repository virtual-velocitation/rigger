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
//! when a registry heartbeat for this store is younger than the idle window. Unit terminality is
//! not a liveness signal: a run whose driver died leaves its units non-terminal forever, and that
//! run - the one whose bloat most needs the compaction - must not need `--force-live` to get it.
//!
//! These tests drive the COMPILED binary against a real `.rigger/events.db`, because the
//! criterion is an operator-facing refusal whose observable effects are the command's exit
//! status, its message, and (for the guard's own writes) that the store is untouched.
//!
//! What this file OWNS and what it deliberately does not:
//!   - OWNS: the three live signals and the definition they make up, the dead-driver run that
//!     proceeds, the quiet-machinery baseline, and `--force-live`.
//!   - NOT OWNED: the `--derived` prune's own selection/report/reclamation mechanics (spec 60,
//!     criterion 5 - `tests/reset_derived_compaction*.rs`), the append-time assertion (spec 71,
//!     criterion 1), and the validate advisory (spec 71, criterion 3).

mod common;
use common::git::run_git;

use common::cli::plant_marker;
use common::cli::run_rigger_envs;
use common::cli::seed_run_events;
use common::cli::temp_store_project;
use common::git::git_out;
use rigger::registry::{self, Instance, StoreIdentity};
use std::path::Path;

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

/// Open, exclusively lock (non-blocking), and return `.rigger/step.lock` under `root` - standing
/// in for a `rigger step` holding it for its whole duration.
fn hold_step_lock(root: &Path) -> std::fs::File {
    use fs2::FileExt;
    let lock_path = root.join(".rigger").join("step.lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    lock_file
        .try_lock_exclusive()
        .expect("the test must be able to take the lock first");
    lock_file
}

/// Runs `rigger <args>` in `root` with `envs` and asserts the derived compaction ran, `why`
/// naming the case.
fn assert_prunes(root: &Path, args: &[&str], envs: &[(&str, &str)], why: &str) {
    let (out, err, ok) = run_rigger_envs(root, args, envs);
    assert!(ok, "{why}; stderr: {err}");
    assert!(
        out.contains("reset --derived: pruned"),
        "must print the usual prune report; got {out:?}"
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

/// A machine-global registry under a fresh `XDG_STATE_HOME` holding one entry for `project` at
/// `root` (its local store under `root/.rigger/events.db`) last heard from at `heartbeat_ms`;
/// returns the state home and the entry's file.
fn seed_registry(
    project: &str,
    root: &str,
    heartbeat_ms: u64,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let state_home = tempfile::tempdir().expect("create XDG_STATE_HOME");
    let instances_dir = registry::instances_dir(state_home.path());
    let inst = Instance {
        project: project.to_string(),
        root: root.to_string(),
        store: StoreIdentity::Local {
            path: format!("{root}/.rigger/events.db"),
        },
        heartbeat_ms,
    };
    let entry = registry::write(&instances_dir, &inst).expect("seed a registry entry");
    (state_home, entry)
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

/// Runs `rigger <args>` over [`DEAD_DRIVER_RUN`] with exactly the `fresh` signals live and every
/// other one stale or free: the step lock free, `a`'s marker touched 301 s ago (just past its
/// 300 s bound), the registry heartbeat a minute past the idle window. `b`'s marker is always
/// touched just now - an answered spawn has ended whatever its marker says - and `c` has none.
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

    let rows_before = row_count(root);
    let (out, err, ok) = run_rigger_envs(
        root,
        args,
        &[
            ("RIGGER_TMPDIR", scratch_root),
            ("XDG_STATE_HOME", state_home.path().to_str().unwrap()),
        ],
    );
    drop(lock);
    Outcome {
        out,
        err,
        ok,
        rows_before,
        rows_after: row_count(root),
    }
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
    let o = reset_over_a_dead_drivers_run(&[], &["reset", "--derived"]);
    assert!(
        o.ok,
        "a run whose driver is dead must compact without --force-live; stderr: {}",
        o.err
    );
    assert!(
        o.out.contains("reset --derived: pruned"),
        "must print the usual prune report; got {:?}",
        o.out
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
    let o = reset_over_a_dead_drivers_run(&ALL_SIGNALS, &["reset", "--derived", "--force-live"]);
    assert!(o.ok, "--force-live must skip the guard; stderr: {}", o.err);
    assert!(
        o.out.contains("reset --derived: pruned"),
        "must print the usual prune report; got {:?}",
        o.out
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
    let dir = temp_store_project();
    let root = dir.path();
    // `git worktree add` needs a real commit to detach onto - `temp_store_project` only `git init`s
    // (an unborn HEAD), so seed one first.
    for args in [
        &["config", "user.email", "t@example.com"][..],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = run_git(root, args).status.success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    let nested = root.join("wt");
    let ok = run_git(root, &["worktree", "add", "-q", "--detach", "wt"])
        .status
        .success();
    assert!(ok, "git worktree add must succeed");
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
