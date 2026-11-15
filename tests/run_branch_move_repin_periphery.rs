//! A run's own moves of the run branch re-pin its definition, so the next launch reads none of
//! them as definition drift. `rigger run`'s launch hashes the checkout's definition and halts when
//! the hash differs from the run's pin ([`rigger::run_store::ensure_started_pinned`]); the
//! conductor re-pins after every move of the run branch that changes that definition. A unit's
//! merge is driven through the CLI in `tests/cli.rs`
//! (`a_run_that_lands_its_own_definition_edit_relaunches_without_drift`); the other three moves
//! (a red post-merge gate's rollback, a compensating revert and a producer's plan commits) need a
//! two-unit merge, a review verdict naming an integrated unit, or a producer's own commits, which
//! this file drives through `conductor::run` with real git and real gate commands.

use crate::common;

use common::fixtures::{
    agent, git_commit_all, mk_stage, plan_stage, review_or_adjudicate,
    temp_git_project_with_commit, test_spawn_host, write_files, MergeBreakDriver, MERGE_BREAK_BASE,
};
use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::run::RunStart;
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The agent file a unit's implementer adds to the project's definition, as a unit whose change
/// edits an agent prompt does.
fn definition_edit(project: &str, unit: &str) -> String {
    let at = if project.is_empty() {
        String::new()
    } else {
        format!("{project}/")
    };
    format!("{at}.rigger/agents/{unit}-note.md")
}

/// What a unit's implementer writes into the definition: one fixed text, so a re-attempt writes
/// the same bytes.
const EDITED: &str = "---\nid: note\n---\nAdded by the unit.\n";

/// Two batch-mates that each add a MARK to `m.rs` ([`MergeBreakDriver`]) and each add its own
/// agent file, so whichever merges second lands a definition change the post-merge gate then
/// rolls back.
struct RolledBackDriver {
    marks: MergeBreakDriver,
}

impl AgentDriver for RolledBackDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let answer = self.marks.spawn(a, prompt, opts, emit)?;
        if opts.id.contains("/implementer#") && !opts.dir.is_empty() {
            let unit = opts.id.split('/').next().unwrap_or_default();
            write_files(
                Path::new(&opts.dir),
                &[(&definition_edit("", unit), EDITED)],
            );
        }
        Ok(answer)
    }
}

/// unit-a lands its own file and, on its first attempt only, an agent file; unit-b's adjudicator
/// approves unit-b and names unit-a as the defect source, so unit-a's landing is reverted and
/// unit-a re-lands without the definition change.
#[derive(Default)]
struct CompensatedDriver {
    edited: AtomicBool,
}

impl AgentDriver for CompensatedDriver {
    fn spawn(
        &self,
        _a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let unit = opts.id.split('/').next().unwrap_or_default();
        if opts.id.contains("/implementer#") {
            if !opts.dir.is_empty() {
                let dir = Path::new(&opts.dir);
                write_files(dir, &[(&format!("{unit}.rs"), "fn work() {}\n")]);
                if unit == "unit-a" && !self.edited.swap(true, Ordering::SeqCst) {
                    write_files(dir, &[(&definition_edit("", unit), EDITED)]);
                }
            }
            return Ok(AgentResult::default());
        }
        if unit == "unit-b" && opts.id.contains("/adjudicator#") {
            return Ok(AgentResult {
                output: r#"{"verdict":"approve","compensate":"unit-a"}"#.into(),
                ..Default::default()
            });
        }
        Ok(review_or_adjudicate(opts))
    }
}

/// A `plan` producer that commits an agent file of the project under `specs/` with its own git
/// access on its first spawn, the commit the conductor cherry-picks onto the run branch.
struct PlanCommitDriver {
    project: &'static str,
    committed: AtomicBool,
}

impl AgentDriver for PlanCommitDriver {
    fn spawn(
        &self,
        a: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if a.id == "planner" && !opts.dir.is_empty() && !self.committed.swap(true, Ordering::SeqCst)
        {
            let dir = Path::new(&opts.dir);
            write_files(dir, &[(&definition_edit(self.project, "plan"), EDITED)]);
            for args in [
                &["add", "-A"][..],
                &["commit", "-q", "-m", "amend the plan"],
            ] {
                let _ = Command::new("git").arg("-C").arg(dir).args(args).output();
            }
        }
        Ok(AgentResult {
            output: format!("{} ok", a.id),
            ..Default::default()
        })
    }
}

/// A `worker` stage gated by `g` and reviewed, needing `needs`.
fn unit(name: &str, needs: &[&str]) -> Stage {
    Stage {
        needs: needs.iter().map(|n| (*n).to_string()).collect(),
        ..mk_stage(name, "g")
    }
}

/// A config over `repo` declaring `agents`, the gate `g` running `gate` (re-run on the merged
/// tree) and `stages`, loaded from the project at `project`.
fn config(repo: &Path, project: &Path, agents: &[&str], gate: &str, stages: Vec<Stage>) -> Config {
    let mut cfg = Config::default();
    cfg.dir = project.to_str().unwrap().to_string();
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo);
    cfg.workflow.defaults.max_retries = 1;
    for id in agents {
        cfg.agents.insert((*id).into(), agent(id));
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
    for stage in stages {
        cfg.workflow.stages.insert(stage.name.clone(), stage);
    }
    cfg
}

