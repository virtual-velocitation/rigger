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
//! worktree now derives ITS OWN real version - the same one its primary checkout would
//! report if the two point at the same commit, and a genuinely different one once the
//! worktree moves ahead - matching the "version moves with the tree" goal spec 74's
//! Problem statement opens with.
//!
//! The regression this must actually catch is narrower than "a worktree derives SOME
//! real version": every real spec-unit worktree in this project is committed AHEAD of
//! whatever primary happens to have checked out, so the test below commits once more
//! INSIDE the worktree after `git worktree add` before deriving from it. A fixture whose
//! worktree HEAD never diverges from primary's cannot tell "`derive_version` correctly
//! resolved the worktree's OWN HEAD" apart from "`derive_version` silently fell back to
//! primary's already-checked-out HEAD" - both would emit the identical version. Once the
//! worktree HEAD diverges, the two are distinguishable in exactly the way that matters:
//! the expected version is computed independently below, from coordinates obtained
//! through a plain `git rev-parse HEAD` in the worktree directory (a basic git
//! operation, correct regardless of the linked-worktree indirection this seam works
//! around) rather than by re-running any of `gitsemver::resolve_repo`'s own logic - so a
//! regression that resolves the wrong commit changes what `derive_version` reports
//! without changing what this test expects, and the assertion catches the divergence.

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

/// Run `git <args>` in `root` and return trimmed stdout, panicking with stderr on
/// failure. Deliberately independent of [`gitsemver::git_rev_parse`]: this test needs a
/// plain, trusted `git rev-parse HEAD` to build its own expectation from, never a call
/// through the code under test's own resolution logic - otherwise a regression in that
/// logic could shift both the expectation and the result together and the test would
/// stay green.
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
fn a_linked_worktree_derives_its_own_diverged_head_not_primarys() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let primary_dir = tempfile::tempdir().unwrap();
    fixture_repo(primary_dir.path());

    // The version derived directly from the primary checkout, at the commit the worktree
    // will be created from below - the reference point the worktree-derived version must
    // DIFFER from once the worktree gets its own extra commit.
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

    // Commit again, ONLY inside the worktree, so its HEAD genuinely diverges from
    // primary's already-checked-out HEAD - see the module doc comment for why an
    // undiverged worktree cannot distinguish the fix from the regression it guards.
    // `feat:` guarantees a minor bump under this fixture's Mainline config, mirroring
    // `tests/gitsemver_derivation.rs::a_feat_commit_after_a_tag_increments_the_minor`.
    std::fs::write(worktree_dir.path().join("worktree-only.txt"), "third\n")
        .expect("write worktree-only fixture file");
    git(worktree_dir.path(), &["add", "worktree-only.txt"]);
    git(
        worktree_dir.path(),
        &["commit", "-q", "-m", "feat: add a worktree-only file"],
    );

    let worktree_version = gitsemver::derive_version("go-gitsemver", worktree_dir.path());

    // The independently-derived expectation for the worktree's own diverged HEAD:
    // obtained via a plain, trusted `git rev-parse HEAD` run directly in the worktree
    // (never through `gitsemver::resolve_repo`), handed to `go-gitsemver` with `-p`
    // pointed at `primary_dir` (already known, not re-derived) and `-c` at that commit.
    // This path shares nothing with `derive_version`'s own resolution logic, so it
    // cannot drift together with a regression in that logic.
    let worktree_head = git_output(worktree_dir.path(), &["rev-parse", "HEAD"]);
    let expected_out = Command::new("go-gitsemver")
        .arg("--no-repair-worktree-config")
        .arg("-p")
        .arg(primary_dir.path())
        .arg("-c")
        .arg(&worktree_head)
        .arg("--show-variable")
        .arg("FullSemVer")
        .output()
        .expect("go-gitsemver FullSemVer must run against the worktree's own HEAD");
    assert!(
        expected_out.status.success(),
        "computing the independent expectation must itself succeed: {}",
        String::from_utf8_lossy(&expected_out.stderr)
    );
    let expected_full_semver = String::from_utf8(expected_out.stdout)
        .unwrap()
        .trim()
        .to_string();
    assert!(
        expected_full_semver.starts_with("1.1.0"),
        "the worktree-only feat: commit must bump the minor, exactly as \
         tests/gitsemver_derivation.rs's identical-shaped scenario does, so this \
         expectation is actually distinct from primary_version below and the comparison \
         is meaningful; got: {expected_full_semver}"
    );

    assert_ne!(
        worktree_version, primary_version,
        "the worktree committed one MORE change than primary after the worktree was \
         created, so its derived version must differ from primary's - a match here \
         would mean the worktree's own new commit was silently ignored; got \
         worktree={worktree_version} primary={primary_version}"
    );
    assert!(
        worktree_version.starts_with(&expected_full_semver),
        "a linked git worktree (`.git` a file pointing into the shared \
         `.git/worktrees/...` directory - this project's own primary way of building \
         every spec unit) must derive the version for its OWN HEAD, not silently fall \
         back to primary's already-checked-out HEAD or to the unversioned marker: \
         go-gitsemver's own git library cannot resolve HEAD through the worktree \
         indirection, so derive_version resolves the primary checkout root and this \
         checkout's own HEAD commit itself (via real `git rev-parse`) and hands them to \
         the tool as -p/-c; got worktree={worktree_version} expected_prefix={expected_full_semver}"
    );
    assert!(
        !worktree_version.contains("unversioned"),
        "a linked-worktree derivation must never carry the fallback marker when the \
         tool is present and the checkout is real; got: {worktree_version}"
    );
}
