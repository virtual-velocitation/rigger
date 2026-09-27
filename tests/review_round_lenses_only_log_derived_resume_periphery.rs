//! Periphery (real public API, real git, cross-module) test for spec 103 criterion 6's
//! ROUND 3 fix (diff base `db13597`, commit `f6d52d0`): `RunCtx::review_round_start_sha`
//! (`src/conductor.rs`) - the LOG-DERIVED round-start sha that replaced a live
//! `worktree::head_sha_of` re-read, closing
//! `sdet-u103c6-r3-round-start-sha-live-not-log-derived-across-a-park`.
//!
//! WHAT THE IMPLEMENTER'S OWN THREE NEW TESTS ALREADY COVER (not re-proven here). All three
//! live in `src/conductor.rs`'s own `mod tests`, driving the SAME `run_review_agents_
//! concurrently` / `review_unit` code this suite drives, through the SAME public `run()`
//! entry point, across the SAME two-separate-`run()`-calls-sharing-one-store shape this
//! suite uses:
//!   - `a_review_rounds_log_derived_start_sha_survives_a_cross_call_resume_after_a_later_tiers_park`
//!     (single-lane, a LATER tier - the adversary - parks after the lens committed residue)
//!   - `a_review_rounds_log_derived_start_sha_survives_a_same_chunk_sibling_park_across_a_resume`
//!     (single-lane, a SAME-CHUNK sibling lens parks)
//!   - `a_speculation_lanes_log_derived_start_sha_survives_a_cross_call_resume_after_a_park`
//!     (the speculation phase-B call site, closing
//!     `adv-u103c6-r3-fix-direction-fails-on-speculation-path`)
//!
//! Every one of those three configures a `judge` (adjudicator) agent - even the same-chunk
//! test, whose own review panel is `lenses: ["a", "b"], adjudicator: "judge"`.
//!
//! WHAT THAT LEAVES UNTESTED. `review_unit` (`src/conductor.rs`) has TWO structurally
//! distinct exits on the success path: the post-adjudicator guard call (line ~4506, what
//! every implementer test above drives), and the EARLIER `if adjudicator.is_empty()`
//! early-return (line ~4498) - the round-1 gap `review_round_no_adjudicator_residue_
//! periphery.rs` (this same directory) exists to close, for the round-1 restore behavior.
//! No test anywhere - not the implementer's three, not either existing periphery file -
//! ever reaches THAT early-return branch WITH the round-3 log-derivation fix in play: a
//! lenses-only panel (no adversary, no adjudicator configured at all) whose lenses park
//! and resume across two separate calls. `adv-u103c6-r3-same-chunk-sibling-park-also-
//! triggers` names this exact shape explicitly - "two lenses parking/completing
//! asynchronously in the SAME chunk is enough - no adversary/adjudicator tier need even be
//! configured" - but the fix that followed was never driven through it. Since `round_start_
//! sha` is computed once, before the panel-shape branch, a future change that special-cases
//! it per call site (or an early return that skips the stamp) would show up here first. This
//! drives that exact combination - the no-adjudicator early return, with a same-chunk
//! sibling park, across a resumed process - through the real `rigger::conductor::run` entry
//! point, a real git repo, and a real `Worktree`, never a conductor-internal stub.
//!
//! Verified RED then GREEN by hand: reverting `review_unit`'s `let round_start_sha = self.
//! review_round_start_sha(&st.name, attempt, dir)?;` line back to the pre-round-3 `let
//! round_start_sha = worktree::head_sha_of(dir);` live re-read makes
//! `a_lenses_only_panels_log_derived_start_sha_survives_a_same_chunk_sibling_park_across_a_resume`
//! fail (the round-start-start mark is still emitted, but window 2 re-derives the sha live
//! off the residue-laden tip instead of reading the stamped mark back, so the lesson never
//! names `a-residue.rs` and the residue file survives into the integrated tree). Restoring
//! the call verbatim (`git diff` on `src/` clean) returns it to green.

