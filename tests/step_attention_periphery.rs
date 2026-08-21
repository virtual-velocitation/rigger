//! Spec 69, criterion 5 (THE STEP STAMPS ATTENTION) - the two watching-discipline signals
//! that need MULTIPLE, SEPARATE real `rigger step` OS processes reading a real on-disk
//! store to prove at all: worker-death-recurred and stalled-frontier, and the "once per
//! threshold crossing" guarantee holding ACROSS a fresh process, not merely across two
//! function calls inside one long-lived test process.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN `mod tests` (src/conductor.rs). The
//! implementer's own direct-call test
//! (`a_second_failure_recurs_and_a_third_also_stalls_the_frontier`) drives the SAME scenario
//! by calling `conductor::run(&cfg, &deps)` several times in a loop, inside ONE test process,
//! against an in-memory `Store::open(":memory:")`. That proves the SIGNAL COMPUTATION is
//! correct (`compute_attention`'s before/after diff, `RunState::attention`), but it cannot
//! prove the thing this criterion actually promises: that a step during which the crossing
//! happens PRINTS the array - i.e. that `cmd_step` (`src/main.rs`) truly moves the live
//! `RunState::attention` onto the `Step` it serializes to real stdout, that the JSON
//! survives a full process boundary and a REAL SQLite file on disk (`Store::open` against a
//! path, not `:memory:`), and that "once per crossing" is a property of the PERSISTED log,
//! not an artifact of Rust state living across calls in the SAME process. A single long-lived
//! process could accidentally look correct from in-process memory that a fresh process never
//! has; only a fresh `rigger step` invocation per round closes that gap.
//!
//! Scenario: one unit, `max_retries: 5` (so it is still retrying, never escalated, once its
//! attempt count passes the stalled-frontier threshold of two - the exact situation the
//! signal exists to catch, per the spec's own recorded incident: "a spawn answered more than
//! twice without the run advancing burns full agent cost per round"). Each round is a
//! SEPARATE `rigger step` subprocess; between rounds, a courier's outcome is seeded directly
//! into the on-disk store (mirroring `tests/cli.rs`'s `seed_run_events`), exactly as `rigger
//! result <id> --error <why>` would leave it for the next step to replay.
//!
//! NOT OWNED here: the `escalated` signal in isolation and the clean-step omission (extended
//! onto `tests/cli.rs`'s pre-existing `step_carries_the_escalated_set_when_a_fixpoint_is_
//! reached_with_a_wedged_unit` and `step_prints_a_disjoint_two_spawn_wave_then_reports_done`);
//! the `halted` / `budget-final-tenth` co-occurrence (extended onto `tests/cli.rs`'s
//! pre-existing `step_prints_a_budget_halt_reason_when_the_breaker_trips`); the exact
//! crossing semantics, ordering, and non-restamping logic themselves (the implementer's own
//! `mod tests` in `src/conductor.rs`, which this file does not re-derive - it only proves
//! that logic's OUTPUT reaches the real wire, across real process boundaries).

mod common;

use std::path::Path;
use std::process::Command;

/// A throwaway project dir that is deliberately NOT a git repo - mirrors `tests/cli.rs`'s
/// identical `temp_repoless_project` helper. `isolation: none` below means the run never
/// touches git, so a repo-less offline project is the faithful, minimal fixture.
fn temp_repoless_project() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// The project identity the binary resolves for `root` - mirrors `tests/cli.rs`'s identical
/// `run_stream_identity` helper (a repo-less project has no git top-level, so this always
/// falls through to `root`'s own basename, never empty).
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

/// Seed run-lifecycle events directly into the namespaced run stream on a REAL on-disk
/// store - mirrors `tests/cli.rs`'s identical `seed_run_events` helper. Standing in for a
/// courier's `rigger result <id> --error <why>`, which the driver runs when a worker's
/// spawn errors.
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
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

/// Run `rigger <args...>` in `cwd`, returning (stdout, stderr, success) - mirrors
/// `tests/cli.rs`'s identical `run_rigger_envs` helper (opts out of the auto-started
/// dashboard and the machine-global instance registry, exactly as every other periphery
/// suite that spawns the product does).
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// A single-unit workflow whose gate always PASSES and whose remediation bound
/// (`max_retries: 5`) is generous enough that the unit is STILL retrying - never escalated -
/// once its attempt count passes the stalled-frontier threshold of two. Offline and
/// repo-less: `nop` grounder, `isolation: none`, `on_pass: none` (never attempts a merge, so
/// nothing here depends on git).
fn write_attention_progression_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: attentiontest
defaults:
  grounder: nop
  budget: 60
  max_retries: 5
