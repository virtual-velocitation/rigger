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
//! ROUND-2 ADDITIONS (review u69c5 rounds 2-3, cause genuine-defect): the fix rounds added
//! TWO new pieces of surface this file's original scenario predates. First, a new PUBLIC
//! function, `ledger::attention_kind_rank` - the canonical-order authority `rigger step`
//! (main.rs) trusts to re-sort `attention` after merging in the hung-liveness half of
//! signal 2, which `conductor::compute_attention` deliberately does not compute (see both
//! functions' own doc comments) - proven directly as a contract, not merely indirectly
//! through one caller. Second, that merge's canonical-position guarantee driven at the REAL
//! binary boundary with OTHER signals co-occurring in the same step, which the existing
//! single-signal hung-liveness end-to-end test (`tests/cli.rs`'s
//! `step_surfaces_a_hung_spawn_with_a_stale_marker_as_a_liveness_halt`) cannot exercise: a
//! one-entry `attention` array looks identical whether the merge appends or correctly
//! re-sorts.
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
//!
//! ROUND-4 ADDITIONS (review u69c5 round 4, cause genuine-defect): the fix replaced the
//! process-start `pre_hung_ids` read with a small persisted cross-process cursor
//! (`liveness::hung_cursor_path` / `read_hung_cursor` / `write_hung_cursor` - THREE new
//! public functions) so a driver-recorded liveness fault landing strictly BETWEEN two
//! `rigger step` invocations still stamps `attention` on the first step able to observe it
//! (see `hung_cursor_path`'s own doc comment for the full crossing-boundary reasoning). The
//! fix's own extension of `tests/cli.rs`'s
//! `step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver` already
//! proves that real cross-process BEHAVIOR end to end (stamp once, no restamp, clear on
//! recovery), and the implementer's own `mod tests` in `src/liveness.rs` already proves each
//! function's contract from INSIDE the crate (path shape, round trip, malformed input).
//! Neither proves the thing `hung_cursor_functions_are_a_working_public_contract_across_the_
//! crate_boundary` below does: that the three functions are usable, AS DOCUMENTED, from
//! OUTSIDE the module's own privileged position - the same api-edge guarantee this file's
//! `attention_kind_rank` test above already established for the round-3 public function - so
//! an accidental drop of `pub` (or of `pub mod liveness` in `lib.rs`) fails HERE, at the
//! crate boundary, rather than only inside the module that would silently stop exporting it.

/// Spec 69, criterion 5 (review u69c5 round 4, cause genuine-defect): `liveness::
/// hung_cursor_path`, `read_hung_cursor`, and `write_hung_cursor` are the three new PUBLIC
/// functions the round-4 fix added - called EXACTLY as an external crate consumer would
/// (`use rigger::liveness::{...}`), never through any crate-internal privilege the
/// implementer's own `src/liveness.rs::tests` has. Full permutation coverage of each
/// function's own contract (path shape variants, per-run scoping, malformed-file handling)
/// already lives there and is not re-derived here; this proves only that the exported
/// symbols work as documented when called from outside the crate, plus one case genuinely
/// missing from BOTH existing layers: `write_hung_cursor` called with the fully EMPTY set -
/// exactly what `cmd_step` does unconditionally on every step where every previously-hung
/// spawn has now recovered (its own doc comment: "Unconditionally overwritten every step...
/// so a recovered spawn drops out of the file the same step it drops out of `hung_spawns`").
/// The implementer's own round-trip test only ever shrinks to a non-empty set; the CLI
/// end-to-end test asserts the WIRE's `attention` field, never the cursor FILE's own bytes -
/// so neither proves the file itself reads back genuinely empty rather than retaining stale
/// content the wire assertion happens not to notice.
#[test]
fn hung_cursor_functions_are_a_working_public_contract_across_the_crate_boundary() {
    use rigger::liveness::{hung_cursor_path, read_hung_cursor, write_hung_cursor};
    use std::collections::BTreeSet;

    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().to_str().unwrap();

    // The documented path shape, proven once here as a CONTRACT reachable from outside the
    // crate: a sibling FILE of the run's own marker subdirectory (`agent-live/<run>/`),
    // never a path inside it.
    let path = hung_cursor_path(root, "r1");
    assert_eq!(
        path,
        std::path::Path::new(root)
            .join("agent-live")
            .join("r1.hung-cursor"),
        "hung_cursor_path's documented shape is part of its public contract; got: {path:?}"
    );

    // Nothing written yet: empty, not an error - callable from outside the crate exactly as
    // `cmd_step` calls it on a run's very first step, before any prior step ever ran.
    assert!(
        read_hung_cursor(root, "r1").is_empty(),
        "an absent cursor must read as empty, not an error, across the crate boundary"
    );

    // A full round trip through the public API.
    let mut hung: BTreeSet<String> = BTreeSet::new();
    hung.insert("x/implementer#0".to_string());
    write_hung_cursor(root, "r1", &hung).unwrap();
    assert_eq!(
        read_hung_cursor(root, "r1"),
        hung,
        "a written cursor must read back exactly the set that was written"
    );

    // The genuinely untested case: an unconditional overwrite with the fully EMPTY set (every
    // previously-hung spawn has now recovered) - `cmd_step` performs exactly this call on
    // every step, and the NEXT step's crossing check depends on the file actually being
    // empty afterward, not merely on the wire omitting `attention` that one time.
    write_hung_cursor(root, "r1", &BTreeSet::new()).unwrap();
    assert!(
        read_hung_cursor(root, "r1").is_empty(),
        "an unconditional overwrite with the empty set (every spawn recovered) must read back \
         empty, never the previous step's stale ids"
    );
}

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

