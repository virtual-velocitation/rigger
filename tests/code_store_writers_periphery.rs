//! Periphery for the code store's writers and its stamp: the conductor advances the store right
//! after a merge lands, `rigger graph build` and `rigger setup` advance it to the head, and
//! `rigger validate` prints the stamp line. Whichever worktree a verb runs in, the store is the
//! one at the operator checkout and describes that checkout's HEAD.

use crate::common;
use common::cli::{run_rigger, run_rigger_ok};
use common::fixtures::{
    assert_outside_any_repository, git_commit_all, git_out, init_repo, review_or_adjudicate,
    test_spawn_host, write_files, MergeBreakDriver, MERGE_BREAK_BASE,
};

use rigger::code_store::CodeStore;
use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Gate, ReviewPanel, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger::Status;
use rigger_domain::code_facts::Stamp;
use rigger_domain::code_view::AdvanceCodeStore;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Readies a fixture repository at its root before the verb under test runs.
type Ready = fn(&Path);

/// The source file every fixture repository starts with.
const SOURCE: (&str, &str) = ("src/lib.rs", "pub fn product() {}\n");

/// The stamp of the snapshot in force in the store of the repository at `root`.
fn stamp_at(root: &Path) -> Stamp {
    CodeStore::in_repository(root)
        .in_force()
        .unwrap_or_else(|| panic!("the store at {} holds a snapshot", root.display()))
}

/// The commit the checkout at `root` has checked out.
fn head_of(root: &Path) -> String {
    git_out(root, &["rev-parse", "HEAD"])
}

/// A repository holding [`SOURCE`] in a commit, its root canonical.
fn source_repository() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    init_repo(&root);
    write_files(&root, &[(SOURCE.0, SOURCE.1)]);
    git_commit_all(&root, "source");
    (dir, root)
}

/// A [`source_repository`] that `rigger setup` provisioned, everything setup wrote committed, so
/// the checkout is clean.
fn provisioned_repository() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = source_repository();
    run_rigger_ok(&root, &["setup"]);
    git_commit_all(&root, "setup");
    (dir, root)
}

/// Forwards to the real store, first recording the head the repository at `repo` has checked out
/// at each call.
struct RecordingStore {
    repo: PathBuf,
    store: CodeStore,
    heads: Mutex<Vec<String>>,
}

impl AdvanceCodeStore for RecordingStore {
    fn advance_to_head(&self) -> Result<(), String> {
        self.heads.lock().unwrap().push(head_of(&self.repo));
        self.store.advance_to_head()
    }
}

/// Writes one new file into the implementer's worktree; every reviewer approves.
struct LandingDriver;

impl AgentDriver for LandingDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if opts.id.contains("/implementer#") && !opts.dir.is_empty() {
            write_files(
                Path::new(&opts.dir),
                &[("src/landed.rs", "pub fn landed() {}\n")],
            );
            return Ok(AgentResult::default());
        }
        Ok(review_or_adjudicate(opts))
    }
}

