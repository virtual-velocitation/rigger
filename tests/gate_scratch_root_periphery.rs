//! Periphery proof for spec 113, THE GATE SCRATCH ROOT IS HANDED GENERICALLY: every gate that
//! runs for a unit is handed `RIGGER_GATE_SCRATCH` naming that unit worktree's
//! `rigger-gate-<slug>` sibling, and a gate that runs for no unit runs with the variable
//! removed, whatever the process running the gate holds.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO. The conductor's own tests record the
//! `gate_scratch` value handed to a `RecordingRunner`, and `gate.rs`'s own test drives
//! `ExecRunner` with literal values; neither layer sees what a REAL gate subprocess spawned by
//! the REAL `conductor::run` wiring receives, so a drift between the value `run_gates` derives
//! and what `ExecRunner` exports (or a gate path that bypasses one of them) passes both. And
//! `tests/checkin_mutation_diff_base_periphery.rs` drives the shipped mutation script with the
//! variable set BY THE TEST, never through the runner that hands it in production. This file
//! drives every one of those seams end to end with the parent process holding an outer root,
//! the exact shape of this repository's own `test` gate running this suite:
//!
//! 1. `every_unit_gate_is_handed_its_own_rigger_gate_sibling_never_the_root_its_parent_holds`:
//!    two units, their pre-merge gates and the post-merge re-gate, through a real `ExecRunner`.
//! 2. `a_standalone_review_worktree_gate_runs_with_no_gate_scratch_root`: a review-only stage.
//! 3. `a_worktree_less_run_gate_runs_with_no_gate_scratch_root`: a run with no repository.
//! 4. `the_shipped_mutation_gate_reads_the_root_the_runner_hands_and_refuses_when_handed_none`:
//!    the runner-to-script contract over the shipped `.rigger/gates/mutation.sh`.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::env_test_lock;
use common::fixtures::{bare_deps, gate_def, mk_stage, run_isolated, scratch_cfg, NoopDriver};
use common::fixtures::{review_or_adjudicate, A_WORK_DRIVER};
use common::git::temp_git_project_with_commit;
use common::repo::mutation_gate_script;
use common::RestoreEnvVars;
use rigger::budget::BuildBudget;
use rigger::conductor::{run, AgentDriver, AgentResult, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::{Autonomy, BuildEnv, ExecRunner, Gate, Kind, Runner, GATE_SCRATCH_ENV};
use rigger::ledger;
use rigger::worktree::UNIT_GATE_SCRATCH_PREFIX;

/// The root the gate's parent process holds in every test here: what the outer unit's `test`
/// gate hands this suite. No gate below may ever see it.
const OUTER_ROOT: &str = "/outer/rigger-gate-outer-unit";

/// The variable naming the file the recording gate appends to, held by the gate's parent.
const LOG_ENV: &str = "GATE_SCRATCH_PERIPHERY_LOG";

/// Sets [`OUTER_ROOT`] and [`LOG_ENV`] (naming `log`) in this process for the life of the
/// returned guards, serialized against every other environment reader in this binary and
/// restored on drop.
fn parent_holds_an_outer_root(log: &Path) -> (std::sync::MutexGuard<'static, ()>, RestoreEnvVars) {
    let lock = env_test_lock();
    let restore = RestoreEnvVars::capture(&["RIGGER_GATE_SCRATCH", LOG_ENV]);
    std::env::set_var("RIGGER_GATE_SCRATCH", OUTER_ROOT);
    std::env::set_var(LOG_ENV, log);
    (lock, restore)
}

/// A gate command appending `<cwd> <$RIGGER_GATE_SCRATCH or UNSET> <present|absent>` to the
/// file [`LOG_ENV`] names, outside the repository (an untracked file inside it would dirty the
/// tree the integration guards). `${VAR-UNSET}` tells a removed variable from an empty one.
const RECORDING_GATE: &str = "r=\"${RIGGER_GATE_SCRATCH-UNSET}\"; \
     if [ -e \"$r\" ]; then s=present; else s=absent; fi; \
     printf '%s %s %s\\n' \"$(pwd -P)\" \"$r\" \"$s\" >> \"$GATE_SCRATCH_PERIPHERY_LOG\"";

/// The `(cwd, root, presence)` triples the recording gate logged, in order.
fn logged(log: &Path) -> Vec<(String, String, String)> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            assert_eq!(f.len(), 3, "one logged gate line has three fields: {l:?}");
            (f[0].to_string(), f[1].to_string(), f[2].to_string())
        })
        .collect()
}

