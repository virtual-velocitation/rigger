//! Periphery for spec 89, criterion 1 (A HALT NEVER DISCARDS A TREE), with spec 103's
//! HALTED-SPAWN CHECKPOINT: the seam `RunCtx::run_single_stage` (`src/conductor.rs`) opens onto
//! `Worktree::commit` (`src/worktree.rs`). A halted spawn (one the log shows was requested, that
//! carries no result, with no other spawn of its unit live) leaves its edit uncommitted in the
//! unit's deterministic worktree; the next `rigger run` that adopts the worktree captures it as a
//! `wip(<unit>): tree of halted spawn <id>` commit, and the relaunched implementer's task names
//! that commit and says "finish and report; do not start over".
//!
//! The conductor's own tests prove the algorithm in-process, against a stub driver's in-memory
//! prompts. What they cannot see, and what this file drives through the compiled binary, a real
//! git worktree, a store written before the run starts and the task the fake host was served:
//!
//! - a run over a genuinely halted spawn records exactly one recovery commit, the relaunched
//!   implementer's task names it, and a relaunched run never repeats it;
//! - every shape the checkpoint's guard refuses (no prior worktree, a dirty worktree no spawn
//!   was ever requested for, a live sibling spawn of the unit, a named spawn that already
//!   answered) records no recovery commit and serves no recovery text, while the tree's content
//!   still lands on the unit's branch;
//! - a resumed reviewed unit whose worktree holds a crash-frozen merge conflict reaches the
//!   idempotent merge path instead of the checkpoint's conflict-marker refusal;
//! - conflict-marker lookalike text in a file the unit never touched never blocks a
//!   checkpoint commit.
//!
//! NOT OWNED HERE: `Worktree::commit`'s conflict-marker refusal itself (`src/worktree.rs`'s own
//! `mod tests`), and the `regenerate_conflicted_paths` integration-merge finalize path end to end
//! (`tests/integrate_conflict_merge_periphery.rs`).

use crate::common;

use common::cli::{run_on_arms, seed_run_events};
use common::cli::{write_workflow_fixture, WorkflowFixture, ISOLATED_WORKER};
use common::fixtures::{first_message_task, stand_in_launches, StandInLaunch, RECORD_EACH_LAUNCH};
use common::git::{git_ok, git_out, temp_git_project_with_commit};
use std::path::{Path, PathBuf};

/// A single, real-worktree implementer unit named "solo": one gate (`ok`, always green), no
/// review tier, `on_pass: none` (verified but never merged), the minimal shape that reaches a
/// durable, inspectable unit branch.
const SOLO_UNIT_WORKFLOW: WorkflowFixture = WorkflowFixture {
    worker: ISOLATED_WORKER,
    body: r#"defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: none
"#,
};

/// The spawn every halt below names: the solo unit's first implementer attempt.
const NAMED_SPAWN: &str = "solo/implementer#0";

/// The subject of the recovery commit a run records for [`NAMED_SPAWN`]'s halted tree.
const RECOVERY_SUBJECT: &str = "wip(solo): tree of halted spawn solo/implementer#0";

/// The instruction a relaunched implementer's task carries when its tree was recovered.
const RECOVERY_INSTRUCTION: &str = "do not start over";

/// The worker's arm: each launch on record, it reports and changes nothing.
fn quiet_worker_arm() -> String {
    format!("  *\"Do the unit.\"*)\n{RECORD_EACH_LAUNCH}    echo done\n    ;;\n")
}

/// The deterministic worktree dir of `unit` under the scratch root `scratch`, the product's own
/// [`rigger::conductor::unit_worktree_dir`], where a run adopts it.
fn unit_worktree_dir(scratch: &Path, unit: &str) -> PathBuf {
    PathBuf::from(rigger::conductor::unit_worktree_dir(
        scratch.to_str().expect("a utf-8 scratch root"),
        unit,
    ))
}

fn unit_branch(unit: &str) -> String {
    format!("rigger/u/{unit}")
}

