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

use common::fixtures::agent;
use common::fixtures::gate_def;
use common::fixtures::gate_def_inputs;
use rigger::conductor::{
    run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, META_WORKTREE_SHA, STREAM,
};
use rigger::config::{self, AgentDef, Config, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::ExecRunner;
use rigger::ledger;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// A throwaway git repo with one empty commit, so a run-branch anchor (`HEAD`) resolves.
/// Mirrors `src/conductor.rs::tests::init_repo` (private to that module) and every other
/// periphery suite's identical copy (e.g. this directory's own
/// `review_round_no_adjudicator_residue_periphery.rs`).
fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().to_str().unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        Command::new("git")
            .arg("-C")
            .arg(p)
            .args(args)
            .output()
            .unwrap();
    }
    dir
}

/// The implementer writes real work on every lane; the sole adjudicator approves every lane
/// it sees. Uniform across lanes (no lane-index branching), mirroring the implementer's own
/// `Stub` driver, which the two lanes of a `speculation_width: 2` unit share identically -
/// candidate 0 wins deterministically against an identical candidate 1.
struct ApproveEveryLaneDriver;

impl AgentDriver for ApproveEveryLaneDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join("feature.rs"), "REAL_WORK\n").unwrap();
            return Ok(AgentResult::default());
        }
        if opts.id.contains("/adjudicator#") {
            return Ok(AgentResult {
                output: r#"{"verdict":"approve"}"#.into(),
                resolved_model: String::new(),
            });
        }
        Ok(AgentResult::default())
    }
}

/// Drives the round-4 fix through the ONE `emit_speculation_winner_status` call site the
/// implementer's own new test never reaches: the `on_pass: none` arm, which calls it directly
/// after the exhaustive gate run with no `integrate_and_emit` in between at all. Proves the
/// winner's deferred `reviewed#{lane}` stamp still carries `round_start_sha` - the sha
/// `review_unit` actually judged - rather than the door gate's own post-review regen commit,
/// even with no merge ever happening on this arm.
#[test]
fn a_speculation_winner_with_no_merge_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit(
) {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let mut cfg = Config::default();
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert("ok".into(), gate_def("true"));
    cfg.workflow.gates.insert(
        "door".into(),
        gate_def_inputs(
            // Idempotent, like a real `regenerate:` command - the same pattern the
            // implementer's own merge-arm test uses, kept here even though this arm never
            // hits a post-merge re-gate, so the two sibling tests stay recognizably the
            // same shape.
            "[ -f regen.txt ] || (echo regenerated > regen.txt && git add regen.txt \
             && git commit -q -m regen-commit)",
            &["never-matches/**"],
        ),
    );
    cfg.workflow.stages.insert(
        "s".into(),
        Stage {
            name: "s".into(),
            agent: "worker".into(),
            gates: vec!["ok".into(), "door".into()],
            on_pass: "none".into(),
            speculation_width: 2,
            review: config::ReviewPanel {
                adjudicator: "judge".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
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

    // The ground truth: the sha `review_unit` ACTUALLY judged, captured durably BEFORE any
    // tier ran and before the door gate's own regen commit ever landed.
    let round_start = events
        .iter()
        .find(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains("\"status\":\"review-round-start\"")
        })
        .expect("review_unit must have durably stamped its round-start sha");
    let round_start_sha = round_start
        .meta
        .get(META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        round_start_sha.len(),
        40,
        "premise: the durable round-start sha must be a real 40-hex sha: {round_start_sha:?}"
    );

    let verified = events
        .iter()
        .find(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(r#""status":"verified"#)
        })
        .expect("the speculation winner's deferred verified status must have been recorded");
    let verified_sha = verified
        .meta
        .get(META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();

    // Non-vacuity: the door gate's post-review regen commit genuinely moved the candidate's
    // tip past what the round reviewed, or this test cannot distinguish the fixed behavior
    // from the pre-round-4 bug.
    assert_ne!(
        round_start_sha, verified_sha,
        "premise: the door gate's post-review regen commit must have moved the candidate's \
         tip strictly past round_start_sha, or this test proves nothing about the live-re-read \
         bug: round_start={round_start_sha:?} verified={verified_sha:?}"
    );

    let reviewed = events
        .iter()
        .find(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(r#""status":"reviewed"#)
        })
        .expect("the speculation winner's deferred reviewed status must have been recorded");
    let reviewed_sha = reviewed
        .meta
        .get(META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();

    // The actual fix, on the ONE call site the implementer's own test never reaches:
    // `reviewed#{lane}` must carry the sha the round REVIEWED (round_start_sha), never a live
    // read of `dir` taken after the exhaustive gate's own regen commit, even with no merge
    // (no `integrate_and_emit`) ever in play on this arm.
    assert_eq!(
        reviewed_sha, round_start_sha,
        "the speculation winner's deferred `reviewed#{{lane}}` stamp must carry THE sha \
         review_unit's round actually judged (round_start_sha), not a live re-read of `dir` \
         taken after the exhaustive gate's own post-review regen commit - even on the \
         `on_pass: none` arm, which never calls integrate_and_emit at all: \
         reviewed={reviewed_sha:?} round_start={round_start_sha:?} verified={verified_sha:?}"
    );
    // `verified#{lane}` legitimately keeps the post-gate sha (what the gates verified, never
    // what the review judged) - unchanged by this fix, same invariant the implementer's own
    // merge-arm test asserts.
    assert_eq!(
        verified_sha.len(),
        40,
        "the verified sha must be a real 40-hex sha: {verified_sha:?}"
    );

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
