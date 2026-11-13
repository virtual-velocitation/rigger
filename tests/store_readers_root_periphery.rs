//! A command that reads the store reads the store at the root its repository resolves to, at the
//! periphery: the compiled binary over a real repository whose root store holds a run, run from a
//! subdirectory, from a linked worktree beside the checkout, where a run's unit worktrees live, and
//! from a subdirectory of a linked worktree of a repository whose git directory is kept apart, which
//! records no main checkout, so that worktree is its own root. From each place it answers exactly
//! what it answers at the root. Below a root that holds no store it says so in one sentence, naming
//! `rigger setup` at the root, as a command that writes the store does.

use crate::common;

use common::cli::{run_rigger, seed_run_events};
use common::git::{git_commit_all, init_repo, linked_worktree};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a reader runs, beside the root whose store it reads.
#[derive(Clone, Copy)]
enum Place {
    /// A subdirectory of the checkout.
    Subdirectory,
    /// A linked worktree of the checkout, beside it.
    LinkedWorktree,
    /// A subdirectory of a linked worktree of a repository whose git directory is kept apart.
    UnrecordedMainSubdirectory,
}

/// The run events the root's store holds: one run whose one unit escalated.
const RUN: &[(&str, &str)] = &[
    ("RunStarted", r#"{"run":"r1","criteria":["spec one"]}"#),
    ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
    ("UnitEscalated", r#"{"id":"u1"}"#),
];

/// `git <args>` in `dir`, which must succeed.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The root and the place `place` names under `at`: a committed checkout with a subdirectory, and
/// the place the reader runs at. The root's store holds [`RUN`] where `stored`.
fn layout(at: &Path, place: Place, stored: bool) -> (PathBuf, PathBuf) {
    let (root, here) = match place {
        Place::Subdirectory | Place::LinkedWorktree => {
            let main = at.join("main");
            std::fs::create_dir_all(main.join("sub")).unwrap();
            init_repo(&main);
            std::fs::write(main.join("sub").join("f.txt"), "x\n").unwrap();
            git_commit_all(&main, "tree");
            match place {
                Place::Subdirectory => (main.clone(), main.join("sub")),
                _ => (main.clone(), linked_worktree(&main, at.join("unit"))),
            }
        }
        Place::UnrecordedMainSubdirectory => {
            let checkout = at.join("apart");
            std::fs::create_dir_all(checkout.join("sub")).unwrap();
            init_repo(&checkout);
            std::fs::write(checkout.join("sub").join("f.txt"), "x\n").unwrap();
            git_commit_all(&checkout, "tree");
            git(
                &checkout,
                &["init", "-q", "--separate-git-dir", "../apart.git"],
            );
            let worktree = at.join("apart-wt");
            git(
                &checkout,
                &[
                    "worktree",
                    "add",
                    "-q",
                    worktree.to_str().unwrap(),
                    "-b",
                    "wt",
                ],
            );
            (worktree.clone(), worktree.join("sub"))
        }
    };
    if stored {
        seed_run_events(&root, RUN);
    }
    (root, here)
}

/// `rigger <args>` at `place` answers exactly what it answers at the root, whose store holds a run
/// the root's answer reports.
fn assert_reads_as_at_the_root(place: Place, args: &[&str]) {
    let at = tempfile::tempdir().unwrap();
    let (root, here) = layout(at.path(), place, true);
    let at_root = run_rigger(&root, args);
    assert!(
        !at_root.0.contains("no run") && !at_root.1.contains("no runs"),
        "the root's answer reports its run; rigger {}\nstdout: {}\nstderr: {}",
        args.join(" "),
        at_root.0,
        at_root.1
    );
    let at_place = run_rigger(&here, args);
    assert_eq!(
        at_place,
        at_root,
        "rigger {} at {} answers as at the root {}",
        args.join(" "),
        here.display(),
        root.display()
    );
}

/// `rigger <args>` from a subdirectory of a checkout whose root holds no store refuses in one
/// sentence naming `rigger setup` at the root, and makes no store anywhere.
fn assert_refuses_naming_setup_at_the_root(args: &[&str]) {
    let at = tempfile::tempdir().unwrap();
    let (root, here) = layout(at.path(), Place::Subdirectory, false);
    let (out, err, ok) = run_rigger(&here, args);
    let named = root.canonicalize().unwrap();
    let said = format!("rigger {}\nstdout: {out}\nstderr: {err}", args.join(" "));
    assert!(
        !ok && err.contains("`rigger setup`") && err.contains(named.to_str().unwrap()),
        "refuses naming `rigger setup` at {}; {said}",
        named.display()
    );
    assert_eq!(
        err.trim().lines().count(),
        1,
        "the refusal is one sentence; {said}"
    );
    for dir in [&root, &here] {
        assert!(
            !dir.join(".rigger").exists(),
            "no store is made at {}; {said}",
            dir.display()
        );
    }
}

use Place::{LinkedWorktree, Subdirectory, UnrecordedMainSubdirectory};

rigger::test_cases! {
    /// `stats` from a subdirectory reports the root's run.
    stats_from_a_subdirectory_reads_the_roots_store:
        assert_reads_as_at_the_root(Subdirectory, &["stats"]);
    /// `stats` from a linked worktree reports the root's run.
    stats_from_a_linked_worktree_reads_the_roots_store:
        assert_reads_as_at_the_root(LinkedWorktree, &["stats"]);
    /// `stats` from a subdirectory of a worktree that is its own root reports that root's run.
    stats_from_below_a_worktree_that_is_its_own_root_reads_its_store:
        assert_reads_as_at_the_root(UnrecordedMainSubdirectory, &["stats"]);
    /// `replay` from a subdirectory reads the root's run stream.
    replay_from_a_subdirectory_reads_the_roots_store:
        assert_reads_as_at_the_root(Subdirectory, &["replay", "nosuch", "--against", "HEAD"]);
    /// `replay` from a linked worktree reads the root's run stream.
    replay_from_a_linked_worktree_reads_the_roots_store:
        assert_reads_as_at_the_root(LinkedWorktree, &["replay", "nosuch", "--against", "HEAD"]);
    /// `replay` from a subdirectory of a worktree that is its own root reads that root's stream.
    replay_from_below_a_worktree_that_is_its_own_root_reads_its_store:
        assert_reads_as_at_the_root(UnrecordedMainSubdirectory, &["replay", "nosuch", "--against", "HEAD"]);
    /// `stats` below a root that holds no store says so, naming `rigger setup` at the root.
    stats_below_a_root_with_no_store_names_setup_there:
        assert_refuses_naming_setup_at_the_root(&["stats"]);
    /// `replay` below a root that holds no store says so, naming `rigger setup` at the root.
    replay_below_a_root_with_no_store_names_setup_there:
        assert_refuses_naming_setup_at_the_root(&["replay", "latest", "--against", "HEAD"]);
}
