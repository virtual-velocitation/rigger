//! Integration tests for spec 71, criterion 2 - COMPACTION REFUSES LIVE WRITERS.
//!
//! The recorded incident this guard exists to prevent: `rigger reset --derived` compacts the
//! event log by keeping only the latest event per replay key, which leaves REVISION GAPS by
//! design. A writer whose append cursor was built before that compaction ran can reissue one of
//! those gap revisions; every later event then sorts BELOW the run boundary in revision order.
//! `rigger reset --derived` must refuse to compact while run machinery looks live, naming what
//! is live, unless the operator explicitly overrides with `--force-live`.
//!
//! These tests drive the COMPILED binary against a real `.rigger/events.db`, because the
//! criterion is an operator-facing refusal whose observable effects are the command's exit
//! status, its message, and (for the guard's own writes) that the store is untouched.
//!
//! What this file OWNS (spec 71, criterion 2) and what it deliberately does not:
//!   - OWNS: the four live signals (held step lock, a non-terminal unit between spawn rounds, an
//!     in-flight spawn in the current run's slice, a live driver registration), the
//!     quiet-machinery baseline, and `--force-live`.
//!   - NOT OWNED: the `--derived` prune's own selection/report/reclamation mechanics (spec 60,
//!     criterion 5 - `tests/reset_derived_compaction*.rs`), the append-time assertion (spec 71,
//!     criterion 1), and the validate advisory (spec 71, criterion 3).

mod common;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};
use rigger::registry::{self, Instance, StoreIdentity};
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

/// A throwaway project: its own git repo (so `project_identity()`, which scopes the namespaced
/// run stream, resolves to the directory's basename deterministically) with an already-seeded
/// `.rigger/events.db` - standing in for the store a prior `rigger run`/`step` would have
/// created, exactly as `tests/cli.rs`'s `seed_store` does. `require_store_dir` (which every
/// courier, `reset` included, resolves through) refuses to fabricate a fresh store from an
/// uninitialized `.rigger`, so a test that drives `reset` must establish one first.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    let rigger = dir.path().join(".rigger");
    std::fs::create_dir_all(&rigger).expect("create .rigger");
    std::fs::File::create(rigger.join("events.db")).expect("seed an empty events.db");
    dir
}

/// The git top-level for `root`, resolved exactly as the product's own `git_repo_at` resolves it
/// (`git -C <root> rev-parse --show-toplevel`) - the SAME string the product uses as the
/// registry's `Instance.root` / the `registry_store_identity` input, so a test-written registry
/// entry lands under the identical key the guard's own read filters on.
fn git_toplevel(root: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .expect("git rev-parse --show-toplevel must succeed for a git-init'd temp project")
}

