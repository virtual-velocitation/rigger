//! Periphery (real-git, crate-boundary) tests for spec 103 criterion 4 (HEAL NEVER TOUCHES
//! A LIVE ADD): `Worktree::create`'s heal predicate (`worktree_admin_is_corrupt` sparing a
//! `locked` or too-young admin entry) and its per-repository admin-directory lock
//! (`repo_admin_lock` - renamed and widened from `repo_create_lock` by the round-4 checkin
//! fix this file's own soak test below drove; see that test's doc comment) serialization.
//!
//! WHY THIS FILE EXISTS. Criterion 4 landed at `9e578a8` (unit u103c4) with ONLY the
//! implementer's own `mod tests` inside `src/worktree.rs` -
//! `heal_never_prunes_a_locked_admin_entry`, `heal_never_prunes_an_admin_entry_younger_than_
//! the_grace_period`, `concurrent_worktree_creates_in_one_repository_all_succeed_across_50_
//! rounds`. That unit's own SDET pass reached a provably-empty periphery accounting
//! (decision `sdet-u103c4-periphery-accounting-empty`), reasoned through - and adjudicator-
//! approved (`adj-u103c4-verdict-approve`) - on the grounds that `repo_create_lock` is a new
//! PRIVATE fn and `conductor.rs::run_batch`, "the only cross-module caller of
//! `Worktree::create`", was untouched by that unit's own diff.
//!
//! That premise no longer holds once the WHOLE SPEC is assembled (the checkin seam this file
//! is authored at): criterion 7 (unit u103c7, POST-MERGE GATES RUN ON THE LANDED TREE) adds a
//! SECOND, genuinely new cross-module caller of `Worktree::create` -
//! `RunCtx::integrate_and_emit`'s throwaway `rigger-postmerge-<unit>-<attempt>` scratch
//! worktree (`src/conductor.rs`) - against the SAME shared repo `run_batch` creates unit
//! worktrees in, during the SAME wave. That call site is preceded by `Worktree::discard`,
//! whose `clear_worktree_dir` path runs `git worktree remove --force` and `git worktree
//! prune` UNGUARDED by `repo_create_lock` - exactly the gap the architecture lens raised
//! against u103c4 (`arch-u103c4-remove-not-under-repo-lock`, "a second, unguarded writer to
//! the resource this criterion names as the one it is making single-authority") and the
//! adjudicator discarded as out of THAT unit's scope ("nothing else in this spec touches
//! `src/worktree.rs`'s heal", recommending a follow-up criterion). Criterion 7 makes it
//! reachable in-spec, not merely theoretical - which is exactly the kind of composition a
//! single unit's own diff-scoped accounting is structurally unable to see, and the whole-spec
//! checkin sweep exists to catch.
//!
//! Building on (not silently diverging from) `u103c4-heal-predicate-and-per-repo-lock-
//! design`, `sdet-u103c4-periphery-accounting-empty` and `adj-u103c4-verdict-approve`: those
//! decisions are correct for u103c4's OWN diff at the time; this file supplies the periphery
//! layer their own scope-limited premise said was unnecessary, now that a second caller
//! exists, and extends coverage to the new discard-then-create composition their premise
//! never had to consider.
//!
//! WHAT THIS FILE OWNS: the heal predicate's 3-way decision table and the admin-directory
//! lock (`repo_admin_lock`), driven through `rigger::worktree::Worktree`'s PUBLIC API only
//! (never `worktree_admin_is_corrupt`/`repo_admin_lock`/`heal_corrupt_worktree_admin`
//! directly, all crate-private) - proving the fix survives the real crate boundary, the thing the
//! implementer's own `mod tests` cannot attest to since they can see (and could accidentally
//! depend on) `worktree.rs`'s private internals; and the NEW discard-then-create-vs-plain-
//! create composition criterion 7 introduces. NOT OWNED: the predicate's own unit-level
//! truth table in isolation (the implementer's `mod tests` already pins it precisely,
//! including the backdating mechanics) and `integrate_and_emit`'s own post-merge wiring
//! (owned by `tests/postmerge_gate_modified_file_periphery.rs` /
//! `tests/postmerge_gate_error_cleanup_periphery.rs`, spec 103 criterion 7).
//!
//! Like the implementer's own concurrency test, `create_serializes_concurrent_sibling_
//! creates_at_the_crate_boundary` and `discard_then_create_never_corrupts_a_concurrent_
//! siblings_admin_entry` below are soak/regression-lock proofs over a real, timing-dependent
//! git race - not a guaranteed reproduction of any specific defect on every run. The two
//! predicate tests are the deterministic proof of the heal decision table itself.

mod common;
use common::git::temp_git_project_with_commit;

