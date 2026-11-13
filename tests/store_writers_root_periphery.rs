//! No command creates a store anywhere but the root its repository resolves to, at the periphery:
//! the compiled binary over a real repository whose root holds the store, run from a
//! subdirectory, from a subdirectory with a scaffold of its own and from a linked worktree beside
//! the checkout, where a run's unit worktrees live.
//!
//! A command that writes the store (`graph build`, `canary`, `playbooks --rebuild`) writes the one
//! store its repository resolves to from wherever it runs, and refuses naming `rigger setup` where
//! no store exists; `critique` and `run` refuse anywhere but the root; `setup` and `init` scaffold
//! only the top of a worktree, and `setup` pays the store's owed rebuild at the root. None of them
//! leaves a file under the `.rigger/` of the directory it ran in.

use crate::common;

use common::cli::{run_recording, run_rigger};
use common::git::{git_commit_all, init_repo, linked_worktree};
use std::path::{Path, PathBuf};

/// Where a command runs, relative to a checkout whose root holds the store.
#[derive(Clone, Copy)]
enum Place {
    /// A subdirectory of the checkout.
    Subdirectory,
    /// A subdirectory holding a `.rigger/` scaffold of its own.
    ScaffoldedSubdirectory,
    /// A linked worktree of the checkout, beside it, as a run's unit worktrees are.
    LinkedWorktree,
}

/// What a command run at a place answers.
#[derive(Clone, Copy)]
enum Answer {
    /// It succeeds.
    Succeeds,
    /// It fails with a message holding the text.
    Refuses(&'static str),
    /// It fails or succeeds by what the fake agent answers its spawns, which is not this suite's
    /// subject.
    EitherWay,
}

/// A checkout `rigger init` scaffolded at `<at>/main`, its scaffold, a canary corpus, a spec and a
/// subdirectory committed, with `rigger graph build` run at its root when `stored`, then a linked
/// worktree of it at `<at>/unit`.
fn checkout(at: &Path, stored: bool) -> PathBuf {
    let main = at.join("main");
    std::fs::create_dir_all(main.join("sub")).unwrap();
    init_repo(&main);
    let (_out, err, ok) = run_rigger(&main, &["init"]);
    assert!(ok, "rigger init scaffolds the checkout; stderr:\n{err}");
    std::fs::create_dir_all(main.join("corpus")).unwrap();
    std::fs::write(
        main.join("corpus").join("control.md"),
        common::fixtures::CONTROL_ITEM,
    )
    .unwrap();
    std::fs::create_dir_all(main.join("specs")).unwrap();
    std::fs::write(
        main.join("specs").join("s.md"),
        "# Spec\n\n## Done when\n\n- a thing\n",
    )
    .unwrap();
    std::fs::write(main.join("sub").join("f.txt"), "x\n").unwrap();
    let own = main.join("own").join(".rigger");
    std::fs::create_dir_all(own.join("agents")).unwrap();
    std::fs::copy(
        main.join(".rigger").join("workflow.yml"),
        own.join("workflow.yml"),
    )
    .unwrap();
    for agent in std::fs::read_dir(main.join(".rigger").join("agents")).unwrap() {
        let agent = agent.unwrap().path();
        std::fs::copy(&agent, own.join("agents").join(agent.file_name().unwrap())).unwrap();
    }
    git_commit_all(&main, "scaffold");
    if stored {
        let (_out, err, ok) = run_rigger(&main, &["graph", "build"]);
        assert!(
            ok,
            "graph build at the root makes its store; stderr:\n{err}"
        );
    }
    linked_worktree(&main, at.join("unit"));
    main
}

/// Every path under `dir/.rigger`, relative to it and sorted; empty when there is none.
fn rigger_listing(dir: &Path) -> Vec<PathBuf> {
    fn walk(base: &Path, dir: &Path, into: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            into.push(path.strip_prefix(base).unwrap().to_path_buf());
            if path.is_dir() {
                walk(base, &path, into);
            }
        }
    }
    let base = dir.join(".rigger");
    let mut into = Vec::new();
    walk(&base, &base, &mut into);
    into.sort();
    into
}

/// `rigger <args>` run at `place` of a checkout whose root holds the store (or none, when not
/// `stored`) answers `answer` and leaves the `.rigger/` of the directory it ran in as it found
/// it. `{main}` in an argument is the checkout's root.
fn assert_no_store_outside_the_root(place: Place, stored: bool, args: &[&str], answer: Answer) {
    let at = tempfile::tempdir().unwrap();
    let main = checkout(at.path(), stored);
    let cwd = match place {
        Place::Subdirectory => main.join("sub"),
        Place::ScaffoldedSubdirectory => main.join("own"),
        Place::LinkedWorktree => at.path().join("unit"),
    };
    let args: Vec<String> = args
        .iter()
        .map(|arg| arg.replace("{main}", main.to_str().unwrap()))
        .collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let scratch = tempfile::tempdir().unwrap();
    let before = rigger_listing(&cwd);

    let (out, err, ok, _fake) = run_recording(&cwd, &args, scratch.path());

    let said = format!("rigger {}\nstdout: {out}\nstderr: {err}", args.join(" "));
    match answer {
        Answer::Succeeds => assert!(ok, "succeeds; {said}"),
        Answer::Refuses(text) => {
            assert!(!ok && err.contains(text), "refuses saying {text:?}; {said}")
        }
        Answer::EitherWay => {}
    }
    assert_eq!(
        rigger_listing(&cwd),
        before,
        "the .rigger/ of {} gains nothing; {said}",
        cwd.display()
    );
    if !stored {
        assert!(
            !main.join(".rigger").join("events.db").exists(),
            "no store is made at the root either; {said}"
        );
    }
}

