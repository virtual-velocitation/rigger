//! Periphery for `rigger hook stop --spawn <id> --dir <dir> (--branch <branch> | --own-commits)`,
//! the `Stop` hook a writing session carries: drives the REAL COMPILED binary, with Claude Code's
//! `Stop` payload on its stdin, against a real `.rigger/progress.db` and a real git tree left in
//! each state the conductor's next step on it refuses or accepts.
//!
//! OWNED HERE: the hook's answer in Claude Code's contract (a `{"decision": "block", "reason"}`
//! object on stdout to hold the session, nothing to allow the stop, exit 0 either way), git's own
//! status words in the reason, the progress lines it records, its bound per launch, its reading of
//! `stop_hook_active`, and that every error allows the stop.
//!
//! NOT OWNED HERE: which refusals a tree has ([`rigger::worktree::checkpoint_refusals`]'s and
//! [`rigger::worktree::own_commits_refusals`]'s own tests), which spawns carry the hook (the settings assertions of the launch tests in `cli.rs`
//! and `hooks.rs`'s own tests), and a session held then fixing its tree end to end (`cli.rs`).

use crate::common;
use common::cli::{courier_project, recorded_progress, seed_a_run, temp_project, with_store_file};
use common::git::{git_ok, git_ok_with_identity, git_out, run_git, temp_git_project_with_commit};

use std::io::Write;
use std::path::Path;
use std::process::{Output, Stdio};

use rigger::progress::{AgentProgress, SpawnLaunched, STOP_HOLD_BOUND};

const SPAWN: &str = "u/implementer#0";
const BRANCH: &str = "rigger/u/u";

/// A unit's tree: its own repository with `unit.txt` committed on [`BRANCH`], which is checked
/// out, a `side` branch one empty commit past it to merge, and an `empty` branch one more empty
/// commit past that to cherry-pick.
fn unit_tree() -> tempfile::TempDir {
    let tree = temp_git_project_with_commit();
    git_ok(tree.path(), &["checkout", "-q", "-b", BRANCH]);
    std::fs::write(tree.path().join("unit.txt"), "committed\n").unwrap();
    git_ok(tree.path(), &["add", "unit.txt"]);
    git_ok_with_identity(tree.path(), &["commit", "-q", "-m", "unit work"]);
    for (branch, message) in [("side", "side"), ("empty", "empty")] {
        git_ok(tree.path(), &["checkout", "-q", "-b", branch]);
        git_ok_with_identity(
            tree.path(),
            &["commit", "-q", "--allow-empty", "-m", message],
        );
    }
    git_ok(tree.path(), &["checkout", "-q", BRANCH]);
    tree
}

/// The bytes of the index of the tree at `dir`.
fn index_bytes(dir: &Path) -> Vec<u8> {
    std::fs::read(dir.join(git_out(dir, &["rev-parse", "--git-path", "index"]))).unwrap()
}

/// Run `rigger hook stop` from `root` for [`SPAWN`] over `args` (the tree and branch flags), with
/// Claude Code's `Stop` payload carrying `active` as `stop_hook_active` on its stdin.
fn stop_hook(root: &Path, args: &[&str], active: bool) -> Output {
    let mut child = common::rigger_courier()
        .args(["hook", "stop"])
        .args(args)
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rigger binary runs");
    let payload = serde_json::json!({
        "session_id": "s",
        "transcript_path": "/dev/null",
        "hook_event_name": "Stop",
        "stop_hook_active": active,
    });
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().expect("the hook ends")
}

/// [`stop_hook`] over the tree at `dir` on [`BRANCH`].
fn stop_hook_on(root: &Path, dir: &Path, active: bool) -> Output {
    let dir = dir.to_str().unwrap();
    stop_hook(
        root,
        &["--spawn", SPAWN, "--dir", dir, "--branch", BRANCH],
        active,
    )
}

/// The hook's answer: `Some(reason)` when it held the stop, `None` when it allowed it. Exit 0
/// either way, and nothing else on stdout.
fn held(case: &str, out: &Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{case}: the hook exits 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    if stdout.trim().is_empty() {
        return None;
    }
    let answer: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("{case}: stdout is one JSON object ({e}): {stdout}"));
    assert_eq!(answer["decision"], "block", "{case}: {answer}");
    Some(answer["reason"].as_str().expect("a reason").to_string())
}