/// Spec 69, criterion 5's ordering CONTRACT (review u69c5 round 3, cause genuine-defect):
/// `ledger::attention_kind_rank` is a NEW public function the round-3 fix added - the
/// single authority `rigger step` (main.rs) trusts to re-sort `attention` after merging in
/// the hung-liveness half of signal 2 (see its own doc comment). Nothing anywhere calls it
/// directly today: every existing assertion on `attention`'s order exercises either
/// `conductor::compute_attention`'s OWN construction order (which never calls this
/// function - its own doc comment says so) or, in `tests/cli.rs`'s hung-liveness scenario,
/// a single-entry array for which any rank function looks identical to a no-op. This is the
/// crate's public ordering contract, proven directly against the function itself: the five
/// known kinds rank strictly increasing in the canonical spec order, an unknown kind sorts
/// past every known one (never panics), and - the actual use this contract serves -
/// sorting a scrambled list by this exact key reproduces the canonical order.
#[test]
fn attention_kind_rank_orders_the_five_known_kinds_and_sorts_an_unknown_kind_last() {
    use rigger::ledger::{
        attention_kind_rank, ATTENTION_BUDGET_FINAL_TENTH, ATTENTION_ESCALATED, ATTENTION_HALTED,
        ATTENTION_STALLED_FRONTIER, ATTENTION_WORKER_DEATH_RECURRED,
    };

    let escalated = attention_kind_rank(ATTENTION_ESCALATED);
    let halted = attention_kind_rank(ATTENTION_HALTED);
    let recurred = attention_kind_rank(ATTENTION_WORKER_DEATH_RECURRED);
    let final_tenth = attention_kind_rank(ATTENTION_BUDGET_FINAL_TENTH);
    let stalled = attention_kind_rank(ATTENTION_STALLED_FRONTIER);
    assert!(
        escalated < halted && halted < recurred && recurred < final_tenth && final_tenth < stalled,
        "the five known kinds must rank in the canonical spec order (escalated, halted, \
         worker-death-recurred, budget-final-tenth, stalled-frontier); got {escalated} \
         {halted} {recurred} {final_tenth} {stalled}"
    );

    let unknown = attention_kind_rank("some-future-kind-nobody-emits-yet");
    assert!(
        unknown > stalled,
        "an unknown kind must sort AFTER every known kind (last), never panic and never \
         interleave; got rank {unknown} vs stalled-frontier's {stalled}"
    );

    let mut kinds = vec![
        ATTENTION_STALLED_FRONTIER,
        ATTENTION_HALTED,
        ATTENTION_BUDGET_FINAL_TENTH,
        ATTENTION_ESCALATED,
        ATTENTION_WORKER_DEATH_RECURRED,
    ];
    kinds.sort_by_key(|k| attention_kind_rank(k));
    assert_eq!(
        kinds,
        vec![
            ATTENTION_ESCALATED,
            ATTENTION_HALTED,
            ATTENTION_WORKER_DEATH_RECURRED,
            ATTENTION_BUDGET_FINAL_TENTH,
            ATTENTION_STALLED_FRONTIER,
        ],
        "sorting a scrambled kind list by attention_kind_rank must reproduce the canonical \
         order - the exact use main.rs::merge_hung_attention makes of it"
    );
}

