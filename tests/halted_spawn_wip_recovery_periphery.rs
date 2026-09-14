//! Periphery for spec 89, criterion 1 (A HALT NEVER DISCARDS A TREE): the cross-module seam
//! `RunCtx::run_single_stage` (`src/conductor.rs`) opens onto `Worktree::commit`
//! (`src/worktree.rs`) - a prior incarnation's abandoned edit, left uncommitted in a unit's
//! deterministic worktree by a halt (a liveness sweep, the outer wall clock, a crash), is
//! captured as a `wip(<unit>): tree of halted spawn <id>` commit the instant this process
//! adopts that worktree, and the fresh implementer's own prompt names the recovery commit
//! and says "finish and report; do not start over".
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TEST. `conductor.rs`'s own
//! `a_halted_spawns_uncommitted_tree_is_captured_as_a_wip_commit_and_named_in_the_next_prompt`
//! proves the ALGORITHM correct - but entirely IN-PROCESS: one `run(&cfg, &deps)` call, a
//! `Stub` driver whose `prompts_by_agent` is a private in-memory map read directly by the
//! test, never the compiled binary's own store or its own `rigger prompt` command. It cannot
//! prove what this criterion actually promises an operator: that the recorded prompt a real
//! courier fetches with `rigger prompt <id>` - reading the SAME `events.db` a completely
//! separate `rigger step` process wrote to - carries the recovery commit and the verbatim
//! instruction. This file closes that gap through the compiled binary, a real git worktree,
//! and a real `Store::open` round trip across two independent process invocations.
//!
//! Also closes the paired negative-space gap the implementer's own test does not touch: the
//! OVERWHELMINGLY common case (no prior incarnation, a freshly created clean worktree) must
//! commit NOTHING and say NOTHING about a halt - through the real binary, not by inspection.
//!
//! NOT OWNED HERE: `Worktree::commit`'s conflict-marker refusal itself (`src/worktree.rs`'s
//! own `mod tests`, `commit_refuses_a_worktree_with_a_merge_left_in_progress`,
//! `commit_refuses_when_tracked_content_carries_conflict_marker_text`, and
//! `commit_ignores_conflict_marker_lookalike_text_in_an_untouched_tracked_file`, already
//! exercise that guard directly against a real git repo through the public `Worktree` API),
//! and the `regenerate_conflicted_paths` integration-merge finalize path that also now runs
//! through the same guarded `commit` (already covered end-to-end, at the real binary
//! boundary, by `tests/integrate_conflict_merge_periphery.rs`).
//!
//! ROUND 2 FIX (three interaction defects a reject verdict named - none visible to the
//! implementer's own in-process unit tests, each proven a real regression here by reverting
//! this file's fixes and confirming the exact failure the round-2 commit describes):
//!
//! (a) `a_declared_units_dirty_worktree_survives_the_step_start_sweep_backstops`. The
//! `cmd_step`-start `sweep_terminal` / `reclaim_orphan_scratch` backstops (`main.rs`) both run
//! BEFORE `run_single_stage`'s halted-commit capture ever gets a chance to run (`ensure_run_
//! branch` mints `rigger-run` at the SAME tip a pre-existing halted worktree branches from,
//! on this project's very first step - so both authorities already see it as "merged" before
//! anything else touches it) - either one discarding the dirty candidate outright defeats the
//! capture by racing it on process control-flow order alone. `sweep_terminal`'s own `mod
//! tests` (`sweep_terminal_spares_a_dirty_no_spawn_worktree_pending_halt_recovery`) and
//! `main.rs`'s own (`reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree`) each prove
//! their OWN function correct in isolation; neither drives BOTH authorities in the SAME real
//! `cmd_step` invocation the way an operator's process actually does.
//!
//! (b) `a_resumed_reviewed_units_real_crash_frozen_merge_conflict_reaches_the_idempotent_path`.
//! `run_single_stage`'s halted-commit capture must never trip its conflict-marker refusal on a
//! resumed `ResumePhase::Reviewed` unit whose worktree ALREADY carries a genuine, still-
//! unresolved merge conflict from a prior window's crash - it must skip straight to the
//! existing `merge_into_worktree`/`merge_in_progress` idempotent resume path (spec 88). The
//! implementer's own new `conductor.rs` unit test
//! (`a_resumed_reviewed_units_genuine_unresolved_conflict_reaches_the_idempotent_merge_path`)
//! proves this in-process, with a `Store::open(":memory:")` and one `run(&cfg, &deps)` call.
//! This drives it through the compiled binary as a genuinely separate process reading a real
//! on-disk store a completely different process wrote to - the shape this file's own header
//! already argues for above.
//!
//! (c) `an_untouched_conflict_marker_lookalike_file_never_blocks_an_unrelated_checkpoint_
//! commit`. `Worktree::commit`'s conflict-marker scan must be scoped to the CALLER's own
//! touched-or-unmerged paths, never an unconditional whole-tracked-tree content scan that can
//! false-positive on lookalike text in a file nobody touched. Deliberately isolated from (a):
//! a plain two-step implementer flow with no pre-existing worktree, so the sweep backstops
//! never enter the picture and only the checkpoint-commit's own scope guard is exercised.
//!
//! ROUND 3 FIX (`both_step_start_backstops_share_path_is_dirty_and_fail_closed_on_an_
//! unreadable_status_read`): a reject-round finding
//! (`arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`, live-confirmed by
//! `adv-u89c1r2-dirty-check-fail-direction-live-confirmed-via-documented-race` against a real
//! corrupted worktree admin dir) caught a gap defect (a) above cannot see: the two step-start
//! authorities agreeing a worktree is dirty by CONTENT says nothing about what happens when
//! the underlying `git status` READ ITSELF fails - round 2 shipped with `sweep_terminal_logged`
//! failing CLOSED (spare) on that error and `reclaim_orphan_scratch`'s own independently
//! hardcoded status call failing OPEN (discard) on the IDENTICAL error, despite a doc comment
//! claiming the two already mirrored each other. Round 3 replaced both inline calls with one
//! shared `rigger::worktree::path_is_dirty` free fn. This drives that fix through the REAL
//! compiled binary with a `git` shim that fails the underlying read itself (not merely
//! reporting dirty content) for exactly the first two real invocations against the candidate,
//! then lets every later call - including the halted-commit capture's own, once adoption
//! proceeds - reach the genuine git unmodified.

