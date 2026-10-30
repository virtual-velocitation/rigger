//! Periphery (real public API, real git, cross-module) test for spec 103 criterion 6's own
//! diff (base `db13597`): `RunCtx::guard_review_round_tree`
//! (`crates/rigger-conductor/src/conductor.rs`) and `Worktree::restore_reviewed_sha`
//! (`crates/rigger-worktree-git/src/worktree.rs`).
//!
//! WHAT THE IMPLEMENTER'S OWN TESTS ALREADY COVER (not re-proven here).
//! `crates/rigger-worktree-git/src/worktree.rs`'s own `mod tests` proves `restore_reviewed_sha`
//! itself discards both tracked and untracked residue on a bare `Worktree`, called directly
//! (`restore_reviewed_sha_discards_both_tracked_and_untracked_residue`, white-box). And
//! `crates/rigger-conductor/src/conductor.rs`'s own `mod tests` proves
//! `guard_review_round_tree` fires from `review_unit` and restores both a dirty-untracked shape
//! and a moved-tip (committed) shape of residue
//! (`a_review_rounds_dirty_residue_is_restored_named_and_never_merged`,
//! `a_review_rounds_moved_tip_is_restored_to_the_reviewed_sha_before_merging`) - but BOTH of
//! those drive it with a panel that has an adjudicator step
//! (`review: ReviewPanel { adjudicator: "judge".into(), ..Default::default() }`), so the guard
//! runs with a verdict still to act on.
//!
//! WHAT THAT LEAVES UNTESTED. `review_unit` guards once, after its step loop, and then a panel
//! with no adjudicator step (a lenses-and/or-adversary-only panel) returns approved: it has no
//! verdict to act on. That return is never reached by either implementer test above - both
//! configure an adjudicator. Nothing anywhere proves a LENS'S OWN residue (left in the real
//! unit worktree, against the review protocol's own "never write this unit's own worktree"
//! rule) is caught, named, and restored when there is no adjudicator to run after it - so a
//! workflow that runs only lenses (a real, supported shape: `ReviewPanel::is_empty` treats a
//! lenses-only panel as non-empty) could merge a lens's own stray write straight into the
//! integrated tree with nothing to stop it, if the guard were ever dropped or moved below that
//! return. This drives that exact branch through the crate's real public entry point
//! (`rigger::conductor::run`), a real git repo, and a real `Worktree`, never a conductor-
//! internal stub.
//!
//! Verified RED then GREEN by hand: moving the `self.guard_review_round_tree(wt, dir,
//! &st.name, &round_start_sha)?;` call below `review_unit`'s return of a panel with no
//! adjudicator step makes `a_lenses_only_panels_residue_is_restored_and_never_merged` fail (the
//! residue file survives into the integrated tree); restoring the call verbatim (`git diff` on
//! `crates/` clean) returns it to green.

mod common;

use common::fixtures::agent;
use common::fixtures::gate_def;
use common::git::temp_git_project_with_commit;
use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM};
use rigger::config::{self, AgentDef, Config, Stage};
use rigger::contextgraph;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::gate::ExecRunner;
use rigger::ledger;
use serde_json::Value;
use std::path::Path;

/// The implementer writes real work; the lone review LENS breaks the review protocol on
/// purpose and drops an untracked file straight into the unit's OWN real worktree
/// (`opts.dir`, never a scratch worktree of its own) - the exact residue shape a review
/// round must never let reach integration.
struct LensResidueDriver;

impl AgentDriver for LensResidueDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join("a.rs"), "A_WORK\n").unwrap();
            return Ok(AgentResult::default());
        }
        if opts.id.contains("/lens:") {
            std::fs::write(Path::new(&opts.dir).join("lens-residue.txt"), "leftover\n").unwrap();
        }
        Ok(AgentResult {
            output: "reviewed the diff".into(),
            resolved_model: String::new(),
            ..Default::default()
        })
    }
}

/// Drives `review_unit`'s FIRST `guard_review_round_tree` call site - a panel whose
/// `adjudicator` is empty, so the round's last real result is the lone lens's - through the
/// real `rigger::conductor::run` entry point, a real git repo, and a real `Worktree`. Proves
/// the lens's own residue is named in a lesson, restored before the merge, never charges a
/// remediation attempt, and never reaches the integrated tree - and that the implementer's
/// real work still lands normally alongside it.
#[test]
fn a_lenses_only_panels_residue_is_restored_and_never_merged() {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();

    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = format!("{repo_path}/.rigger-test-scratch");
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("lens".into(), agent("lens"));
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
                lenses: vec!["lens".into()],
                // Deliberately no adversary, no adjudicator: `ReviewPanel::is_empty` still
                // treats this as a real panel (a lens alone is non-empty), and
                // `review_unit`'s return of a panel with no adjudicator step is the one under test.
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = LensResidueDriver;
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
        check_emit: &rigger::contextgraph::check_emit,
    };

    let rs = run(&cfg, &deps).expect(
        "a lens leaving residue in the unit's own worktree must not fail the run - the \
         adjudicator-empty guard call site restores it and the round still approves",
    );
    assert_eq!(rs.units["unit-a"].status, ledger::Status::Integrated);
    assert_eq!(
        rs.units["unit-a"].attempts, 0,
        "a reviewer's own residue is infrastructure hygiene, never a charged remediation \
         attempt - on the no-adjudicator path exactly as on the with-adjudicator one"
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let lesson = events
        .iter()
        .find(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
        .expect(
            "the lens's residue must be named in a lesson, never silently discarded, even \
             with no adjudicator to run after it",
        );
    assert!(
        String::from_utf8_lossy(&lesson.data).contains("lens-residue.txt"),
        "the lesson must name the residue path: {:?}",
        String::from_utf8_lossy(&lesson.data)
    );

    assert!(
        !events.iter().any(|e| e.type_ == ledger::TYPE_UNIT_FAILED),
        "residue on the no-adjudicator path must charge NO remediation attempt - the review \
         still approves and integrates, never an extra UnitFailed"
    );

    assert_eq!(
        std::fs::read_to_string(Path::new(&repo_path).join("a.rs")).unwrap(),
        "A_WORK\n",
        "the implementer's real, reviewed work must still land"
    );
    assert!(
        !Path::new(&repo_path).join("lens-residue.txt").exists(),
        "the lens's own residue must NEVER reach integration - this is the one call site \
         (review_unit's adjudicator-empty branch) neither of the implementer's own \
         adjudicator-only panel tests exercises"
    );

    drop(repo);
}