/// [`SPAWN`]'s progress lines in `root`'s current run, in order.
fn spawn_lines(root: &Path) -> Vec<String> {
    recorded_progress::<AgentProgress>(root)
        .into_iter()
        .filter(|line| line.id == SPAWN)
        .map(|line| line.activity)
        .collect()
}

/// Asserts `lines` are as many as `starts` and each begins with its start.
fn assert_lines(case: &str, lines: &[String], starts: &[String]) {
    assert_eq!(lines.len(), starts.len(), "{case}: {lines:#?}");
    for (line, start) in lines.iter().zip(starts) {
        assert!(
            line.starts_with(start.as_str()),
            "{case}: {line:?} begins {start:?}"
        );
    }
}

/// EACH REFUSAL HOLDS THE STOP WITH GIT'S OWN WORDS, AND A TREE THE CHECKPOINT ACCEPTS DOES NOT:
/// conflict-marker text the commit would carry, an unfinished merge, a cherry-pick git stopped
/// on and a HEAD off the unit's branch each hold the session once, the reason naming the refusal
/// and carrying `git status` as git prints it; a clean tree, one holding uncommitted work the
/// checkpoint sweeps, and a clean tree after a hold (`stop_hook_active`) each allow it. Each
/// check records its line, then the hold or the allowance, and leaves the index as it was.
#[test]
fn each_refusal_holds_the_stop_with_gits_words_and_an_accepted_tree_allows_it() {
    // Each row: what leaves the tree so, `stop_hook_active`, the start of the refusal and the
    // git status words the reason carries (`None` for a stop allowed).
    type Row = (
        &'static str,
        fn(&Path),
        bool,
        Option<(&'static str, &'static str)>,
    );
    let rows: [Row; 7] = [
        (
            "conflict markers",
            |dir| {
                std::fs::write(
                    dir.join("unit.txt"),
                    "<<<<<<< ours\na\n=======\nb\n>>>>>>> theirs\n",
                )
                .unwrap()
            },
            false,
            Some(("refusing to commit in ", "modified:   unit.txt")),
        ),
        (
            "an unfinished merge",
            |dir| {
                git_ok(dir, &["merge", "-q", "--no-ff", "--no-commit", "side"]);
            },
            false,
            Some((
                "a merge is in progress in ",
                "All conflicts fixed but you are still merging.",
            )),
        ),
        (
            "a cherry-pick git stopped on",
            |dir| {
                run_git(dir, &["cherry-pick", "empty"]);
            },
            false,
            Some((
                "a cherry-pick is in progress in ",
                "You are currently cherry-picking commit",
            )),
        ),
        (
            "a HEAD off the unit's branch",
            |dir| {
                git_ok(dir, &["checkout", "-q", "--detach"]);
            },
            false,
            Some(("refusing to commit in ", "HEAD detached at ")),
        ),
        ("a clean tree", |_| {}, false, None),
        (
            "uncommitted work the checkpoint sweeps",
            |dir| {
                std::fs::write(dir.join("unit.txt"), "edited\n").unwrap();
                std::fs::write(dir.join("new.txt"), "new\n").unwrap();
            },
            false,
            None,
        ),
        ("a clean tree after a hold", |_| {}, true, None),
    ];
    for (case, leave, active, refused) in rows {
        let project = courier_project();
        seed_a_run(project.path());
        let tree = unit_tree();
        leave(tree.path());
        let index = index_bytes(tree.path());
        let dir = tree.path().to_str().unwrap();

        let reason = held(case, &stop_hook_on(project.path(), tree.path(), active));

        let continuing = if active {
            ", continuing after a hold"
        } else {
            ""
        };
        let check = format!("stop check: {dir} on {BRANCH}{continuing}");
        match (refused, reason) {
            (Some((refusal, status)), Some(reason)) => {
                assert!(
                    reason.starts_with("This session cannot end yet."),
                    "{case}: {reason}"
                );
                assert!(
                    reason.contains(&format!("{refusal}{dir}")),
                    "{case}: {reason}"
                );
                assert!(reason.contains(status), "{case}: git's own words: {reason}");
                assert!(reason.contains(BRANCH), "{case}: {reason}");
                let hold = format!("stop held (1 of {STOP_HOLD_BOUND}): {refusal}{dir}");
                assert_lines(case, &spawn_lines(project.path()), &[check, hold]);
            }
            (None, None) => {
                let allowed = "stop allowed: nothing refuses the checkpoint".to_string();
                assert_lines(case, &spawn_lines(project.path()), &[check, allowed]);
            }
            (refused, reason) => panic!("{case}: expected {refused:?}, held {reason:?}"),
        }
        assert_eq!(
            index_bytes(tree.path()),
            index,
            "{case}: the check leaves the index as it was"
        );
    }
}

