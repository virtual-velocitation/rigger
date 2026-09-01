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
//! still passes after this diff. Neither side's tests ever construct the relocated shape
//! `tests/scratch_workdir_config.rs::a_present_workdir_deserializes_exactly` proves is a real,
//! documented, tested configuration surface of this crate (`defaults.workdir: /custom/scratch`,
//! an absolute path with no necessary relationship to the project repo at all -
//! `rigger::worktree::scratch_root_path`'s own doc comment names the motivating case: a small
//! root partition where `<repo>` itself may sit, and a large unrelated mount the operator
//! points scratch at instead).
//!
//! `is_reapable_base` (this diff) requires the base to canonicalize to somewhere STRICTLY
//! under `<repo>/.rigger/tmp`. A worktree dir created under a RELOCATED scratch root fails
//! that requirement by construction whenever the relocation target is not itself nested inside
//! the project repo - which is precisely the deployment shape the relocation knob exists to
//! reach (a scratch root OUTSIDE a small-root-partitioned repo). `Worktree::remove`'s own doc
//! comment still promises "Reap any process still rooted inside this worktree BEFORE git
//! removes the dir (spec 23): otherwise a build or tool an agent left running holds a
//! now-deleted cwd and outlives its worktree, leaking memory" - unconditionally, with no
//! carve-out for a relocated root. This file proves that promise is now broken for exactly
//! that deployment shape: a `cargo`/`rustc`-shaped process (SIGTERM-ignoring, so only the
//! SIGKILL escalation would end it) left running in a worktree under a relocated scratch root
//! now outlives `Worktree::remove()` deleting that worktree's dir out from under it.
//!
//! EXPECTED RED at u78c2: this is the boundary bug itself (the same class already flagged by
//! the implementer for the mutation-scratch caller, `u78c2-mutation-scratch-reap-now-refused-
//! flagging-for-review`, extended here to the CORE worktree-teardown path spec 78's own Design
//! text names as a caller that "keeps its signature": `src/worktree.rs:551`), not a test
//! defect - the DEFAULT (unrelocated) shape is independently proven still correct by the
//! pre-existing sibling unit test named above, which stays green.

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
fn worktree_remove_no_longer_reaps_a_process_when_its_dir_lives_under_a_relocated_scratch_root() {
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
         must still be SIGKILLed, exactly as it was before this diff and exactly as the \
         DEFAULT (unrelocated) shape still is (see \
         worktree::tests::remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside, \
         independently re-run and confirmed green against this same diff). It survived \
         instead: is_reapable_base's new <repo>/.rigger/tmp requirement refuses this dir, \
         because a scratch root relocated via defaults.workdir/RIGGER_TMPDIR to a location \
         outside the project repo (a real, tested configuration surface - see \
         tests/scratch_workdir_config.rs) can never canonicalize under that repo's own \
         .rigger/tmp - so reap_processes_rooted_under silently no-ops here now, exactly the \
         same regression class already flagged for the mutation-scratch caller in decision \
         u78c2-mutation-scratch-reap-now-refused-flagging-for-review, extended here to the \
         CORE worktree-teardown path."
    );
}