mod common;
use common::git::run_git;

use common::fixtures::agent;
use common::fixtures::gate_def;
use common::git::git_ok;
use common::git::temp_git_project_with_commit;
use rigger::conductor::{
    parked_spawn, run, unit_branch, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM,
};
use rigger::config::{self, AgentDef, Config, Stage};
use rigger::contextgraph;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::ExecRunner;
use rigger::ledger;
use serde_json::Value;
use std::path::Path;

/// WINDOW 1's driver. The implementer writes real work. Of the two lenses in the SAME
/// concurrent chunk (`MAX_CONCURRENCY` is 4, so both fit in one chunk): lens `a` breaks
/// review protocol, commits a real residue commit straight onto the unit's own worktree,
/// and returns `Ok`; lens `b` parks via the real public [`parked_spawn`] signal - the exact
/// same signal a real stepwise driver returns for an unanswered spawn. No adversary, no
/// adjudicator is configured at all, so `review_unit`'s early-return (`adjudicator.
/// is_empty()`) branch is the ONLY exit this panel shape can ever reach.
struct Window1Driver;

impl AgentDriver for Window1Driver {
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
        if opts.id.contains("/lens:a#") {
            std::fs::write(Path::new(&opts.dir).join("a-residue.rs"), "leftover\n").unwrap();
            git_ok(&opts.dir, &["add", "-A"]);
            git_ok(&opts.dir, &["commit", "-q", "-m", "lens a residue"]);
            return Ok(AgentResult {
                output: "reviewed: no blocker".into(),
                resolved_model: String::new(),
            });
        }
        if opts.id.contains("/lens:b#") {
            return Err(parked_spawn(&opts.id));
        }
        Ok(AgentResult::default())
    }
}

/// WINDOW 2's driver, in a SECOND, separate `run()` call sharing only the durable store and
/// repo with window 1. Lens `a` runs again but commits nothing this time, so the only
/// residue present at this entry is entirely window 1's own commit; lens `b` now succeeds
/// normally.
struct Window2Driver;

impl AgentDriver for Window2Driver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/lens:a#") || opts.id.contains("/lens:b#") {
            return Ok(AgentResult {
                output: "reviewed: no blocker".into(),
                resolved_model: String::new(),
            });
        }
        Ok(AgentResult::default())
    }
}

