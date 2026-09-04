//! Periphery (cross-module, real-binary) test for spec 79 criterion 1: the two removal sites
//! the spec-78 run's adversary proved leaking a LIVE process (Goal: "the spec-78 run's
//! adversary proved the class live rather than by inspection") - `reclaim_cache_sibling`'s
//! per-unit build-cache dir, reached via [`Worktree::remove`] (the DOMINANT graceful teardown,
//! spec 79's own "even the exemplar leaks here" - it already reaps the worktree dir itself via
//! git identity, but the SIBLING cache dirs it reclaims afterward were bare `remove_dir_all`
//! calls with no reap of their own), and `Worktree::discard`'s review-fence sibling (spec 79
//! Goal: "`Worktree::discard` leaks a review-fence sibling process; live-confirmed during the
//! spec-78 run").
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/worktree.rs`'s own `remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_
//! outside` test (spec 23) proves the WORKTREE dir itself is reaped by `remove()`, and
//! `discard_also_reclaims_the_review_worktrees_store_fence_sibling` (spec 70) proves the
//! fence sibling is REMOVED - but neither ever plants a LIVE process rooted in the sibling
//! dir (the cache dir `remove()` reclaims, or the fence dir `discard()` reclaims), so neither
//! is structurally able to see the exact defect this spec closes: a process rooted in the
//! SIBLING survives the sibling's own bare `remove_dir_all`, outliving the removed dir with a
//! now-deleted cwd. A file-survival or dir-gone assertion passes identically whether the
//! sibling was reaped first or not; only a LIVE PROCESS inside it tells the two cases apart -
//! exactly the same structural blind spot `worktree_remove_relocated_scratch_base_guard_
//! periphery.rs` and `spawn_scratch_reap_authorized_root_periphery.rs` already close for their
//! own call chains.

use std::path::Path;
use std::process::{Child, Command};

use rigger::reap::processes_rooted_under;
use rigger::worktree::{review_fence_sibling, unit_cache_sibling, Worktree};

/// Spawn a long-lived process rooted at `dir` that IGNORES SIGTERM, so only a SIGKILL
/// escalation can end it - exercising the full SIGTERM-then-SIGKILL mechanism
/// `reap_processes_rooted_under`/`reap_authorized` runs. Mirrors the identical fixture in
/// `src/reap.rs`, `src/worktree.rs`'s own test module, and the sibling periphery tests.
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
fn worktree_remove_reaps_a_process_rooted_in_its_sibling_build_cache_before_reclaiming_it() {
    // `Worktree::remove` is spec 79's named EXEMPLAR ("even the exemplar leaks here"): it
    // already reaps the worktree dir itself (spec 23), then calls `reclaim_cache_sibling`
    // to remove the per-unit `cargo-target-<slug>` cache dir a gate's build populated - a
    // BARE `remove_dir_all` with no reap of its own before this fix. A build a gate spawned
    // with its `CARGO_TARGET_DIR` pointed at the cache (the everyday case, Gap 19) can still
    // be alive when the unit's worktree is torn down; it must be reaped before the cache dir
    // that holds its cwd is removed.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let wt_dir = scratch_path.join("rigger-wt-cachereaptest");

    let wt = Worktree::create(
        repo_path.to_str().unwrap(),
        wt_dir.to_str().unwrap(),
        "rigger/u/cachereaptest",
    )
    .expect("create the unit worktree");

    let cache_dir = unit_cache_sibling(wt_dir.to_str().unwrap())
        .expect("a rigger-wt-* worktree dir has a cache sibling");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let cache_path = Path::new(&cache_dir).to_path_buf();

    let mut child = sigterm_ignorer_in(&cache_path);
    assert!(
        wait_until(|| processes_rooted_under(&cache_path)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted in the cache dir before remove() runs"
    );

    wt.remove().expect("remove() itself must still succeed");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "a process rooted in the per-unit build-cache dir that Worktree::remove reclaims as \
         reclaim_cache_sibling must be reaped (SIGTERM then SIGKILL) BEFORE that dir is \
         removed - a bare remove_dir_all here leaks it as an orphan holding a deleted cwd \
         (spec 79 Goal: 'even the exemplar leaks here')"
    );
    assert!(
        !cache_path.exists(),
        "the cache dir is still reclaimed once its rooted process is reaped"
    );
}

#[test]
fn discard_reaps_a_process_rooted_in_the_review_worktrees_fence_sibling_before_reclaiming_it() {
    // spec 79 Goal: "`Worktree::discard` leaks a review-fence sibling process; live-confirmed
    // during the spec-78 run". A standalone review stage's fenced gate run leaves a real
    // sqlite store open in `<dir>-store-fence` (spec 70); a courier process rooted there that
    // is still alive when a resumed review step discards the stale scaffolding must be reaped
    // before that fence dir is removed out from under it.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let branch = "rigger/review/discard-reap-0";
    let dir = scratch_path.join("rigger-review-discard-reap-0");

    let stale = Worktree::create(repo_path.to_str().unwrap(), dir.to_str().unwrap(), branch)
        .expect("create the throwaway review worktree");
    drop(stale); // the Rust struct is gone but the worktree registration + dir survive.

    let fence_dir =
        review_fence_sibling(dir.to_str().unwrap()).expect("a review worktree has a fence sibling");
    std::fs::create_dir_all(&fence_dir).unwrap();
    let fence_path = Path::new(&fence_dir).to_path_buf();

    let mut child = sigterm_ignorer_in(&fence_path);
    assert!(
        wait_until(|| processes_rooted_under(&fence_path)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted in the fence dir before discard() runs"
    );

    Worktree::discard(repo_path.to_str().unwrap(), dir.to_str().unwrap(), branch)
        .expect("discard() itself must still succeed");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "a process rooted in the review worktree's store-fence sibling that Worktree::discard \
         reclaims must be reaped (SIGTERM then SIGKILL) BEFORE that dir is removed - a bare \
         remove_dir_all here leaks it as an orphan (spec 79 Goal, live-confirmed spec-78 run)"
    );
    assert!(
        !fence_path.exists(),
        "the fence dir is still reclaimed once its rooted process is reaped"
    );
}
