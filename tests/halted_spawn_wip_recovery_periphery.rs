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
//! own `mod tests`, `commit_refuses_a_worktree_with_a_merge_left_in_progress` and
//! `commit_refuses_when_tracked_content_carries_conflict_marker_text`, already exercise that
//! guard directly against a real git repo through the public `Worktree` API), and the
//! `regenerate_conflicted_paths` integration-merge finalize path that also now runs through
//! the same guarded `commit` (already covered end-to-end, at the real binary boundary,
//! by `tests/integrate_conflict_merge_periphery.rs`).

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

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success). Mirrors
/// `tests/escalation_resume_periphery.rs`'s identically-named helper.
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

/// The DETERMINISTIC dir/branch `stage_worktree`'s `Worktree::create` would derive for a
/// unit named `unit` in `root`'s default (unconfigured) scratch root - mirrors
/// `unit_worktree_dir`/`unit_branch` in `src/conductor.rs`, which this file cannot import
/// (they are private), so it reconstructs the same well-known convention every other
/// `tests/cli.rs` fixture already asserts against (e.g.
/// `step_halts_on_an_exhausted_lens_beside_a_parked_sibling_and_keeps_the_unit_worktree`'s
/// `root/.rigger/tmp/rigger-wt-solo`).
fn unit_worktree_dir(root: &Path, unit: &str) -> std::path::PathBuf {
    root.join(".rigger")
        .join("tmp")
        .join(format!("rigger-wt-{unit}"))
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