mod common;

use std::path::Path;
use std::process::Command;

fn run_git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn git {args:?}: {e}"))
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = run_git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = run_git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A throwaway git project with a real commit, so `HEAD` resolves for `git worktree add`
/// and the run's base ref is real. Mirrors `tests/cli.rs`'s `temp_git_project_with_commit`.
fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let root = dir.path();
    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    git_ok(root, &["commit", "--allow-empty", "-q", "-m", "init"]);
    dir
}

/// Scaffold a single, real-worktree implementer unit named "solo": one gate (`ok`, always
/// green), no review tier, `on_pass: none` (verified but never merged - the minimal shape
/// that reaches a durable, inspectable unit branch without a merge or a review panel's own
/// agents to answer). Mirrors `tests/cli.rs`'s `write_unit_review_lenses_workflow` minus its
/// `review:` block - this file needs the unit's own durable worktree, not the review layer.
fn write_solo_unit_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: haltrecoverytest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: none
"#,
    )
    .unwrap();
}

/// Run `rigger <args...>` in `cwd`, with `envs` layered on top of the same baseline every
/// call needs (`RIGGER_NO_DASH`, a per-invocation `XDG_STATE_HOME`) - the caller's own entries
/// win on a name collision (e.g. round 3's `PATH` shim below). Mirrors `tests/cli.rs`'s
/// identically-named helper. [`run_rigger`] is this with an empty `envs` slice - the common
/// case every call site up to round 3 needs.
fn run_rigger_envs(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
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

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success). Mirrors
/// `tests/escalation_resume_periphery.rs`'s identically-named helper.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    run_rigger_envs(cwd, args, &[])
}

/// The DETERMINISTIC dir/branch `stage_worktree`'s `Worktree::create` would derive for a
/// unit named `unit` in `root`'s default (unconfigured) scratch root - mirrors
/// `unit_worktree_dir`/`unit_branch` in `src/conductor.rs`, which this file cannot import
/// (they are private), so it reconstructs the same well-known convention every other
/// fixture in this suite already asserts against via `common::default_scratch_root` (spec
/// 89, criterion 2: SCRATCH IS OUTSIDE THE STORE TREE moved the default off the old bare
/// `root.join(".rigger").join("tmp")` literal this helper used to hardcode - that literal
/// stopped matching what `sweep_terminal`'s own `d.starts_with(root)` gate and the
/// conductor's worktree adoption actually resolve, so a worktree this helper pre-seeded
/// there silently fell outside every step-start authority's own sweep domain).
fn unit_worktree_dir(root: &Path, unit: &str) -> std::path::PathBuf {
    common::default_scratch_root(root).join(format!("rigger-wt-{unit}"))
}

fn unit_branch(unit: &str) -> String {
    format!("rigger/u/{unit}")
}