/// A canonical scratch root of its own, kept alive by the returned guard.
fn scratch_root() -> (tempfile::TempDir, PathBuf) {
    let held = tempfile::tempdir().unwrap();
    let scratch = std::fs::canonicalize(held.path()).unwrap();
    (held, scratch)
}

/// What one `rigger run` answered: its stdout, stderr and success, and every launch the fake host
/// put on record, in spawn order.
struct Run {
    out: String,
    err: String,
    ok: bool,
    launches: Vec<StandInLaunch>,
}

impl Run {
    /// The spawn of each launch, in spawn order.
    fn spawns(&self) -> Vec<&str> {
        self.launches.iter().map(|l| l.spawn.as_str()).collect()
    }

    /// The task the one launch of `spawn` was served.
    fn served(&self, spawn: &str) -> String {
        match self
            .launches
            .iter()
            .filter(|l| l.spawn == spawn)
            .collect::<Vec<_>>()[..]
        {
            [StandInLaunch {
                first_message: Some(first),
                ..
            }] => first_message_task(first),
            _ => panic!(
                "one launch of {spawn} recording its task; on record: {:?}",
                self.spawns()
            ),
        }
    }

    /// Whether the run state it printed shows `unit` verified.
    fn verified(&self, unit: &str) -> bool {
        self.out
            .lines()
            .any(|line| line.split_whitespace().collect::<Vec<_>>() == [unit, "verified"])
    }
}

/// `rigger run` over `root` under the scratch root `scratch`, the fake host answering from
/// `arms`.
fn run_over(root: &Path, scratch: &Path, arms: &str) -> Run {
    let (out, err, ok, fake) = run_on_arms(root, &["run"], scratch, arms);
    let on_record = fake.path().join("bin/launches");
    let launches = if on_record.exists() {
        stand_in_launches(&on_record)
    } else {
        Vec::new()
    };
    Run {
        out,
        err,
        ok,
        launches,
    }
}

/// Create unit `unit`'s durable worktree under `scratch` exactly as `Worktree::create`'s
/// branch-does-not-exist-yet path would (`git worktree add -b <branch> <dir> HEAD`), then leave
/// each of `files` uncommitted in it.
fn seed_dirty_worktree(root: &Path, scratch: &Path, unit: &str, files: &[(&str, &str)]) {
    let dir = unit_worktree_dir(scratch, unit);
    git_ok(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &unit_branch(unit),
            dir.to_str().unwrap(),
            "HEAD",
        ],
    );
    for (file, content) in files {
        std::fs::write(dir.join(file), content).unwrap();
    }
    assert!(
        !git_out(&dir, &["status", "--porcelain"]).is_empty(),
        "setup must leave the worktree genuinely dirty"
    );
}

/// The log a driver that died mid-spawn leaves: run `r1` started, unit `solo` started on its
/// branch, [`NAMED_SPAWN`] requested, then each event of `after`.
fn seed_requested_spawn(root: &Path, after: &[(&str, String)]) {
    let named = common::spawn_request("solo", "solo", rigger::spawn::ROLE_IMPLEMENTER, 0, "go");
    let mut events = vec![
        (
            "RunStarted",
            serde_json::json!({"run": "r1", "criteria": []}).to_string(),
        ),
        (
            "UnitStarted",
            serde_json::json!({"id": "solo", "agent": "worker", "branch": unit_branch("solo")})
                .to_string(),
        ),
        (
            rigger::spawn::TYPE_SPAWN_REQUESTED,
            serde_json::to_value(&named).unwrap().to_string(),
        ),
    ];
    events.extend(after.iter().map(|(t, b)| (*t, b.clone())));
    let borrowed: Vec<(&str, &str)> = events.iter().map(|(t, b)| (*t, b.as_str())).collect();
    seed_run_events(root, &borrowed);
}

