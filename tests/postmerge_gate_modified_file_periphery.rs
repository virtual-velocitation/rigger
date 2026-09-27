//! Periphery (real-git, real-subprocess-gate) proof for spec 103, criterion 7 (POST-MERGE
//! GATES RUN ON THE LANDED TREE): "the post-merge re-gate runs in a clean worktree of the
//! landed sha, so an UNTRACKED OR MODIFIED file in the operator's working directory cannot
//! change a verdict". `RunCtx::integrate_and_emit` (src/conductor.rs) now checks the merged
//! tree out into its own throwaway `rigger-postmerge-<unit>-<attempt>` worktree instead of
//! running the re-gate directly in `self.deps.repo`.
//!
//! WHAT THE EXISTING COVERAGE PROVES, AND WHAT IT DOES NOT.
//!
//! The implementer's own `mod tests` entry in src/conductor.rs,
//! `the_post_merge_re_gate_runs_in_its_own_scratch_worktree_never_the_repo`, drives the same
//! real production boundary this file does (a real `conductor::run()` call, a real git repo, a
//! real `gate::ExecRunner` subprocess) and proves the UNTRACKED half of the criterion: a brand
//! new file dropped into the operator's checkout is invisible to the re-gate. It never touches
//! a file that is part of the landed commit itself - so the MODIFIED half the criterion text
//! names in the same sentence ("untracked OR modified") is asserted nowhere, by any test, unit
//! or periphery. A worktree that is merely careless about which of the operator's UNTRACKED
//! scratch it leaves behind could still, in principle, pick up the operator's own dirty copy of
//! a TRACKED file (a naive "clone the working tree" implementation, a stray `cp -r`/`rsync`
//! over the live checkout, or a worktree add that forgot to specify the landed sha and defaulted
//! to whatever the operator's branch happens to be sitting on) - a different failure shape than
//! an untracked stray, and not implied by it. This file closes that half.
//!
//! MECHANISM. A single unit's clean fast-forward merge lands a tree byte-identical to the one
//! its own pre-merge gate already gated - the post-merge re-gate content-cache-hits and never
//! runs a command at all (spec 12, unit 1's digest cache), so no worktree of any kind is ever
//! created to prove anything about. Forcing a REAL post-merge run therefore needs the same
//! mechanism spec 12 unit 5 (and the implementer's own round of this criterion) use: two
//! same-wave, no-dependency batch-mates editing disjoint regions of one file, so the SECOND
//! integrator's merge produces a two-mark tree neither unit's own pre-merge gate ever saw. This
//! fixture is authored independently here (never reaching into the implementer's private
//! `mod tests` fixtures, per this role's boundary) for that reason alone - not because the
//! scenario itself is this file's real subject.

mod common;
use common::git::git_ok;

use common::fixtures::{MergeBreakDriver, MERGE_BREAK_BASE};

use std::path::Path;

