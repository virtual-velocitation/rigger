//! Periphery (cross-module contract) test for the spec-78 base-guard's interaction with
//! `src/worktree.rs::Worktree::remove` when the scratch root is RELOCATED (spec 77 criterion 4,
//! BOUNDED SHARED CACHE / `defaults.workdir`; design-intent Gap 14) to a path that is not
//! nested under the worktree's own project repo's `.rigger/tmp`.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/reap.rs`'s own unit tests for `is_reapable_base` exercise a `FakeRepo` fixture where
//! the base is ALWAYS strictly under that same fixture's `.rigger/tmp`, or (for the refusal
//! cases) a bare directory with NO enclosing repo at all - never the specific shape a
//! RELOCATED, legitimately-configured scratch root takes. `src/worktree.rs`'s own pre-existing
//! `remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside` test already
//! mirrors production for the DEFAULT (unrelocated) case - the worktree lives under
//! `<repo>/.rigger/tmp` - and (verified independently for this file: `cargo test --lib
//! worktree::tests::remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside`)
//! still passes. Neither side's tests ever construct the relocated shape
//! `tests/scratch_workdir_config.rs::a_present_workdir_deserializes_exactly` proves is a real,
//! documented, tested configuration surface of this crate (`defaults.workdir: /custom/scratch`,
//! an absolute path with no necessary relationship to the project repo at all -
//! `rigger::worktree::scratch_root_path`'s own doc comment names the motivating case: a small
//! root partition where `<repo>` itself may sit, and a large unrelated mount the operator
//! points scratch at instead).
//!
//! ROUND 1 (`is_reapable_base` canonicalizing a hardcoded `<repo>/.rigger/tmp` literal) broke
//! `Worktree::remove`'s own doc comment promise ("Reap any process still rooted inside this
//! worktree BEFORE git removes the dir (spec 23): otherwise a build or tool an agent left
//! running holds a now-deleted cwd and outlives its worktree, leaking memory") for exactly the
//! relocated deployment shape the relocation knob exists to reach: a worktree dir under a
//! scratch root with no relationship to the project repo could never canonicalize under that
//! repo's `.rigger/tmp`, so a `cargo`/`rustc`-shaped process (SIGTERM-ignoring, so only the
//! SIGKILL escalation would end it) left running there outlived `Worktree::remove()` deleting
//! the worktree's dir out from under it. This test proved that boundary bug RED
//! (`adj-u78c2-verdict-reject-reap-authority-conflict`), driving the round-1 reject.
//!
//! ROUND 2 FIX (decision `u78c2r2-worktree-remove-identity-not-tree`): a worktree's own dir can
//! legitimately live anywhere relative to its repo, so no `authorized_root` a caller could
//! compute would reliably contain it (the same relocation surface this file exercises).
//! `Worktree::remove` now authorizes its reap by GIT IDENTITY instead of containment - reusing
//! the existing `worktree_on_branch` predicate to confirm `self.dir` IS a real, currently
//! checked-out worktree of `self.branch` before calling `crate::reap::reap_authorized`
//! directly (bypassing `is_reapable_base`'s containment gate entirely). This test now CONFIRMS
//! that fix holds: independently re-run against the round-2 diff, it PASSES - a live process
//! under a relocated scratch root is reaped exactly as it always was for the default case.

use std::path::Path;
use std::process::{Child, Command};

use rigger::reap::processes_rooted_under;
use rigger::worktree::Worktree;

/// Spawn a long-lived process rooted at `dir` that IGNORES SIGTERM, so only a SIGKILL
/// escalation can end it - exercising the full SIGTERM-then-SIGKILL mechanism
/// `reap_processes_rooted_under` runs. Mirrors the identical fixture in `src/reap.rs` and
/// `src/worktree.rs`'s own test module.
fn sigterm_ignorer_in(dir: &Path) -> Child {
    Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; while :; do sleep 1; done")
        .current_dir(dir)
        .spawn()
        .expect("spawn a SIGTERM-ignoring fixture process")
}

/// Poll up to 5s for `pred`, matching the scan/escalation latency tolerance every sibling
/// reap test in this tree already uses.
fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if pred() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

fn init_repo(path: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        assert!(Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
}

#[test]
fn worktree_remove_still_reaps_a_process_when_its_dir_lives_under_a_relocated_scratch_root() {
    // `project_repo` is the worktree's OWN repo (what `Worktree::create`'s `repo` argument
    // names, and what `is_reapable_base` resolves `<repo>` from via the worktree dir's git
    // context). `relocated_scratch` stands in for a `defaults.workdir`-configured root
    // (`tests/scratch_workdir_config.rs` proves an arbitrary absolute path like this is a
    // real, supported value for that field) that has NO relationship to `project_repo` at
    // all - not nested inside it, not a repo of its own - exactly like `$HOME`/an unrelated
    // large mount would be relative to a small-root-partitioned project checkout.
    let project_repo = tempfile::tempdir().unwrap();
    let project_repo_path = project_repo.path().canonicalize().unwrap();
    init_repo(&project_repo_path);

    let relocated_scratch = tempfile::tempdir().unwrap();
    let wt_dir = relocated_scratch
        .path()
        .canonicalize()
        .unwrap()
        .join("rigger-wt-relocatedtest");

    let wt = Worktree::create(
        project_repo_path.to_str().unwrap(),
        wt_dir.to_str().unwrap(),
        "rigger/u/relocatedtest",
    )
    .expect("create a worktree at the relocated dir");

    let mut child = sigterm_ignorer_in(&wt_dir);
    assert!(
        wait_until(|| processes_rooted_under(&wt_dir)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the worktree dir before \
         remove() runs"
    );

    wt.remove().expect("remove() itself must still succeed");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "Worktree::remove's own doc comment promises every process rooted in the worktree is \
         reaped BEFORE the dir is removed, unconditionally - a SIGTERM-ignoring process here \
         must still be SIGKILLed, exactly as the DEFAULT (unrelocated) shape already is (see \
         worktree::tests::remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside, \
         independently re-run and confirmed green). Round 1 broke this for a scratch root \
         relocated via defaults.workdir/RIGGER_TMPDIR to a location outside the project repo \
         (a real, tested configuration surface - see tests/scratch_workdir_config.rs): \
         is_reapable_base's <repo>/.rigger/tmp containment requirement could never accept such \
         a dir, so reap_processes_rooted_under silently no-opped (adj-u78c2-verdict-reject- \
         reap-authority-conflict). Round 2 (decision u78c2r2-worktree-remove-identity-not-tree) \
         fixed it by authorizing the reap via GIT IDENTITY (is self.dir a real, currently \
         checked-out worktree of self.branch?) instead of path containment, calling \
         reap_authorized directly - a regression back to the round-1 shape would fail this \
         assertion again."
    );
}
