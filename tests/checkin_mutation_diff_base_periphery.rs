//! Periphery (real-git) proof for spec 91's round-2 fix (upheld finding
//! adv-u91c2-mutation-gate-diff-base-collapses-to-empty): the checkin stage's `mutation`
//! gate must diff the whole spec against `$RIGGER_RUN_BASE` - the run branch's tip commit
//! sha AT THE MOMENT the run started (`RunStarted.base_tip`) - NEVER a `git merge-base`
//! with the run branch.
//!
//! WHY A MERGE-BASE COLLAPSES TO EMPTY HERE. The checkin stage's own worktree branches off
//! the run branch (`rigger-run`) AFTER every implement unit has already integrated onto it
//! (`needs: [implement]`), so by the time the mutation gate's command ever runs,
//! `git merge-base rigger-run HEAD` is trivially `HEAD` itself - a `git diff` against that
//! merge-base is always empty, and the whole-spec mutation sweep (this gate's entire stated
//! purpose) would silently certify nothing, every run.
//!
//! This file drives the LITERAL, shipped `.rigger/workflow.yml` gate command (never a
//! hand-copied stand-in that could quietly drift from what actually ships) against a real
//! git repository whose topology reproduces the exact defect shape, so a future edit that
//! reintroduces the merge-base idiom - or drops the `test -n` guard - fails this test, not
//! just a human's re-reading of the YAML.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The path to this repo's own committed `.rigger/workflow.yml`, resolved the same
/// CWD-independent way every other committed-file pin in this suite does.
fn workflow_yml_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".rigger")
        .join("workflow.yml")
}

/// The REAL, shipped `mutation` gate's `run:` command, read straight off
/// `.rigger/workflow.yml` (a plain YAML parse - no `Config::validate`, so this test needs no
/// `cargo-mutants` on PATH to even load the string it is about to slice).
fn shipped_mutation_gate_command() -> String {
    let path = workflow_yml_path();
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    doc["gates"]["mutation"]["run"]
        .as_str()
        .unwrap_or_else(|| panic!("gates.mutation.run must be a string in {}", path.display()))
        .to_string()
}

/// The diff-computation PREFIX of the shipped mutation gate command - the
/// `test -n "$RIGGER_RUN_BASE" && git diff "$RIGGER_RUN_BASE" -- '*.rs' > unit.diff` clause
/// this file's two tests exercise - sliced off BEFORE the `rm -rf "$MUTANTS" && mkdir -p ...`
/// / `cargo mutants` tail, which neither test needs to run (no Rust-project fixture, no
/// multi-minute sweep, no `cargo-mutants` dependency for this test binary at all).
fn diff_computation_prefix(full: &str) -> String {
    let marker = "rm -rf";
    let idx = full.find(marker).unwrap_or_else(|| {
        panic!(
            "the shipped mutation gate no longer contains {marker:?} - it \
                                    may have been restructured; update this fixture's slice \
                                    point to match. got: {full}"
        )
    });
    full[..idx]
        .trim_end()
        .trim_end_matches("&&")
        .trim_end()
        .to_string()
}

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

fn init_repo(dir: &Path) {
    git_ok(dir, &["init", "-q"]);
    git_ok(dir, &["config", "user.email", "t@example.com"]);
    git_ok(dir, &["config", "user.name", "t"]);
}

fn commit_all(dir: &Path, msg: &str) {
    git_ok(dir, &["add", "-A"]);
    git_ok(dir, &["commit", "-q", "-m", msg]);
}

#[test]
fn the_shipped_mutation_gate_guards_on_rigger_run_base_never_a_merge_base() {
    let full = shipped_mutation_gate_command();
    assert!(
        full.contains("test -n \"$RIGGER_RUN_BASE\""),
        "the shipped mutation gate must guard on RIGGER_RUN_BASE before diffing; got: {full}"
    );
    assert!(
        full.contains("git diff \"$RIGGER_RUN_BASE\""),
        "the shipped mutation gate must diff against RIGGER_RUN_BASE; got: {full}"
    );
    assert!(
        !full.contains("merge-base"),
        "the shipped mutation gate must never recompute a merge-base with the run branch - \
         spec 91's own amendment retired it because it collapses to HEAD once the checkin \
         stage's worktree branches off the run branch, after every implement unit has \
         integrated (adv-u91c2-mutation-gate-diff-base-collapses-to-empty); got: {full}"
    );
}

