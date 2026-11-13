//! Spec 89, criterion 4 (THE DRIVER RESOLVES THE MAIN WORKTREE, AND EXACTLY ONE ROOT), at the
//! periphery: the compiled binary's `rigger run` over real git repositories.
//!
//! `rigger run` refuses when its working directory, the repository git resolves for it and its
//! scratch root disagree on their root (`refuse_unless_one_root`), and it refuses before it
//! anchors the run branch, so a refused run leaves every repository it could have reached on the
//! branch it was on. Two independently divergible shapes trip the refusal: a git-less project
//! nested under another repository's scratch tree (the working directory is not the repository
//! git resolves), and a scratch root inside a different repository while the working directory
//! is the repository itself. `tests/cli.rs` owns the refusal from a linked worktree, naming both
//! trees.
//!
//! Reaching the main tree through a symlink is not a linked worktree: both sides of
//! `resolve_main_worktree_or_refuse`'s comparison are canonicalized, so a run from a symlinked
//! path drives exactly as from the real one. No other fixture reaches a symlinked path, since a
//! temporary directory here never resolves through one.
//!
//! A run's unit worktrees are linked worktrees of the checkout it starts in, and each finds the
//! run's store at the main checkout git records, so a run refuses in a repository git records no
//! main checkout for, before it anchors anything: from a checkout whose git directory lies apart,
//! its units would each be their own root and hold a store of their own.

use crate::common;

use common::cli::{run_payloads, run_recording, run_rigger_envs};
use common::cli::{write_workflow_fixture, REVIEWLESS_GIT_UNIT_WORKFLOW};
use common::git::temp_git_project_with_commit;
use common::git::{git_ok, git_out};
use rigger::worktree::branch_exists;
use std::path::{Path, PathBuf};

/// Where a refused run's scratch root and working directory sit, relative to a project.
#[derive(Clone, Copy)]
enum Divergence {
    /// A git-less project nested under the project's own scratch tree, run from there: its
    /// working directory is not the repository git resolves for it.
    NestedUnderAnotherRepo,
    /// The project itself, run with its scratch root inside a different repository.
    ScratchInAnotherRepo,
}