/// The implementer writes `<unit>.rs` (so two units never conflict); the review approves.
struct WriteOwnFileAndApprove;

impl AgentDriver for WriteOwnFileAndApprove {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if let Some((unit, _)) = opts.id.split_once("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join(format!("{unit}.rs")), "work\n").unwrap();
            return Ok(AgentResult::default());
        }
        Ok(review_or_adjudicate(opts))
    }
}

#[test]
fn every_unit_gate_is_handed_its_own_rigger_gate_sibling_never_the_root_its_parent_holds() {
    let repo = temp_git_project_with_commit();
    let repo_path = std::fs::canonicalize(repo.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("gate.log");
    let _env = parent_holds_an_outer_root(&log);

    let mut cfg = scratch_cfg(&repo_path);
    cfg.workflow
        .gates
        .insert("g".into(), gate_def(RECORDING_GATE));
    for unit in ["unit-a", "unit-b"] {
        cfg.workflow.stages.insert(unit.into(), mk_stage(unit, "g"));
    }
    let store = Store::open(":memory:").unwrap();
    let rs = run(
        &cfg,
        &bare_deps(&store, &WriteOwnFileAndApprove, &ExecRunner, &repo_path),
    )
    .unwrap();
    assert_eq!(rs.units["unit-a"].status, ledger::Status::Integrated);
    assert_eq!(rs.units["unit-b"].status, ledger::Status::Integrated);

    let lines = logged(&log);
    let seen = format!("{lines:#?}");
    let mut unit_roots = BTreeSet::new();
    let mut unit_dirs = BTreeSet::new();
    let mut postmerge = Vec::new();
    for (cwd, root, presence) in &lines {
        assert_eq!(
            presence, "absent",
            "rigger never creates the gate scratch root; a gate found {root:?} on disk: {seen}"
        );
        let wt = Path::new(cwd);
        let name = wt.file_name().unwrap().to_str().unwrap();
        match name.strip_prefix("rigger-wt-") {
            Some(slug) => {
                // A pre-merge gate in a unit worktree: exactly that worktree's sibling, spelled
                // out literally rather than through the derivation under test.
                let want = format!("{}/rigger-gate-{slug}", wt.parent().unwrap().display());
                assert_eq!(
                    root, &want,
                    "a gate in {cwd} must be handed its own rigger-gate sibling: {seen}"
                );
                unit_dirs.insert(name.to_string());
                unit_roots.insert(root.clone());
            }
            None => {
                // The post-merge re-gate runs in a throwaway `rigger-postmerge-<unit>-<n>`
                // worktree and falls back to the integrating unit's own root.
                let unit = ["unit-a", "unit-b"]
                    .into_iter()
                    .find(|u| name.starts_with(&format!("rigger-postmerge-{u}-")))
                    .unwrap_or_else(|| panic!("a gate ran outside any unit's worktree: {seen}"));
                let want = format!("{}/rigger-gate-{unit}", wt.parent().unwrap().display());
                assert_eq!(
                    root, &want,
                    "the post-merge re-gate in {cwd} must be handed {unit}'s own root, never \
                     an empty, inherited or foreign one: {seen}"
                );
                postmerge.push(root.clone());
            }
        }
    }
    assert_eq!(
        unit_dirs,
        BTreeSet::from([
            "rigger-wt-unit-a".to_string(),
            "rigger-wt-unit-b".to_string()
        ]),
        "both units' pre-merge gates ran in their own worktrees: {seen}"
    );
    assert_eq!(
        unit_roots.len(),
        2,
        "two units' gates are never handed the same root: {seen}"
    );
    assert_ne!(
        postmerge,
        Vec::<String>::new(),
        "a landing onto a moved run branch re-gates the merged tree: {seen}"
    );
}

#[test]
fn a_standalone_review_worktree_gate_runs_with_no_gate_scratch_root() {
    let repo = temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("gate.log");
    let _env = parent_holds_an_outer_root(&log);

    let mut cfg = scratch_cfg(&repo_path);
    cfg.workflow
        .gates
        .insert("g".into(), gate_def(RECORDING_GATE));
    cfg.workflow.stages.insert(
        "review".into(),
        Stage {
            name: "review".into(),
            agents: vec!["lens".into()],
            gates: vec!["g".into()],
            ..Default::default()
        },
    );
    let store = Store::open(":memory:").unwrap();
    let rs = run(
        &cfg,
        &bare_deps(&store, &A_WORK_DRIVER, &ExecRunner, &repo_path),
    )
    .unwrap();
    assert_eq!(rs.units["review"].status, ledger::Status::Integrated);

    let lines = logged(&log);
    assert_eq!(lines.len(), 1, "the review gate ran once: {lines:?}");
    let (cwd, root, _) = &lines[0];
    assert!(
        Path::new(cwd)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("rigger-review-"),
        "the gate ran in the standalone review worktree: {cwd}"
    );
    assert_eq!(
        root, "UNSET",
        "a review worktree's gate runs with RIGGER_GATE_SCRATCH removed, never the root its \
         parent holds"
    );
}

#[test]
fn a_worktree_less_run_gate_runs_with_no_gate_scratch_root() {
    let log_dir = tempfile::tempdir().unwrap();
    let log = log_dir.path().join("gate.log");
    let _env = parent_holds_an_outer_root(&log);

    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow
        .gates
        .insert("g".into(), gate_def(RECORDING_GATE));
    cfg.workflow.stages.insert(
        "a".into(),
        Stage {
            name: "a".into(),
            agent: "worker".into(),
            gates: vec!["g".into()],
            on_pass: "none".into(),
            ..Default::default()
        },
    );
    let store = Store::open(":memory:").unwrap();
    run_isolated(&cfg, &bare_deps(&store, &NoopDriver, &ExecRunner, "")).unwrap();

    let roots: Vec<String> = logged(&log).into_iter().map(|(_, r, _)| r).collect();
    assert_eq!(
        roots,
        vec!["UNSET".to_string()],
        "a run with no worktree runs its gate with RIGGER_GATE_SCRATCH removed"
    );
}

#[test]
fn the_shipped_mutation_gate_reads_the_root_the_runner_hands_and_refuses_when_handed_none() {
    // The variable's name and the sibling prefix are a contract with every gate script: the
    // shipped script spells the name literally on its root line.
    assert_eq!(GATE_SCRATCH_ENV, "RIGGER_GATE_SCRATCH");
    assert_eq!(UNIT_GATE_SCRATCH_PREFIX, "rigger-gate-");
    let script = mutation_gate_script();

    let work = tempfile::tempdir().unwrap();
    let _env = parent_holds_an_outer_root(&work.path().join("unused.log"));
    let dir = work.path().to_str().unwrap();
    // RIGGER_RUN_BASE is removed so a script that got past its root line stops at the next
    // refusal, before it touches anything.
    let gate = Gate {
        id: "mutation".into(),
        run: format!("unset RIGGER_RUN_BASE; sh '{}'", script.display()),
        kind: Kind::Core,
        autonomy: Autonomy::Manual,
        history: Vec::new(),
    };
    let run = |gate_scratch: &str| {
        ExecRunner.run(
            &gate,
            dir,
            "",
            gate_scratch,
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        )
    };
    let no_root = "RIGGER_GATE_SCRATCH: is empty or unset";
    let past_root = "mutation gate: RIGGER_RUN_BASE is unset";

    let none = run("");
    assert!(!none.pass, "handed no root the gate refuses: {none:?}");
    assert!(
        none.evidence.contains(no_root) && !none.evidence.contains(past_root),
        "handed no root, the gate refuses at its root line although its parent holds one: \
         {none:?}"
    );

    let handed = work.path().join("rigger-gate-unit-7");
    let given = run(handed.to_str().unwrap());
    assert!(
        !given.pass,
        "the next refusal still fails the gate: {given:?}"
    );
    assert!(
        given.evidence.contains(past_root) && !given.evidence.contains(no_root),
        "handed a root, the gate reads it and passes its root line: {given:?}"
    );
    assert!(
        !handed.exists(),
        "a refusing gate creates nothing under the root it was handed"
    );
}