#[test]
fn mutation_gate_diffs_against_rigger_run_base_capturing_the_whole_spec_diff() {
    let prefix = diff_computation_prefix(&shipped_mutation_gate_command());

    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    init_repo(dir);
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    commit_all(dir, "origin");

    // The run branch anchors HERE - RIGGER_RUN_BASE is stamped from this exact tip when a
    // real run mints its RunStarted (spec 91, RunStarted.base_tip).
    git_ok(dir, &["checkout", "-q", "-b", "rigger-run"]);
    let base_tip = git_out(dir, &["rev-parse", "HEAD"]);

    // Two implement units land, each merging onto rigger-run - exactly as they do in a real
    // run, BEFORE the checkin stage's own worktree ever exists.
    std::fs::write(dir.join("a.rs"), "fn a() { 1; }\n").unwrap();
    commit_all(dir, "unit one lands");
    std::fs::write(dir.join("b.rs"), "fn b() {}\n").unwrap();
    commit_all(dir, "unit two lands");

    // The checkin stage's OWN worktree branches off rigger-run's tip AFTER both units above
    // have already integrated - the exact topology spec 91's amendment names.
    git_ok(dir, &["checkout", "-q", "-b", "checkin-worktree"]);
    let head = git_out(dir, &["rev-parse", "HEAD"]);
    let merge_base = git_out(dir, &["merge-base", "rigger-run", "HEAD"]);
    assert_eq!(
        merge_base, head,
        "fixture precondition: the checkin worktree's merge-base with rigger-run must already \
         equal HEAD (reproducing the exact topology the retired line silently diffed nothing \
         against) - otherwise this test would not be exercising the defect at all"
    );

    let status = Command::new("sh")
        .arg("-c")
        .arg(&prefix)
        .current_dir(dir)
        .env("RIGGER_RUN_BASE", &base_tip)
        .status()
        .expect("run the shipped diff-computation prefix");
    assert!(
        status.success(),
        "the diff-computation prefix must succeed once RIGGER_RUN_BASE is set"
    );

    let produced =
        std::fs::read_to_string(dir.join("unit.diff")).expect("unit.diff must be written");
    let expected = git_out(dir, &["diff", &base_tip, "--", "*.rs"]);
    assert!(
        !expected.is_empty(),
        "fixture precondition: the whole spec diff must itself be non-empty"
    );
    assert_eq!(
        produced.trim_end(),
        expected.trim_end(),
        "RIGGER_RUN_BASE must diff against the run's ACTUAL starting tip, capturing the whole \
         spec diff across every implement unit - never the empty diff a merge-base with the \
         (already-advanced) run branch would produce at this exact topology"
    );
}

#[test]
fn mutation_gate_refuses_loud_when_rigger_run_base_is_unset_rather_than_sweeping_an_empty_diff() {
    let prefix = diff_computation_prefix(&shipped_mutation_gate_command());

    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    init_repo(dir);
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    commit_all(dir, "origin");

    // A legacy run whose RunStarted predates `base_tip` (spec 91) simply never has
    // RIGGER_RUN_BASE to export. `env_remove` guards against the ambient test-runner
    // environment ever carrying a stray value of its own.
    let status = Command::new("sh")
        .arg("-c")
        .arg(&prefix)
        .current_dir(dir)
        .env_remove("RIGGER_RUN_BASE")
        .status()
        .expect("run the shipped diff-computation prefix");

    assert!(
        !status.success(),
        "with no RIGGER_RUN_BASE, the gate's own `test -n` guard must fail the command loud - \
         never silently proceed to an empty (or missing) diff"
    );
    assert!(
        !dir.join("unit.diff").exists(),
        "a refused gate must never even attempt the git diff - no unit.diff should be written \
         at all when RIGGER_RUN_BASE is unset"
    );
}
