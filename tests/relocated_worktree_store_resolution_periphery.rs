//! Periphery (cross-module, real-binary) test for spec 89, criterion 2 (SCRATCH IS OUTSIDE
//! THE STORE TREE) at the store-RESOLUTION boundary a real courier crosses.
//!
//! `find_store_dir_from`'s own new unit tests in `src/main.rs`
//! (`find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it`,
//! `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_
//! foreign_store`) prove the PURE walk-classification logic returns the right directory when
//! called directly with the right arguments - but that function (and `require_store_dir`,
//! which wraps it) is private to the `rigger` BINARY crate, reachable only from its own
//! in-process unit tests. Nothing proves the real, compiled COURIER commands spec 89's own
//! Problem statement names (`emit`/`result`/`peers`/`reported` - a worker's self-report from
//! inside its unit worktree) actually WIRE this logic correctly: that `cmd_emit` reaches
//! `require_store_dir` with the process's genuine cwd, that a relocated worktree's real
//! subprocess lands its write in the owning repo's real store rather than refusing or
//! fabricating a stray one, and - the class of bug a white-box call-the-function-directly
//! test structurally cannot see - that it does not silently bind a FOREIGN store an unrelated
//! ancestor of the relocated worktree happens to carry. This file closes that gap end to end,
//! driving the real compiled binary exactly as a worker's own `rigger emit` self-report would,
//! from inside a REAL git-linked worktree living outside the repo's own directory tree - the
//! shape every real spawn's worktree takes once this criterion's relocated scratch default is
//! in effect (spec 89 Design, SCRATCH LIVES OUTSIDE THE STORE TREE).

use std::path::Path;
use std::process::Command;

mod common;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};

/// `git init` plus one real commit under `root` - a bare `git init` has no commit for `git
/// worktree add -b <new-branch>` to branch from.
fn git_init_committed(root: &Path) {
    let run = |args: &[&str]| {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .expect("spawn git")
                .success(),
            "git {args:?} must succeed"
        );
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.invalid"]);
    run(&["config", "user.name", "test"]);
    std::fs::write(root.join("README"), "x").expect("write README");
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "init"]);
}

/// Seed an initialized, EMPTY `.rigger/events.db` under `root` - `rigger emit` refuses to
/// fabricate a fresh store from the wrong cwd (spec 05), mirroring
/// `tests/cli.rs::seed_store`/`tests/spawn_scratch_reap_authorized_root_periphery.rs::seed_store`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).expect("create .rigger");
    std::fs::File::create(rigger.join("events.db")).expect("create events.db");
}

