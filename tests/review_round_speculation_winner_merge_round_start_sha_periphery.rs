//! Periphery (real public API, real git, cross-module) test for spec 103 criterion 6's
//! ROUND 4 fix (diff base `db13597`, commit `0d8d43b`,
//! `adv-u103c6-r4-speculation-winner-sha-not-round-start`): `emit_speculation_winner_status`
//! (`src/conductor.rs`) stamps the speculation winner's DEFERRED `reviewed#{lane}` event
//! with `review.round_start_sha` - `review_unit`'s own log-derived round-start sha, carried
//! on `ReviewOutcome` - in place of a live `worktree::head_sha_of(dir)` read taken AFTER the
//! caller's exhaustive gate run and, on this (`on_pass: merge`) arm, AFTER
//! `integrate_and_emit` too (whose own `catch_up_owed_regeneration` can itself add a real
//! regenerate commit to `dir`).
//!
//! WHAT THE IMPLEMENTER'S OWN NEW TEST ALREADY COVERS (not re-proven by internals here, but
//! independently re-driven from OUTSIDE the crate below):
//! `src/conductor.rs`'s
//! `speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_integrate`
//! drives this exact `on_pass: merge` arm from INSIDE the crate's own `mod tests`, through the
//! SAME public `run()` entry point this suite uses, and asserts the same event-log invariant.
//! It is a real, correct proof of the fix - but it is the implementer's OWN instrumented test,
//! never independently re-driven through the crate's public boundary the way every other
//! periphery file in this directory re-drives its own round's fix. This suite closes that gap:
//! same underlying fix, same call site (the winning candidate's deferred `reviewed#{lane}`
//! stamp on the merge arm, strictly after `integrate_and_emit`), driven here through
//! `rigger::conductor::run` with this suite's own `AgentDriver` and its own `door` gate, never
//! reaching into `src/conductor.rs`'s `Stub`/`gate_def_inputs` or any other crate-internal
//! test scaffolding. (This suite's own sibling,
//! `review_round_speculation_winner_no_merge_round_start_sha_periphery.rs`, closes the
//! SEPARATE `on_pass: none` call site neither test above ever reaches.)
//!
//! Verified RED then GREEN by hand: reverting `emit_speculation_winner_status`'s
//! `(META_WORKTREE_SHA, &review.round_start_sha)` line back to the pre-round-4
//! `(META_WORKTREE_SHA, &winner_sha)` live read makes
//! `a_speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_merge`
//! fail - the `reviewed#{lane}` event's sha drifts to the door gate's own post-review regen
//! commit (and, on this arm, whatever `integrate_and_emit` itself may add) instead of naming
//! the sha the round actually judged. Restoring the field verbatim (`git diff` on `src/`
//! clean) returns it to green.

mod common;

use common::fixtures::{
    assert_winner_reviewed_sha_is_round_start, speculation_regen_door_cfg, WriteAndApprove,
};
use common::git::temp_git_project_with_commit;
use rigger::conductor::{run, Deps, STREAM};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::ExecRunner;
use rigger::ledger;

/// Independently re-drives the round-4 fix's `on_pass: merge` call site - the one the
/// implementer's own new test already covers from inside the crate - through this suite's own
/// `AgentDriver` and public entry point, never reaching into any crate-internal test
/// scaffolding. Proves the winner's deferred `reviewed#{lane}` stamp still carries
/// `round_start_sha` after BOTH the door gate's post-review regen commit AND a genuine
/// `integrate_and_emit` merge, never a live re-read taken after either.
#[test]
fn a_speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_merge(
) {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let cfg = speculation_regen_door_cfg("merge");
    let store = Store::open(":memory:").unwrap();
    let driver = WriteAndApprove {
        file: "feature.rs",
        body: "REAL_WORK\n",
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
        log: &|_| {},
        hash_blob: &|_| Ok(String::new()),
    };
    let rs = run(&cfg, &deps).expect("the winning candidate must integrate cleanly");

    assert_eq!(
        rs.units["s"].status,
        ledger::Status::Integrated,
        "the winning candidate must actually integrate, so integrate_and_emit really ran - \
         the exact post-review window this round's fix names"
    );
    // Candidate 0's own worktree is torn down right after integrating, so the door gate's
    // regen commit is checked in the BASE it merged into instead.
    assert!(
        repo.path().join("regen.txt").exists(),
        "premise: the door gate's own regenerate commit must really have landed (merged into \
         the base), or this test proves nothing about the post-review window"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();

    assert_winner_reviewed_sha_is_round_start(&events);
    drop(repo);
}
