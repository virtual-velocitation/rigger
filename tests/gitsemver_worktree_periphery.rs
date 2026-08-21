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
//! Verified independently (outside any test, against a disposable fixture) before writing
//! this test: `go-gitsemver`'s git library cannot resolve `HEAD` through a linked
//! worktree's `gitdir:` indirection - `-p <linked-worktree-dir> --show-variable
//! FullSemVer` exits non-zero with "resolving target branch: getting HEAD: reference not
//! found", even though the SAME commit resolves correctly when `-p` points at the
//! worktree's own primary checkout. That non-zero exit is already covered by
//! [`gitsemver::derive_version`]'s documented "ANY failure" fallback contract (the same
//! branch `tool_not_found_...` and `outside_a_git_checkout_...` in
//! `tests/gitsemver_derivation.rs` already exercise), so this is NOT a contract violation
//! for criterion 1 (never fabricates, never fails the build) - it is locked in here as a
//! real, previously-unverified scenario, not a bug this unit's code must fix.
//!
//! Practical consequence (noted for visibility, not asserted as a requirement of this
//! test): a build performed inside any linked worktree - this project's own primary way
//! of building each spec unit - reports `+unversioned` even when the tool is present and
//! the checkout is real, so the "version moves with the tree" goal spec 74's Problem
//! statement opens with is not realized in that build context. That is a product-level
//! observation outside criterion 1's literal scope (which names only "tool absent" and
//! "outside a checkout"), left for a follow-up spec rather than fixed here.

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
fn a_linked_worktree_falls_back_to_the_unversioned_marker_even_though_it_is_a_real_checkout() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let primary_dir = tempfile::tempdir().unwrap();
    fixture_repo(primary_dir.path());

    // Sanity check: the SAME commit history derives a real version when NOT invoked
    // through a worktree indirection, so the fallback asserted below can only be
    // attributed to the worktree, never to thin or malformed fixture history.
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
        worktree_version,
        format!(
            "{}{}",
            env!("CARGO_PKG_VERSION"),
            gitsemver::UNVERSIONED_SUFFIX
        ),
        "a linked git worktree must fall back to the bare crate semver plus the explicit \
         unversioned marker (go-gitsemver cannot resolve HEAD through the worktree's \
         gitdir indirection), matching the documented ANY-failure contract - never \
         fabricate a derived-looking version, never fail the build; got: {worktree_version}"
    );
}