gates:
  ok: { run: "true", kind: core }
stages:
  u:
    agent: worker
    gates: [ok]
    on_pass: none
"#,
    )
    .unwrap();
}

/// Spec 69, criterion 5: worker-death-recurred and stalled-frontier, driven across FIVE
/// separate `rigger step` subprocesses against ONE persisted on-disk store, each round
/// exactly as a real courier/driver cycle would leave it.
#[test]
fn recurrence_and_stalled_frontier_survive_real_process_boundaries() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_attention_progression_workflow(root);

    // Round 1: the unit is ready, so its implementer parks fresh as attempt #0. Nothing has
    // crossed a threshold yet - not even a failure has happened - so `attention` is omitted.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 1 step must succeed; stderr: {err}");
    let line = out.trim().to_string();
    assert!(
        line.contains(r#""id":"u/implementer#0""#),
        "round 1 must park attempt #0; got: {line:?}"
    );
    assert!(
        !line.contains("attention"),
        "parking the first attempt crosses no threshold; got: {line:?}"
    );

    // Attempt #0 fails (a worker's driver-error result, exactly what a courier's `rigger
    // result --error` leaves for the next step to replay). The FIRST failure is not a
    // recurrence.
    seed_run_events(
        root,
        &[("SpawnResult", r#"{"id":"u/implementer#0","error":"boom"}"#)],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 2 step must succeed; stderr: {err}");
    let line = out.trim().to_string();
    assert!(
        line.contains(r#""id":"u/implementer#1""#),
        "round 2 must park the remediation attempt #1; got: {line:?}"
    );
    assert!(
        !line.contains("attention"),
        "a unit's FIRST recorded failure is not a recurrence and must stamp nothing across a \
         fresh process reading it back from disk; got: {line:?}"
    );

    // Attempt #1 fails - the SECOND failure on this unit - a recurrence.
    seed_run_events(
        root,
        &[("SpawnResult", r#"{"id":"u/implementer#1","error":"boom"}"#)],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 3 step must succeed; stderr: {err}");
    let line = out.trim().to_string();
    assert!(
        line.contains(r#""id":"u/implementer#2""#),
        "round 3 must park the remediation attempt #2; got: {line:?}"
    );
    assert!(
        line.contains(
            r#""attention":[{"kind":"worker-death-recurred","unit":"u","detail":"2 attempts"}]"#
        ),
        "a unit's SECOND recorded failure must stamp exactly one worker-death-recurred entry, \
         read back fresh from the on-disk store by a brand-new process; got: {line:?}"
    );

    // Attempt #2 fails - the THIRD failure, AND the unit now carries more than two recorded
    // (failed) results while a fresh attempt (#3) parks still unanswered: the stalled-frontier
    // signal joins the recurrence.
    seed_run_events(
        root,
        &[("SpawnResult", r#"{"id":"u/implementer#2","error":"boom"}"#)],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 4 step must succeed; stderr: {err}");
    let line = out.trim().to_string();
    assert!(
        line.contains(r#""id":"u/implementer#3""#),
        "round 4 must park the remediation attempt #3, still unanswered; got: {line:?}"
    );
    assert!(
        line.contains(
            r#""attention":[{"kind":"worker-death-recurred","unit":"u","detail":"3 attempts"},{"kind":"stalled-frontier","unit":"u","detail":"3 recorded results, still parked"}]"#
        ),
        "the THIRD recorded failure must stamp BOTH a recurrence and a stalled-frontier \
         entry, in order, on a fresh process's own stdout; got: {line:?}"
    );

    // Round 5: NOTHING new recorded - attempt #3 is still the same parked, unanswered spawn,
    // read back by yet another fresh process. Neither signal may re-stamp: "once per
    // threshold crossing" (spec 69) means the crossing, not the still-exceeded state, and
    // this is the one property a same-process, in-memory-store test structurally cannot
    // prove - it needs a REAL persisted log a NEW process re-derives the same "nothing new"
    // verdict from, not Rust state a single process happened to carry forward.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 5 step must succeed; stderr: {err}");
    let line = out.trim().to_string();
    assert!(
        line.contains(r#""id":"u/implementer#3""#),
        "round 5 must still show attempt #3 as the parked wave, unchanged; got: {line:?}"
    );
    assert!(
        !line.contains("attention"),
        "a fresh process reading back a store with no new result folded must not re-stamp a \
         crossing an earlier process already surfaced; got: {line:?}"
    );
}
