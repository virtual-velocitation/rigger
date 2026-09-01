//! Periphery (cross-module contract) test for the spec-78 base-guard's interaction with
//! `src/driver/replay.rs::reclaim_unit_mutation_scratch` (spec 77 criterion 3, UNIT-TERMINAL
//! REAP).
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/reap.rs`'s own unit tests exercise `is_reapable_base` and `reap_processes_rooted_under`
//! entirely through a `FakeRepo` fixture it constructs itself - a git-inited root with an empty
//! `.rigger/tmp` created for exactly this purpose. `src/driver/replay.rs`'s own unit tests
//! exercise `reclaim_unit_mutation_scratch` with a bare `tempfile::tempdir()` `cache_home` and
//! never a LIVE PROCESS (only files, via `std::fs::write`) - the reap half of the call is never
//! actually exercised there either. Neither side's unit tests ever drive the two together with
//! a real subprocess, so neither can see what this diff (spec 78, THE REAPER) does to an
//! ALREADY-SHIPPED caller: `reclaim_unit_mutation_scratch` reaps a dir under
//! `<cache_home>/rigger-mutants/<spawn>` (spec 77 criteria 2/3's registered mutation-scratch
//! root), and `cache_home` is `$XDG_CACHE_HOME` or `$HOME/.cache` - NEVER nested inside any
//! project's `<repo>/.rigger/tmp`. `is_reapable_base` (this diff) refuses any base that does
//! not canonicalize to somewhere strictly under `<repo>/.rigger/tmp`, so
//! `reap_processes_rooted_under` inside `reclaim_unit_mutation_scratch` is now an
//! unconditional, silently-logged no-op for this caller - even though
//! `reclaim_unit_mutation_scratch`'s OWN doc comment still promises
//! "[`crate::reap::reap_processes_rooted_under`] reaps any process a hung `cargo mutants` run
//! left rooted inside a matched dir BEFORE it is removed". This file drives the real,
//! documented public contract with a real subprocess and proves it: a `cargo-mutants`-shaped
//! process that IGNORES SIGTERM (so only the SIGKILL escalation would end it) now survives its
//! own registered scratch dir being deleted out from under it.
//!
//! Already flagged by the implementer for arch/adjudicator disposition
//! (`u78c2-mutation-scratch-reap-now-refused-flagging-for-review`); this file turns that prose
//! flag into a concrete, machine-verified boundary proof, per this loop's sdet charter ("a
//! failing periphery test reveals a boundary BUG ... it drives remediation of the CODE, never
//! a weakening of the test"). EXPECTED RED at u78c2: the first test below is the boundary bug
//! itself, not a test defect - the second test is a positive control that proves the SAME
//! reclaim call succeeds once its base legitimately lies under `<repo>/.rigger/tmp`, isolating
//! the failure to the base-guard's new scope rather than to this file's own
//! spawn/scan/signal mechanics.

use std::path::Path;
use std::process::{Child, Command};

use rigger::driver::replay::{mutation_scratch_path, reclaim_unit_mutation_scratch};
use rigger::reap::processes_rooted_under;

/// Spawn a long-lived process rooted at `dir` that IGNORES SIGTERM, so only a SIGKILL
/// escalation can end it - exercising the full SIGTERM-then-SIGKILL mechanism
/// `reap_processes_rooted_under` runs, not just a plain `sleep` a bare SIGTERM would already
/// end. Mirrors the identical fixture in `src/reap.rs` and `src/worktree.rs`.
fn sigterm_ignorer_in(dir: &Path) -> Child {
    Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; while :; do sleep 1; done")
        .current_dir(dir)
        .spawn()
        .expect("spawn a SIGTERM-ignoring fixture process")
}

/// Poll up to 5s for `pred`, matching the scan/escalation latency tolerance every sibling
/// reap test in this tree already uses (`src/reap.rs`, `src/worktree.rs`).
fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if pred() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

/// Kill-and-wait a fixture child unconditionally, ignoring errors - test cleanup only, via
/// the `Child` handle it was spawned with (never a computed pid).
fn cleanup(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn reclaim_unit_mutation_scratch_reaps_a_live_process_rooted_in_its_registered_scratch_dir() {
    // `cache_home` mirrors production exactly: a directory NOT nested inside any project's
    // `.rigger/tmp` (here, not inside a git repo at all - the same shape
    // `is_reapable_base_refuses_a_dir_outside_any_repos_dot_rigger_tmp` in src/reap.rs already
    // proves is refused; this test proves what that refusal now COSTS at the real caller
    // `reclaim_unit_mutation_scratch`, spec 77 criterion 3's crash-residue backstop).
    let cache_home_dir = tempfile::tempdir().unwrap();
    let cache_home = cache_home_dir.path();
    let unit_id = "u-periphery-mutation-reap";
    let spawn_id = format!("{unit_id}/implementer#0");
    let leaf = mutation_scratch_path(cache_home, &spawn_id)
        .expect("a well-formed spawn id must encode to a real path");
    std::fs::create_dir_all(&leaf).unwrap();

    let mut child = sigterm_ignorer_in(&leaf);
    assert!(
        wait_until(|| processes_rooted_under(&leaf)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the registered \
         mutation-scratch dir before the reclaim runs"
    );

    reclaim_unit_mutation_scratch(cache_home, unit_id);

    // Give the SIGTERM-then-grace-then-SIGKILL sequence its full window before observing.
    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        cleanup(&mut child);
    }
    assert!(
        died,
        "reclaim_unit_mutation_scratch's own doc comment promises every process rooted in a \
         matched registered mutation-scratch dir is reaped BEFORE the dir is removed - a \
         SIGTERM-ignoring process here must still be SIGKILLed, exactly as it was before this \
         diff. It survived instead: is_reapable_base's new <repo>/.rigger/tmp requirement \
         refuses this real, ALWAYS-outside-any-repo registered root (see this file's header \
         doc comment and decision u78c2-mutation-scratch-reap-now-refused-flagging-for-review), \
         so reap_processes_rooted_under silently no-ops here now."
    );
}

#[test]
fn reclaim_unit_mutation_scratch_still_reaps_when_its_base_legitimately_lies_under_repo_dot_rigger_tmp(
) {
    // Positive control: isolates the sibling test's failure to the base-guard's NEW scope,
    // not to any defect in this file's own spawn/scan/signal mechanics or in
    // `reclaim_unit_mutation_scratch`'s directory walk. Same fixture shape, same helper
    // functions, same assertions - the ONLY difference is that `cache_home` here legitimately
    // canonicalizes to somewhere strictly under a real repo's `.rigger/tmp`, so
    // `is_reapable_base` accepts it. (Not a realistic production `cache_home` value - real
    // mutation scratch never nests under a project's own `.rigger/tmp` - purely a mechanical
    // isolation of which half of the call chain is refusing the reap.)
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().canonicalize().unwrap();
    assert!(Command::new("git")
        .arg("-C")
        .arg(&repo_path)
        .args(["init", "-q"])
        .status()
        .unwrap()
        .success());
    let cache_home = repo_path
        .join(".rigger")
        .join("tmp")
        .join("fake-cache-home");
    std::fs::create_dir_all(&cache_home).unwrap();

    let unit_id = "u-periphery-mutation-reap-control";
    let spawn_id = format!("{unit_id}/implementer#0");
    let leaf = mutation_scratch_path(&cache_home, &spawn_id).unwrap();
    std::fs::create_dir_all(&leaf).unwrap();

    let mut child = sigterm_ignorer_in(&leaf);
    assert!(
        wait_until(|| processes_rooted_under(&leaf)
            .iter()
            .any(|(pid, _)| *pid == child.id())),
        "precondition: the fixture process must actually be rooted in the registered \
         mutation-scratch dir before the reclaim runs"
    );

    reclaim_unit_mutation_scratch(&cache_home, unit_id);

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    if !died {
        cleanup(&mut child);
    }
    assert!(
        died,
        "control: when the registered scratch root legitimately lies under a real repo's \
         .rigger/tmp, reclaim_unit_mutation_scratch must still reap a live process rooted in \
         it - proving the sibling test's failure is specifically the base-guard's new scope, \
         not a defect in this file's own mechanics"
    );
}