/// What the solo unit's worktree and log hold as a run over it starts.
#[derive(Clone, Copy)]
enum Prior {
    /// Nothing: the run creates the worktree itself.
    Fresh,
    /// A dirty worktree on disk (a restored snapshot, a desynced store) and no spawn ever
    /// requested for the unit.
    DirtyUnrequested,
    /// [`NAMED_SPAWN`] requested and never answered, its edit left uncommitted: a genuine halt.
    Halted,
    /// The genuine halt, beside a sibling spawn of the same unit (attempt 1, bounded, its
    /// liveness marker fresh) that is still live.
    HaltedBesideALiveSibling,
    /// [`NAMED_SPAWN`] requested and answered, its own edit and a later, unrelated edit (a
    /// reviewer's temporary repro) both uncommitted.
    DirtyAfterTheNamedSpawnAnswered,
}

impl Prior {
    /// Plant this prior state for `root` under `scratch`, returning the files it leaves
    /// uncommitted in the unit's worktree.
    fn plant(self, root: &Path, scratch: &Path) -> Vec<(&'static str, &'static str)> {
        let halted = vec![("halted-work.txt", "abandoned mid-edit\n")];
        match self {
            Prior::Fresh => Vec::new(),
            Prior::DirtyUnrequested => {
                seed_dirty_worktree(root, scratch, "solo", &halted);
                halted
            }
            Prior::Halted => {
                seed_dirty_worktree(root, scratch, "solo", &halted);
                seed_requested_spawn(root, &[]);
                halted
            }
            Prior::HaltedBesideALiveSibling => {
                seed_dirty_worktree(root, scratch, "solo", &halted);
                let mut sibling =
                    common::spawn_request("solo", "solo", rigger::spawn::ROLE_IMPLEMENTER, 1, "go");
                sibling.max_wall_clock = Some(3600);
                seed_requested_spawn(
                    root,
                    &[(
                        rigger::spawn::TYPE_SPAWN_REQUESTED,
                        serde_json::to_value(&sibling).unwrap().to_string(),
                    )],
                );
                let marker =
                    rigger::liveness::marker_path(scratch.to_str().unwrap(), "r1", &sibling.id)
                        .expect("a non-degenerate spawn id encodes a marker path");
                std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
                std::fs::write(&marker, b"heartbeat").unwrap();
                halted
            }
            Prior::DirtyAfterTheNamedSpawnAnswered => {
                let files = vec![
                    ("work.rs", "pub fn work() {}\n"),
                    ("reviewer-repro.txt", "temporary red repro\n"),
                ];
                seed_dirty_worktree(root, scratch, "solo", &files);
                seed_requested_spawn(
                    root,
                    &[(
                        "SpawnResult",
                        format!(r#"{{"id":"{NAMED_SPAWN}","output":"implemented"}}"#),
                    )],
                );
                files
            }
        }
    }

    /// The spawns a run over this prior state launches: none once the named spawn answered,
    /// else the named spawn itself, relaunched or fresh.
    fn launches(self) -> Vec<&'static str> {
        match self {
            Prior::DirtyAfterTheNamedSpawnAnswered => Vec::new(),
            _ => vec![NAMED_SPAWN],
        }
    }
}

/// The sha of each recovery commit `branch` carries, newest first.
fn recovery_commits(root: &Path, branch: &str) -> Vec<String> {
    git_out(root, &["log", "--pretty=%H %s", branch])
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(_, subject)| *subject == RECOVERY_SUBJECT)
        .map(|(sha, _)| sha.to_string())
        .collect()
}

