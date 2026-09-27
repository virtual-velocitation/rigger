//! Periphery (real-git, real-process-boundary) proof that spec 103 criterion 7's post-merge
//! re-gate worktree/branch are reclaimed even when the setup that builds them errs - the two
//! resource-leak classes the adversary independently caught and recorded as MISSED BY SDET
//! (`adv-u103c7-postmerge-worktree-branch-leak-on-run-gates-err`,
//! `adv-u103c7-r3-createbranch-then-create-leaks-branch`), across two rounds of this same unit.
//!
//! WHAT THE EXISTING COVERAGE PROVES, AND WHAT IT DOES NOT.
//!
//! `tests/postmerge_gate_modified_file_periphery.rs` proves criterion 7's HAPPY-PATH isolation
//! guarantee (the re-gate reads the landed tree, never the operator's dirty/untracked copy).
//! The implementer's own `mod tests` entries in src/conductor.rs,
//! `postmerge_run_gates_err_still_reaps_the_throwaway_worktree_and_branch` and
//! `postmerge_worktree_create_err_still_reaps_the_just_created_branch`, drive the real
//! production boundary (a real `conductor::run()` call, real git) and prove the two ERROR-PATH
//! cleanup guarantees this file re-proves - but they are the SAME role's own regression tests
//! for a defect that same role's review pass missed twice already on this unit (both adversary
//! findings above name "missed by ... sdet"), authored with the implementer's own private test
//! doubles (`Stub`, `FailAppendMetaContaining`, `init_repo`) that this file's role may not
//! reach into. An independently-authored test at the real periphery boundary is the only thing
//! that keeps this guarantee from resting on the same judgment that already missed it twice.
//!
//! MECHANISM. Both tests drive a single unit through `rigger::conductor::run()` with real git
//! and a real `gate::ExecRunner` subprocess gate, force an error at one of the two specific
//! git-orchestration steps `RunCtx::integrate_and_emit` performs to stand up its throwaway
//! `rigger-postmerge-<unit>-<attempt>` worktree, and assert that NOTHING of that throwaway
//! setup survives `run()` returning `Err`: neither the worktree directory nor the
//! `rigger/postmerge/<unit>-<attempt>` branch ref. The deterministic naming
//! (`<scratch-root>/rigger-postmerge-<unit-slug>-<attempt>` /
//! `rigger/postmerge/<unit-slug>-<attempt>`) is documented on the production
//! `Throwaway::POSTMERGE.dir_and_branch` doc comments (src/conductor.rs) as part of the
//! criterion's own contract, not a private implementation detail; both tests below use a unit
//! id (`unit-a`) and attempt (`0`, the first integration attempt of a fresh single-pass run)
//! for which that slugging is the identity transform, so the path is reconstructed here without
//! calling any private helper.

mod common;

use rigger::worktree::branch_exists;

use std::path::Path;
use std::process::Command;

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Appended, Event, EventStore, ExpectedRevision};
use rigger::gate::ExecRunner;
use serde_json::Value;

fn git_ok(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The re-gate worktree/branch naming spec 103 criterion 7 documents as part of its contract
/// (`Throwaway::POSTMERGE.dir_and_branch`, src/conductor.rs) - reconstructed here rather
/// than called, since those helpers are private to the crate's own module. Valid for a unit id
/// that needs no sanitizing (`unit-a`: ASCII alphanumerics and a single interior hyphen), which
/// this file's fixtures always use.
fn expected_postmerge_dir(scratch_root: &str, unit_id: &str, attempt: u32) -> String {
    format!("{scratch_root}/rigger-postmerge-{unit_id}-{attempt}")
}
fn expected_postmerge_branch(unit_id: &str, attempt: u32) -> String {
    format!("rigger/postmerge/{unit_id}-{attempt}")
}

/// A single stage, one gate that always passes, no review panel - the same minimal shape the
/// implementer's own round-2/round-3 regression tests use to reach `integrate_and_emit`'s
/// post-merge re-gate setup without any merge-break/multi-unit machinery, since neither error
/// this file forces depends on the merged tree's content. Authored independently of the
/// implementer's own private `Stub` driver.
struct WriteOneFileDriver;

impl AgentDriver for WriteOneFileDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if !opts.dir.is_empty() {
            std::fs::write(Path::new(&opts.dir).join("f.txt"), "hello\n").unwrap();
        }
        Ok(AgentResult::default())
    }
}

fn base_config(repo: &Path) -> Config {
    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo);
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "g".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        "unit-a".into(),
        Stage {
            name: "unit-a".into(),
            agent: "worker".into(),
            gates: vec!["g".into()],
            on_pass: "merge".into(),
            needs: vec![],
            ..Default::default()
        },
    );
    cfg
}

fn init_base_repo(repo: &Path) -> String {
    let repo_path = repo.to_str().unwrap().to_string();
    git_ok(repo, &["init", "-q"]);
    git_ok(repo, &["config", "user.email", "t@example.com"]);
    git_ok(repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("base.txt"), "base\n").unwrap();
    git_ok(repo, &["add", "-A"]);
    git_ok(repo, &["commit", "-q", "-m", "base"]);
    repo_path
}