use rigger::worktree::Worktree;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// The worktree admin entry `Worktree::create` registers for a worktree at `dir`: git names
/// it after `dir`'s own leaf path component, under the repo's `.git/worktrees/`.
fn admin_dir(repo: &str, dir: &str) -> std::path::PathBuf {
    let name = Path::new(dir).file_name().unwrap();
    Path::new(repo).join(".git").join("worktrees").join(name)
}

/// Backdate `path`'s mtime by `secs_ago` seconds, so a heal grace-period check reads it as
/// old without the test actually sleeping. Mirrors `src/worktree.rs`'s own `mod tests`
/// helper of the same shape, reimplemented here since it is crate-private.
fn backdate(path: &Path, secs_ago: u64) {
    let target = SystemTime::now() - Duration::from_secs(secs_ago);
    std::fs::File::open(path)
        .unwrap()
        .set_modified(target)
        .unwrap();
}

#[test]
fn create_at_the_crate_boundary_spares_a_locked_or_too_young_admin_entry_and_heals_an_old_unlocked_one(
) {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path();

    // Three real admin entries, each created through the public API exactly like a real
    // unit worktree, while still healthy - EVERY `Worktree::create` call heal-scans the
    // WHOLE admin directory as its own first step, so doctoring one entry before the next
    // sibling is created would let that sibling's own create prematurely heal it. Create
    // all three first, THEN doctor all three, so the trigger call below heal-scans all
    // three simultaneously - exactly the multi-entry shape production hits.
    let old_dir = root.join("wt-old-unlocked-corrupt");
    let old_dir = old_dir.to_str().unwrap();
    Worktree::create(&repo_path, old_dir, "rigger/u/old-unlocked", "").unwrap();

    let locked_dir = root.join("wt-locked-corrupt");
    let locked_dir = locked_dir.to_str().unwrap();
    Worktree::create(&repo_path, locked_dir, "rigger/u/locked", "").unwrap();

    let young_dir = root.join("wt-young-corrupt");
    let young_dir = young_dir.to_str().unwrap();
    Worktree::create(&repo_path, young_dir, "rigger/u/young", "").unwrap();

    // Now doctor all three into the corrupt-and-abandoned shape a killed `git worktree
    // remove` leaves (its `commondir` marker missing) - the precondition every scenario
    // shares - before any further `create` call can heal-scan them.
    let old_admin = admin_dir(&repo_path, old_dir);
    std::fs::remove_file(old_admin.join("commondir")).unwrap();
    backdate(&old_admin, 120);

    let locked_admin = admin_dir(&repo_path, locked_dir);
    std::fs::remove_file(locked_admin.join("commondir")).unwrap();
    std::fs::write(locked_admin.join("locked"), b"").unwrap();
    backdate(&locked_admin, 120);

    let young_admin = admin_dir(&repo_path, young_dir);
    std::fs::remove_file(young_admin.join("commondir")).unwrap();
    // Left exactly as `create` just touched it - no backdate - well inside the grace period.

    assert!(
        old_admin.is_dir(),
        "precondition: old-unlocked entry exists"
    );
    assert!(locked_admin.is_dir(), "precondition: locked entry exists");
    assert!(young_admin.is_dir(), "precondition: young entry exists");

    // A trigger `create` for a brand-new worktree runs the SAME heal scan production does
    // before every `git worktree add` - through the public API only, never a direct call
    // to the crate-private `heal_corrupt_worktree_admin`.
    let trigger_dir = root.join("wt-trigger");
    let trigger_dir = trigger_dir.to_str().unwrap();
    let trigger = Worktree::create(&repo_path, trigger_dir, "rigger/u/trigger", "");
    assert!(
        trigger.is_ok(),
        "the triggering create must itself succeed: {:?}",
        trigger.err()
    );

    assert!(
        !old_admin.is_dir(),
        "an old, unlocked, provably-corrupt admin entry must be healed"
    );
    assert!(
        locked_admin.is_dir(),
        "a locked admin entry must survive the heal even though its commondir marker is \
         missing and it is well past the grace period"
    );
    assert!(
        young_admin.is_dir(),
        "an admin entry younger than the grace period must survive the heal even though its \
         commondir marker is missing"
    );
}