/// The project identity a courier binds its namespaced stream to for `root`: the tracked
/// `.rigger/project.id` when present, else `root`'s own basename - every fixture in this file
/// has `root` AS its own git toplevel (no nested-worktree identity indirection to mirror),
/// matching `StoreLocation::identity`'s documented precedence for that shape.
fn stream_identity(root: &Path) -> String {
    if let Ok(raw) = std::fs::read_to_string(root.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    root.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Every `DecisionMade` payload recorded in `root`'s real store, read back through a FRESH
/// `Store::open` independent of the courier subprocess that wrote it - so a passing assertion
/// proves the subprocess's write genuinely landed on disk at `root`, not merely that the
/// child process exited 0.
fn decision_summaries_in(root: &Path) -> Vec<String> {
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap())
        .expect("open the real store for readback");
    let store = Namespaced::new(&backend, &stream_identity(root));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .expect("read the real store's stream")
        .into_iter()
        .filter(|e| e.type_ == "DecisionMade")
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .collect()
}

/// Run `rigger <args...>` in `cwd`, mirroring `tests/cli.rs::run_rigger_envs`: opts out of the
/// auto-started dashboard and isolates the machine-global instance registry (`cmd_emit`'s own
/// `refresh_registry_entry` call touches it on every invocation), so this file never seeds a
/// phantom registry entry or leaked dashboard process into the operator's real state.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    let out = common::rigger_courier()
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .output()
        .expect("spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Spec 89, criterion 2: a real `rigger emit`, run from inside a git-linked worktree that
/// lives WHOLLY OUTSIDE the repo's own directory tree (`elsewhere/rigger-wt-x`, never a
/// descendant of `root` - the shape a real spawn's worktree takes once the relocated scratch
/// default is in effect), must still resolve and write to `root`'s real store: never refuse
/// with "no rigger store found", and never fabricate a stray store inside the worktree itself.
///
/// Non-vacuous against the real binary: before this criterion's fix, `find_store_dir_from`'s
/// plain `.parent()` climb from `start` never physically passes through `root` for a worktree
/// living outside it, so this test fails closed (emit refuses) on the pre-fix code path - it
/// is not merely re-asserting a tautology the current binary always satisfies.
#[test]
fn rigger_emit_from_a_relocated_worktree_resolves_the_owning_repos_real_store() {
    let dir = tempfile::tempdir().expect("create the fixture repo dir");
    let root = dir.path();
    git_init_committed(root);
    seed_store(root);

    let elsewhere = tempfile::tempdir().expect("create the relocated-scratch sibling dir");
    let worktree = elsewhere.path().join("rigger-wt-x");
    assert!(
        Command::new("git")
            .args(["worktree", "add", "-q"])
            .arg(&worktree)
            .args(["-b", "rigger/u/x"])
            .current_dir(root)
            .status()
            .expect("spawn git worktree add")
            .success(),
        "git worktree add must succeed for the fixture"
    );

    let marker = "probe-outside-the-repo-tree";
    let (stdout, stderr, ok) = run_rigger(
        &worktree,
        &[
            "emit",
            "DecisionMade",
            &format!(r#"{{"id":"{marker}","summary":"s"}}"#),
        ],
    );
    assert!(
        ok,
        "rigger emit from a relocated (outside-the-repo) worktree must succeed; stderr: {stderr}"
    );
    assert!(
        stdout.contains("emitted DecisionMade"),
        "must report the emit landing, not a silent no-op; stdout: {stdout:?}"
    );

    assert!(
        !worktree.join(".rigger").join("events.db").is_file(),
        "the relocated worktree must never grow its OWN stray store - the write must land on \
         the real repo's store, never fabricate a local one at the worktree"
    );
    let decisions = decision_summaries_in(root);
    assert!(
        decisions.iter().any(|d| d.contains(marker)),
        "the emit from the relocated worktree must land in the REAL repo's own store at \
         {root:?}; DecisionMade payloads found there: {decisions:?}"
    );
}

/// Spec 89, criterion 2: when a git-linked worktree lives outside the repo tree AND a FOREIGN
/// project's store sits at one of the worktree's own filesystem ancestors (an unrelated
/// project, or a leftover fixture, parked under the same cache-home mount the relocated
/// default now shares with every OTHER project on the machine), a real `rigger emit` run from
/// inside that worktree must still resolve the REAL owning repo's store - never silently climb
/// into and bind the foreign ancestor's, the exact cross-project escape
/// `adv9-walkup-cross-project` (cited by this criterion's own production doc comment) names.
///
/// Non-vacuous against the real binary: an unbounded ancestor climb from a relocated worktree
/// (the naive fix) would find the FOREIGN store first (nearer to `worktree` than `root`) and
/// bind to it instead - this test's negative assertion on the foreign store fails on that
/// shape, distinguishing the real fix from the naive one, not merely from no fix at all.
#[test]
fn rigger_emit_from_a_relocated_worktree_never_climbs_into_a_foreign_ancestors_store() {
    let dir = tempfile::tempdir().expect("create the fixture repo dir");
    let root = dir.path();
    git_init_committed(root);
    seed_store(root);

    // A FOREIGN store sitting at an ancestor of the relocated worktree - the exact shape a
    // plain unbounded climb would wrongly bind to (mirrors the implementer's own
    // `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_
    // foreign_store` fixture shape).
    let elsewhere = tempfile::tempdir().expect("create the foreign-ancestor sibling dir");
    seed_store(elsewhere.path());
    let worktree = elsewhere.path().join("nested").join("rigger-wt-y");
    std::fs::create_dir_all(worktree.parent().unwrap()).expect("create the worktree's parent");
    assert!(
        Command::new("git")
            .args(["worktree", "add", "-q"])
            .arg(&worktree)
            .args(["-b", "rigger/u/y"])
            .current_dir(root)
            .status()
            .expect("spawn git worktree add")
            .success(),
        "git worktree add must succeed for the fixture"
    );

    let marker = "probe-past-a-foreign-ancestor";
    let (_stdout, stderr, ok) = run_rigger(
        &worktree,
        &[
            "emit",
            "DecisionMade",
            &format!(r#"{{"id":"{marker}","summary":"s"}}"#),
        ],
    );
    assert!(
        ok,
        "rigger emit must succeed, resolving the real owning repo past the foreign ancestor; \
         stderr: {stderr}"
    );

    let foreign_decisions = decision_summaries_in(elsewhere.path());
    assert!(
        foreign_decisions.is_empty(),
        "the foreign ancestor's store must receive NOTHING from this emit - a climb that binds \
         it would be the exact adv9-walkup-cross-project hazard reopened; found: \
         {foreign_decisions:?}"
    );
    let real_decisions = decision_summaries_in(root);
    assert!(
        real_decisions.iter().any(|d| d.contains(marker)),
        "the emit must land in the REAL owning repo's store at {root:?}, never the foreign \
         ancestor's; DecisionMade payloads found there: {real_decisions:?}"
    );
}
