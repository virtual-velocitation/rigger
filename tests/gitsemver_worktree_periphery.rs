//! Periphery test for spec 74 criterion 1, closing a gap left open by the derivation
//! seam's own fixture tests (`tests/gitsemver_derivation.rs`): every one of that file's
//! fixtures is a plain, freshly `git init`-ed repository. None of them exercise a LINKED
//! git worktree - a directory whose `.git` is a file (a `gitdir:` pointer into a shared
//! `.git/worktrees/<name>` directory) rather than a full `.git` directory. A worktree
//! genuinely IS a git checkout with real commit history, distinct from both fallback
//! causes criterion 1's Done-when text names ("tool absent" and "outside a checkout"), so
//! it is a real, previously-unexercised input class for [`gitsemver::derive_version`].
//!
//! This matters concretely for this project: every one of rigger's own spec units is
//! built inside a linked worktree exactly like the one this test constructs (this very
//! test file's own worktree included), so this is not a hypothetical edge case.
//!
//! `go-gitsemver`'s OWN git library cannot resolve `HEAD` through a linked worktree's
//! `gitdir:` indirection - `-p <linked-worktree-dir> --show-variable FullSemVer` exits
//! non-zero with "resolving target branch: getting HEAD: reference not found", even
//! though the SAME commit resolves correctly when `-p` points at the worktree's own
//! primary checkout. [`gitsemver::derive_version`] works around this the same way
//! `build.rs`'s `git_watch_paths` already resolves a linked worktree's shared git state
//! for `BUILD_PROVENANCE`: it resolves the primary checkout's root and this checkout's
//! own `HEAD` commit through REAL `git rev-parse` first, then hands `go-gitsemver`
//! `-p <primary root> -c <that commit>` - coordinates it CAN resolve, still delegating
//! 100% of the version computation to the tool. So a build performed inside a linked
//! worktree now derives the SAME real version the primary checkout would, matching the
//! "version moves with the tree" goal spec 74's Problem statement opens with.

#[path = "../build/gitsemver.rs"]
#[allow(dead_code)]
mod gitsemver;

use std::path::Path;
use std::process::Command;

/// Run `git <args>` in `root`, panicking with stderr on failure - fixture setup must
/// never silently half-succeed. Mirrors `tests/gitsemver_derivation.rs`'s identical
/// helper (integration-test binaries cannot share private helpers without a `tests/common`
/// module, and this fixture's needs - a second `worktree add` step - differ enough from
/// that file's to not warrant one).
fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("spawning git {args:?} failed: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build a fixture git repository at `root`: the same committed `go-gitsemver.yml`
/// (`mode: Mainline`, `tag-prefix: v`) as this repo's own root, an initial commit tagged
/// `v1.0.0`, then one more plain commit - enough history for a real derivation to
/// succeed, so a failure to derive from the worktree built off it can only be attributed
/// to the worktree indirection, never to thin fixture history.
fn fixture_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(
        root.join("go-gitsemver.yml"),
        "mode: Mainline\ntag-prefix: v\n",
    )
    .expect("write fixture go-gitsemver.yml");
    git(root, &["add", "go-gitsemver.yml"]);
    git(root, &["commit", "-q", "-m", "chore: initial"]);
    git(root, &["tag", "v1.0.0"]);
    std::fs::write(root.join("file.txt"), "second\n").expect("write fixture file");
    git(root, &["add", "file.txt"]);
    git(root, &["commit", "-q", "-m", "docs: update the readme"]);
}

/// Same availability gate as `tests/gitsemver_derivation.rs`, for the same reason:
/// provisioning the environment with `go-gitsemver` on PATH is criterion 3's, not this
/// test's.
fn gitsemver_available() -> bool {
    Command::new("go-gitsemver")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn a_linked_worktree_derives_the_same_real_version_as_its_primary_checkout() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let primary_dir = tempfile::tempdir().unwrap();
    fixture_repo(primary_dir.path());

    // The version derived directly from the primary checkout: the reference the
    // worktree-derived version below must match exactly, since both point at the same
    // commit.
    let primary_version = gitsemver::derive_version("go-gitsemver", primary_dir.path());
    assert!(
        !primary_version.contains("unversioned"),
        "the fixture history must derive a real version from its primary checkout so \
         the worktree comparison below is meaningful; got: {primary_version}"
    );

    let worktree_dir = tempfile::tempdir().unwrap();
    // Remove the empty tempdir itself first - `git worktree add` creates its own
    // target directory and refuses to reuse an existing empty one on some git versions.
    std::fs::remove_dir(worktree_dir.path()).unwrap();
    git(
        primary_dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked-branch",
            worktree_dir.path().to_str().unwrap(),
        ],
    );

    let worktree_version = gitsemver::derive_version("go-gitsemver", worktree_dir.path());

    assert_eq!(
        worktree_version, primary_version,
        "a linked git worktree (`.git` a file pointing into the shared \
         `.git/worktrees/...` directory - this project's own primary way of building \
         every spec unit) must derive the SAME real version its primary checkout does, \
         not silently fall back to the unversioned marker: go-gitsemver's own git \
         library cannot resolve HEAD through the worktree indirection, so \
         derive_version resolves the primary checkout root and this checkout's own HEAD \
         commit itself (via real `git rev-parse`) and hands them to the tool as -p/-c; \
         got worktree={worktree_version} primary={primary_version}"
    );
    assert!(
        !worktree_version.contains("unversioned"),
        "a linked-worktree derivation must never carry the fallback marker when the \
         tool is present and the checkout is real; got: {worktree_version}"
    );
}