/// Pre-create unit `unit`'s durable worktree exactly as `Worktree::create`'s
/// branch-does-not-exist-yet path would (`git worktree add -b <branch> <dir> HEAD`), then
/// leave `file` uncommitted in it with `content` - simulating a PRIOR incarnation of this
/// unit's implementer spawn that edited the tree and was halted (a liveness sweep, the
/// outer wall clock, a crash) before it could ever commit or report. This is the real-git
/// stand-in for `conductor.rs`'s own `Worktree::create` + `std::fs::write` setup, one layer
/// further out: a `rigger step` that has NEVER YET RUN in this project is about to adopt a
/// worktree that nonetheless already exists, dirty, on disk - the "first entry to THIS
/// process's handling of the unit" case the fix's own doc comment names.
fn seed_halted_worktree(root: &Path, unit: &str, file: &str, content: &str) -> std::path::PathBuf {
    let dir = unit_worktree_dir(root, unit);
    let branch = unit_branch(unit);
    git_ok(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            dir.to_str().unwrap(),
            "HEAD",
        ],
    );
    std::fs::write(dir.join(file), content).unwrap();
    let status = git_out(&dir, &["status", "--porcelain"]);
    assert!(
        !status.is_empty(),
        "setup must leave the pre-created worktree genuinely dirty: {status:?}"
    );
    dir
}

/// The full end-to-end proof: a unit's worktree exists, dirty, from a halted prior
/// incarnation before this project's very first `rigger step`. That step must (1) capture
/// the abandoned edit as its own `wip` commit on the unit's durable branch before spawning
/// the fresh implementer, (2) leave the abandoned file's content intact inside that commit,
/// and (3) hand the fresh implementer - reachable only through the REAL `rigger prompt`
/// command, reading the SAME on-disk store a separate process wrote - a prompt naming the
/// exact recovery commit and saying "finish and report; do not start over". A second,
/// independent `rigger step` while the spawn is still parked must not repeat the capture:
/// the branch keeps exactly one such commit and its tip does not move.
#[test]
fn a_halted_units_worktree_is_recovered_as_a_wip_commit_and_the_real_prompt_names_it() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_solo_unit_workflow(root);

    let wt_dir = seed_halted_worktree(root, "solo", "halted-work.txt", "abandoned mid-edit\n");

    // Step 1: this project's FIRST EVER `rigger step`. Nothing has been recorded for "solo"
    // yet, so this is attempt 0's Fresh-phase adoption - and the worktree it adopts, by a
    // deterministic path lookup alone, already carries the dirt seeded above.
    let (out1, err1, ok1) = run_rigger(root, &["step"]);
    assert!(ok1, "the first step must succeed; stderr: {err1}");
    assert!(
        out1.contains(r#""id":"solo/implementer#0""#),
        "the implementer must park at attempt 0 despite the pre-existing dirty tree; \
         got: {out1:?}"
    );

    // The recovery commit landed on the unit's durable branch, and the worktree itself is
    // clean again - the abandoned edit was swept IN, not discarded and not left dangling.
    let expected_subject = "wip(solo): tree of halted spawn solo/implementer#0";
    let log = git_out(root, &["log", "--pretty=%H %s", &unit_branch("solo")]);
    let recovery_line = log
        .lines()
        .find(|l| l.ends_with(expected_subject))
        .unwrap_or_else(|| {
            panic!(
                "the branch must carry a wip commit recovering the halted tree; wanted a \
                 line ending {expected_subject:?}, got:\n{log}\nstep 1 stderr (evidence of \
                 what actually happened to the pre-seeded worktree before recovery could \
                 run):\n{err1}"
            )
        });
    let recovery_sha = recovery_line
        .split_whitespace()
        .next()
        .expect("log line must start with a sha")
        .to_string();
    let tree_status = git_out(&wt_dir, &["status", "--porcelain"]);
    assert!(
        tree_status.is_empty(),
        "the recovery commit must leave the worktree clean: {tree_status:?}"
    );
    let survived = git_out(root, &["show", &format!("{recovery_sha}:halted-work.txt")]);
    assert_eq!(
        survived, "abandoned mid-edit",
        "the halted spawn's own file must survive, byte-for-byte, inside the recovery commit"
    );

    // The fresh implementer's prompt - fetched through the REAL `rigger prompt` command,
    // reading back the SAME store a separate `rigger step` process just wrote to - names
    // this exact recovery commit and tells the agent to finish rather than start over.
    let (prompt_out, prompt_err, prompt_ok) = run_rigger(root, &["prompt", "solo/implementer#0"]);
    assert!(
        prompt_ok,
        "rigger prompt must succeed for the parked implementer; stderr: {prompt_err}"
    );
    assert!(
        prompt_out.contains(&recovery_sha),
        "the real prompt must name the recovery commit {recovery_sha}; got:\n{prompt_out}"
    );
    assert!(
        prompt_out.contains("finish and report; do not start over")
            || prompt_out.contains("Finish and report; do not start over"),
        "the real prompt must tell the agent to finish and report rather than start over; \
         got:\n{prompt_out}"
    );

    // A second, independent step process - the spawn is still parked, nothing has resulted
    // it - must NOT repeat the capture: no second wip commit, and the branch tip is exactly
    // where step 1 left it (the ordinary, overwhelmingly common no-op path).
    let tip_after_step1 = git_out(root, &["rev-parse", &unit_branch("solo")]);
    let (out2, err2, ok2) = run_rigger(root, &["step"]);
    assert!(ok2, "the second step must succeed; stderr: {err2}");
    assert!(
        out2.contains(r#""id":"solo/implementer#0""#),
        "the same still-parked implementer must still be the reported wave entry; \
         got: {out2:?}"
    );
    let tip_after_step2 = git_out(root, &["rev-parse", &unit_branch("solo")]);
    assert_eq!(
        tip_after_step1, tip_after_step2,
        "a later step over the same still-parked spawn must not add another commit"
    );
    let log_after = git_out(root, &["log", "--pretty=%s", &unit_branch("solo")]);
    assert_eq!(
        log_after.lines().filter(|l| *l == expected_subject).count(),
        1,
        "the recovery commit must appear exactly once on the branch, never repeated on a \
         later poll of the same parked spawn: {log_after}"
    );
}

/// The paired negative-space case: a unit whose worktree is created FRESH by `rigger step`
/// itself (the overwhelmingly common path - no prior incarnation, nothing to recover) must
/// commit nothing extra and say nothing about a halt, through the real binary. Proves the
/// new capture is silent in the case that runs on every other unit in the entire suite,
/// not merely correct in the one dirty case the test above drives at.
#[test]
fn an_ordinary_freshly_created_worktree_never_gains_a_halt_recovery_commit_or_prompt_text() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_solo_unit_workflow(root);

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#),
        "the implementer must park at attempt 0; got: {out:?}"
    );

    let log = git_out(root, &["log", "--pretty=%s", &unit_branch("solo")]);
    assert!(
        !log.lines()
            .any(|l| l.starts_with("wip(solo): tree of halted spawn")),
        "a freshly created, never-touched worktree must never gain a halt-recovery commit: \
         {log:?}"
    );

    let (prompt_out, prompt_err, prompt_ok) = run_rigger(root, &["prompt", "solo/implementer#0"]);
    assert!(
        prompt_ok,
        "rigger prompt must succeed for the parked implementer; stderr: {prompt_err}"
    );
    assert!(
        !prompt_out.to_lowercase().contains("halted spawn")
            && !prompt_out.contains("do not start over"),
        "the ordinary first prompt must carry no halt-recovery language at all; \
         got:\n{prompt_out}"
    );
}