/// The definition the checkout at `repo` holds for the project at `project`, hashed as a launch
/// of the run hashes it.
fn definition_of(repo: &Path, project: &Path) -> String {
    let helpers = rigger::hooks::resolved_helpers(repo).unwrap();
    rigger::config_store::definition_hash(project.to_str().unwrap(), &helpers).unwrap()
}

/// The `(from, to)` hashes of each re-pin line the run logged, in order.
fn re_pins(said: Vec<String>) -> Vec<(String, String)> {
    said.iter()
        .filter_map(|line| line.split_once(" re-pinned run ")?.1.split_once(" from "))
        .filter_map(|(_, hashes)| hashes.split_once(" to "))
        .map(|(from, to)| (from.to_string(), to.to_string()))
        .collect()
}

/// Each row drives a run pinned to the definition its base commit holds through one move of the
/// run branch that changes that definition, then asks the launch's own decision whether the next
/// `rigger run` adopts the run or halts on drift. A rollback and a revert move the definition
/// back to one a landing moved it from, so each re-pins back to it; a plan commit lands a new one.
#[test]
fn a_run_branch_move_that_changes_the_definition_re_pins_the_run_so_its_next_launch_adopts_it() {
    struct Row {
        name: &'static str,
        project: &'static str,
        config: fn(&Path, &Path) -> Config,
        driver: fn(&Path) -> Box<dyn AgentDriver>,
        rolls_back: bool,
    }
    let rows = [
        Row {
            name: "a red post-merge gate's rollback",
            project: "",
            config: |repo, project| {
                config(
                    repo,
                    project,
                    &["worker", "lens", "judge"],
                    "c=$(grep -c MARK m.rs 2>/dev/null); c=${c:-0}; [ \"$c\" -le 1 ]",
                    vec![unit("unit-a", &[]), unit("unit-b", &[])],
                )
            },
            driver: |repo| {
                Box::new(RolledBackDriver {
                    marks: MergeBreakDriver {
                        repo: repo.to_str().unwrap().to_string(),
                    },
                })
            },
            rolls_back: true,
        },
        Row {
            name: "a compensating revert",
            project: "",
            config: |repo, project| {
                config(
                    repo,
                    project,
                    &["worker", "lens", "judge"],
                    "exit 0",
                    vec![unit("unit-a", &[]), unit("unit-b", &["unit-a"])],
                )
            },
            driver: |_| Box::new(CompensatedDriver::default()),
            rolls_back: true,
        },
        Row {
            name: "a producer's plan commit",
            project: "specs",
            config: |repo, project| {
                config(repo, project, &["planner"], "exit 0", vec![plan_stage()])
            },
            driver: |_| {
                Box::new(PlanCommitDriver {
                    project: "specs",
                    committed: AtomicBool::new(false),
                })
            },
            rolls_back: false,
        },
    ];
    for row in rows {
        let dir = temp_git_project_with_commit();
        let repo = dir.path();
        let project = repo.join(row.project);
        let at = |rel: &str| {
            if row.project.is_empty() {
                rel.to_string()
            } else {
                format!("{}/{rel}", row.project)
            }
        };
        write_files(
            repo,
            &[
                ("m.rs", MERGE_BREAK_BASE),
                (
                    &at(".rigger/workflow.yml"),
                    "# the definition the run pins\n",
                ),
                (
                    &at(".rigger/agents/worker.md"),
                    "---\nid: worker\n---\nDo the unit.\n",
                ),
            ],
        );
        git_commit_all(repo, "the definition");
        let start = definition_of(repo, &project);
        let store = Store::open(":memory:").unwrap();
        let RunStart::Ready(pinned) =
            rigger::run_store::ensure_started_pinned(&store, &[], &start, false, "", "", "")
                .unwrap()
        else {
            panic!("{}: a first launch mints a run", row.name);
        };

        let said = Mutex::new(Vec::<String>::new());
        let log = |line: &str| said.lock().unwrap().push(line.to_string());
        let driver = (row.driver)(repo);
        let deps = Deps {
            store: &store,
            driver: driver.as_ref(),
            gates: &ExecRunner,
            repo: repo.to_str().unwrap().to_string(),
            grounder: None,
            graph: None,
            criteria: Vec::new(),
            log: &log,
            check_emit: &rigger::contextgraph::check_emit,
            spawn_host: test_spawn_host(),
            code_store: None,
            context: None,
        };
        run(&(row.config)(repo, &project), &deps).unwrap();

        let repins = re_pins(said.into_inner().unwrap());
        let moved = if row.rolls_back {
            repins
                .iter()
                .enumerate()
                .any(|(i, (from, _))| repins[i + 1..].iter().any(|(_, to)| to == from))
        } else {
            repins.iter().any(|(from, _)| *from == start)
        };
        assert!(
            moved,
            "{}: the run's move re-pinned its definition; re-pins: {repins:?}",
            row.name
        );
        let now = definition_of(repo, &project);
        assert_eq!(
            rigger::run_store::ensure_started_pinned(&store, &[], &now, false, "", "", "").unwrap(),
            RunStart::Ready(pinned),
            "{}: the next launch adopts the run with no definition drift; re-pins: {repins:?}",
            row.name
        );
    }
}