/// Forces the FIRST git-orchestration step in `integrate_and_emit`'s post-merge setup
/// (`Worktree::create`, the `git worktree add <pm_dir> <pm_branch>` immediately after
/// `Worktree::create_branch_at` has already minted `pm_branch` as a real ref) to fail, via a
/// dangling symlink planted at the deterministic worktree path: `Path::exists` follows a
/// symlink and reports `false` for a dangling one, so the pre-create leftover check and
/// `Worktree::discard` both skip it and `create_branch_at` succeeds exactly as in the real
/// leak window - but the path is still OCCUPIED on disk, so `git worktree add` at that exact
/// path hard-fails with "already exists" (a deterministic filesystem property, not a race).
/// Proves `adv-u103c7-r3-createbranch-then-create-leaks-branch` stays fixed: the just-minted
/// `rigger/postmerge/<unit>-<attempt>` branch must not survive.
#[test]
fn worktree_create_err_during_postmerge_regate_leaves_no_branch_behind() {
    let repo = tempfile::tempdir().unwrap();
    let repo_path = init_base_repo(repo.path());
    let cfg = base_config(repo.path());

    // Created as a side effect of resolving it (the resolver's own documented contract), so
    // the symlink below lands in an already-existing parent directory.
    let scratch =
        rigger::worktree::scratch_root_from_env(&repo_path, &cfg.workflow.defaults.workdir);
    let pm_dir = expected_postmerge_dir(&scratch, "unit-a", 0);
    let pm_branch = expected_postmerge_branch("unit-a", 0);
    std::os::unix::fs::symlink("/nonexistent-sdet-u103c7-r3-target", &pm_dir)
        .expect("plant the dangling symlink that occupies pm_dir without `exists()`-ing");

    let store = Store::open(":memory:").unwrap();
    let driver = WriteOneFileDriver;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };

    assert!(
        run(&cfg, &deps).is_err(),
        "a genuine post-merge worktree-create infra error must propagate out of run(), never \
         be swallowed as a passing/failing gate verdict"
    );

    assert!(
        !branch_exists(&repo_path, &pm_branch),
        "the post-merge re-gate's own throwaway branch {pm_branch:?} must be deleted even \
         when the worktree-create step itself errors right after the branch was minted, never \
         left behind"
    );
}

/// An `EventStore` decorator that forwards every call to a real in-memory store unchanged
/// except `append`, which fails (a real `Backend` error, indistinguishable from a genuine
/// backend fault) for exactly the event whose metadata names the post-merge gate verdict -
/// the `postmerge-gate:` infix `postmerge_gate_verdict_key` (src/conductor.rs) stamps into
/// that event's replay-key metadata, distinct from the pre-merge `gate:` key every other gate
/// write in this run carries, so only the post-merge re-gate's own verdict write is affected.
/// Authored independently of the implementer's own private `FailAppendMetaContaining` double
/// in src/conductor.rs's `mod tests`.
struct FailPostmergeVerdictWrite<'a> {
    inner: &'a dyn EventStore,
}

impl EventStore for FailPostmergeVerdictWrite<'_> {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, rigger::eventstore::Error> {
        let targets_postmerge_verdict = events
            .iter()
            .any(|e| e.meta.values().any(|v| v.contains("postmerge-gate:")));
        if targets_postmerge_verdict {
            return Err(rigger::eventstore::Error::Backend(
                "simulated store failure appending the post-merge gate verdict".into(),
            ));
        }
        self.inner.append(stream, expected, events)
    }
    crate::delegate_event_store_reads!();
}

/// Forces the SECOND git-orchestration step (`self.run_gates(..., GateSelection::PostMerge)`,
/// which runs the gate suite AND writes its `GateVerdict`) to fail after the throwaway
/// worktree was already created successfully. Proves
/// `adv-u103c7-postmerge-worktree-branch-leak-on-run-gates-err` stays fixed: neither the
/// worktree directory nor its branch must survive.
#[test]
fn run_gates_err_during_postmerge_regate_leaves_no_worktree_or_branch_behind() {
    let repo = tempfile::tempdir().unwrap();
    let repo_path = init_base_repo(repo.path());
    let cfg = base_config(repo.path());

    let scratch =
        rigger::worktree::scratch_root_from_env(&repo_path, &cfg.workflow.defaults.workdir);
    let pm_dir = expected_postmerge_dir(&scratch, "unit-a", 0);
    let pm_branch = expected_postmerge_branch("unit-a", 0);

    let real_store = Store::open(":memory:").unwrap();
    let store = FailPostmergeVerdictWrite { inner: &real_store };
    let driver = WriteOneFileDriver;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };

    assert!(
        run(&cfg, &deps).is_err(),
        "a genuine post-merge gate-suite infra error must propagate out of run(), never be \
         swallowed as a passing/failing gate verdict"
    );

    assert!(
        !Path::new(&pm_dir).exists(),
        "the post-merge re-gate's own throwaway worktree must be reaped even when its gate \
         suite errors, never leaked for a later step to find; {pm_dir} still exists"
    );
    assert!(
        !branch_exists(&repo_path, &pm_branch),
        "the post-merge re-gate's own throwaway branch {pm_branch:?} must be deleted even \
         when its gate suite errors, never left behind"
    );
}
