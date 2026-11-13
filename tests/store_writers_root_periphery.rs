//! No command creates a store anywhere but the root its repository resolves to, at the periphery:
//! the compiled binary over a real repository, run from a subdirectory, from a subdirectory with a
//! scaffold of its own and from a linked worktree beside the checkout, where a run's unit worktrees
//! live.
//!
//! A command that writes the store (`graph build`, `canary`, `playbooks --rebuild`, `setup`)
//! writes the store at the root its repository resolves to from wherever it runs, and where that
//! root was never set up it refuses, naming `rigger setup` at the root, after which the same
//! command succeeds; `critique` and `run` refuse anywhere but the root; `init` and `setup` scaffold
//! the directory they run in. None of them leaves a store file under the `.rigger/` of the
//! directory it ran in, and a refusal leaves the root's `.rigger/` as it found it.

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

/// What a checkout's root holds before a command runs.
#[derive(Clone, Copy, PartialEq)]
enum Root {
    /// No `.rigger/` at all: the repository was never set up.
    Bare,
    /// The scaffold `rigger init` writes, committed, and no store file.
    Scaffolded,
    /// The scaffold, committed, and the store `rigger graph build` makes beside it.
    Built,
}

/// What a command run at a place answers.
#[derive(Clone, Copy)]
enum Answer {
    /// It succeeds, leaving the `.rigger/` of the directory it ran in as it found it.
    Succeeds,
    /// It succeeds, adding to the `.rigger/` of the directory it ran in only the scaffold `rigger
    /// init` writes.
    Scaffolds,
    /// It fails with a message holding the text.
    Refuses(&'static str),
    /// It fails or succeeds by what the fake agent answers its spawns, which is not this suite's
    /// subject.
    EitherWay,
}

/// A checkout at `<at>/main` holding what `root` says at its root, with a canary corpus, a spec, a
/// subdirectory and, unless `root` is [`Root::Bare`], a subdirectory `own` holding a copy of the
/// root's scaffold, committed; then a linked worktree of it at `<at>/unit`.
fn checkout(at: &Path, root: Root) -> PathBuf {
    let main = at.join("main");
    std::fs::create_dir_all(main.join("sub")).unwrap();
    init_repo(&main);
    if root != Root::Bare {
        let (_out, err, ok) = run_rigger(&main, &["init"]);
        assert!(ok, "rigger init scaffolds the checkout; stderr:\n{err}");
    }
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
    if root != Root::Bare {
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
    }
    git_commit_all(&main, "scaffold");
    if root == Root::Built {
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

/// Every path `rigger init` scaffolds under the `.rigger/` of an empty directory.
fn init_scaffold() -> Vec<PathBuf> {
    let empty = tempfile::tempdir().unwrap();
    let (_out, err, ok) = run_rigger(empty.path(), &["init"]);
    assert!(
        ok,
        "rigger init scaffolds an empty directory; stderr:\n{err}"
    );
    rigger_listing(empty.path())
}

/// The directory `place` names beside the checkout `main` made under `at`.
fn place_dir(at: &Path, main: &Path, place: Place) -> PathBuf {
    match place {
        Place::Subdirectory => main.join("sub"),
        Place::ScaffoldedSubdirectory => main.join("own"),
        Place::LinkedWorktree => at.join("unit"),
    }
}

/// `args` with `{main}` replaced by the checkout's root `main`.
fn with_main(args: &[&str], main: &Path) -> Vec<String> {
    args.iter()
        .map(|arg| arg.replace("{main}", main.to_str().unwrap()))
        .collect()
}

/// `rigger <args>` run at `place` of a checkout whose root holds what `root` says answers
/// `answer`: what the `.rigger/` of the directory it ran in gains is nothing, or only the scaffold
/// `rigger init` writes where it [`Answer::Scaffolds`], and a refusal leaves the root's `.rigger/`
/// as it found it. `{main}` in an argument is the checkout's root.
fn assert_no_store_outside_the_root(place: Place, root: Root, args: &[&str], answer: Answer) {
    let at = tempfile::tempdir().unwrap();
    let main = checkout(at.path(), root);
    let cwd = place_dir(at.path(), &main, place);
    let args = with_main(args, &main);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let scratch = tempfile::tempdir().unwrap();
    let before = rigger_listing(&cwd);
    let root_before = rigger_listing(&main);

    let (out, err, ok, _fake) = run_recording(&cwd, &args, scratch.path());

    let said = format!("rigger {}\nstdout: {out}\nstderr: {err}", args.join(" "));
    match answer {
        Answer::Succeeds | Answer::Scaffolds => assert!(ok, "succeeds; {said}"),
        Answer::Refuses(text) => {
            assert!(!ok && err.contains(text), "refuses saying {text:?}; {said}");
            assert_eq!(
                rigger_listing(&main),
                root_before,
                "the root's .rigger/ is left as it was; {said}"
            );
        }
        Answer::EitherWay => {}
    }
    let gained: Vec<PathBuf> = rigger_listing(&cwd)
        .into_iter()
        .filter(|path| !before.contains(path))
        .collect();
    let may_gain = match answer {
        Answer::Scaffolds => init_scaffold(),
        _ => Vec::new(),
    };
    assert!(
        gained.iter().all(|path| may_gain.contains(path)),
        "the .rigger/ of {} gains {gained:?}, beyond the scaffold {may_gain:?}; {said}",
        cwd.display()
    );
}

/// `rigger <args>` run at `place` of a checkout that was never set up refuses, naming `rigger
/// setup` and the root it runs at; once `rigger setup` ran there, the same command succeeds. The
/// `.rigger/` of the directory it ran in gains nothing throughout.
fn assert_the_refusal_names_what_clears_it(place: Place, args: &[&str]) {
    let at = tempfile::tempdir().unwrap();
    let main = checkout(at.path(), Root::Bare);
    let cwd = place_dir(at.path(), &main, place);
    let before = rigger_listing(&cwd);
    let named = main.canonicalize().unwrap();

    let (out, err, ok) = run_rigger(&cwd, args);
    let said = format!("rigger {}\nstdout: {out}\nstderr: {err}", args.join(" "));
    assert!(
        !ok && err.contains("`rigger setup`") && err.contains(named.to_str().unwrap()),
        "refuses naming `rigger setup` at {}; {said}",
        named.display()
    );
    let (_out, setup_err, setup_ok) = run_rigger(&main, &["setup"]);
    assert!(
        setup_ok,
        "rigger setup at the root succeeds; stderr:\n{setup_err}"
    );
    let (out, err, ok) = run_rigger(&cwd, args);
    assert!(
        ok,
        "succeeds once the root is set up; rigger {}\nstdout: {out}\nstderr: {err}",
        args.join(" ")
    );
    assert_eq!(
        rigger_listing(&cwd),
        before,
        "the .rigger/ of {} gains nothing; {said}",
        cwd.display()
    );
}

use Answer::{EitherWay, Refuses, Scaffolds, Succeeds};
use Place::{LinkedWorktree, ScaffoldedSubdirectory, Subdirectory};
use Root::{Bare, Built, Scaffolded};

rigger::test_cases! {
    /// `graph build` from a subdirectory advances the root's store.
    graph_build_from_a_subdirectory_writes_the_roots_store:
        assert_no_store_outside_the_root(Subdirectory, Built, &["graph", "build"], Succeeds);
    /// `graph build` from a scaffolded subdirectory advances the root's store.
    graph_build_from_a_scaffolded_subdirectory_writes_the_roots_store:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, Built, &["graph", "build"], Succeeds);
    /// `graph build` from a linked worktree advances the root's store.
    graph_build_from_a_linked_worktree_writes_the_roots_store:
        assert_no_store_outside_the_root(LinkedWorktree, Built, &["graph", "build"], Succeeds);
    /// `graph build` from a subdirectory of a checkout set up but never built makes the store at
    /// the root.
    graph_build_from_a_subdirectory_makes_the_store_at_a_set_up_root:
        assert_no_store_outside_the_root(Subdirectory, Scaffolded, &["graph", "build"], Succeeds);
    /// `graph build` from a linked worktree of a checkout set up but never built makes the store at
    /// the root, beside the scaffold the worktree carries.
    graph_build_from_a_linked_worktree_makes_the_store_at_a_set_up_root:
        assert_no_store_outside_the_root(LinkedWorktree, Scaffolded, &["graph", "build"], Succeeds);
    /// `graph build` from a subdirectory of a checkout never set up refuses, naming the command
    /// that makes a store, and makes none.
    graph_build_where_no_store_exists_refuses_naming_setup:
        assert_no_store_outside_the_root(Subdirectory, Bare, &["graph", "build"], Refuses("rigger setup"));
    /// `canary` from a subdirectory makes no store there.
    canary_from_a_subdirectory_makes_no_store_there:
        assert_no_store_outside_the_root(Subdirectory, Built, &["canary", "--corpus", "{main}/corpus"], EitherWay);
    /// `canary` from a scaffolded subdirectory makes no store there.
    canary_from_a_scaffolded_subdirectory_makes_no_store_there:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, Built, &["canary", "--corpus", "{main}/corpus"], EitherWay);
    /// `canary` from a linked worktree makes no store there.
    canary_from_a_linked_worktree_makes_no_store_there:
        assert_no_store_outside_the_root(LinkedWorktree, Built, &["canary", "--corpus", "{main}/corpus"], EitherWay);
    /// `playbooks --rebuild` from a subdirectory rebuilds the root's pool.
    playbooks_from_a_subdirectory_rebuild_the_roots_pool:
        assert_no_store_outside_the_root(Subdirectory, Built, &["playbooks", "--rebuild"], Succeeds);
    /// `playbooks --rebuild` from a scaffolded subdirectory rebuilds the root's pool.
    playbooks_from_a_scaffolded_subdirectory_rebuild_the_roots_pool:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, Built, &["playbooks", "--rebuild"], Succeeds);
    /// `playbooks --rebuild` from a linked worktree rebuilds the root's pool.
    playbooks_from_a_linked_worktree_rebuild_the_roots_pool:
        assert_no_store_outside_the_root(LinkedWorktree, Built, &["playbooks", "--rebuild"], Succeeds);
    /// `critique` from a subdirectory fails reading its configuration there, before it opens a
    /// store.
    critique_from_a_subdirectory_refuses:
        assert_no_store_outside_the_root(Subdirectory, Built, &["critique", "{main}/specs/s.md"], Refuses("read agents dir"));
    /// `critique` from a scaffolded subdirectory refuses: it and the root disagree on their root.
    critique_from_a_scaffolded_subdirectory_refuses:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, Built, &["critique", "{main}/specs/s.md"], Refuses("disagree on their root"));
    /// `critique` from a linked worktree refuses.
    critique_from_a_linked_worktree_refuses:
        assert_no_store_outside_the_root(LinkedWorktree, Built, &["critique", "{main}/specs/s.md"], Refuses("linked worktree"));
    /// `run` from a subdirectory refuses before it opens a store.
    run_from_a_subdirectory_refuses:
        assert_no_store_outside_the_root(Subdirectory, Built, &["run", "{main}/specs/s.md"], Refuses("rigger run"));
    /// `run` from a scaffolded subdirectory refuses: it and the root disagree on their root.
    run_from_a_scaffolded_subdirectory_refuses:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, Built, &["run", "{main}/specs/s.md"], Refuses("disagree on their root"));
    /// `run` from a linked worktree refuses.
    run_from_a_linked_worktree_refuses:
        assert_no_store_outside_the_root(LinkedWorktree, Built, &["run", "{main}/specs/s.md"], Refuses("linked worktree"));
    /// `setup` from a subdirectory scaffolds a project there and pays the root store's rebuild.
    setup_from_a_subdirectory_scaffolds_there_and_writes_the_roots_store:
        assert_no_store_outside_the_root(Subdirectory, Built, &["setup"], Scaffolds);
    /// `setup` from a scaffolded subdirectory pays the root store's rebuild.
    setup_from_a_scaffolded_subdirectory_writes_the_roots_store:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, Built, &["setup"], Scaffolds);
    /// `setup` from a subdirectory of a checkout never set up refuses, naming `rigger setup` at the
    /// root, before it scaffolds anything.
    setup_from_a_subdirectory_where_no_store_exists_refuses:
        assert_no_store_outside_the_root(Subdirectory, Bare, &["setup"], Refuses("rigger setup"));
    /// `setup` at a linked worktree's top installs there and pays the root store's rebuild.
    setup_at_a_linked_worktrees_top_writes_the_roots_store:
        assert_no_store_outside_the_root(LinkedWorktree, Built, &["setup"], Succeeds);
    /// `init` from a subdirectory scaffolds a project there and makes no store file.
    init_from_a_subdirectory_scaffolds_there:
        assert_no_store_outside_the_root(Subdirectory, Built, &["init"], Scaffolds);
    /// `init` from a scaffolded subdirectory completes its scaffold and makes no store file.
    init_from_a_scaffolded_subdirectory_scaffolds_there:
        assert_no_store_outside_the_root(ScaffoldedSubdirectory, Built, &["init"], Scaffolds);
    /// `init` at a linked worktree's top finds its scaffold in place.
    init_at_a_linked_worktrees_top_succeeds:
        assert_no_store_outside_the_root(LinkedWorktree, Built, &["init"], Succeeds);
    /// `graph build` from a subdirectory of a checkout never set up names `rigger setup` at the
    /// root, which clears it.
    graph_build_from_a_subdirectory_names_what_clears_its_refusal:
        assert_the_refusal_names_what_clears_it(Subdirectory, &["graph", "build"]);
    /// `graph build` from a linked worktree of a checkout never set up names `rigger setup` at the
    /// root, which clears it.
    graph_build_from_a_linked_worktree_names_what_clears_its_refusal:
        assert_the_refusal_names_what_clears_it(LinkedWorktree, &["graph", "build"]);
    /// `playbooks --rebuild` from a subdirectory of a checkout never set up names `rigger setup` at
    /// the root, which clears it.
    playbooks_from_a_subdirectory_names_what_clears_its_refusal:
        assert_the_refusal_names_what_clears_it(Subdirectory, &["playbooks", "--rebuild"]);
}
