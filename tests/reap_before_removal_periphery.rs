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

use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command};

use rigger::gate::STORE_FENCE_SUFFIX;
use rigger::reap::processes_rooted_under;
use rigger::worktree::{
    reclaim_worktree_on_branch, review_fence_sibling, scratch_root, sweep_terminal,
    unit_cache_sibling, unit_mutants_sibling, Worktree, UNIT_WORKTREE_PREFIX,
};

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
        scratch_path.to_str().unwrap(),
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

/// SDET periphery (spec 91, THE GATE ENVIRONMENT): the identical structural gap as
/// `worktree_remove_reaps_a_process_rooted_in_its_sibling_build_cache_before_reclaiming_it`
/// above, for the NEW THIRD sibling `reclaim_cache_sibling` widened to reclaim - the
/// `checkin` stage's `mutation` gate's own per-unit `cargo-mutants-<slug>` root.
///
/// WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO. `src/worktree.rs`'s own
/// `worktree_remove_also_reclaims_the_sibling_mutants_root` test (spec 91) proves the dir is
/// gone after `remove()` - but it never plants a LIVE process inside it first, so it cannot
/// see the exact defect class this file exists to close (see this file's own module doc):
/// a process rooted in the sibling can survive a bare `remove_dir_all`, outliving the
/// removed dir with a now-deleted cwd. This is the realistic shape for THIS sibling
/// specifically: `cargo mutants` forks one `cargo test` (and its own child test binary) per
/// mutant into `$MUTANTS`, and spec 91's own Goal cites a real one that survived its
/// launcher's death for 6.6 hours - if a unit's worktree is torn down (escalation,
/// supersede, crash resume) while a sweep's process tree is still rooted in this exact
/// directory, only a real reap-before-remove closes the same class of orphan the sibling
/// build-cache case already guards.
#[test]
fn worktree_remove_reaps_a_process_rooted_in_its_sibling_mutants_root_before_reclaiming_it() {
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let wt_dir = scratch_path.join("rigger-wt-mutantsreaptest");

    let wt = Worktree::create(
        repo_path.to_str().unwrap(),
        wt_dir.to_str().unwrap(),
        "rigger/u/mutantsreaptest",
        scratch_path.to_str().unwrap(),
    )
    .expect("create the unit worktree");

    let mutants_dir = unit_mutants_sibling(wt_dir.to_str().unwrap())
        .expect("a rigger-wt-* worktree dir has a mutants-root sibling");
    std::fs::create_dir_all(&mutants_dir).unwrap();
    let mutants_path = Path::new(&mutants_dir).to_path_buf();

    let mut child = sigterm_ignorer_in(&mutants_path);
    assert!(
        wait_until(|| processes_rooted_under(&mutants_path)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted in the mutants root before remove() runs"
    );

    wt.remove().expect("remove() itself must still succeed");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "a process rooted in the per-unit cargo-mutants-<slug> root that Worktree::remove \
         reclaims as reclaim_cache_sibling must be reaped (SIGTERM then SIGKILL) BEFORE that \
         dir is removed - a bare remove_dir_all here leaks a mutation-sweep child as an \
         orphan holding a deleted cwd, the same class spec 79 closed for the build cache"
    );
    assert!(
        !mutants_path.exists(),
        "the mutants root is still reclaimed once its rooted process is reaped"
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

    let stale = Worktree::create(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
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

    Worktree::discard(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
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

#[test]
fn worktree_remove_reaps_a_process_rooted_in_the_cache_dirs_own_store_fence_sibling_before_reclaiming_it(
) {
    // `reclaim_cache_sibling` removes THREE distinct dirs, each behind its own
    // `reap_dir_before_removal` call: the cache dir's OWN store-fence sibling
    // (`cargo-target-<slug>-store-fence`, populated by a fenced gate courier's real sqlite
    // store per spec 70), the cache dir itself (covered by the sibling test above), and a
    // review-worktree's fence sibling (covered by the `discard` test above). None of the
    // three call sites shares code with another - a process rooted in one and reaped
    // correctly proves nothing about whether the NEXT removal in the same function also
    // reaps first. This test roots a live process in the cache dir's OWN store-fence sibling
    // specifically, the one site neither existing periphery test touches.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let wt_dir = scratch_path.join("rigger-wt-cachefencereaptest");

    let wt = Worktree::create(
        repo_path.to_str().unwrap(),
        wt_dir.to_str().unwrap(),
        "rigger/u/cachefencereaptest",
        scratch_path.to_str().unwrap(),
    )
    .expect("create the unit worktree");

    let cache_dir = unit_cache_sibling(wt_dir.to_str().unwrap())
        .expect("a rigger-wt-* worktree dir has a cache sibling");
    let fence_dir = format!("{cache_dir}{STORE_FENCE_SUFFIX}");
    std::fs::create_dir_all(&fence_dir).unwrap();
    let fence_path = Path::new(&fence_dir).to_path_buf();

    let mut child = sigterm_ignorer_in(&fence_path);
    assert!(
        wait_until(|| processes_rooted_under(&fence_path)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted in the cache dir's store-fence sibling \
         before remove() runs"
    );

    wt.remove().expect("remove() itself must still succeed");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "a process rooted in the cache dir's OWN store-fence sibling that reclaim_cache_sibling \
         removes must be reaped (SIGTERM then SIGKILL) BEFORE that dir is removed - this is a \
         separate removal call from the cache dir itself, so reaping the cache dir correctly \
         proves nothing about this sibling"
    );
    assert!(
        !fence_path.exists(),
        "the cache dir's store-fence sibling is still reclaimed once its rooted process is reaped"
    );
}

#[test]
fn discard_reaps_a_process_rooted_in_the_review_worktree_itself_before_clearing_it() {
    // `clear_worktree_dir` (spec 79 criterion 1) now reaps `dir` ITSELF before either its
    // `git worktree remove --force` attempt or its bare fallback removal - the shared
    // authority behind `Worktree::create`'s self-heal, `Worktree::discard`, and
    // `reclaim_worktree_on_branch`. The existing `discard` periphery test above proves the
    // review-fence SIBLING is reaped, but plants nothing in the worktree dir itself, so it
    // is structurally blind to whether `clear_worktree_dir`'s OWN reap-then-remove of `dir`
    // actually runs on the real `Worktree::discard` public API path - a process left running
    // with its cwd directly inside a stale review worktree (not a sibling) must be reaped
    // before discard tears the worktree dir down.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let branch = "rigger/review/discard-dir-reap-0";
    let dir = scratch_path.join("rigger-review-discard-dir-reap-0");

    let stale = Worktree::create(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
    .expect("create the throwaway review worktree");
    drop(stale); // the Rust struct is gone but the worktree registration + dir survive.
    let dir_path = dir.clone();

    let mut child = sigterm_ignorer_in(&dir_path);
    assert!(
        wait_until(|| processes_rooted_under(&dir_path)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted directly in the review worktree dir \
         before discard() runs"
    );

    Worktree::discard(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
    .expect("discard() itself must still succeed");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "a process rooted directly in the review worktree dir that Worktree::discard clears \
         via clear_worktree_dir must be reaped (SIGTERM then SIGKILL) BEFORE that dir is \
         removed - a bare git-worktree-remove/remove_dir_all here leaks it as an orphan"
    );
    assert!(
        !dir_path.exists(),
        "the review worktree dir is still cleared once its rooted process is reaped"
    );
}

#[test]
fn sweep_terminal_reaps_a_process_rooted_in_a_terminal_worktree_through_the_real_api() {
    // `sweep_terminal` (spec 79 criterion 1) is the crash-recovery teardown: it now reaps a
    // terminal worktree BEFORE its `git worktree remove --force`, closing the class the
    // spec's Goal names ("a build, test binary, or dash whose cwd is inside the removed dir
    // survives the removal as an orphan"). `src/worktree.rs`'s own inline unit test proves
    // this white-box; this test proves the SAME defect is closed from OUTSIDE the module,
    // driving only `sweep_terminal`'s public signature exactly as `rigger step`'s crash
    // recovery calls it.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);
    assert!(Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["checkout", "-b", "rigger-run"])
        .status()
        .unwrap()
        .success());

    let repo_str = repo_path.to_str().unwrap().to_string();
    let root = scratch_root(&repo_str, "", None);

    let done_dir = format!("{root}/{UNIT_WORKTREE_PREFIX}sweepreaptest");
    Worktree::create(&repo_str, &done_dir, "rigger/u/sweepreaptest", &root)
        .expect("create a worktree whose tip is already an ancestor of rigger-run");
    let done_path = Path::new(&done_dir).to_path_buf();

    let mut child = sigterm_ignorer_in(&done_path);
    assert!(
        wait_until(|| processes_rooted_under(&done_path)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted in the terminal worktree before \
         sweep_terminal() runs"
    );

    let removed = sweep_terminal(
        &repo_str,
        &root,
        "rigger-run",
        &HashSet::new(),
        &HashSet::new(),
        &[],
    )
    .expect("sweep_terminal must still succeed");
    assert_eq!(removed, 1, "the merged terminal worktree is swept");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "sweep_terminal must reap a process rooted inside a terminal worktree (SIGTERM then \
         SIGKILL) BEFORE removing its dir, driven through the real public API"
    );
    assert!(
        !done_path.exists(),
        "the terminal worktree is still removed once its rooted process is reaped"
    );
}

#[test]
fn discard_never_reaps_a_dir_when_the_supplied_authorized_root_does_not_actually_contain_it() {
    // Round-2 fix, spec 79 criterion 1 re-review (adjudication REJECT on diff 0af5281..f8eb209):
    // the round-1 shape derived `reap_dir_before_removal`'s authorized_root as `dir.parent()`,
    // which a canonicalized path ALWAYS `starts_with` after canonicalization - a tautology that
    // could never refuse, for any `dir` with a parent, regardless of whether `dir` was actually
    // placed under the caller's real, independently-resolved scratch root
    // (`arch-u79c1-reap-dir-before-removal-self-authorizes` / `sdet-u79c1-authorized-root-
    // tautology`, both UPHELD). This test is the one no version of that round-1 shape could ever
    // pass: it calls the real `Worktree::discard` public API with an `authorized_root` that does
    // NOT contain `dir` at all (an unrelated tempdir), and proves the live process rooted
    // directly in `dir` SURVIVES - the containment boundary actually refuses an untrusted root,
    // it does not silently substitute `dir`'s own parent for whatever the caller passed.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    // `dir`'s REAL parent - what the round-1 `dir.parent()` shape would have (wrongly) used as
    // the authority, and also the value `Worktree::create` below is given to set the worktree
    // up correctly in the first place.
    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let branch = "rigger/review/wrong-root-refused-0";
    let dir = scratch_path.join("rigger-review-wrong-root-refused-0");

    let stale = Worktree::create(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
    .expect("create the throwaway review worktree");
    drop(stale); // the Rust struct is gone but the worktree registration + dir survive.
    let dir_path = dir.clone();

    let mut child = sigterm_ignorer_in(&dir_path);
    assert!(
        wait_until(|| processes_rooted_under(&dir_path)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted directly in the review worktree dir \
         before discard() runs"
    );

    // An UNRELATED root - not an ancestor of `dir` at all (a sibling tempdir, not
    // `scratch_path`). If `reap_dir_before_removal` still derived its own authority from
    // `dir.parent()` (the round-1 defect), this argument would be ignored entirely and the
    // reap would proceed anyway; it must now be the value that actually gates the reap.
    let unrelated_root = tempfile::tempdir().unwrap();
    let unrelated_root_path = unrelated_root.path().canonicalize().unwrap();
    assert!(
        !dir_path.starts_with(&unrelated_root_path),
        "test setup: the wrong root must not actually contain dir"
    );

    Worktree::discard(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        unrelated_root_path.to_str().unwrap(),
    )
    .expect("discard() itself must still succeed even when the reap it gates is refused");

    // Give the (wrongly-authorized-would-have-been) SIGTERM/SIGKILL sequence every chance to
    // have fired if the containment check were a no-op, then assert the process is UNTOUCHED.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let still_alive = matches!(child.try_wait(), Ok(None));
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        still_alive,
        "a process rooted in `dir` must be LEFT ALONE when the caller-supplied authorized_root \
         does not actually contain `dir` - reaping it anyway would mean the containment check \
         is a rubber stamp (exactly the round-1 defect: dir.parent() always contains dir, so \
         ANY authorized_root value would have been silently ignored in favor of one derived \
         from dir's own filesystem position)"
    );
    assert!(
        !dir_path.exists(),
        "the dir is still removed even though its reap was refused - reap-refusal must never \
         block the removal itself, matching every other reap call site's best-effort contract"
    );
}

#[test]
fn create_reaps_a_process_rooted_in_a_leftover_dir_at_the_deterministic_path_before_healing_it() {
    // Mechanical re-enumeration (spec 79 round-2 diff, `git diff BASE HEAD -- '*.rs' | grep
    // '^\+.*\bpub fn'`) names THREE new/changed public API items: `Worktree::create`,
    // `Worktree::discard`, `reclaim_worktree_on_branch`. `discard` is exhaustively covered
    // above (the fence-sibling, dir-itself, and wrong-root-refusal tests); `create` is not -
    // every existing test in this file calls `Worktree::create` only as SETUP for a
    // brand-new branch (the `else` arm, `git worktree add -b`), never hitting its OWN
    // reap-gated removal.
    //
    // `create`'s doc comment (spec 79 round-2 fix) is explicit: `authorized_root` "gates the
    // self-heal reap below via reap_dir_before_removal" in the "DEFEND THE DETERMINISTIC DIR"
    // branch - a populated, UNREGISTERED leftover at the deterministic path (the residue a
    // SIGKILL mid `git worktree add` leaves) is cleared via `clear_worktree_dir` before the
    // branch is checked out afresh. `src/worktree.rs`'s own `create_heals_a_leftover_dir_at_
    // the_deterministic_path` unit test drives this exact branch but passes an EMPTY
    // `authorized_root`, which `reap_dir_before_removal`'s own doc comment defines as a no-op
    // ("nothing to authorize") - so that test proves the FILE healing, structurally blind to
    // whether a LIVE process in the leftover would actually be reaped. No test, unit or
    // periphery, ever plants one there. This test does.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let branch = "rigger/u/create-heal-reap-0";
    let dir = scratch_path.join(format!("{UNIT_WORKTREE_PREFIX}create-heal-reap-0"));

    // Establish the durable branch checkpoint, then remove the worktree dir - the branch
    // survives as the resume checkpoint, exactly `create`'s self-heal branch's precondition.
    let wt1 = Worktree::create(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
    .expect("create the initial worktree");
    std::fs::write(dir.join("carried.txt"), "checkpoint\n").unwrap();
    wt1.commit("rigger: prior window work").unwrap();
    wt1.remove()
        .expect("remove the worktree, leaving the branch as the durable checkpoint");
    assert!(
        !dir.exists(),
        "precondition: the deterministic dir is gone after remove"
    );

    // Plant a POPULATED, UNREGISTERED leftover at the exact deterministic path with a LIVE
    // process rooted inside it.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("leftover.txt"), "torn\n").unwrap();
    let mut child = sigterm_ignorer_in(&dir);
    assert!(
        wait_until(|| processes_rooted_under(&dir)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted in the leftover dir before create() runs"
    );

    let wt2 = Worktree::create(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
    .expect("create must self-heal the leftover dir, not wedge on git worktree add");

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(
        died,
        "Worktree::create's self-heal branch (clear_worktree_dir on a leftover, unregistered \
         dir at the deterministic path) must reap a process rooted inside it (SIGTERM then \
         SIGKILL) BEFORE healing the dir - a bare remove_dir_all here leaks it as an orphan \
         holding a now-deleted cwd, exactly the class spec 79 closes for every other removal \
         site"
    );
    assert!(
        dir.join("carried.txt").exists(),
        "the healed worktree still checks out the branch's committed checkpoint"
    );
    assert!(
        !dir.join("leftover.txt").exists(),
        "the unregistered leftover residue is still removed, not merged into the fresh checkout"
    );
    wt2.remove().unwrap();
}

#[test]
fn reclaim_worktree_on_branch_never_reaps_a_process_when_the_supplied_authorized_root_does_not_actually_contain_it(
) {
    // `reclaim_worktree_on_branch` is the third new/changed public API item the mechanical
    // probe names, and the ONE call site the round-2 diff added a wholly NEW authority
    // resolution for: `RunCtx::gc_integrated_branches` (src/conductor.rs) previously called
    // this function with no authorized root at all (a bare `(repo, branch)` signature); round
    // 2 added a fresh `scratch_root_from_env` call INSIDE that function purely to feed this
    // new parameter (decision `u79c1r2-reap-dir-before-removal-authorized-root-param`). Every
    // other conductor.rs call site the diff touches (unit-worktree creation, the resume
    // review-worktree discard+create) only threads an ALREADY-computed `scratch` variable it
    // was already using to build `dir` in that same function - mechanical wiring with no new
    // derivation to independently break, and already exercised by every `Worktree::create`/
    // `discard` test in this file (each constructs `dir` directly under the `authorized_root`
    // it passes).
    //
    // `discard_never_reaps_a_dir_when_the_supplied_authorized_root_does_not_actually_contain_
    // it` above proves the SHARED `reap_dir_before_removal` containment check refuses a wrong
    // root via `discard`'s call path; `reclaim_worktree_on_branch` forwards its own
    // `authorized_root` straight through to the identical chain
    // (`clear_worktree_dir`/`reclaim_cache_sibling`/`reap_dir_before_removal`) with no
    // transformation - but this is the one public API path no existing test, unit or
    // periphery, ever drove with a MISMATCHED root; `src/worktree.rs`'s own inline tests for
    // this function only ever pass `""` (no-op) or the dir's own correct parent.
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    init_repo(&repo_path);

    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let branch = "rigger/u/reclaim-wrong-root-0";
    let dir = scratch_path.join(format!("{UNIT_WORKTREE_PREFIX}reclaim-wrong-root-0"));

    let wt = Worktree::create(
        repo_path.to_str().unwrap(),
        dir.to_str().unwrap(),
        branch,
        scratch_path.to_str().unwrap(),
    )
    .expect("create a lingering worktree for the branch");
    std::fs::write(dir.join("work.rs"), "fn work() {}\n").unwrap();
    wt.commit("rigger: prior window work").unwrap();
    // Drop the handle without calling remove()/discard() - the git registration and dir
    // survive, exactly the "step process killed before Worktree::remove ran" precondition
    // gc_integrated_branches's resume-path reclaim exists to clean up.
    drop(wt);

    let mut child = sigterm_ignorer_in(&dir);
    assert!(
        wait_until(|| processes_rooted_under(&dir)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process is rooted in the lingering worktree before \
         reclaim_worktree_on_branch() runs"
    );

    // An UNRELATED root - not an ancestor of `dir` at all.
    let unrelated_root = tempfile::tempdir().unwrap();
    let unrelated_root_path = unrelated_root.path().canonicalize().unwrap();
    assert!(
        !dir.starts_with(&unrelated_root_path),
        "test setup: the wrong root must not actually contain dir"
    );

    reclaim_worktree_on_branch(
        repo_path.to_str().unwrap(),
        branch,
        unrelated_root_path.to_str().unwrap(),
    )
    .expect(
        "reclaim_worktree_on_branch itself must still succeed even when the reap it gates is refused",
    );

    // Give the (wrongly-authorized-would-have-been) SIGTERM/SIGKILL sequence every chance to
    // have fired if the containment check were a no-op, then assert the process is UNTOUCHED.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let still_alive = matches!(child.try_wait(), Ok(None));
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        still_alive,
        "a process rooted in the lingering worktree must be LEFT ALONE when the caller- \
         supplied authorized_root does not actually contain it - reclaim_worktree_on_branch \
         forwards authorized_root straight through to the same reap_dir_before_removal \
         containment check discard() uses, and this is the one call path no existing test \
         drove with a mismatched root"
    );
    assert!(
        !dir.exists(),
        "the lingering worktree is still torn down even though its reap was refused - reap- \
         refusal must never block the removal itself, matching every other reap call site's \
         best-effort contract"
    );
}