/// Drives the round-3 log-derived `round_start_sha` through the ONE call shape none of the
/// implementer's own three new tests (nor either existing periphery file) reaches: a
/// lenses-only panel - no adversary, no adjudicator configured - whose same-chunk sibling
/// lens parks, resumed from a SECOND, separate process sharing only the durable store. A
/// live `worktree::head_sha_of` re-read at that second entry would read the residue-laden
/// tip lens `a`'s window-1 commit left behind and silently adopt it as the round's own new
/// baseline - exactly the failure this criterion exists to prevent, now proven at the one
/// call site none of round 3's own coverage exercises.
#[test]
#[expect(clippy::too_many_lines)] // lesson: lesson-split-clippy-too-many-lines
fn a_lenses_only_panels_log_derived_start_sha_survives_a_same_chunk_sibling_park_across_a_resume() {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();

    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = format!("{repo_path}/.rigger-test-scratch");
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("a".into(), agent("a"));
    cfg.agents.insert("b".into(), agent("b"));
    cfg.workflow.gates.insert("g".into(), gate_def("exit 0"));
    cfg.workflow.stages.insert(
        "unit-a".into(),
        Stage {
            name: "unit-a".into(),
            agent: "worker".into(),
            gates: vec!["g".into()],
            on_pass: "merge".into(),
            needs: vec![],
            review: config::ReviewPanel {
                lenses: vec!["a".into(), "b".into()],
                // Deliberately no adversary, no adjudicator: the only reachable exit is
                // review_unit's `if adjudicator.is_empty()` early return.
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();

    // WINDOW 1: lens "a" commits residue and returns Ok; lens "b" (same chunk) parks. The
    // round unwinds cleanly (run() still returns Ok - a park is a normal, non-terminal
    // unwind, never a run failure) with the residue still on the branch, unrestored -
    // `guard_review_round_tree_on_tier_err` skips by design while a tier is still parked.
    let deps1 = Deps {
        store: &store,
        driver: &Window1Driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    run(&cfg, &deps1).expect(
        "a same-chunk sibling park on a lenses-only panel must not fail the run - the round \
         unwinds cleanly and waits to be resumed",
    );

    let events1 = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert!(
        !events1.iter().any(|e| e.type_ == ledger::TYPE_UNIT_STATUS
            && String::from_utf8_lossy(&e.data).contains("\"status\":\"reviewed\"")),
        "premise: the round must NOT have finished in window 1 - lens b's park must have \
         actually stopped it short of the early-return, or this test proves nothing about a \
         resumed second entry"
    );

    let branch = unit_branch("unit-a");
    let residue_tip = run_git(&repo_path, &["rev-parse", &branch]);
    assert!(
        residue_tip.status.success(),
        "premise: the unit branch must still exist after a park"
    );
    let residue_tip = String::from_utf8_lossy(&residue_tip.stdout)
        .trim()
        .to_string();

    // WINDOW 2: a SECOND, separate process (a fresh driver and Deps, sharing only the store
    // and repo). Lens "a" commits nothing this time; lens "b" now succeeds.
    let deps2 = Deps {
        store: &store,
        driver: &Window2Driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps2).expect("the resumed window must finish the round");
    assert_eq!(
        rs.units["unit-a"].status,
        ledger::Status::Integrated,
        "the round must approve and integrate once the resumed window finishes"
    );
    assert_eq!(
        rs.units["unit-a"].attempts, 0,
        "reviewer residue hygiene across a same-chunk sibling park charges NO remediation \
         attempt, even with no adjudicator ever configured"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();

    let round_start_marks = events
        .iter()
        .filter(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains("\"status\":\"review-round-start\"")
        })
        .count();
    assert_eq!(
        round_start_marks, 1,
        "the round-start mark must be stamped exactly once, on window 1's first entry - \
         window 2's resumed re-entry must READ it back, never emit a second one"
    );

    let verified_sha = events
        .iter()
        .find(|e| {
            e.type_ == ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains("\"status\":\"verified\"")
        })
        .and_then(|e| e.meta.get(rigger::conductor::META_WORKTREE_SHA).cloned())
        .unwrap_or_default();
    assert_eq!(
        verified_sha.len(),
        40,
        "premise: the verified stamp (window 1's real round-start, taken before either lens \
         ran) must carry a real sha: {verified_sha:?}"
    );
    assert_ne!(
        verified_sha, residue_tip,
        "premise: the true round-start sha must differ from the residue-laden tip, or this \
         test cannot distinguish a log-derived read from a live re-read"
    );

    let lesson = events
        .iter()
        .find(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
        .expect(
            "lens a's window-1 residue must be named in a lesson once the resumed round \
             finishes - never silently adopted as the new baseline",
        );
    assert!(
        String::from_utf8_lossy(&lesson.data).contains("a-residue.rs"),
        "the lesson must name the residue lens a committed: {:?}",
        String::from_utf8_lossy(&lesson.data)
    );

    assert!(
        !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
        "reviewer residue hygiene across a same-chunk sibling park charges NO remediation \
         attempt"
    );

    assert_eq!(
        std::fs::read_to_string(Path::new(&repo_path).join("feature.rs")).unwrap(),
        "REAL_WORK\n",
        "the implementer's real, reviewed work must still land"
    );
    assert!(
        !Path::new(&repo_path).join("a-residue.rs").exists(),
        "lens a's own residue must NEVER reach integration, even after surviving a \
         same-chunk sibling park across a resumed process, with no adjudicator ever \
         configured"
    );

    drop(repo);
}
