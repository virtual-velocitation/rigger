//! Conductor-port fixtures.

use std::path::Path;

use rigger::conductor::{AgentDriver, AgentResult, Error, SpawnOpts, META_WORKTREE_SHA};
use rigger::config::{AgentDef, Config, ReviewPanel, Stage};
use rigger::eventstore::Event;
use rigger::ledger::TYPE_UNIT_STATUS;

use super::{agent, gate_def, gate_def_inputs};

/// A no-op emit sink for an `AgentDriver::spawn` whose emits the test does not observe.
pub fn no_emit(_: &str, _: serde_json::Value) -> Result<(), Error> {
    Ok(())
}

/// The spawn options of an attempt-0 implementer spawn `id` carrying a fixed persona prompt.
pub fn implementer_opts(id: &str) -> SpawnOpts {
    SpawnOpts {
        id: id.to_string(),
        attempt: 0,
        system_prompt: "You implement findings.".to_string(),
        ..Default::default()
    }
}

/// A reviewer double: the adjudicator approves, every other reviewer returns a plain note.
pub fn review_or_adjudicate(opts: &SpawnOpts) -> AgentResult {
    if opts.id.contains("/adjudicator#") {
        return AgentResult {
            output: r#"{"verdict":"approve"}"#.into(),
            resolved_model: String::new(),
        };
    }
    AgentResult {
        output: "reviewed the diff".into(),
        resolved_model: String::new(),
    }
}

/// How many `UnitStatus` markers in `events` carry the `status` token.
pub fn count_status_marker(events: &[Event], status: &str) -> usize {
    events
        .iter()
        .filter(|e| {
            e.type_ == TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(&format!("\"status\":\"{status}\""))
        })
        .count()
}

/// Whether `events` carries a `UnitStatus` marker whose `status` is `status`.
pub fn has_status_marker(events: &[Event], status: &str) -> bool {
    count_status_marker(events, status) > 0
}

/// A driver whose implementer writes `body` to `file` in its worktree and whose reviewers answer
/// through [`review_or_adjudicate`] - the adjudicator approves, every other reviewer returns a
/// plain note. Uniform across lanes (no lane-index branching), so under speculation candidate 0
/// wins deterministically against an identical candidate 1.
pub struct WriteAndApprove {
    pub file: &'static str,
    pub body: &'static str,
}

impl AgentDriver for WriteAndApprove {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join(self.file), self.body).unwrap();
            return Ok(AgentResult::default());
        }
        Ok(review_or_adjudicate(opts))
    }
}

/// One `speculation_width: 2` stage `s` run by `worker`, reviewed by an adjudicator-only panel
/// (`judge`), passing to `on_pass`. Besides the always-passing `ok` gate it carries a `door`
/// gate scoped to `inputs` that never match the (always-empty, no grounder) blast radius, so it
/// is SKIPPED at the narrowed pass and fires for the FIRST time at the EXHAUSTIVE pass - strictly
/// AFTER the review captured its round-start sha - committing a real `regen.txt` onto the
/// candidate's worktree. The command is idempotent, like a real `regenerate:` command: the
/// post-merge re-gate re-runs every exhaustive-tier gate against the merged tree.
pub fn speculation_regen_door_cfg(on_pass: &str) -> Config {
    let mut cfg = Config::default();
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert("ok".into(), gate_def("true"));
    cfg.workflow.gates.insert(
        "door".into(),
        gate_def_inputs(
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
            on_pass: on_pass.into(),
            speculation_width: 2,
            review: ReviewPanel {
                adjudicator: "judge".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    cfg
}

/// The worktree sha stamped on the first `UnitStatus` event whose data carries `needle`,
/// panicking with `missing` when none does.
pub fn status_sha(events: &[Event], needle: &str, missing: &str) -> String {
    events
        .iter()
        .find(|e| e.type_ == TYPE_UNIT_STATUS && String::from_utf8_lossy(&e.data).contains(needle))
        .expect(missing)
        .meta
        .get(META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default()
}

/// Assert a speculation winner's deferred `reviewed#{lane}` stamp carries the sha the review
/// round ACTUALLY judged (its durable round-start sha), never a live re-read of the worktree
/// taken after the exhaustive gate's post-review regen commit (and, on the merge arm,
/// `integrate_and_emit`) - while `verified#{lane}` legitimately keeps the post-gate sha.
pub fn assert_winner_reviewed_sha_is_round_start(events: &[Event]) {
    // The ground truth: the sha `review_unit` ACTUALLY judged, captured durably BEFORE any
    // tier ran and before the door gate's own regen commit ever landed.
    let round_start_sha = status_sha(
        events,
        "\"status\":\"review-round-start\"",
        "review_unit must have durably stamped its round-start sha",
    );
    assert_eq!(
        round_start_sha.len(),
        40,
        "premise: the durable round-start sha must be a real 40-hex sha: {round_start_sha:?}"
    );
    let verified_sha = status_sha(
        events,
        r#""status":"verified"#,
        "the speculation winner's deferred verified status must have been recorded",
    );
    // Non-vacuity: the regen commit genuinely moved the tip past what the round reviewed, or
    // the check cannot distinguish the fixed behavior from the live-re-read bug.
    assert_ne!(
        round_start_sha, verified_sha,
        "premise: the door gate's post-review regen commit must have moved the candidate's \
         tip strictly past round_start_sha, or this test proves nothing about the live-re-read \
         bug: round_start={round_start_sha:?} verified={verified_sha:?}"
    );
    let reviewed_sha = status_sha(
        events,
        r#""status":"reviewed"#,
        "the speculation winner's deferred reviewed status must have been recorded",
    );
    assert_eq!(
        reviewed_sha, round_start_sha,
        "the speculation winner's deferred `reviewed#{{lane}}` stamp must carry THE sha \
         review_unit's round actually judged (round_start_sha), not a live re-read of `dir` \
         taken after the exhaustive gate's own post-review regen commit: \
         reviewed={reviewed_sha:?} round_start={round_start_sha:?} verified={verified_sha:?}"
    );
    assert_eq!(
        verified_sha.len(),
        40,
        "the verified sha must be a real 40-hex sha: {verified_sha:?}"
    );
}

/// A driver that does nothing and reports nothing: for a test about what a run RECORDS rather
/// than about agent behaviour - an empty result and a passing gate are enough to reach
/// integration.
#[derive(Default)]
pub struct NoopDriver;

impl AgentDriver for NoopDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        _opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        Ok(AgentResult {
            output: String::new(),
            resolved_model: String::new(),
        })
    }
}