/// Plant a SYNTHETIC STALE MARKER at exactly `marker`, touched an hour ago - mirrors
/// `tests/cli.rs`'s identical `plant_stale_marker` helper (this crate's own established
/// per-file duplication convention - see this file's other helpers' doc comments above).
fn plant_stale_marker(marker: &Path) {
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(marker, b"heartbeat").unwrap();
    let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(marker)
        .unwrap()
        .set_modified(stale)
        .unwrap();
}

/// Extract the `marker_path` field carried by the wave item whose `id` is exactly `id` -
/// like `tests/cli.rs`'s `json_string_field`, but scoped to ONE item's own JSON object,
/// since this file's ordering scenario below carries TWO wave items (`u` and `h`) in the
/// SAME line, each with its own `marker_path`; a whole-line first-match search (as a single-
/// item scenario can get away with) would risk reading the wrong item's path.
fn marker_path_for_wave_item(line: &str, id: &str) -> String {
    let id_needle = format!("\"id\":\"{id}\"");
    let start = line
        .find(&id_needle)
        .unwrap_or_else(|| panic!("wave item {id:?} not found in step output; got: {line:?}"));
    let rest = &line[start..];
    // Scope the search to THIS item's own object: up to the next `"id":"` (the following
    // wave item), or the rest of the line if this is the last item.
    let end = rest[1..]
        .find("\"id\":\"")
        .map(|p| p + 1)
        .unwrap_or(rest.len());
    let item = &rest[..end];
    let needle = "\"marker_path\":\"";
    let mstart = item
        .find(needle)
        .unwrap_or_else(|| panic!("wave item {id:?} must carry a marker_path; got: {item:?}"))
        + needle.len();
    let mrest = &item[mstart..];
    let mend = mrest
        .find('"')
        .unwrap_or_else(|| panic!("marker_path must be terminated; got: {mrest:?}"));
    mrest[..mend].to_string()
}

/// A real, minimally-committed git repo - mirrors `tests/cli.rs`'s identical
/// `temp_git_project_with_commit` helper. Unlike [`temp_repoless_project`] above, this file's
/// own ordering scenario NEEDS one: `rigger step`'s `scratch_root` (main.rs) resolves to
/// `None` - disabling the liveness sweep and the wave's `marker_path` stamp entirely -
/// whenever `repo` is empty, which only a git-less project produces. A marker-driven hung
/// spawn is therefore structurally impossible to test against a repo-less project.
fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    for args in [
        &["config", "user.email", "t@example.com"][..],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    dir
}

/// Two independent stages that never answer normally: `u` keeps failing (the exact
/// worker-death-recurred/stalled-frontier scenario above), `h` parks and is later driven
/// hung via a planted stale marker. Both share `max_wall_clock` (so BOTH carry a
/// `marker_path` in the wave - proving the ordering test below reads the RIGHT item's path)
/// and `max_retries: 5` (so `u` is never escalated across the whole scenario, matching
/// `write_attention_progression_workflow` above).
fn write_attention_ordering_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: attentionordertest
defaults:
  grounder: nop
  budget: 60
  max_retries: 5
  max_wall_clock: 60
stages:
  u:
    agent: worker
    on_pass: none
  h:
    agent: worker
    on_pass: none
"#,
    )
    .unwrap();
}