/// Round 2 fix, defect (a): a unit's worktree can be dirty, genuinely halted, at the exact
/// "branch tip is an ancestor of `rigger-run`, no spawn ever recorded" shape on THIS
/// project's very first `rigger step` - `ensure_run_branch` mints `rigger-run` at the current
/// HEAD before the step-start sweeps ever run, and a worktree seeded (by a restored snapshot,
/// or a prior incarnation of this exact process) off that SAME HEAD is trivially an ancestor
/// of it from the first instant. `sweep_terminal` and `reclaim_orphan_scratch` (`main.rs`)
/// both run BEFORE `run_single_stage`'s halted-commit capture ever gets a chance - so either
/// one discarding this candidate outright, before the capture runs, destroys the abandoned
/// edit permanently rather than merely deferring its capture by one step. This proves BOTH
/// authorities spare it, in the SAME real `cmd_step` invocation, through the compiled binary.
#[test]
fn a_declared_units_dirty_worktree_survives_the_step_start_sweep_backstops() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_solo_unit_workflow(root);

    let wt_dir = seed_halted_worktree(root, "solo", "halted-work.txt", "abandoned mid-edit\n");

    let (out1, err1, ok1) = run_rigger(root, &["step"]);
    assert!(ok1, "the first step must succeed; stderr: {err1}");
    assert!(
        out1.contains(r#""id":"solo/implementer#0""#),
        "the implementer must still park at attempt 0 despite the pre-existing dirty tree \
         racing the step-start sweeps; got: {out1:?}"
    );
    assert!(
        err1.contains("worktree sweep: kept") && err1.contains("halt-recovery commit"),
        "the sweep must explicitly log that it kept this declared, dirty candidate for the \
         halt-recovery capture rather than silently doing nothing; stderr: {err1}"
    );

    // The abandoned edit survived BOTH backstops and was then captured as this process's own
    // wip commit - the SAME recovery this file's first test already proves in the simpler
    // (no pre-existing `rigger-run`) case.
    assert!(
        wt_dir.join("halted-work.txt").exists(),
        "the abandoned edit must survive both step-start sweep backstops untouched"
    );
    let expected_subject = "wip(solo): tree of halted spawn solo/implementer#0";
    let log = git_out(root, &["log", "--pretty=%s", &unit_branch("solo")]);
    assert!(
        log.lines().any(|l| l == expected_subject),
        "the branch must still carry the halt-recovery wip commit despite the sweeps having \
         run first; got:\n{log}"
    );
    let survived = git_out(&wt_dir, &["show", "HEAD:halted-work.txt"]);
    assert_eq!(
        survived, "abandoned mid-edit",
        "the halted spawn's own file must survive, byte-for-byte, inside the recovery commit"
    );
}

