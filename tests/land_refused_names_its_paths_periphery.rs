//! Periphery (real-on-disk-config, real-git, cross-module) tests for spec 103, criterion 8
//! (A REFUSED LANDING NAMES ITS PATHS): `Worktree::land`'s `git merge --ff-only` can be
//! refused by LOCAL content already sitting in the run checkout (untracked, or a tracked file
//! dirtied but never committed) at a path the unit's own landing would touch. This is never
//! the unit's fault - its own branch is untouched and still fast-forwardable once the
//! blocking content is cleared - so the conductor records ONE lesson naming every blocking
//! path (and any unit branch whose tip already carries byte-identical content there, proof
//! the blocked content is not lost work) and charges no remediation attempt.
//!
//! `src/worktree.rs`'s own `mod tests` proves `Worktree::land` itself recognizes both of
//! git's local-changes refusal wordings and returns `LandOutcome::Blocked` with the parsed
//! path list, and that `worktree::blob_at`/`worktree::unit_branches` read committed content
//! and the unit-branch list correctly - none of that is re-derived here. This file owns the
//! CONDUCTOR-SIDE seam: `Worktree::land -> integrate_and_emit -> run_wave -> run()`, driven
//! through a real `AgentDriver` and a real git repo, never hand-seeded events.

mod common;
use common::git::run_git;

use common::fixtures::agent;
use common::fixtures::gate_def;
use common::fixtures::mk_stage;
use common::fixtures::review_or_adjudicate;
use common::git::git_stdout;
use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM};
use rigger::config::{AgentDef, Config};
use rigger::contextgraph;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};
use rigger::ledger;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

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

fn base_cfg(repo_path: &str) -> Config {
    let mut cfg = Config::default();
    // Spec 89, criterion 2 relocated the scratch/worktree default off the fixture's own repo
    // tree onto a machine-wide cache root shared with every concurrently-running fixture and
    // agent on the machine; nesting it back inside this fixture's own unique repo tempdir
    // restores per-test isolation (mirrors every other periphery suite's identical fix).
    cfg.workflow.defaults.workdir = format!("{repo_path}/.rigger-test-scratch");
    cfg.agents.insert("worker".into(), agent("worker"));
    cfg.agents.insert("lens".into(), agent("lens"));
    cfg.agents.insert("judge".into(), agent("judge"));
    cfg.workflow.gates.insert("g".into(), gate_def("exit 0"));
    cfg
}

/// A single unit ("unit-a") whose implementer both does its own real work (a fresh `new.txt`
/// on its own worktree branch) AND, in the SAME spawn, deposits local content at the exact
/// same path directly in the run checkout (`repo`) - untracked, never committed - so
/// `Worktree::land`'s later `git merge --ff-only` in `repo` is refused for local changes, not
/// a content conflict (`merge_into_worktree`, which runs entirely inside the unit's OWN
/// worktree, never even sees it).
struct LandRefusedDriver {
    repo: String,
    stray_content: &'static str,
}

impl AgentDriver for LandRefusedDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join("new.txt"), "A_WORK\n").unwrap();
            std::fs::write(Path::new(&self.repo).join("new.txt"), self.stray_content).unwrap();
            return Ok(AgentResult::default());
        }
        Ok(review_or_adjudicate(opts))
    }
}

#[test]
fn a_land_refused_for_local_changes_names_the_blocking_path_and_charges_no_attempt() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let store = Store::open(":memory:").unwrap();
    let driver = LandRefusedDriver {
        repo: repo_path.clone(),
        stray_content: "STRAY LOCAL CONTENT\n",
    };
    let mut cfg = base_cfg(&repo_path);
    cfg.workflow
        .stages
        .insert("unit-a".into(), mk_stage("unit-a", "g"));

    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let err = match run(&cfg, &deps) {
        Err(e) => e,
        Ok(_) => panic!(
            "a land refusal for local changes must surface as a genuine run() Err, never a \
             silently completed run"
        ),
    };
    assert!(
        err.0.contains("new.txt"),
        "the surfaced error names the blocking path: {}",
        err.0
    );
    assert!(
        err.0.contains("landing refused for local changes"),
        "the refusal is classified as its OWN outcome (`LandOutcome::Blocked`), never relayed \
         as git's raw, unclassified `git merge --ff-only ...` failure text: {}",
        err.0
    );
    assert!(
        !err.0.contains('\u{1}'),
        "the infra-fault marker must be stripped before the error reaches the operator: {}",
        err.0
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == ledger::TYPE_UNIT_FAILED || e.type_ == ledger::TYPE_UNIT_ESCALATED),
        "a land refusal for local changes is a conductor-side infra fault, never the unit's \
         own failure - it must charge NO remediation attempt (no UnitFailed/UnitEscalated)"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == ledger::TYPE_UNIT_INTEGRATED),
        "the blocked unit must never be recorded as integrated"
    );

    let lessons: Vec<String> = events
        .iter()
        .filter(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
        .map(|e| String::from_utf8_lossy(&e.data).to_string())
        .collect();
    assert_eq!(
        lessons.len(),
        1,
        "exactly one lesson must be recorded for the refusal, not zero and not a duplicate \
         per-stage-failure lesson too: {lessons:?}"
    );
    assert!(
        lessons[0].contains("new.txt"),
        "the lesson names the blocking path: {}",
        lessons[0]
    );

    // The unit's own branch keeps exactly its own real work - untouched by the refused
    // landing, still a clean fast-forward candidate once the stray content is cleared.
    let branch_log = git_stdout(&repo_path, &["log", "--oneline", "rigger/u/unit-a"]);
    assert_eq!(
        branch_log.lines().count(),
        2,
        "unit-a's branch must carry exactly its base commit plus its own one real commit; \
         got:\n{branch_log}"
    );
    let a_content = git_stdout(&repo_path, &["show", "rigger/u/unit-a:new.txt"]);
    assert_eq!(
        a_content, "A_WORK",
        "unit-a's own real work survives on its branch"
    );

    // The repo checkout is never touched by the refused landing.
    assert!(
        !repo.path().join(".git").join("MERGE_HEAD").exists(),
        "a refused landing never leaves the repo mid-merge"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("new.txt")).unwrap(),
        "STRAY LOCAL CONTENT\n",
        "the blocking local content itself is left exactly as the operator/other process left \
         it"
    );

    drop(repo);
}