/// Spec 69, criterion 5's ordering CONTRACT, proven at the REAL binary boundary (review
/// u69c5 round 3, cause genuine-defect): `rigger step` (main.rs) computes the hung-liveness
/// half of signal 2 SEPARATELY from `conductor::compute_attention` and merges it in via
/// `ledger::attention_kind_rank` (see both functions' own doc comments). The only existing
/// end-to-end proof of that merge (`tests/cli.rs`'s
/// `step_surfaces_a_hung_spawn_with_a_stale_marker_as_a_liveness_halt`) drives a hung spawn
/// ALONE - a single-entry `attention` array, for which even a naive "always append at the
/// end" bug would look identical to a correct canonical-order merge. This test drives a
/// hung spawn (`h`) CO-OCCURRING, in the SAME real `rigger step` process, with two OTHER
/// signals `compute_attention` already produced for a DIFFERENT unit (`u`):
/// worker-death-recurred (canonical rank 2) and stalled-frontier (rank 4) - both AFTER
/// `halted`'s rank 1. A naive append-at-the-end bug would print `worker-death-recurred,
/// stalled-frontier, halted` (wrong order); only a genuine stable re-sort produces `halted,
/// worker-death-recurred, stalled-frontier` (`halted` moved to the FRONT of an
/// already-built two-entry array).
#[test]
fn hung_liveness_halt_lands_ahead_of_real_worker_death_and_stalled_frontier_signals() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_attention_ordering_workflow(root);

    // Round 1: both independent units are ready, so both park together in one wave.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 1 step must succeed; stderr: {err}");
    let line = out.trim().to_string();
    assert!(
        line.contains(r#""id":"u/implementer#0""#) && line.contains(r#""id":"h/implementer#0""#),
        "round 1 must park BOTH units' implementer spawns in one wave; got: {line:?}"
    );
    assert!(
        !line.contains("attention"),
        "round 1 crosses no threshold for either unit; got: {line:?}"
    );
    let h_marker = marker_path_for_wave_item(&line, "h/implementer#0");

    // u's first failure - not a recurrence. h is untouched (healthy, no marker planted yet).
    seed_run_events(
        root,
        &[("SpawnResult", r#"{"id":"u/implementer#0","error":"boom"}"#)],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 2 step must succeed; stderr: {err}");
    assert!(
        !out.trim().contains("attention"),
        "u's first recorded failure crosses no threshold; got: {out:?}"
    );

    // u's second failure - a recurrence, the ONLY signal this round.
    seed_run_events(
        root,
        &[("SpawnResult", r#"{"id":"u/implementer#1","error":"boom"}"#)],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 3 step must succeed; stderr: {err}");
    assert!(
        out.trim().contains(
            r#""attention":[{"kind":"worker-death-recurred","unit":"u","detail":"2 attempts"}]"#
        ),
        "round 3 must stamp exactly the recurrence signal, h still healthy; got: {out:?}"
    );

    // NOW plant h's stale marker - the sweep will find it stale on the NEXT step, the exact
    // same round u's third failure crosses BOTH worker-death-recurred and stalled-frontier.
    plant_stale_marker(std::path::Path::new(&h_marker));

    // u's third failure crosses worker-death-recurred AND stalled-frontier in the SAME call
    // `compute_attention` produces (ranks 2 and 4, already correctly ordered between
    // themselves); the same real step's sweep independently finds h newly hung, and main.rs
    // must merge its `halted` entry (rank 1) in FRONT of both.
    seed_run_events(
        root,
        &[("SpawnResult", r#"{"id":"u/implementer#2","error":"boom"}"#)],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "round 4 step must succeed; stderr: {err}");
    let line = out.trim();
    assert_eq!(
        line.matches(r#"{"kind":"#).count(),
        3,
        "round 4 must stamp exactly three attention entries (halted, worker-death-recurred, \
         stalled-frontier), no fewer and no more; got: {line:?}"
    );
    let halted_pos = line
        .find(r#"{"kind":"halted","detail":"#)
        .unwrap_or_else(|| {
            panic!("the hung halt must stamp a run-scoped halted entry; got: {line:?}")
        });
    assert!(
        line[halted_pos..].contains("h/implementer#0") && line[halted_pos..].contains("infra"),
        "the halted entry's detail must name the hung spawn and its infra classification; \
         got: {line:?}"
    );
    let recurred_pos = line
        .find(r#"{"kind":"worker-death-recurred","unit":"u","detail":"3 attempts"}"#)
        .unwrap_or_else(|| panic!("round 4's recurrence entry must still stamp; got: {line:?}"));
    let stalled_pos = line
        .find(
            r#"{"kind":"stalled-frontier","unit":"u","detail":"3 recorded results, still parked"}"#,
        )
        .unwrap_or_else(|| {
            panic!("round 4's stalled-frontier entry must still stamp; got: {line:?}")
        });
    assert!(
        halted_pos < recurred_pos && recurred_pos < stalled_pos,
        "the hung halt must be merged into CANONICAL position - ahead of BOTH signals \
         `compute_attention` already produced this call, not appended after them; \
         got: {line:?}"
    );
}