#[test]
fn create_serializes_concurrent_sibling_creates_at_the_crate_boundary() {
    // Mirrors `src/worktree.rs`'s own `concurrent_worktree_creates_in_one_repository_all_
    // succeed_across_50_rounds` (the original race `repo_admin_lock`, formerly named
    // `repo_create_lock`, exists to close), but
    // driven entirely through the compiled crate's public `Worktree::create`, from outside
    // the crate - proving the lock's contract holds at the real crate boundary, not merely
    // inside the module that defines it.
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().to_str().unwrap().to_string();

    for round in 0..30 {
        let dir_a = format!("{root}/wt-race-a-{round}");
        let dir_b = format!("{root}/wt-race-b-{round}");
        let branch_a = format!("rigger/u/race-a-{round}");
        let branch_b = format!("rigger/u/race-b-{round}");

        let (ra, rb) = std::thread::scope(|s| {
            let ha = s.spawn(|| Worktree::create(&repo_path, &dir_a, &branch_a, ""));
            let hb = s.spawn(|| Worktree::create(&repo_path, &dir_b, &branch_b, ""));
            (ha.join().unwrap(), hb.join().unwrap())
        });

        assert!(
            ra.is_ok(),
            "round {round}: thread A's create must never lose the admin-directory race \
             at the crate boundary: {:?}",
            ra.err()
        );
        assert!(
            rb.is_ok(),
            "round {round}: thread B's create must never lose the admin-directory race \
             at the crate boundary: {:?}",
            rb.err()
        );
    }
}

#[test]
fn discard_then_create_never_corrupts_a_concurrent_siblings_admin_entry() {
    // THE composition unreachable when u103c4 was scoped: `RunCtx::integrate_and_emit`
    // (spec 103 criterion 7) calls `Worktree::discard` - a `git worktree remove`/`git
    // worktree prune` against the shared repo's admin directory - immediately before
    // `Worktree::create_branch_at` + `Worktree::create` for its throwaway post-merge scratch
    // worktree, and this can run concurrently with a sibling unit's own plain `Worktree::
    // create` in the same `run_batch` wave. BEFORE the round-4 checkin fix, `repo_create_lock`
    // (since renamed to `repo_admin_lock`) only ever serialized `create` calls against each
    // other, leaving `discard`'s admin-directory writes unguarded - this exact test caught
    // that gap empirically (a ~3.6% panic rate reproduced in 55 isolated runs, `fatal: could
    // not create directory of .git/worktrees/...`) and drove the fix that widened the lock to
    // cover every in-process admin-directory mutator (create, discard, remove, sweep,
    // reclaim). This test now REGRESSION-LOCKS the fixed behavior: discard-then-create on one
    // thread, a plain sibling create on another, both against one shared repo, across many
    // rounds - the composition must never corrupt a concurrent sibling's admin entry.
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().to_str().unwrap().to_string();
    const ROUNDS: u32 = 20;

    let postmerge_result: std::thread::Result<Result<(), String>> = std::thread::scope(|s| {
        let repo_path_pm = repo_path.clone();
        let root_pm = root.clone();
        let pm = s.spawn(move || -> Result<(), String> {
            // Mirrors `RunCtx::integrate_and_emit`'s post-merge scratch-worktree sequence
            // (conductor.rs): discard whatever a prior attempt left, mint a fresh branch,
            // create the worktree, then reap both - deterministic dir/branch reused every
            // round, exactly like production's `postmerge_worktree_dir`/`postmerge_branch`.
            let pm_dir = format!("{root_pm}/rigger-postmerge-sim-0");
            let pm_branch = "rigger/postmerge/sim-0".to_string();
            for round in 0..ROUNDS {
                Worktree::discard(&repo_path_pm, &pm_dir, &pm_branch, "")
                    .map_err(|e| format!("postmerge round {round} discard: {e}"))?;
                Worktree::create_branch_at(&repo_path_pm, &pm_branch, "HEAD")
                    .map_err(|e| format!("postmerge round {round} create_branch_at: {e}"))?;
                let wt = Worktree::create(&repo_path_pm, &pm_dir, &pm_branch, "")
                    .map_err(|e| format!("postmerge round {round} create: {e}"))?;
                let _ = wt.remove();
                let _ = Worktree::delete_branch(&repo_path_pm, &pm_branch);
            }
            Ok(())
        });

        let sibling = s.spawn(|| -> Result<(), String> {
            for round in 0..ROUNDS {
                let dir = format!("{root}/wt-sibling-{round}");
                let branch = format!("rigger/u/sibling-{round}");
                let wt = Worktree::create(&repo_path, &dir, &branch, "")
                    .map_err(|e| format!("sibling round {round} create: {e}"))?;
                let _ = wt.remove();
                let _ = Worktree::delete_branch(&repo_path, &branch);
            }
            Ok(())
        });

        let pm_res = pm.join();
        let sibling_res = sibling.join().unwrap();
        assert!(
            sibling_res.is_ok(),
            "a concurrent sibling create must never be corrupted by the post-merge \
             discard-then-create sequence: {sibling_res:?}"
        );
        pm_res
    });

    match postmerge_result {
        Ok(inner) => assert!(
            inner.is_ok(),
            "the post-merge discard-then-create sequence must never fail on the admin- \
             directory race it introduces: {inner:?}"
        ),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