#[test]
fn a_land_refused_names_a_unit_branch_whose_tip_already_holds_identical_content() {
    // A prior (unrelated, already-abandoned) unit branch already carries the exact bytes the
    // refusal is about to block on - proof the blocked local content is not lost work at all,
    // it is already durably captured elsewhere. The lesson must name that branch.
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let stray = "PREVIOUSLY CAPTURED CONTENT\n";
    run_git(&repo_path, &["branch", "rigger/u/unit-old"]);
    let old_wt = tempfile::tempdir().unwrap();
    run_git(
        &repo_path,
        &[
            "worktree",
            "add",
            "-q",
            old_wt.path().to_str().unwrap(),
            "rigger/u/unit-old",
        ],
    );
    std::fs::write(old_wt.path().join("new.txt"), stray).unwrap();
    run_git(old_wt.path().to_str().unwrap(), &["add", "new.txt"]);
    run_git(
        old_wt.path().to_str().unwrap(),
        &["commit", "-q", "-m", "unit-old: lands new.txt"],
    );
    run_git(
        &repo_path,
        &[
            "worktree",
            "remove",
            "--force",
            old_wt.path().to_str().unwrap(),
        ],
    );

    let store = Store::open(":memory:").unwrap();
    let driver = LandRefusedDriver {
        repo: repo_path.clone(),
        stray_content: stray,
    };
    let mut cfg = base_cfg(&repo_path);
    cfg.workflow
        .stages
        .insert("unit-a".into(), mk_stage("unit-a", "g"));

    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    let err = match run(&cfg, &deps) {
        Err(e) => e,
        Ok(_) => panic!("the landing must still be refused"),
    };
    assert!(err.0.contains("new.txt"), "{}", err.0);

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let lesson = events
        .iter()
        .find(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
        .expect("a lesson must be recorded");
    let text = String::from_utf8_lossy(&lesson.data).to_string();
    assert!(
        text.contains("rigger/u/unit-old"),
        "the lesson must name the unit branch whose tip already holds identical content: {text}"
    );

    drop(repo);
}

/// A blocking path git refuses to overwrite is NEVER promised to be a regular file - a FIFO
/// (mirrors `tests/integrate_conflict_merge_periphery.rs::Row4LandCrashDriver`'s `mkfifo`,
/// which blocks the SAME `git merge --ff-only` for the SAME non-content reason) blocks the
/// merge identically. `Worktree::land`/`parse_blocking_paths` never opens the blocking path
/// (they only parse git's own text), but the conductor's lesson-building code, in comparing
/// the blocked LOCAL content against every unit branch's tip, must not either - `std::fs::read`
/// on a FIFO with nothing on its far end to write blocks the calling thread forever, hanging
/// the whole conductor on every future landing.
struct LandRefusedFifoDriver {
    repo: String,
}

impl AgentDriver for LandRefusedFifoDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") {
            std::fs::write(Path::new(&opts.dir).join("new.txt"), "A_WORK\n").unwrap();
            assert!(
                Command::new("mkfifo")
                    .arg(Path::new(&self.repo).join("new.txt"))
                    .status()
                    .unwrap()
                    .success(),
                "test setup: mkfifo must succeed"
            );
            return Ok(AgentResult::default());
        }
        Ok(review_or_adjudicate(opts))
    }
}

#[test]
#[cfg(unix)]
fn a_land_refused_for_a_fifo_blocking_path_never_hangs_and_still_names_it() {
    let repo = init_repo();
    let repo_path = repo.path().to_str().unwrap().to_string();
    let store = Store::open(":memory:").unwrap();
    let driver = LandRefusedFifoDriver {
        repo: repo_path.clone(),
    };
    let mut cfg = base_cfg(&repo_path);
    cfg.workflow
        .stages
        .insert("unit-a".into(), mk_stage("unit-a", "g"));

    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &rigger::gate::ExecRunner,
        repo: repo_path.clone(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    };
    // If the fix regresses, this call hangs forever rather than returning Err - that IS the
    // failure mode this test exists to catch, so it is deliberately a plain call with no
    // internal timeout: a hang here fails the whole suite loudly instead of passing quietly.
    let err = match run(&cfg, &deps) {
        Err(e) => e,
        Ok(_) => panic!("a land refusal for a FIFO-blocked path must still surface as Err"),
    };
    assert!(
        err.0.contains("new.txt"),
        "the surfaced error names the blocking path even though it was never opened: {}",
        err.0
    );

    let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
    let lesson = events
        .iter()
        .find(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
        .expect("a lesson must be recorded");
    let text = String::from_utf8_lossy(&lesson.data).to_string();
    assert!(
        text.contains("new.txt"),
        "the lesson names the blocking path without ever having opened it: {text}"
    );

    drop(repo);
}
