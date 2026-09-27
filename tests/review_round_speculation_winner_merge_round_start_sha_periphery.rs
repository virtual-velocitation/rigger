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
/// it sees. Uniform across lanes (no lane-index branching) - candidate 0 wins deterministically
/// against an identical candidate 1.
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

/// Independently re-drives the round-4 fix's `on_pass: merge` call site - the one the
/// implementer's own new test already covers from inside the crate - through this suite's own
/// `AgentDriver` and public entry point, never reaching into any crate-internal test
/// scaffolding. Proves the winner's deferred `reviewed#{lane}` stamp still carries
/// `round_start_sha` after BOTH the door gate's post-review regen commit AND a genuine
/// `integrate_and_emit` merge, never a live re-read taken after either.
#[test]
fn a_speculation_winner_reviewed_sha_stays_the_round_start_sha_across_a_post_review_regen_commit_and_merge(
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
            // Idempotent, like a real `regenerate:` command (the same idempotency the
            // post-merge re-gate, spec 12 unit 5, requires of every exhaustive-tier gate
            // re-run against the merged tree).
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
            on_pass: "merge".into(),
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

    // Non-vacuity: the door gate's post-review regen commit (plus, on this arm, whatever
    // integrate_and_emit itself may add) genuinely moved the tip past what the round
    // reviewed, or this test cannot distinguish the fixed behavior from the pre-round-4 bug.
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

    // The actual fix: `reviewed#{lane}` must carry the sha the round REVIEWED
    // (round_start_sha), never a live read of `dir` taken after the exhaustive gate's own
    // regen commit and integrate_and_emit.
    assert_eq!(
        reviewed_sha, round_start_sha,
        "the speculation winner's deferred `reviewed#{{lane}}` stamp must carry THE sha \
         review_unit's round actually judged (round_start_sha), not a live re-read of `dir` \
         taken after the exhaustive gate's own post-review regen commit and \
         integrate_and_emit: reviewed={reviewed_sha:?} round_start={round_start_sha:?} \
         verified={verified_sha:?}"
    );
    // `verified#{lane}` legitimately keeps the post-gate sha (what the gates verified, never
    // what the review judged) - unchanged by this fix.
    assert_eq!(
        verified_sha.len(),
        40,
        "the verified sha must be a real 40-hex sha: {verified_sha:?}"
    );

    drop(repo);
}
