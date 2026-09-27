//! Periphery (real public API, real git, cross-module) test for spec 103 criterion 6's
//! ROUND 4 fix (diff base `db13597`, commit `0d8d43b`,
//! `adv-u103c6-r4-speculation-winner-sha-not-round-start`): `emit_speculation_winner_status`
//! (`src/conductor.rs`) stamps the speculation winner's DEFERRED `reviewed#{lane}` event
//! with `review.round_start_sha` - `review_unit`'s own log-derived round-start sha, carried
//! on `ReviewOutcome` - in place of a live `worktree::head_sha_of(dir)` read taken AFTER the
//! caller's exhaustive gate run (and, on the merge path, after `integrate_and_emit`).
//!
//! WHAT THE IMPLEMENTER'S OWN NEW TEST ALREADY COVERS (not re-proven here):
//! `src/conductor.rs`'s
//! `speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_integrate`
//! drives this fix end to end for the `on_pass: merge` call site of
//! `emit_speculation_winner_status` (`run_speculation` phase B, the winner-integrates arm) -
//! after `integrate_and_emit` genuinely runs.
//!
//! WHAT THAT LEAVES UNTESTED. `emit_speculation_winner_status` has TWO distinct call sites in
//! `run_speculation`'s phase B loop: the `on_pass: merge` arm the implementer's test drives
//! (after `integrate_and_emit`), and the EARLIER `!integrates(st)` (`on_pass: none`) arm -
//! "verified + approved but never merges" - which calls it DIRECTLY after the exhaustive gate
//! run, with NO `integrate_and_emit` call in between at all. Both arms pass the SAME `review`
//! outcome into the SAME function, so the fix's own code is shared - but no test anywhere
//! (not the implementer's, not any periphery file) ever drives the `on_pass: none` arm with a
//! post-review regen commit in play, so a future change that special-cases the merge arm (or
//! an early emit on the `on_pass: none` arm that runs before `review.round_start_sha` is
//! populated) would show up first here. This drives that exact arm - a `speculation_width: 2`
//! unit, `on_pass: none`, an adjudicator-only panel that approves, and a `door` gate scoped to
//! never match the (always-empty, no grounder) blast radius so it fires for the first time at
//! the EXHAUSTIVE pass, strictly after `review_unit` already captured `round_start_sha` and
//! returned - through the real `rigger::conductor::run` entry point, a real git repo, and a
//! real `Worktree`, never a conductor-internal stub.
//!
//! Verified RED then GREEN by hand: reverting `emit_speculation_winner_status`'s
//! `(META_WORKTREE_SHA, &review.round_start_sha)` line back to the pre-round-4
//! `(META_WORKTREE_SHA, &winner_sha)` live read makes
//! `a_speculation_winner_with_no_merge_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit`
//! fail - the `reviewed#{lane}` event's sha drifts to the door gate's own post-review regen
//! commit instead of naming the sha the round actually judged. Restoring the field verbatim
//! (`git diff` on `src/` clean) returns it to green.

mod common;

use common::fixtures::{
    assert_winner_reviewed_sha_is_round_start, speculation_regen_door_cfg, ApproveEveryLaneDriver,
};
use common::git::temp_git_project_with_commit;
use rigger::conductor::{run, Deps, STREAM};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::ExecRunner;
use rigger::ledger;

/// Drives the round-4 fix through the ONE `emit_speculation_winner_status` call site the
/// implementer's own new test never reaches: the `on_pass: none` arm, which calls it directly
/// after the exhaustive gate run with no `integrate_and_emit` in between at all. Proves the
/// winner's deferred `reviewed#{lane}` stamp still carries `round_start_sha` - the sha
/// `review_unit` actually judged - rather than the door gate's own post-review regen commit,
/// even with no merge ever happening on this arm.
#[test]
fn a_speculation_winner_with_no_merge_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit(
) {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let cfg = speculation_regen_door_cfg("none");
    let store = Store::open(":memory:").unwrap();
    let driver = ApproveEveryLaneDriver;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let result = run(&cfg, &deps);
    assert!(
        result.is_ok(),
        "a verified-but-unmerged (`on_pass: none`) speculation winner with a post-review \
         regen commit must complete the run cleanly: {:?}",
        result.err()
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();

    assert_winner_reviewed_sha_is_round_start(&events);

    assert!(
        !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
        "a verified-but-unmerged speculation winner must charge no remediation attempt"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
        "premise: the `on_pass: none` arm must never integrate, or this test is not actually \
         driving the call site it claims to"
    );

    drop(repo);
}
