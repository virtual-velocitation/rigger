//! Periphery test for spec 74 criterion 1, round 4: proves `git_watch_paths` watches the
//! loose `refs/tags` directory and `packed-refs`, not just `HEAD` and the current branch
//! ref, so a tag-only change - `go-gitsemver`'s Mainline-mode PRIMARY version-order input,
//! per `build/gitsemver.rs`'s own module doc comment - actually re-triggers `build.rs`'s
//! `cargo:rerun-if-changed` protocol instead of leaving `RIGGER_GITSEMVER_VERSION` stale
//! until an unrelated watched path happens to change.
//!
//! `git_watch_paths` is extracted from `build.rs` into `build/watch.rs`, `#[path]`-included
//! from both `build.rs` and this file, for the identical reason `build/gitsemver.rs`'s own
//! module doc comment already gives for the same technique: a build script cannot be
//! exercised by `cargo test` directly (it is compiled and run as its own crate before the
//! package builds, never linked into a test binary), so the seam that decides WHICH paths
//! re-trigger it has to live in a plain file with no `fn main`, included by both sides,
//! rather than a reimplementation of it.

#[path = "../build/watch.rs"]
#[allow(dead_code)]
mod watch;

use std::path::Path;
use std::process::Command;

/// Run `git <args>` in `root`, panicking with stderr on failure - fixture setup must
/// never silently half-succeed. Mirrors `tests/gitsemver_derivation.rs`'s identical helper.
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

/// Run `git <args>` in `root` and return trimmed stdout, panicking with stderr on
/// failure. Deliberately independent of `watch::git_watch_paths`'s own git invocations:
/// this test needs a plain, trusted `git symbolic-ref` to build its own expectation from,
/// never a call through the code under test's own resolution logic.
fn git_output(root: &Path, args: &[&str]) -> String {
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
    String::from_utf8(out.stdout)
        .unwrap_or_else(|e| panic!("git {args:?} produced non-utf8 output: {e}"))
        .trim()
        .to_string()
}

/// A plain fixture repo: one commit, no tag yet - just enough for `git_watch_paths` to
/// resolve a real `.git` directory and a real current branch.
fn fixture_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("f.txt"), "one\n").expect("write fixture file");
    git(root, &["add", "f.txt"]);
    git(root, &["commit", "-q", "-m", "chore: initial"]);
}

#[test]
fn watch_paths_cover_head_the_branch_ref_the_tags_dir_and_packed_refs() {
    let repo = tempfile::tempdir().unwrap();
    fixture_repo(repo.path());

    let branch_ref = git_output(repo.path(), &["symbolic-ref", "HEAD"]);
    let common_git_dir = repo.path().join(".git");

    let paths = watch::git_watch_paths(repo.path());

    assert!(
        paths.contains(&common_git_dir.join("HEAD")),
        "must watch HEAD itself so a detached-HEAD checkout move re-triggers the build; \
         got {paths:?}"
    );
    assert!(
        paths.contains(&common_git_dir.join(&branch_ref)),
        "must watch the resolved branch ref ({branch_ref}) so a new commit on the branch \
         re-triggers the build; got {paths:?}"
    );
    assert!(
        paths.contains(&common_git_dir.join("refs/tags")),
        "must watch the loose tag-ref directory so a fresh `git tag` re-triggers the \
         build - go-gitsemver's Mainline mode takes the nearest reachable tag as a \
         PRIMARY input, a state the HEAD/branch-ref watch alone cannot see; got {paths:?}"
    );
    assert!(
        paths.contains(&common_git_dir.join("packed-refs")),
        "must watch packed-refs so a tag introduced by ref-packing (e.g. after `git gc` \
         or a shallow/packed clone) also re-triggers the build, not only a freshly \
         created loose tag; got {paths:?}"
    );
}

#[test]
fn a_tag_only_change_actually_touches_a_watched_paths_on_disk_state() {
    // The behavioral proof, not just the static path list: snapshot the existence/mtime
    // of every watched path, tag the commit with NO other change (HEAD and the branch
    // ref stay byte-identical - the exact shape `adv3-u74c1-tag-rebuild-fell-through-
    // cracks` reproduced), and assert at least one watched path's on-disk state actually
    // changed. This is the precise condition cargo's `rerun-if-changed` protocol relies
    // on to decide whether to re-run `build.rs`; a "list contains the right strings"
    // check alone cannot prove the fix actually causes a rebuild.
    let repo = tempfile::tempdir().unwrap();
    fixture_repo(repo.path());

    let head_before = std::fs::read(repo.path().join(".git/HEAD")).unwrap();
    let branch_ref = git_output(repo.path(), &["symbolic-ref", "HEAD"]);
    let branch_ref_path = repo.path().join(".git").join(&branch_ref);
    let branch_ref_before = std::fs::read(&branch_ref_path).unwrap();

    let paths = watch::git_watch_paths(repo.path());
    let before: Vec<Option<std::time::SystemTime>> = paths
        .iter()
        .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .collect();

    // Tag-only change: no HEAD move, no branch-ref move, no other file touched.
    git(repo.path(), &["tag", "v1.0.0"]);

    assert_eq!(
        head_before,
        std::fs::read(repo.path().join(".git/HEAD")).unwrap(),
        "the fixture step itself must be tag-only: HEAD must stay byte-identical"
    );
    assert_eq!(
        branch_ref_before,
        std::fs::read(&branch_ref_path).unwrap(),
        "the fixture step itself must be tag-only: the branch ref must stay byte-identical"
    );

    let after: Vec<Option<std::time::SystemTime>> = paths
        .iter()
        .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .collect();

    assert_ne!(
        before, after,
        "tagging the commit changed neither HEAD nor the branch ref, so at least one \
         watched path's on-disk state (existence or mtime) must still change or cargo \
         will never re-run build.rs after a tag-only change; watched paths: {paths:?}"
    );
}