/// The `project_identity` a fresh, real `rigger` process resolves for `root` - mirrors
/// `tests/cli.rs`'s identically-named helper (the tracked `.rigger/project.id` at the git
/// top-level when present, else the git top-level basename, else `root`'s own basename), so a
/// seed appended under this identity lands in the exact stream a later `rigger step` reads.
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

/// Seed run-lifecycle events directly into the namespaced run stream, standing in for the
/// conductor minting them - mirrors `tests/cli.rs`'s identically-named helper. `rigger emit`
/// (the guarded courier CLI) refuses these conductor-owned boundary types (spec 22), so a
/// test that must seed a PRIOR window's recorded lifecycle appends through the store
/// directly, at the SAME identity a later real `rigger step` process resolves for `root`.
fn seed_run_events(root: &Path, events: &[(&str, serde_json::Value)]) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for (ty, data) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(*ty, serde_json::to_vec(data).unwrap())],
            )
            .unwrap();
    }
}

/// Round 2 fix, defect (b): a resumed `ResumePhase::Reviewed` unit whose worktree ALREADY
/// carries a genuine, still-unresolved merge conflict from a prior window's real crash (a
/// liveness sweep, the outer wall clock) must reach the existing `merge_into_worktree`/
/// `merge_in_progress` idempotent resume path (spec 88) - never trip `run_single_stage`'s
/// halted-commit capture's conflict-marker refusal first, which would turn a resumable state
/// into a hard, no-attempt-charged error.
///
/// The pre-crash state is built directly through the PUBLIC `Worktree` API (`create` +
/// `merge_into_worktree`), never through a `run()` call - a real `run()` call that reached
/// this same state and then genuinely errored would ALSO tear its own worktree down as
/// ordinary cleanup (confirmed empirically: a real crash is the only way this exact shape
/// arises, exactly why the implementer's own unit test builds it the same way), so this
/// mirrors that construction one layer further out - a real git worktree, and a real
/// `Store::open` round trip a completely separate `rigger step` process reads back - the
/// shape this file's own header already argues for.
#[test]
fn a_resumed_reviewed_units_real_crash_frozen_merge_conflict_reaches_the_idempotent_path() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    for (id, body) in [("worker", "Do the unit."), ("judge", "Adjudicate it.")] {
        std::fs::write(
            rigger.join("agents").join(format!("{id}.md")),
            format!("---\nid: {id}\nmodel: sonnet\ntools: [Read, Edit]\n---\n{body}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        rigger.join("workflow.yml"),
        "name: resumedconflicttest\n\
         defaults:\n  grounder: nop\n  budget: 60\n\
         gates:\n  ok: { run: \"true\", kind: core }\n\
         regenerate:\n  - paths: [\"shared.rs\"]\n    run: \"printf 'REGENERATED\\n' > shared.rs\"\n\
         stages:\n  s:\n    agent: worker\n    gates: [ok]\n    on_pass: merge\n    \
         review:\n      adjudicator: judge\n",
    )
    .unwrap();

    // The unit's OWN deterministic worktree/branch, created directly at the same dir/branch
    // `stage_worktree`'s adopt-by-path-lookup would derive - and, critically, never `.remove()`d,
    // so the real `rigger step` below ADOPTS this exact on-disk state rather than a fresh one.
    let repo_path = root.to_str().unwrap().to_string();
    let wt_dir = unit_worktree_dir(root, "s");
    let unit_wt = rigger::worktree::Worktree::create(
        &repo_path,
        wt_dir.to_str().unwrap(),
        &unit_branch("s"),
        "",
    )
    .unwrap();
    std::fs::write(wt_dir.join("shared.rs"), "UNIT VERSION\n").unwrap();
    let approved = unit_wt.commit("rigger: prior window work").unwrap();
    assert!(
        !approved.is_empty(),
        "the prior window must commit the approved work"
    );

    // The checked-out repo independently gains a DIFFERENT version of the same path since -
    // exactly what an already-integrated batch-mate would have landed by the time this unit's
    // own merge finally runs.
    std::fs::write(root.join("shared.rs"), "RUN VERSION\n").unwrap();
    git_ok(root, &["add", "shared.rs"]);
    git_ok(
        root,
        &[
            "commit",
            "-q",
            "-m",
            "a batch-mate's own conflicting change",
        ],
    );

    // Simulate the interruption directly: a PRIOR incarnation's own `integrate_and_emit`
    // already ran exactly this merge and was killed before conflict resolution ever ran,
    // leaving MERGE_HEAD and literal conflict-marker text sitting in `shared.rs` untouched.
    match unit_wt
        .merge_into_worktree("rigger: integrate s (interrupted before this ever finished)")
        .unwrap()
    {
        rigger::worktree::MergeOutcome::Conflict(paths) => {
            assert_eq!(
                paths,
                ["shared.rs"],
                "setup must conflict on shared.rs alone"
            );
        }
        rigger::worktree::MergeOutcome::Ready(_) => {
            panic!("setup premise: the divergent shared.rs edits must conflict")
        }
    }
    assert!(
        unit_wt.merge_in_progress(),
        "setup must leave a genuine merge in progress, unresolved"
    );
    let conflicted_before = std::fs::read_to_string(wt_dir.join("shared.rs")).unwrap();
    assert!(
        conflicted_before.contains("<<<<<<<"),
        "setup must leave real conflict-marker text in place: {conflicted_before:?}"
    );

    // The prior window's recorded lifecycle: implemented, verified, and adjudicator-approved -
    // seeded directly into the real on-disk store a separate `rigger step` process reads back.
    seed_run_events(
        root,
        &[
            (
                "RunStarted",
                serde_json::json!({"run": "r1", "criteria": []}),
            ),
            (
                "UnitStarted",
                serde_json::json!({"id": "s", "agent": "worker", "branch": unit_branch("s")}),
            ),
            (
                "UnitStatus",
                serde_json::json!({"id": "s", "status": "verified"}),
            ),
            (
                "UnitStatus",
                serde_json::json!({"id": "s", "status": "reviewed"}),
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a resumed unit's own already-in-progress merge conflict must reach the idempotent \
         merge_into_worktree/merge_in_progress path, never trip the halted-commit capture's \
         conflict-marker refusal first; stderr: {err}"
    );
    assert!(
        out.contains(r#""done":true"#) && out.contains(r#""wave":[]"#),
        "the conflict is confined to a registered regenerable path: it must resolve with NO \
         new lifecycle spawn at all (never a re-spawned implementer or adjudicator), proving \
         the real conflict-handling path ran rather than some other fallback; got: {out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("shared.rs")).unwrap(),
        "REGENERATED\n",
        "the run branch must carry the real regeneration, not a discarded or half-merged tree"
    );
    // The halted-commit capture's own guard fired (skipped the commit outright) rather than
    // ever running it: the ONLY commit this unit's own side ever gained is the prior window's
    // real work, folded straight into the merge's own commit - never an extra `wip(s):` one.
    let merge_commit_parents = git_out(root, &["log", "--pretty=%P", "-1", "rigger-run"]);
    assert_eq!(
        merge_commit_parents.split_whitespace().count(),
        2,
        "the run branch's tip must be the real regenerate-and-merge commit (two parents), not \
         some other shape; got parents: {merge_commit_parents:?}"
    );
}

/// Round 2 fix, defect (c): `Worktree::commit`'s conflict-marker scan must be confined to the
/// CALLER's own touched-or-unmerged paths - never an unconditional whole-tracked-tree content
/// scan that can false-positive on conflict-marker-lookalike text sitting in a file nobody
/// touched. Deliberately isolated from defect (a)'s own test: a plain two-step implementer
/// flow with NO pre-existing worktree at all, so the step-start sweep backstops never enter
/// the picture and only the checkpoint-commit's own scope guard is exercised. Proven a real
/// regression (not merely a plausible one) by reverting to the pre-round-2 `worktree.rs`: this
/// exact scenario then fails on step 1 itself - even a brand-new, freshly-adopted worktree's
/// own halted-commit capture (a no-op "nothing to commit" on a truly clean tree) still ran the
/// unconditional whole-tree scan first and refused on the lookalike text.
#[test]
fn an_untouched_conflict_marker_lookalike_file_never_blocks_an_unrelated_checkpoint_commit() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_solo_unit_workflow(root);

    // A file already committed BEFORE this unit's worktree ever branches - never edited by
    // this unit - that merely happens to contain literal conflict-marker-lookalike text (a
    // Markdown/prose example of what a real conflict looks like).
    std::fs::write(
        root.join("conflict-markers-doc.txt"),
        "Git conflict markers look like:\n<<<<<<< ours\nold\n=======\nnew\n>>>>>>> theirs\n",
    )
    .unwrap();
    git_ok(root, &["add", "conflict-markers-doc.txt"]);
    git_ok(root, &["commit", "-q", "-m", "document conflict markers"]);

    let (out1, err1, ok1) = run_rigger(root, &["step"]);
    assert!(ok1, "the first step must succeed; stderr: {err1}");
    assert!(
        out1.contains(r#""id":"solo/implementer#0""#),
        "the implementer must park at attempt 0; got: {out1:?}"
    );

    // The implementer's own real edit - a DIFFERENT file, left uncommitted for the
    // conductor's own per-attempt checkpoint commit to pick up.
    let wt_dir = unit_worktree_dir(root, "solo");
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    let (_out, err, ok) = run_rigger(root, &["result", "solo/implementer#0", "implemented"]);
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2 is where the conductor's own checkpoint commit runs, over a worktree whose
    // TRACKED CONTENT includes the untouched lookalike file end to end - the false-positive
    // shape this criterion closes.
    let (out2, err2, ok2) = run_rigger(root, &["step"]);
    assert!(
        ok2,
        "the checkpoint commit must never be blocked by conflict-marker-lookalike text in a \
         file this unit never touched; stderr: {err2}"
    );
    assert!(
        out2.contains(r#""done":true"#),
        "the unit (a single-gate, unreviewed, on_pass: none stage) must reach its terminal \
         verified state once the checkpoint commit succeeds; got: {out2:?}"
    );
    let committed = git_out(root, &["show", &format!("{}:work.rs", unit_branch("solo"))]);
    assert_eq!(
        committed, "pub fn work() {}",
        "the implementer's own real edit must have actually landed in the checkpoint commit"
    );
    let untouched = git_out(
        root,
        &[
            "show",
            &format!("{}:conflict-markers-doc.txt", unit_branch("solo")),
        ],
    );
    assert!(
        untouched.contains("<<<<<<<"),
        "the untouched lookalike file's own content must be completely unaffected: {untouched:?}"
    );
}

/// Resolve the REAL `git` binary's absolute path from THIS process's own, as-yet-unmodified
/// `PATH` - captured before [`stage_status_failing_git_shim`] below could shadow it, so the
/// shim script always execs the genuine binary directly rather than searching a `PATH` this
/// file is about to prefix.
fn real_git_path() -> String {
    let out = Command::new("sh")
        .arg("-c")
        .arg("command -v git")
        .output()
        .expect("locate the real git binary via the shell's own PATH search");
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        !path.is_empty(),
        "git must be resolvable on PATH to build the status-failing shim"
    );
    path
}

/// Stage a `git` SHIM directory (never on the real `PATH`) whose one script, driven entirely
/// by env vars the caller sets on the spawned `rigger` process, fails the first
/// `$SDET_FAIL_STATUS_TIMES` real invocations of `-C $SDET_FAIL_STATUS_DIR status --porcelain
/// -z` - the EXACT argv `rigger::worktree::path_is_dirty`'s own `git`/`run_git` primitive
/// builds (`src/worktree.rs`) - then execs the real git (`$SDET_REAL_GIT`, see
/// [`real_git_path`]) for every OTHER invocation immediately, and for a MATCHING one too once
/// that budget is spent. The git repository itself is never touched or corrupted by this -
/// only this one exact command shape, against this one exact directory, is ever intercepted -
/// so nothing here engages `Worktree::create`'s own, unrelated admin self-heal
/// (`heal_corrupt_worktree_admin`), which a REAL corrupted-admin-dir repro (the shape
/// `adv-u89c1r2-dirty-check-fail-direction-live-confirmed-via-documented-race` used) would.
/// Returns the shim's own directory, to be prefixed onto `PATH`.
fn stage_status_failing_git_shim(root: &Path) -> std::path::PathBuf {
    let bindir = root.join("shim-bin");
    std::fs::create_dir_all(&bindir).unwrap();
    let shim = bindir.join("git");
    std::fs::write(
        &shim,
        r#"#!/bin/sh
if [ "$1" = "-C" ] && [ "$2" = "$SDET_FAIL_STATUS_DIR" ] && [ "$3" = "status" ] \
    && [ "$4" = "--porcelain" ] && [ "$5" = "-z" ]; then
    n=$(cat "$SDET_FAIL_STATUS_COUNTER" 2>/dev/null)
    n=${n:-0}
    if [ "$n" -lt "$SDET_FAIL_STATUS_TIMES" ]; then
        echo $((n + 1)) > "$SDET_FAIL_STATUS_COUNTER"
        echo "fatal: simulated unreadable git status (sdet periphery shim)" >&2
        exit 128
    fi
fi
exec "$SDET_REAL_GIT" "$@"
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bindir
}

/// ROUND 3 FIX (`arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`, live-
/// confirmed by `adv-u89c1r2-dirty-check-fail-direction-live-confirmed-via-documented-race`):
/// round 2's own `a_declared_units_dirty_worktree_survives_the_step_start_sweep_backstops`
/// above proves both step-start authorities spare a worktree that LOOKS dirty (real
/// uncommitted content) - it cannot tell that apart from a worktree whose git status READ
/// ITSELF fails, and round 2 shipped with exactly that gap: `sweep_terminal_logged`
/// (worktree.rs) fell back to `unwrap_or(false)` on `run_git`'s `Err` (dirty=true, spare),
/// while `reclaim_orphan_scratch`'s OWN independently-hardcoded `git status` call (main.rs)
/// collapsed the IDENTICAL failure to `dirty=false` (discard) - the opposite direction on the
/// same input, live-reproduced against a real corrupted worktree admin dir. Round 3 replaced
/// both inline calls with the one shared `rigger::worktree::path_is_dirty` free fn,
/// `unwrap_or(true)` at both sites.
///
/// This proves the fix through the REAL compiled binary without corrupting the git
/// repository itself (see [`stage_status_failing_git_shim`]'s own doc comment for why not): a
/// `git` shim on `PATH` fails the underlying `status --porcelain -z` READ itself (never
/// merely reporting dirty CONTENT) for exactly the first two real invocations against this
/// worktree - one budget slot per authority - then passes every later matching call straight
/// through to the genuine git, so the rest of the pipeline (the halted-commit capture, which
/// reads this SAME candidate's real, genuinely-dirty status once adoption begins) completes
/// exactly as it does without the shim. The shim's own counter file, read back after the
/// step, independently confirms BOTH real call sites actually reached the shared primitive
/// and actually hit the simulated failure - a count other than 2 means one of them stopped
/// sharing it (round 2's own duplicated-and-diverges defect returning) or the fail-closed
/// budget leaked into a later, unrelated read; content-only dirtiness (the round-2 test above)
/// cannot distinguish either regression from a pass.
#[test]
fn both_step_start_backstops_share_path_is_dirty_and_fail_closed_on_an_unreadable_status_read() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_solo_unit_workflow(root);

    let wt_dir = seed_halted_worktree(root, "solo", "halted-work.txt", "abandoned mid-edit\n");

    let real_git = real_git_path();
    let shim_dir = stage_status_failing_git_shim(root);
    let orig_path = std::env::var("PATH").unwrap_or_default();
    let shimmed_path = format!("{}:{}", shim_dir.display(), orig_path);
    let counter = tempfile::NamedTempFile::new().expect("create the shim's counter file");
    let counter_path = counter.path().to_string_lossy().into_owned();
    let fail_dir = wt_dir.to_string_lossy().into_owned();

    let (out1, err1, ok1) = run_rigger_envs(
        root,
        &["step"],
        &[
            ("PATH", shimmed_path.as_str()),
            ("SDET_REAL_GIT", real_git.as_str()),
            ("SDET_FAIL_STATUS_DIR", fail_dir.as_str()),
            ("SDET_FAIL_STATUS_TIMES", "2"),
            ("SDET_FAIL_STATUS_COUNTER", counter_path.as_str()),
        ],
    );
    assert!(
        ok1,
        "the step must still succeed even though both backstops' own status read genuinely \
         errors for this candidate; stderr: {err1}"
    );
    assert!(
        out1.contains(r#""id":"solo/implementer#0""#),
        "the implementer must still park at attempt 0 despite both status reads erroring; \
         got: {out1:?}"
    );
    assert!(
        err1.contains("worktree sweep: kept") && err1.contains("halt-recovery commit"),
        "sweep_terminal_logged must still log that it kept this candidate, exactly as it does \
         for a content-dirty tree, when the status read itself is what failed; stderr: {err1}"
    );

    let hits: u32 = std::fs::read_to_string(counter.path())
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or(0);
    assert_eq!(
        hits, 2,
        "both real call sites - sweep_terminal_logged and reclaim_orphan_scratch - must reach \
         the shared path_is_dirty primitive and hit the simulated unreadable-status failure \
         exactly once each; a count other than 2 means one of them stopped sharing the \
         primitive (round 2's own duplicated-and-diverges defect) or the fail-closed budget \
         leaked into a later, unrelated status read; got {hits} matching shim invocations"
    );

    // Neither authority force-removed the candidate: the abandoned edit is still on disk, and
    // the halt-recovery capture - reached once adoption proceeds, its OWN status read now past
    // the shim's budget and answered by the genuine, unmodified git - still committed it.
    assert!(
        wt_dir.join("halted-work.txt").exists(),
        "the abandoned edit must survive both backstops despite the simulated read failures"
    );
    let expected_subject = "wip(solo): tree of halted spawn solo/implementer#0";
    let log = git_out(root, &["log", "--pretty=%s", &unit_branch("solo")]);
    assert!(
        log.lines().any(|l| l == expected_subject),
        "the branch must still carry the halt-recovery wip commit once the shim's budget is \
         spent and the real status read resumes; got:\n{log}"
    );
    let survived = git_out(&wt_dir, &["show", "HEAD:halted-work.txt"]);
    assert_eq!(
        survived, "abandoned mid-edit",
        "the halted spawn's own file must survive, byte-for-byte, inside the recovery commit"
    );
}