/// Given the solo unit at `prior`, when `rigger run` drives it, then the run verifies the unit;
/// it records exactly one recovery commit when `recovered` and none otherwise; every file the
/// prior state left uncommitted lands on the unit's branch byte for byte (in the recovery commit
/// when there is one) and the worktree is left clean; the run launches exactly the spawns the
/// prior state leaves owed, and each one's task names the recovery commit and says "do not
/// start over" exactly when `recovered`; and a relaunched run over the finished run moves the
/// branch no further and repeats no recovery commit.
fn a_run_over_a_solo_unit(prior: Prior, recovered: bool) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_workflow_fixture(root, &SOLO_UNIT_WORKFLOW);
    let (_held, scratch) = scratch_root();
    let left = prior.plant(root, &scratch);
    let branch = unit_branch("solo");

    let run = run_over(root, &scratch, &quiet_worker_arm());
    assert!(
        run.ok && run.verified("solo"),
        "the run verifies the unit; stdout: {}\nstderr: {}",
        run.out,
        run.err
    );

    let recovery = recovery_commits(root, &branch);
    assert_eq!(
        recovery.len(),
        usize::from(recovered),
        "recovery commits on {branch}; log:\n{}",
        git_out(root, &["log", "--pretty=%s", &branch])
    );
    let landed_in = recovery.first().cloned().unwrap_or_else(|| branch.clone());
    for (file, content) in &left {
        assert_eq!(
            git_out(root, &["show", &format!("{landed_in}:{file}")]),
            content.trim_end(),
            "{file} lands on {landed_in} byte for byte, never discarded"
        );
    }
    let worktree = unit_worktree_dir(&scratch, "solo");
    assert!(
        !worktree.exists() || git_out(&worktree, &["status", "--porcelain"]).is_empty(),
        "the run leaves nothing uncommitted in the unit's worktree"
    );

    assert_eq!(
        run.spawns(),
        prior.launches(),
        "the spawns the run launches"
    );
    for spawn in prior.launches() {
        let task = run.served(spawn);
        assert_eq!(
            (
                task.contains(RECOVERY_INSTRUCTION),
                recovery
                    .first()
                    .is_some_and(|sha| task.contains(sha.as_str())),
            ),
            (recovered, recovered),
            "{spawn}'s task names the recovery commit and says not to start over exactly when \
             the tree was recovered; task:\n{task}"
        );
    }

    let tip = git_out(root, &["rev-parse", &branch]);
    let relaunched = run_over(root, &scratch, &quiet_worker_arm());
    assert!(
        relaunched.ok,
        "the relaunched run succeeds; stderr: {}",
        relaunched.err
    );
    assert_eq!(
        (
            git_out(root, &["rev-parse", &branch]),
            recovery_commits(root, &branch)
        ),
        (tip, recovery),
        "a relaunched run over the finished run moves the branch no further and repeats no \
         recovery commit"
    );
}

rigger::test_cases! {
    /// A genuine halt: the run records the recovery commit holding the abandoned edit, and the
    /// relaunched implementer is told to finish from it.
    a_halted_units_worktree_is_recovered_as_a_wip_commit_and_the_relaunched_prompt_names_it:
        a_run_over_a_solo_unit(Prior::Halted, true);
    /// The common path: a worktree the run creates itself gains no recovery commit, and its
    /// implementer no recovery text.
    an_ordinary_freshly_created_worktree_never_gains_a_halt_recovery_commit_or_prompt_text:
        a_run_over_a_solo_unit(Prior::Fresh, false);
    /// A dirty worktree no spawn was ever requested for is no halt: nothing is recorded as one,
    /// and its content is kept, landing on the unit's branch.
    a_declared_units_dirty_worktree_with_no_requested_spawn_keeps_its_content_without_a_wip_commit:
        a_run_over_a_solo_unit(Prior::DirtyUnrequested, false);
    /// A live sibling spawn of the unit suppresses the capture though the named spawn alone
    /// would be a halt.
    a_dirty_tree_gets_no_wip_recovery_commit_while_a_sibling_spawn_of_the_unit_is_still_live:
        a_run_over_a_solo_unit(Prior::HaltedBesideALiveSibling, false);
    /// A named spawn that already answered is no halt, however dirty a later edit left the tree.
    a_dirty_tree_gets_no_wip_recovery_commit_once_the_named_spawn_already_has_a_real_result:
        a_run_over_a_solo_unit(Prior::DirtyAfterTheNamedSpawnAnswered, false);
}