/// A PRODUCER'S STOP IS HELD WHILE ITS TREE HOLDS WORK OUTSIDE A COMMIT: with `--own-commits`
/// (a stage the conductor lands only the commits of), a changed file and an untracked one each
/// hold the session once, the reason naming the tree and carrying `git status` as git prints it;
/// a clean tree and one whose work is committed each allow the stop. Each check records its
/// line, then the hold or the allowance, and leaves the index as it was.
#[test]
fn a_producers_stop_is_held_while_its_tree_holds_work_outside_a_commit() {
    // Each row: what leaves the tree so, and the git status words the reason carries (`None` for
    // a stop allowed).
    type Row = (&'static str, fn(&Path), Option<&'static str>);
    let rows: [Row; 4] = [
        (
            "a changed file",
            |dir| std::fs::write(dir.join("unit.txt"), "edited\n").unwrap(),
            Some("modified:   unit.txt"),
        ),
        (
            "an untracked file",
            |dir| std::fs::write(dir.join("plan.md"), "planned\n").unwrap(),
            Some("Untracked files:"),
        ),
        ("a clean tree", |_| {}, None),
        (
            "committed work",
            |dir| {
                std::fs::write(dir.join("plan.md"), "planned\n").unwrap();
                git_ok(dir, &["add", "plan.md"]);
                git_ok_with_identity(dir, &["commit", "-q", "-m", "plan"]);
            },
            None,
        ),
    ];
    for (case, leave, refused) in rows {
        let project = courier_project();
        seed_a_run(project.path());
        let tree = unit_tree();
        leave(tree.path());
        let index = index_bytes(tree.path());
        let dir = tree.path().to_str().unwrap();

        let out = stop_hook(
            project.path(),
            &["--spawn", SPAWN, "--dir", dir, "--own-commits"],
            false,
        );
        let reason = held(case, &out);

        let check = format!("stop check: {dir} for its own commits");
        let refusal = format!("{dir} holds work no commit records");
        match (refused, reason) {
            (Some(status), Some(reason)) => {
                assert!(
                    reason.starts_with("This session cannot end yet."),
                    "{case}: {reason}"
                );
                assert!(reason.contains(&refusal), "{case}: {reason}");
                assert!(reason.contains(status), "{case}: git's own words: {reason}");
                let hold = format!("stop held (1 of {STOP_HOLD_BOUND}): {refusal}");
                assert_lines(case, &spawn_lines(project.path()), &[check, hold]);
            }
            (None, None) => {
                let allowed = "stop allowed: no work is left outside a commit".to_string();
                assert_lines(case, &spawn_lines(project.path()), &[check, allowed]);
            }
            (refused, reason) => panic!("{case}: expected {refused:?}, held {reason:?}"),
        }
        assert_eq!(
            index_bytes(tree.path()),
            index,
            "{case}: the check leaves the index as it was"
        );
    }
}