/// The project identity the binary resolves for `root` (the tracked `.rigger/project.id` at the
/// git top-level when present, else that top-level's basename) - mirrors `project_identity_at`'s
/// precedence so a seed appended under this identity lands in the exact stream the binary reads.
fn run_stream_identity(root: &Path) -> String {
    let toplevel = git_toplevel(root);
    let base = Path::new(&toplevel);
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

/// Seed run-lifecycle events directly into the namespaced run stream, standing in for the
/// conductor minting them - mirrors `tests/cli.rs`'s `seed_run_events`.
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
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

/// Run `rigger <args...>` in `cwd` with an OWNED, per-call `XDG_STATE_HOME` (so the machine-
/// global instance registry never leaks a phantom entry into the operator's real one) unless the
/// caller supplies its own via `envs` - matching `tests/cli.rs`'s `run_rigger_envs` convention.
fn run_rigger(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

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

// ---------------------------------------------------------------------------------------
// Baseline: quiet machinery prunes exactly as today
// ---------------------------------------------------------------------------------------

/// With no run ever started (the quietest possible machinery), `reset --derived` succeeds
/// exactly as it did before this guard existed - the guard adds a refusal path, never a new
/// precondition on the happy path.
#[test]
fn reset_derived_prunes_when_no_run_has_ever_started() {
    let dir = temp_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
    assert!(ok, "a quiet store must still compact; stderr: {err}");
    assert!(
        out.contains("reset --derived: pruned"),
        "must print the usual prune report; got {out:?}"
    );
}

/// A recorded spawn that already carries a result is ANSWERED, not in flight, and a unit that
/// has INTEGRATED is terminal, not live - neither must ever block compaction (the guard would
/// otherwise refuse forever on ordinary run history).
#[test]
fn reset_derived_prunes_when_every_unit_is_terminal_and_every_spawn_is_answered() {
    let dir = temp_project();
    let root = dir.path();
    seed_run_events(
        root,
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
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
    assert!(
        ok,
        "a terminal unit with only answered spawns must not block compaction; stderr: {err}"
    );
    assert!(out.contains("reset --derived: pruned"), "got {out:?}");
}

/// A prior, DIFFERENT run's unanswered spawn and non-terminal unit are residue, not live work in
/// the CURRENT run's slice - the design names "in-flight spawns in the current run's slice"
/// precisely so a zombie from an abandoned campaign can never wedge compaction forever.
#[test]
fn reset_derived_ignores_a_prior_runs_unanswered_spawn_and_non_terminal_unit() {
    let dir = temp_project();
    let root = dir.path();
    seed_run_events(
        root,
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
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
    assert!(
        ok,
        "a PRIOR run's unanswered spawn/non-terminal unit must not block THIS run's compaction; \
         stderr: {err}"
    );
    assert!(out.contains("reset --derived: pruned"), "got {out:?}");
}

// ---------------------------------------------------------------------------------------
// Signal 1: a held step lock
// ---------------------------------------------------------------------------------------

/// While another `rigger step` holds `.rigger/step.lock`, `reset --derived` refuses, naming the
/// lock; once released, the identical command succeeds.
#[test]
fn reset_derived_refuses_a_held_step_lock_and_succeeds_once_released() {
    use fs2::FileExt;
    let dir = temp_project();
    let root = dir.path();

    let lock_file = hold_step_lock(root);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
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

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
    assert!(
        ok,
        "once released, reset --derived must succeed; stderr: {err}"
    );
    assert!(out.contains("reset --derived: pruned"), "got {out:?}");
}

/// The step-lock probe is resolved at the STORE's own directory, never the process cwd - so a
/// `reset --derived` invoked from a NESTED WORKTREE (the exact geometry rigger runs its own
/// units in) still peeks the real, resolved store's lock rather than a nonexistent `.rigger`
/// under the worktree's own path.
#[test]
fn reset_derived_from_a_nested_worktree_still_refuses_the_resolved_stores_held_lock() {
    use fs2::FileExt;
    let dir = temp_project();
    let root = dir.path();
    // `git worktree add` needs a real commit to detach onto - `temp_project` only `git init`s
    // (an unborn HEAD), so seed one first.
    for args in [
        &["config", "user.email", "t@example.com"][..],
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
    let nested = root.join("wt");
    let ok = Command::new("git")
        .args(["worktree", "add", "-q", "--detach", "wt"])
        .current_dir(root)
        .status()
        .expect("git worktree add must run")
        .success();
    assert!(ok, "git worktree add must succeed");
    assert!(
        !nested.join(".rigger").exists(),
        "the nested worktree must carry no store of its own"
    );

    let lock_file = hold_step_lock(root);

    let (out, err, ok) = run_rigger(&nested, &["reset", "--derived"], &[]);
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
// Signal 2: a non-terminal unit in the current run - live BETWEEN spawn rounds
// ---------------------------------------------------------------------------------------

/// A unit that has STARTED and had its only spawn ANSWERED, but has neither integrated nor
/// escalated, is live BETWEEN rounds (its next spawn - e.g. a reviewer - has not been parked
/// yet). An in-flight-spawn check alone would miss this; `reset --derived` must still refuse,
/// naming the unit, and prune nothing.
#[test]
fn reset_derived_refuses_a_non_terminal_unit_between_spawn_rounds_and_prunes_nothing() {
    let dir = temp_project();
    let root = dir.path();
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
            ("UnitStarted", r#"{"id":"a","branch":"rigger/u/a"}"#),
            (
                "SpawnRequested",
                r#"{"id":"a/implementer#0","unit":"a","stage":"implement","prompt":"go"}"#,
            ),
            ("SpawnResult", r#"{"id":"a/implementer#0","output":"done"}"#),
        ],
    );
    let before = row_count(root);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
    assert!(
        !ok,
        "reset --derived must refuse a non-terminal unit between spawn rounds; stdout: {out:?}"
    );
    assert!(
        err.contains('a') && err.contains("not yet terminal"),
        "the refusal must name the non-terminal unit; stderr: {err:?}"
    );
    assert_eq!(
        row_count(root),
        before,
        "a refused compaction must prune NOTHING - the guard may only refuse, never partially act"
    );
}

// ---------------------------------------------------------------------------------------
// Signal 3: in-flight spawns in the current run's slice
// ---------------------------------------------------------------------------------------

/// A recorded spawn request with no result yet, in the CURRENT run, refuses compaction naming
/// the spawn id - and PRUNES NOTHING (the refusal is total, never partial).
#[test]
fn reset_derived_refuses_an_in_flight_spawn_naming_its_id_and_prunes_nothing() {
    let dir = temp_project();
    let root = dir.path();
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
            (
                "SpawnRequested",
                r#"{"id":"a/implementer#0","unit":"a","stage":"implement","prompt":"go"}"#,
            ),
        ],
    );
    let before = row_count(root);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
    assert!(
        !ok,
        "reset --derived must refuse while a spawn is in flight; stdout: {out:?}"
    );
    assert!(
        err.contains("a/implementer#0"),
        "the refusal must name the in-flight spawn id; stderr: {err:?}"
    );
    assert_eq!(
        row_count(root),
        before,
        "a refused compaction must prune NOTHING - the guard may only refuse, never partially act"
    );
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
    let dir = temp_project();
    let root = dir.path();
    // A `SpawnRequested` body missing every field `spawn::recorded` needs to decode it - valid
    // JSON, but not a valid request, so the current-run in-flight-spawn read fails outright
    // rather than seeing "no spawns in flight".
    seed_run_events(root, &[("SpawnRequested", "{}")]);
    let before = row_count(root);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
    assert!(
        !ok,
        "a malformed current-run spawn event must fail the CLI, never read as quiet; \
         stdout: {out:?}"
    );
    assert!(
        !err.is_empty(),
        "the CLI failure must carry a message an operator can act on"
    );
    assert_eq!(
        row_count(root),
        before,
        "a refusal on an unreadable signal must prune NOTHING"
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
    let dir = temp_project();
    let root = dir.path();
    // `temp_project` seeds an empty `events.db` FILE with no schema yet (the schema is created on
    // the first real `Store::open`, as every other `row_count` caller below arranges via
    // `seed_run_events`) - an empty seed establishes it without recording any event.
    seed_run_events(root, &[]);
    std::fs::create_dir_all(root.join(".rigger").join("step.lock"))
        .expect("stand up step.lock as a directory so the probe's open() faults");
    let before = row_count(root);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"], &[]);
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
    let dir = temp_project();
    let root = dir.path();
    let toplevel = git_toplevel(root);

    let state_home = tempfile::tempdir().expect("create XDG_STATE_HOME");
    let instances_dir = registry::instances_dir(state_home.path());
    let inst = Instance {
        project: "proj".to_string(),
        root: toplevel.clone(),
        store: StoreIdentity::Local {
            path: format!("{toplevel}/.rigger/events.db"),
        },
        heartbeat_ms: registry::now_ms(),
    };
    registry::write(&instances_dir, &inst).expect("seed a live registry entry");

    let (out, err, ok) = run_rigger(
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
    let dir = temp_project();
    let root = dir.path();

    let state_home = tempfile::tempdir().expect("create XDG_STATE_HOME");
    let instances_dir = registry::instances_dir(state_home.path());
    let inst = Instance {
        project: "other-proj".to_string(),
        root: "/tmp/some-other-project".to_string(),
        store: StoreIdentity::Local {
            path: "/tmp/some-other-project/.rigger/events.db".to_string(),
        },
        heartbeat_ms: registry::now_ms(),
    };
    registry::write(&instances_dir, &inst).expect("seed an unrelated registry entry");

    let (out, err, ok) = run_rigger(
        root,
        &["reset", "--derived"],
        &[("XDG_STATE_HOME", state_home.path().to_str().unwrap())],
    );
    assert!(
        ok,
        "an unrelated project's registration must never block this store's compaction; stderr: {err}"
    );
    assert!(out.contains("reset --derived: pruned"), "got {out:?}");
}

// ---------------------------------------------------------------------------------------
// The override: --force-live
// ---------------------------------------------------------------------------------------

/// `--force-live` skips the guard entirely: it compacts even while a step lock is held, and
/// prints the ordinary prune report exactly as the quiet path does.
#[test]
fn reset_derived_force_live_compacts_despite_a_held_step_lock() {
    use fs2::FileExt;
    let dir = temp_project();
    let root = dir.path();

    let lock_file = hold_step_lock(root);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived", "--force-live"], &[]);
    assert!(
        ok,
        "--force-live must skip the guard even while a step lock is held; stderr: {err}"
    );
    assert!(out.contains("reset --derived: pruned"), "got {out:?}");

    FileExt::unlock(&lock_file).unwrap();
    drop(lock_file);
}

/// `--force-live` also skips the guard while a spawn is in flight, pruning as normal.
#[test]
fn reset_derived_force_live_compacts_despite_an_in_flight_spawn() {
    let dir = temp_project();
    let root = dir.path();
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
            (
                "SpawnRequested",
                r#"{"id":"a/implementer#0","unit":"a","stage":"implement","prompt":"go"}"#,
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived", "--force-live"], &[]);
    assert!(
        ok,
        "--force-live must skip the guard even with an in-flight spawn; stderr: {err}"
    );
    assert!(out.contains("reset --derived: pruned"), "got {out:?}");
}

/// `--force-live` composed with `--runs` alone (no `--derived`) is inert - there is nothing for
/// it to override, so `reset --runs --force-live` behaves byte-identically to plain
/// `reset --runs`.
#[test]
fn force_live_with_runs_alone_is_inert() {
    let with_force = temp_project();
    let plain = temp_project();

    let (out_f, err_f, ok_f) =
        run_rigger(with_force.path(), &["reset", "--runs", "--force-live"], &[]);
    let (out_p, err_p, ok_p) = run_rigger(plain.path(), &["reset", "--runs"], &[]);
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
    let dir = temp_project();
    let root = dir.path();
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#),
            (
                "SpawnRequested",
                r#"{"id":"a/implementer#0","unit":"a","stage":"implement","prompt":"go"}"#,
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--runs", "--derived"], &[]);
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
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["reset", "--force-live"], &[]);
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
    let dir = temp_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["--help"], &[]);
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