/// A resumed reviewed unit whose worktree already carries a genuine, unresolved merge conflict
/// from a prior run's crash reaches the idempotent `merge_into_worktree`/`merge_in_progress`
/// resume path (spec 88) and never trips the halted-commit capture's conflict-marker refusal,
/// which would turn a resumable state into a hard error.
///
/// The pre-crash state is built through the public `Worktree` API (`create` +
/// `merge_into_worktree`): a run that reached this state and then errored would tear its own
/// worktree down, so a crash is the only way the shape arises. The run adopts it from a store
/// written before it started.
#[test]
fn a_resumed_reviewed_units_real_crash_frozen_merge_conflict_reaches_the_idempotent_path() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    for (id, body) in [("worker", "Do the unit."), ("judge", "Adjudicate it.")] {
        std::fs::write(
            rigger.join("agents").join(format!("{id}.md")),
            format!("---\nid: {id}\nmodel: sonnet\ntools: [Read, Edit]\n---\n{body}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        rigger.join("workflow.yml"),
        "defaults:\n  grounder: nop\n  budget: 60\n\
         gates:\n  ok: { run: \"true\", kind: core }\n\
         regenerate:\n  - paths: [\"shared.rs\"]\n    run: \"printf 'REGENERATED\\n' > shared.rs\"\n\
         stages:\n  s:\n    agent: worker\n    gates: [ok]\n    on_pass: merge\n    \
         review:\n      adjudicator: judge\n",
    )
    .unwrap();
    let (_held, scratch) = scratch_root();

    // The unit's own deterministic worktree and branch, never removed, so the run adopts this
    // exact on-disk state.
    let wt_dir = unit_worktree_dir(&scratch, "s");
    let unit_wt = rigger::worktree::Worktree::create(
        root.to_str().unwrap(),
        wt_dir.to_str().unwrap(),
        &unit_branch("s"),
        "",
    )
    .unwrap();
    std::fs::write(wt_dir.join("shared.rs"), "UNIT VERSION\n").unwrap();
    assert!(
        !unit_wt
            .commit("rigger: prior window work")
            .unwrap()
            .is_empty(),
        "the prior run must commit the approved work"
    );

    // The checked-out repo gains a different version of the same path since: what an
    // already-integrated batch-mate would have landed by the time this unit's merge runs.
    std::fs::write(root.join("shared.rs"), "RUN VERSION\n").unwrap();
    git_ok(root, &["add", "shared.rs"]);
    git_ok(
        root,
        &[
            "commit",
            "-q",
            "-m",
            "a batch-mate's own conflicting change",
        ],
    );

    // The prior run's integration ran this merge and died before conflict resolution, leaving
    // MERGE_HEAD and literal conflict-marker text in `shared.rs`.
    match unit_wt
        .merge_into_worktree("rigger: integrate s (interrupted before this ever finished)")
        .unwrap()
    {
        rigger::worktree::MergeOutcome::Conflict(paths) => {
            assert_eq!(
                paths,
                ["shared.rs"],
                "setup must conflict on shared.rs alone"
            );
        }
        rigger::worktree::MergeOutcome::Ready(_) => {
            panic!("setup premise: the divergent shared.rs edits must conflict")
        }
    }
    assert!(
        unit_wt.merge_in_progress(),
        "setup must leave a genuine merge in progress, unresolved"
    );
    assert!(
        std::fs::read_to_string(wt_dir.join("shared.rs"))
            .unwrap()
            .contains("<<<<<<<"),
        "setup must leave real conflict-marker text in place"
    );

    // The prior run's recorded lifecycle: implemented, verified and adjudicator-approved.
    seed_run_events(
        root,
        &[
            (
                "RunStarted",
                &serde_json::json!({"run": "r1", "criteria": []}).to_string(),
            ),
            (
                "UnitStarted",
                &serde_json::json!({"id": "s", "agent": "worker", "branch": unit_branch("s")})
                    .to_string(),
            ),
            (
                "UnitStatus",
                &serde_json::json!({"id": "s", "status": "verified"}).to_string(),
            ),
            (
                "UnitStatus",
                &serde_json::json!({"id": "s", "status": "reviewed"}).to_string(),
            ),
        ],
    );

    let arms = format!(
        "  *\"Do the unit.\"*|*\"Adjudicate it.\"*)\n{RECORD_EACH_LAUNCH}    echo done\n    ;;\n"
    );
    let run = run_over(root, &scratch, &arms);
    assert!(
        run.ok,
        "a resumed unit's in-progress merge conflict reaches the idempotent merge path, never \
         the halted-commit capture's conflict-marker refusal; stdout: {}\nstderr: {}",
        run.out, run.err
    );
    assert_eq!(
        run.spawns(),
        Vec::<&str>::new(),
        "a conflict confined to a registered regenerable path resolves with no implementer or \
         adjudicator launched, so the conflict-handling path ran rather than a fallback"
    );
    assert_eq!(
        git_out(root, &["show", "rigger-run:shared.rs"]),
        "REGENERATED",
        "the run branch carries the regeneration, not a discarded or half-merged tree"
    );
    assert_eq!(
        git_out(root, &["log", "--pretty=%P", "-1", "rigger-run"])
            .split_whitespace()
            .count(),
        2,
        "the run branch's tip is the regenerate-and-merge commit, with two parents"
    );
    assert!(
        !git_out(root, &["log", "--pretty=%s", "rigger-run"]).contains("wip(s):"),
        "the capture skipped the conflicted tree: no recovery commit"
    );
}

/// `Worktree::commit`'s conflict-marker scan is confined to the caller's own touched or unmerged
/// paths: conflict-marker lookalike text in a tracked file the unit never touched never blocks
/// the per-attempt checkpoint commit. No worktree exists before the run, so only the checkpoint
/// commit's own scope guard is exercised.
#[test]
fn an_untouched_conflict_marker_lookalike_file_never_blocks_an_unrelated_checkpoint_commit() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_workflow_fixture(root, &SOLO_UNIT_WORKFLOW);
    // Committed before the unit's worktree branches and never edited by the unit: prose
    // showing what a real conflict looks like.
    let lookalike =
        "Git conflict markers look like:\n<<<<<<< ours\nold\n=======\nnew\n>>>>>>> theirs\n";
    std::fs::write(root.join("conflict-markers-doc.txt"), lookalike).unwrap();
    git_ok(root, &["add", "conflict-markers-doc.txt"]);
    git_ok(root, &["commit", "-q", "-m", "document conflict markers"]);
    let (_held, scratch) = scratch_root();

    // The implementer's own edit, a different file, left uncommitted for the run's checkpoint.
    let arms = format!(
        "  *\"Do the unit.\"*)\n{RECORD_EACH_LAUNCH}    printf 'pub fn work() {{}}\\n' > work.rs\n    echo implemented\n    ;;\n"
    );
    let run = run_over(root, &scratch, &arms);
    assert!(
        run.ok && run.verified("solo"),
        "the checkpoint commit is never blocked by lookalike text in a file the unit never \
         touched, so the unit verifies; stdout: {}\nstderr: {}",
        run.out,
        run.err
    );
    assert_eq!(
        run.spawns(),
        [NAMED_SPAWN],
        "the run launches the implementer once"
    );
    assert_eq!(
        git_out(root, &["show", &format!("{}:work.rs", unit_branch("solo"))]),
        "pub fn work() {}",
        "the implementer's edit lands in the checkpoint commit"
    );
    assert_eq!(
        git_out(
            root,
            &[
                "show",
                &format!("{}:conflict-markers-doc.txt", unit_branch("solo"))
            ]
        ),
        lookalike.trim_end(),
        "the untouched lookalike file's content is unaffected"
    );
}
