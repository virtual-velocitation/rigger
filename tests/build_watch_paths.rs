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
//!
//! sdet round 3: the two tests above only ever fixture a plain, freshly `git init`-ed
//! repository - never a LINKED git worktree, the exact same previously-unexercised input
//! class `tests/gitsemver_worktree_periphery.rs` closed for the sibling `derive_version`
//! seam (`sdet-u74c1-worktree-gap`), and for the identical reason: every one of this
//! project's own spec units - this very worktree included - is built inside one.
//! `git_watch_paths`'s own doc comment resolves TWO different roots (`--absolute-git-dir`,
//! worktree-LOCAL, for `HEAD`; `--path-format=absolute --git-common-dir`, SHARED, for the
//! branch ref/tags-dir/packed-refs) and its module doc comment asserts in prose that this
//! makes "a tag or commit made from ANY worktree re-trigger every worktree's own build" -
//! an affirmative claim about cross-worktree behavior no test here proves. The two tests
//! below close both gaps: a static proof that a worktree's watch list uses its OWN
//! `HEAD`/branch-ref paths (never primary's) while sharing primary's `refs/tags`/
//! `packed-refs` paths byte-for-byte, and a behavioral proof that a tag created from
//! PRIMARY actually touches a path the WORKTREE's own list watches - the literal condition
//! `cargo`'s `rerun-if-changed` protocol needs for that cross-worktree claim to hold in
//! practice, not just in the path strings.

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

/// A linked worktree branched off `fixture_repo(primary)`. Mirrors
/// `tests/gitsemver_worktree_periphery.rs`'s identical fixture step (including removing
/// the empty tempdir first - `git worktree add` creates its own target directory and
/// refuses to reuse an existing empty one on some git versions).
fn fixture_worktree(primary: &Path, worktree: &Path) {
    std::fs::remove_dir(worktree).unwrap();
    git(
        primary,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked-branch",
            worktree.to_str().unwrap(),
        ],
    );
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

#[test]
fn watch_paths_from_a_linked_worktree_use_its_own_head_and_branch_ref_never_primarys() {
    let primary = tempfile::tempdir().unwrap();
    fixture_repo(primary.path());
    let worktree = tempfile::tempdir().unwrap();
    fixture_worktree(primary.path(), worktree.path());

    let primary_paths = watch::git_watch_paths(primary.path());
    let worktree_paths = watch::git_watch_paths(worktree.path());

    let worktree_branch_ref = git_output(worktree.path(), &["symbolic-ref", "HEAD"]);
    let primary_common_git_dir = primary.path().join(".git");
    // Resolved authoritatively through git itself (never assumed from the worktree
    // tempdir's own basename): git is free to pick a different internal worktree name on
    // a collision, so this must match whatever `--absolute-git-dir` actually reports, the
    // same source `git_watch_paths` itself resolves through.
    let worktree_git_dir = Path::new(&git_output(
        worktree.path(),
        &["rev-parse", "--absolute-git-dir"],
    ))
    .to_path_buf();

    assert!(
        worktree_paths.contains(&worktree_git_dir.join("HEAD")),
        "a linked worktree's HEAD lives at its own per-worktree HEAD file \
         (.git/worktrees/<name>/HEAD), never the shared common dir's; got {worktree_paths:?}"
    );
    assert!(
        !worktree_paths.contains(&primary_common_git_dir.join("HEAD")),
        "a linked worktree's watch list must never watch PRIMARY's own HEAD file in its \
         place - that would silently miss every commit made only inside the worktree; \
         got {worktree_paths:?}"
    );
    assert!(
        worktree_paths.contains(&primary_common_git_dir.join(&worktree_branch_ref)),
        "the worktree's OWN branch ref ({worktree_branch_ref}), resolved from the \
         worktree's own HEAD content, must be watched (branch refs live in the shared \
         common dir, unlike HEAD itself); got {worktree_paths:?}"
    );
    assert!(
        !worktree_paths
            .iter()
            .any(|p| p.ends_with("refs/heads/master") || p.ends_with("refs/heads/main")),
        "the worktree's watch list must never carry primary's own branch ref in place of \
         its own diverged one; got {worktree_paths:?}"
    );

    // refs/tags and packed-refs are SHARED state (git's own semantics: only one worktree
    // may have a given branch checked out at a time, but tags belong to the repository as
    // a whole) - this project's own build.rs comment claims a tag made from ANY worktree
    // re-triggers every worktree's build, which is only true if these two paths are
    // byte-identical between primary's and the worktree's own lists, not merely present in
    // both under the same relative name.
    let shared_tail = [
        primary_common_git_dir.join("refs/tags"),
        primary_common_git_dir.join("packed-refs"),
    ];
    for shared in shared_tail {
        assert!(
            primary_paths.contains(&shared),
            "sanity: primary's own list must contain {shared:?}; got {primary_paths:?}"
        );
        assert!(
            worktree_paths.contains(&shared),
            "the worktree's watch list must point at the SAME shared-common-dir path \
             primary's does ({shared:?}), not a worktree-local lookalike, so a tag made \
             from either checkout re-triggers both builds; got {worktree_paths:?}"
        );
    }
}

#[test]
fn a_tag_made_from_primary_touches_a_path_the_worktrees_own_list_watches() {
    // The behavioral counterpart to the static proof above, mirroring
    // `a_tag_only_change_actually_touches_a_watched_paths_on_disk_state`'s on-disk-state
    // technique: a tag created from PRIMARY must change the on-disk state of at least one
    // path the WORKTREE's own `git_watch_paths` result contains - the literal condition
    // cargo's `rerun-if-changed` protocol needs for the module doc comment's cross-worktree
    // claim ("a tag or commit made from ANY worktree re-triggers every worktree's own
    // build") to actually hold for a build running inside the worktree, not just for one
    // running inside primary.
    let primary = tempfile::tempdir().unwrap();
    fixture_repo(primary.path());
    let worktree = tempfile::tempdir().unwrap();
    fixture_worktree(primary.path(), worktree.path());

    let worktree_paths = watch::git_watch_paths(worktree.path());
    let before: Vec<Option<std::time::SystemTime>> = worktree_paths
        .iter()
        .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .collect();

    // Tag the commit from PRIMARY, not from inside the worktree.
    git(primary.path(), &["tag", "v1.0.0"]);

    let after: Vec<Option<std::time::SystemTime>> = worktree_paths
        .iter()
        .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .collect();

    assert_ne!(
        before, after,
        "tagging the commit from PRIMARY must still change the on-disk state of at least \
         one path the WORKTREE's own watch list contains, or a `cargo build` run inside \
         the worktree (this project's own primary way of building every spec unit) will \
         never re-derive the version after a tag made from primary or any other sibling \
         worktree; worktree's watched paths: {worktree_paths:?}"
    );
}