/// THE HOLDS ARE BOUNDED PER LAUNCH: a tree that stays refused holds its session
/// [`STOP_HOLD_BOUND`] times, `stop_hook_active` read on each continuation; the next stop is
/// allowed with a line recording that the refusal stands; a new launch of the spawn starts the
/// count over.
#[test]
fn the_holds_are_bounded_per_launch_and_the_stop_past_the_bound_records_the_refusal_standing() {
    let project = courier_project();
    let run_id = seed_a_run(project.path());
    let tree = unit_tree();
    git_ok(tree.path(), &["checkout", "-q", "--detach"]);
    let dir = tree.path().to_str().unwrap();
    let refusal = format!("refusing to commit in {dir}: HEAD is not on the unit's branch");
    let check = |continuing: bool| {
        let after = if continuing {
            ", continuing after a hold"
        } else {
            ""
        };
        format!("stop check: {dir} on {BRANCH}{after}")
    };
    let mut expected = Vec::new();
    for n in 1..=STOP_HOLD_BOUND {
        let active = n > 1;
        let reason = held("a hold", &stop_hook_on(project.path(), tree.path(), active));
        assert!(reason.is_some(), "hold {n} of {STOP_HOLD_BOUND}");
        expected.push(check(active));
        expected.push(format!("stop held ({n} of {STOP_HOLD_BOUND}): {refusal}"));
    }
    let past = held(
        "past the bound",
        &stop_hook_on(project.path(), tree.path(), true),
    );
    assert_eq!(past, None, "the stop past the bound is allowed");
    expected.push(check(true));
    expected.push(format!(
        "stop allowed with the refusal standing after {STOP_HOLD_BOUND} holds: {refusal}"
    ));
    assert_lines("past the bound", &spawn_lines(project.path()), &expected);

    with_store_file(project.path(), "progress.db", |store| {
        let launch = SpawnLaunched {
            spawn: SPAWN.to_string(),
            launch: 1,
            session_id: "relaunched".to_string(),
            ..SpawnLaunched::default()
        };
        rigger::progress_store::record_launch(store, &run_id, &launch).unwrap();
    });
    let relaunched = held(
        "a new launch",
        &stop_hook_on(project.path(), tree.path(), false),
    );
    assert!(relaunched.is_some(), "a new launch is held again");
    let lines = spawn_lines(project.path());
    let last = lines.last().unwrap();
    assert!(
        last.starts_with(&format!("stop held (1 of {STOP_HOLD_BOUND}): {refusal}")),
        "a new launch counts from one: {last}"
    );
}

/// EVERY ERROR ALLOWS THE STOP: an argument the hook cannot read, a project with no store, and a
/// tree it cannot read each exit 0 with nothing on stdout and the error on stderr, and the tree
/// it could not read is on record.
#[test]
fn every_error_allows_the_stop() {
    // Each row: the case, the project it runs in, the hook's arguments there, what stderr
    // carries and the progress lines left on record.
    type Row = (
        &'static str,
        fn() -> tempfile::TempDir,
        fn(&Path) -> Vec<String>,
        &'static str,
        &'static [&'static str],
    );
    let rows: [Row; 3] = [
        (
            "a missing argument",
            courier_project,
            |_| vec!["--spawn".into(), SPAWN.into(), "--dir".into(), "/wt".into()],
            "--spawn and --dir are required, with exactly one of --branch <branch> and \
             --own-commits",
            &[],
        ),
        (
            "a project with no store",
            temp_project,
            |root| tree_args(&root.join("tree")),
            "",
            &[],
        ),
        (
            "a tree it cannot read",
            courier_project,
            |root| tree_args(&root.join("gone")),
            "",
            &["stop check: ", "stop allowed: the check could not read: "],
        ),
    ];
    for (case, project, args, error, lines) in rows {
        let project = project();
        if project.path().join(".rigger").exists() {
            seed_a_run(project.path());
        }
        let args = args(project.path());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = stop_hook(project.path(), &args, false);
        assert_eq!(held(case, &out), None, "{case}: the stop is allowed");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.starts_with("rigger hook stop: ")
                && stderr.contains(error)
                && stderr.trim_end().ends_with("; the stop is allowed"),
            "{case}: {stderr}"
        );
        if project.path().join(".rigger").exists() {
            let starts: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            assert_lines(case, &spawn_lines(project.path()), &starts);
        }
    }
}

/// `--spawn`, `--dir` and `--branch` over the tree at `dir` on [`BRANCH`].
fn tree_args(dir: &Path) -> Vec<String> {
    [
        "--spawn",
        SPAWN,
        "--dir",
        dir.to_str().unwrap(),
        "--branch",
        BRANCH,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