use rigger::conductor::{run, Deps};
use rigger::config::{AgentDef, Config, Gate, ReviewPanel, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger;

/// The operator's tracked file's content AT THE LANDED COMMIT - what a correctly-isolated
/// post-merge worktree must read, regardless of what `self.deps.repo`'s own working copy holds
/// at the moment the gate runs.
const SENTINEL_COMMITTED: &str = "clean";

/// What this test locally (never committed) overwrites the operator's own checkout's copy of
/// the sentinel file with, for the entire duration of the run - simulating an operator mid-edit
/// on a tracked file while a run executes, exactly as real as the untracked-scratch-file case
/// the implementer's own test already covers.
const SENTINEL_DIRTY: &str = "operator-modified";

#[test]
#[expect(clippy::too_many_lines)] // lesson: lesson-split-clippy-too-many-lines
fn a_locally_modified_tracked_file_in_the_operators_checkout_never_reaches_the_post_merge_gate() {
    let repo = tempfile::tempdir().unwrap();
    let repo_path = repo.path().to_str().unwrap().to_string();
    git_ok(repo.path(), &["init", "-q"]);
    git_ok(repo.path(), &["config", "user.email", "t@example.com"]);
    git_ok(repo.path(), &["config", "user.name", "t"]);
    std::fs::write(repo.path().join("m.rs"), MERGE_BREAK_BASE).unwrap();
    // A file that is part of the LANDED commit from the very start - never touched by either
    // unit's own edits (they only ever write m.rs), so any difference the re-gate sees in it
    // can only have come from the operator's own live, uncommitted checkout.
    std::fs::write(repo.path().join("sentinel.txt"), SENTINEL_COMMITTED).unwrap();
    git_ok(repo.path(), &["add", "-A"]);
    git_ok(repo.path(), &["commit", "-q", "-m", "base"]);

    // The operator's own checkout goes dirty on a TRACKED file - never `git add`ed - for the
    // entire run. A worktree checkout of the landed sha reads this file from git's object
    // store, not from any other checkout's live working copy, so it can never observe this.
    std::fs::write(repo.path().join("sentinel.txt"), SENTINEL_DIRTY).unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("sentinel.txt")).unwrap(),
        SENTINEL_DIRTY,
        "fixture precondition: the operator's checkout must actually be dirty before run()"
    );

    // Every gate run appends "<physical cwd>\t<sentinel.txt content>" to a log outside the
    // repo (an untracked file inside it would itself dirty the tree the integrate lock guards).
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("gate-runs.log");

    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
    cfg.workflow.defaults.max_retries = 2;
    for id in ["worker", "lens", "judge"] {
        cfg.agents.insert(
            id.into(),
            AgentDef {
                id: id.into(),
                ..Default::default()
            },
        );
    }
    cfg.workflow.gates.insert(
        "g".into(),
        Gate {
            run: format!(
                "printf '%s\\t%s\\n' \"$(pwd -P)\" \"$(cat sentinel.txt 2>/dev/null || echo MISSING)\" >> '{}'",
                log.display()
            ),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    let panel = ReviewPanel {
        lenses: vec!["lens".into()],
        adjudicator: "judge".into(),
        ..Default::default()
    };
    let mk = |name: &str| Stage {
        name: name.into(),
        agent: "worker".into(),
        gates: vec!["g".into()],
        on_pass: "merge".into(),
        needs: vec![],
        review: panel.clone(),
        ..Default::default()
    };
    cfg.workflow.stages.insert("unit-a".into(), mk("unit-a"));
    cfg.workflow.stages.insert("unit-b".into(), mk("unit-b"));

    let store = Store::open(":memory:").unwrap();
    let driver = MergeBreakDriver {
        repo: repo_path.clone(),
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let rs = run(&cfg, &deps).unwrap();
    for u in ["unit-a", "unit-b"] {
        assert_eq!(
            rs.units[u].status,
            ledger::Status::Integrated,
            "{u}: the gate only records its environment, so both merges land"
        );
    }

    let seen = std::fs::read_to_string(&log).unwrap();
    let repo_physical = std::fs::canonicalize(&repo_path).unwrap();
    let scratch =
        rigger::worktree::scratch_root_from_env(&repo_path, &cfg.workflow.defaults.workdir);

    let mut postmerge_lines: Vec<(&str, &str)> = Vec::new();
    for line in seen.lines() {
        let mut fields = line.splitn(2, '\t');
        let cwd = fields.next().unwrap_or_default();
        let sentinel = fields.next().unwrap_or_default();
        // No gate run - pre-merge or post-merge - may ever see the operator's dirty content:
        // every gate runs in SOME worktree checkout, never `self.deps.repo` directly.
        assert_eq!(
            sentinel, SENTINEL_COMMITTED,
            "no gate run may see the operator's locally modified tracked file; cwd={cwd}, \
             log:\n{seen}"
        );
        assert_ne!(
            cwd,
            repo_physical.to_str().unwrap(),
            "no gate run may use the operator's own checkout as its cwd; log:\n{seen}"
        );
        let is_postmerge = Path::new(cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rigger-postmerge-"));
        if is_postmerge {
            postmerge_lines.push((cwd, sentinel));
        }
    }
    assert_eq!(
        postmerge_lines.len(),
        1,
        "exactly one post-merge re-gate should have run for real (a content-cache miss over \
         the merged two-mark tree); gate runs seen:\n{seen}"
    );
    let (pm_cwd, pm_sentinel) = postmerge_lines[0];
    assert!(
        pm_cwd.starts_with(&scratch),
        "the post-merge re-gate must run under the scratch root, got {pm_cwd:?}"
    );
    assert_eq!(
        pm_sentinel, SENTINEL_COMMITTED,
        "the post-merge re-gate's own throwaway worktree must read the sentinel file as it \
         was AT THE LANDED COMMIT, never the operator's live dirty checkout; got {pm_sentinel:?}"
    );
}