/// A run over `units`, each a merging stage reviewed by one lens and an adjudicator, gated by
/// `gate`, which is re-run on the merged tree.
fn merging_run(repo: &Path, units: &[&str], gate: &str) -> Config {
    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo);
    cfg.workflow.defaults.max_retries = 1;
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
            run: gate.into(),
            kind: "core".into(),
            inputs: Vec::new(),
            requires: Vec::new(),
            post_merge: true,
        },
    );
    for unit in units {
        cfg.workflow.stages.insert(
            (*unit).into(),
            Stage {
                name: (*unit).into(),
                agent: "worker".into(),
                gates: vec!["g".into()],
                on_pass: "merge".into(),
                review: ReviewPanel {
                    lenses: vec!["lens".into()],
                    adjudicator: "judge".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
    }
    cfg
}

/// Each row drives a real run and counts the merges that landed. The store is advanced once per
/// landed merge, with the operator checkout already at the landed commit, and a merge the
/// post-merge gate blocks advances nothing: the two batch-mates each add one MARK to `m.rs`, and
/// the gate refuses the tree holding both, so the second merge is rolled back.
#[test]
fn the_conductor_advances_the_store_after_a_landed_merge() {
    struct Row {
        name: &'static str,
        units: &'static [&'static str],
        gate: &'static str,
        driver: fn(&Path) -> Box<dyn AgentDriver>,
        landed: usize,
    }
    let rows = [
        Row {
            name: "one landed merge",
            units: &["unit-a"],
            gate: "true",
            driver: |_| Box::new(LandingDriver),
            landed: 1,
        },
        Row {
            name: "a merge the post-merge gate blocks",
            units: &["unit-a", "unit-b"],
            gate: "c=$(grep -c MARK m.rs 2>/dev/null); c=${c:-0}; [ \"$c\" -le 1 ]",
            driver: |repo| {
                Box::new(MergeBreakDriver {
                    repo: repo.to_str().unwrap().to_string(),
                })
            },
            landed: 1,
        },
    ];
    for row in rows {
        let (_dir, repo) = source_repository();
        write_files(&repo, &[("m.rs", MERGE_BREAK_BASE)]);
        git_commit_all(&repo, "base");
        let base = head_of(&repo);
        let recording = RecordingStore {
            repo: repo.clone(),
            store: CodeStore::in_repository(&repo),
            heads: Mutex::new(Vec::new()),
        };
        let store = Store::open(":memory:").unwrap();
        let driver = (row.driver)(&repo);
        let deps = Deps {
            store: &store,
            driver: driver.as_ref(),
            gates: &ExecRunner,
            repo: repo.to_str().unwrap().to_string(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
            log: &|_| {},
            check_emit: &rigger::contextgraph::check_emit,
            spawn_host: test_spawn_host(),
            code_store: Some(&recording),
            context: None,
        };
        let rs = run(&merging_run(&repo, row.units, row.gate), &deps).unwrap();
        let integrated = rs
            .units
            .values()
            .filter(|u| u.status == Status::Integrated)
            .count();
        assert_eq!(integrated, row.landed, "{}: {:?}", row.name, rs.units);
        let tip = head_of(&repo);
        assert_ne!(tip, base, "{}: a merge landed", row.name);
        assert_eq!(
            *recording.heads.lock().unwrap(),
            vec![tip.clone()],
            "{}: one advance per landed merge, each at the landed commit",
            row.name
        );
        assert_eq!(
            stamp_at(&repo).tip.commit.as_deref(),
            Some(tip.as_str()),
            "{}: the store describes the landed commit",
            row.name
        );
    }
}

/// Each row readies a repository, then `rigger graph build` runs there; the store describes the
/// head after it, from empty and after a new commit alike.
#[test]
fn graph_build_advances_to_head() {
    let rows: [(&str, Ready); 2] = [
        ("from empty", |_| {}),
        ("after a new commit", |root| {
            run_rigger_ok(root, &["graph", "build"]);
            write_files(root, &[("src/more.rs", "pub fn more() {}\n")]);
            git_commit_all(root, "more");
        }),
    ];
    for (name, ready) in rows {
        let (_dir, root) = source_repository();
        ready(&root);
        run_rigger_ok(&root, &["graph", "build"]);
        let head = head_of(&root);
        assert_eq!(
            stamp_at(&root).tip.commit.as_deref(),
            Some(head.as_str()),
            "{name}: graph build advances the store to the head"
        );
    }
}

/// Each row runs `rigger setup` twice where no store exists: the first advances the store from
/// empty to the head the project's source names and prints the stamp line, and the second, with
/// nothing changed, prints nothing.
#[test]
fn setup_advances_from_empty() {
    struct Row {
        name: &'static str,
        project: fn(&Path),
        store: fn(&Path) -> CodeStore,
        commit: fn(&Path) -> Option<String>,
    }
    let rows = [
        Row {
            name: "a repository",
            project: |root| {
                init_repo(root);
                write_files(root, &[(SOURCE.0, SOURCE.1)]);
                git_commit_all(root, "source");
            },
            store: CodeStore::in_repository,
            commit: |root| Some(head_of(root)),
        },
        Row {
            name: "a directory with no git",
            project: |root| {
                assert_outside_any_repository(root);
                write_files(root, &[(SOURCE.0, SOURCE.1)]);
            },
            store: CodeStore::in_directory,
            commit: |_| None,
        },
    ];
    for row in rows {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        (row.project)(&root);
        let store = (row.store)(&root);
        assert!(
            store.in_force().is_none(),
            "{}: the store starts empty",
            row.name
        );
        let first = run_rigger_ok(&root, &["setup"]);
        let stamp = store
            .in_force()
            .unwrap_or_else(|| panic!("{}: setup advances the store", row.name));
        assert_eq!(stamp.tip.commit, (row.commit)(&root), "{}", row.name);
        let described = stamp.tip.commit.as_deref().unwrap_or(&stamp.tip.tree);
        assert!(
            first.contains(&format!("code store: describes {described}")),
            "{}: setup prints the stamp line; stdout: {first}",
            row.name
        );
        let rerun = run_rigger_ok(&root, &["setup"]);
        assert_eq!(
            rerun, "",
            "{}: a rerun with nothing changed is silent",
            row.name
        );
    }
}

/// Each row runs a verb in a unit worktree whose own branch carries one commit more than the
/// operator checkout: the store at the operator checkout describes the operator checkout's HEAD,
/// never the worktree's, and no store is made in the worktree. The reader's stamp line counts the
/// one file the worktree differs by.
#[test]
fn a_reader_in_a_unit_worktree_advances_from_the_operator_checkout_head() {
    let rows: [(&str, &[&str], bool); 2] = [
        ("a writer, graph build", &["graph", "build"], false),
        ("a reader, validate", &["validate"], true),
    ];
    for (name, verb, prints_the_count) in rows {
        let (_dir, operator) = provisioned_repository();
        let units = tempfile::tempdir().unwrap();
        let unit = units.path().canonicalize().unwrap().join("unit");
        git_out(
            &operator,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "unit",
                unit.to_str().unwrap(),
            ],
        );
        write_files(&unit, &[(SOURCE.0, "pub fn product() { changed() }\n")]);
        git_commit_all(&unit, "unit work");
        let out = run_rigger_ok(&unit, verb);
        let operator_head = head_of(&operator);
        assert_ne!(
            operator_head,
            head_of(&unit),
            "{name}: fixture precondition"
        );
        assert_eq!(
            stamp_at(&operator).tip.commit.as_deref(),
            Some(operator_head.as_str()),
            "{name}: the store describes the operator checkout's HEAD"
        );
        assert!(
            !unit.join(".rigger/store/code").exists(),
            "{name}: no store is made in the unit worktree"
        );
        if prints_the_count {
            let line = format!(
                "code store: describes {operator_head}; 1 file in {} differs",
                unit.display()
            );
            assert!(
                out.lines().any(|l| l == line),
                "{name}: the stamp line `{line}`; stdout: {out}"
            );
        }
    }
}

/// Each row readies a provisioned checkout and runs `rigger validate` there: it prints the line
/// naming the commit the store describes and, when the checkout differs from it, how many files
/// differ.
#[test]
fn validate_prints_the_stamp_line() {
    let rows: [(&str, Ready, &str); 3] = [
        ("a clean checkout", |_| {}, ""),
        (
            "a modified file",
            |root| write_files(root, &[(SOURCE.0, "pub fn product() { edited() }\n")]),
            "; 1 file in {root} differs",
        ),
        (
            "a modified and an untracked file",
            |root| {
                write_files(root, &[(SOURCE.0, "pub fn product() { edited() }\n")]);
                write_files(root, &[("src/new.rs", "pub fn new() {}\n")]);
            },
            "; 2 files in {root} differ",
        ),
    ];
    for (name, ready, differ) in rows {
        let (_dir, root) = provisioned_repository();
        ready(&root);
        let (out, err, ok) = run_rigger(&root, &["validate"]);
        assert!(ok, "{name}: validate succeeds; stderr: {err}");
        let line = format!(
            "code store: describes {}{}",
            head_of(&root),
            differ.replace("{root}", &root.display().to_string())
        );
        assert!(
            out.lines().any(|l| l == line),
            "{name}: validate prints `{line}`; stdout: {out}"
        );
    }
}