use Answer::{EitherWay, Refuses, Succeeds};
use Place::{LinkedWorktree, ScaffoldedSubdirectory, Subdirectory};

rigger::test_cases! {
    /// `graph build` from a subdirectory advances the root's store.
    graph_build_from_a_subdirectory_writes_the_roots_store:
        assert_no_store_outside_the_root(Subdirectory, true, &["graph", "build"], Succeeds);
    /// `graph build` from a scaffolded subdirectory advances the root's store.
    graph_build_from_a_scaffolded_subdirectory_writes_the_roots_store:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, true, &["graph", "build"], Succeeds);
    /// `graph build` from a linked worktree advances the root's store.
    graph_build_from_a_linked_worktree_writes_the_roots_store:
        assert_no_store_outside_the_root(LinkedWorktree, true, &["graph", "build"], Succeeds);
    /// `graph build` from a subdirectory of a checkout with no store refuses, naming the command
    /// that makes one, and makes none.
    graph_build_where_no_store_exists_refuses_naming_setup:
        assert_no_store_outside_the_root(Subdirectory, false, &["graph", "build"], Refuses("rigger setup"));
    /// `canary` from a subdirectory makes no store there.
    canary_from_a_subdirectory_makes_no_store_there:
        assert_no_store_outside_the_root(Subdirectory, true, &["canary", "--corpus", "{main}/corpus"], EitherWay);
    /// `canary` from a scaffolded subdirectory makes no store there.
    canary_from_a_scaffolded_subdirectory_makes_no_store_there:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, true, &["canary", "--corpus", "{main}/corpus"], EitherWay);
    /// `canary` from a linked worktree makes no store there.
    canary_from_a_linked_worktree_makes_no_store_there:
        assert_no_store_outside_the_root(LinkedWorktree, true, &["canary", "--corpus", "{main}/corpus"], EitherWay);
    /// `playbooks --rebuild` from a subdirectory rebuilds the root's pool.
    playbooks_from_a_subdirectory_rebuild_the_roots_pool:
        assert_no_store_outside_the_root(Subdirectory, true, &["playbooks", "--rebuild"], Succeeds);
    /// `playbooks --rebuild` from a scaffolded subdirectory rebuilds the root's pool.
    playbooks_from_a_scaffolded_subdirectory_rebuild_the_roots_pool:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, true, &["playbooks", "--rebuild"], Succeeds);
    /// `playbooks --rebuild` from a linked worktree rebuilds the root's pool.
    playbooks_from_a_linked_worktree_rebuild_the_roots_pool:
        assert_no_store_outside_the_root(LinkedWorktree, true, &["playbooks", "--rebuild"], Succeeds);
    /// `critique` from a subdirectory fails reading its configuration there, before it opens a
    /// store.
    critique_from_a_subdirectory_refuses:
        assert_no_store_outside_the_root(Subdirectory, true, &["critique", "{main}/specs/s.md"], Refuses("read agents dir"));
    /// `critique` from a scaffolded subdirectory refuses: it and the root disagree on their root.
    critique_from_a_scaffolded_subdirectory_refuses:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, true, &["critique", "{main}/specs/s.md"], Refuses("disagree on their root"));
    /// `critique` from a linked worktree refuses.
    critique_from_a_linked_worktree_refuses:
        assert_no_store_outside_the_root(LinkedWorktree, true, &["critique", "{main}/specs/s.md"], Refuses("linked worktree"));
    /// `run` from a subdirectory refuses before it opens a store.
    run_from_a_subdirectory_refuses:
        assert_no_store_outside_the_root(Subdirectory, true, &["run", "{main}/specs/s.md"], Refuses("rigger run"));
    /// `run` from a scaffolded subdirectory refuses: it and the root disagree on their root.
    run_from_a_scaffolded_subdirectory_refuses:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, true, &["run", "{main}/specs/s.md"], Refuses("disagree on their root"));
    /// `run` from a linked worktree refuses.
    run_from_a_linked_worktree_refuses:
        assert_no_store_outside_the_root(LinkedWorktree, true, &["run", "{main}/specs/s.md"], Refuses("linked worktree"));
    /// `setup` from a subdirectory refuses, naming the top of its worktree.
    setup_from_a_subdirectory_refuses:
        assert_no_store_outside_the_root(Subdirectory, true, &["setup"], Refuses("the top of its worktree"));
    /// `setup` from a scaffolded subdirectory refuses, naming the top of its worktree.
    setup_from_a_scaffolded_subdirectory_refuses:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, true, &["setup"], Refuses("the top of its worktree"));
    /// `setup` at a linked worktree's top installs there and pays the root store's rebuild.
    setup_at_a_linked_worktrees_top_writes_the_roots_store:
        assert_no_store_outside_the_root(LinkedWorktree, true, &["setup"], Succeeds);
    /// `init` from a subdirectory refuses, naming the top of its worktree.
    init_from_a_subdirectory_refuses:
        assert_no_store_outside_the_root(Subdirectory, true, &["init"], Refuses("the top of its worktree"));
    /// `init` from a scaffolded subdirectory refuses, naming the top of its worktree.
    init_from_a_scaffolded_subdirectory_refuses:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, true, &["init"], Refuses("the top of its worktree"));
    /// `init` at a linked worktree's top finds its scaffold in place.
    init_at_a_linked_worktrees_top_succeeds:
        assert_no_store_outside_the_root(LinkedWorktree, true, &["init"], Succeeds);
}