/// A run diverging as `divergence` says refuses with the one-root refusal whose text holds each
/// of `names`, and every repository involved keeps its checked-out branch and gains no run
/// branch: the refusal lands before the run branch is anchored.
fn assert_a_diverging_run_refuses_before_anchoring(divergence: Divergence, names: &[&str]) {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_workflow_fixture(root, &REVIEWLESS_GIT_UNIT_WORKFLOW);
    let other = temp_git_project_with_commit();
    let (cwd, scratch, repos): (PathBuf, PathBuf, Vec<&Path>) = match divergence {
        Divergence::NestedUnderAnotherRepo => {
            let scratch = root.join("scratchroot");
            let fixture = scratch.join("nested-fixture");
            write_workflow_fixture(&fixture, &REVIEWLESS_GIT_UNIT_WORKFLOW);
            (fixture, scratch, vec![root])
        }
        Divergence::ScratchInAnotherRepo => (
            root.to_path_buf(),
            other.path().join("scratch-elsewhere"),
            vec![root, other.path()],
        ),
    };
    let branches: Vec<String> = repos
        .iter()
        .map(|repo| git_out(repo, &["rev-parse", "--abbrev-ref", "HEAD"]))
        .collect();

    let (_out, err, ok) = run_rigger_envs(
        &cwd,
        &["run"],
        &[("RIGGER_TMPDIR", scratch.to_str().unwrap())],
    );

    assert!(!ok, "a run whose roots disagree refuses; stderr:\n{err}");
    for name in names {
        assert!(
            err.contains(name),
            "the refusal names {name:?}; stderr:\n{err}"
        );
    }
    for (repo, branch) in repos.iter().zip(&branches) {
        assert_eq!(
            &git_out(repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
            branch,
            "{} keeps its checked-out branch",
            repo.display()
        );
        assert!(
            !branch_exists(repo.to_str().unwrap(), "rigger-run"),
            "{} gains no run branch",
            repo.display()
        );
    }
}

rigger::test_cases! {
    /// A git-less project nested under another repository's scratch tree is refused naming the
    /// root disagreement.
    a_run_nested_under_another_repos_scratch_refuses_before_anchoring:
        assert_a_diverging_run_refuses_before_anchoring(
            Divergence::NestedUnderAnotherRepo,
            &["disagree on their root"],
        );
    /// A scratch root inside a different repository is refused naming the scratch root, even
    /// though the working directory is the repository itself.
    a_run_whose_scratch_root_is_in_another_repo_refuses_before_anchoring:
        assert_a_diverging_run_refuses_before_anchoring(
            Divergence::ScratchInAnotherRepo,
            &["scratch root", "DIFFERENT repository"],
        );
}

/// A run from a symlink onto the main tree is not refused as a linked worktree: it drives the
/// unit's implementer as a run from the real path does.
#[test]
fn a_run_via_a_symlinked_main_tree_is_not_refused() {
    let dir = temp_git_project_with_commit();
    let real_root = dir.path();
    write_workflow_fixture(real_root, &REVIEWLESS_GIT_UNIT_WORKFLOW);
    let link_parent = tempfile::tempdir().unwrap();
    let via_symlink = link_parent.path().join("via-symlink");
    std::os::unix::fs::symlink(real_root, &via_symlink).unwrap();
    let scratch = tempfile::tempdir().unwrap();

    let (out, err, ok, _fake) = run_recording(&via_symlink, &["run"], scratch.path());

    assert!(
        ok,
        "a run from a symlinked main tree succeeds; stdout: {out}\nstderr: {err}"
    );
    assert!(
        !err.contains("refusing to run from inside a linked worktree"),
        "the symlink never trips the linked-worktree refusal; stderr: {err}"
    );
    let requested: Vec<serde_json::Value> = run_payloads(real_root, "SpawnRequested")
        .into_iter()
        .map(|request| request["id"].clone())
        .collect();
    assert_eq!(
        requested,
        [serde_json::json!("solo/implementer#0")],
        "the run requested the unit's implementer"
    );
}

/// A checkout of a repository git records no main checkout for, a run could start in.
#[derive(Clone, Copy)]
enum NoMainCheckout {
    /// A clone whose git directory lies apart (`git clone --separate-git-dir`).
    GitDirApart,
    /// A linked worktree of such a clone.
    LinkedOfGitDirApart,
    /// A bare repository's linked worktree.
    BareLinked,
}

/// The checkout `layout` names, laid out under `at` from the committed project `origin`.
fn checkout_with_no_main(layout: NoMainCheckout, origin: &Path, at: &Path) -> PathBuf {
    let origin = origin.to_str().unwrap();
    let apart = |at: &Path| {
        git_ok(
            at,
            &[
                "clone",
                "-q",
                "--separate-git-dir",
                "apart.git",
                origin,
                "apart",
            ],
        );
        at.join("apart")
    };
    match layout {
        NoMainCheckout::GitDirApart => apart(at),
        NoMainCheckout::LinkedOfGitDirApart => {
            git_ok(
                apart(at),
                &["worktree", "add", "-q", "../apart-linked", "-b", "linked"],
            );
            at.join("apart-linked")
        }
        NoMainCheckout::BareLinked => {
            git_ok(at, &["clone", "-q", "--bare", origin, "bare.git"]);
            git_ok(
                at.join("bare.git"),
                &["worktree", "add", "-q", "../bare-wt", "-b", "linked"],
            );
            at.join("bare-wt")
        }
    }
}

/// A run from the checkout `layout` names refuses saying git records no main checkout and naming
/// the clone to run from instead, and the checkout gains no run branch.
fn assert_a_run_with_no_main_checkout_refuses(layout: NoMainCheckout) {
    let origin = temp_git_project_with_commit();
    let at = tempfile::tempdir().unwrap();
    let checkout = checkout_with_no_main(layout, origin.path(), at.path());
    write_workflow_fixture(&checkout, &REVIEWLESS_GIT_UNIT_WORKFLOW);
    let scratch = tempfile::tempdir().unwrap();

    let (out, err, ok, _fake) = run_recording(&checkout, &["run"], scratch.path());

    assert!(
        !ok,
        "a run git records no main checkout for refuses; stdout: {out}\nstderr: {err}"
    );
    for name in ["git records no main checkout", "git clone"] {
        assert!(
            err.contains(name),
            "the refusal names {name:?}; stderr:\n{err}"
        );
    }
    assert!(
        !branch_exists(checkout.to_str().unwrap(), "rigger-run"),
        "the checkout gains no run branch"
    );
}

rigger::test_cases! {
    /// A clone whose git directory lies apart is refused: its run's unit worktrees would each be
    /// their own root.
    a_run_from_a_checkout_whose_git_directory_lies_apart_refuses:
        assert_a_run_with_no_main_checkout_refuses(NoMainCheckout::GitDirApart);
    /// A linked worktree of that clone is refused the same way.
    a_run_from_a_linked_worktree_of_a_checkout_whose_git_directory_lies_apart_refuses:
        assert_a_run_with_no_main_checkout_refuses(NoMainCheckout::LinkedOfGitDirApart);
    /// A bare repository's linked worktree is refused the same way.
    a_run_from_a_bare_repositorys_linked_worktree_refuses:
        assert_a_run_with_no_main_checkout_refuses(NoMainCheckout::BareLinked);
}
