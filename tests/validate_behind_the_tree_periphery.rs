//! CLI periphery for spec 74, criterion 2 - the behind-the-tree and missing-go-gitsemver-
//! binary advisories `rigger validate` gains (`missing_gitsemver_binary_advisory`,
//! `behind_the_tree_message`, `behind_the_tree_advisory`, `git_commit_distance` in
//! `src/main.rs`).
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN `mod tests`: every one of those four
//! functions is a PRIVATE, non-`pub` free function inside the `rigger` BINARY crate
//! (`src/main.rs`), not the `rigger` LIBRARY crate - so an integration-test binary under
//! `tests/` cannot call any of them directly (unlike `gitsemver::derive_version`, which is
//! reachable here only because it lives in a `#[path]`-included plain source file). The
//! implementer's own colocated unit tests already prove each function's pure decision and
//! `behind_the_tree_advisory`'s direct-call composition against a real git repo and a real
//! `go-gitsemver` - see `src/main.rs`'s `mod tests` for `git_commit_distance_counts_commits_
//! ahead_in_a_real_repo`, `missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_
//! marker`, the four `behind_the_tree_message_*` tests, and the two `behind_the_tree_
//! advisory_*` tests. What NONE of those can see, by construction (a direct call inside the
//! same compilation unit, never the compiled product): whether `validate_advisories` (the
//! actual `cmd_validate` wiring, reached only through the real `rigger validate` CLI) truly
//! composes `GITSEMVER_VERSION`/`BUILD_PROVENANCE` into these functions, prints exactly what
//! an operator sees on stdout/stderr, and leaves the exit status untouched - the Design's own
//! "advisory tone; never a hard failure" contract, observable ONLY at the compiled-binary
//! boundary.
//!
//! Two tests:
//!   1. `validate_names_the_behind_the_tree_advisory_through_the_real_binary` - the FIRE path.
//!      A project genuinely descended from THIS test binary's own build commit
//!      (`RIGGER_BUILD_PROVENANCE`), with a real `go-gitsemver`-derivable version that
//!      differs from `RIGGER_GITSEMVER_VERSION`, drives `rigger validate` for real and pins
//!      the exact advisory text - both versions and the commit distance
//!      (`git_commit_distance`'s wiring) - plus the untouched exit status.
//!   2. `validate_stays_silent_...for_an_ordinary_project` - the default/common path. An
//!      ordinary, unrelated temp project (the shape every non-self-hosting `rigger validate`
//!      caller actually has) stays silent on both new advisories.
//!
//! Test 2 also pins a boundary BUG this file's own construction surfaced: `behind_the_tree_
//! advisory` calls the real `git_is_ancestor` UNCONDITIONALLY on every `rigger validate`
//! invocation (never guarded the way `workflow_drift_advisory` guards its own use of the same
//! helper behind "a drift was already detected"), with THIS binary's own build commit as the
//! id to resolve - which is absent from the git history of every target project that is not
//! rigger's own source tree, i.e. nearly every real caller. `git_is_ancestor` used to run its
//! `git merge-base --is-ancestor` child with `.status()`, which INHERITS the parent's stdio,
//! so the unresolvable-id case (a case its own doc comment already calls out as normal and
//! meant to resolve to a silent `None`) leaked git's raw `fatal: Not a valid object name
//! <hash>` straight onto `rigger validate`'s own curated advisory stderr - on nearly every
//! invocation against nearly every real project. Fixed in the same commit as this test
//! (`.output()` instead of `.status()`, discarding the child's stdio instead of inheriting
//! it - mirrors `git_commit_distance`'s already-correct pattern); test 2 is the regression
//! guard.
//!
//! NOT OWNED here: the pure decision logic and the direct-call real-git/real-go-gitsemver
//! composition (the implementer's own tests, above); `go-gitsemver`'s derivation fallback
//! itself (`tests/gitsemver_derivation.rs`, `tests/gitsemver_worktree_periphery.rs`, spec 74
//! criterion 1).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

/// A throwaway project: its own git repo with a `.rigger` dir - mirrors
/// `tests/validate_advisories.rs`'s identical fixture.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    std::fs::create_dir_all(dir.path().join(".rigger")).expect("create .rigger");
    dir
}

/// Run `rigger <args...>` in `cwd`, returning (stdout, stderr, success) - mirrors
/// `tests/validate_advisories.rs`'s identical helper.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// `git <args>` in `root` with a fixed committer identity, panicking with stderr on failure -
/// mirrors `tests/gitsemver_worktree_periphery.rs`'s identical helper.
fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e")
        .output()
        .unwrap_or_else(|e| panic!("spawning git {args:?} failed: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `git <args>` in `root`, returning trimmed stdout, panicking with stderr on failure -
/// mirrors `tests/gitsemver_worktree_periphery.rs`'s identical helper.
fn git_output(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        // Same fixed identity as `git()` above: commands like `commit-tree` create
        // commits too, and a CI runner has no global git identity to fall back on.
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e")
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

/// Same availability gate as `tests/gitsemver_derivation.rs` and `tests/gitsemver_worktree_
/// periphery.rs`: provisioning the environment with `go-gitsemver` on PATH is criterion 3's,
/// not this test's.
fn gitsemver_available() -> bool {
    Command::new("go-gitsemver")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The `objects` dir of the git repository THIS test binary was compiled from - the same
/// object store `RIGGER_BUILD_PROVENANCE`'s commit lives in, whether this checkout is a
/// plain clone or (as every one of this project's own spec units is built) a linked
/// worktree, in which case `--git-common-dir` already resolves to the shared primary `.git`.
fn source_objects_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let common_dir = git_output(manifest_dir, &["rev-parse", "--git-common-dir"]);
    let common_dir = PathBuf::from(common_dir);
    let common_dir = if common_dir.is_absolute() {
        common_dir
    } else {
        manifest_dir.join(common_dir)
    };
    common_dir.join("objects")
}

/// Widen `root`'s object visibility to include every object `source_objects_dir` holds, via
/// git's standard `objects/info/alternates` mechanism - the same sharing a linked worktree
/// already gets natively, borrowed here into a plain `git init`-ed repo so a commit id from
/// the real project resolves without cloning or copying anything.
fn borrow_objects(root: &Path, objects_dir: &Path) {
    let info_dir = root.join(".git").join("objects").join("info");
    std::fs::create_dir_all(&info_dir).expect("create .git/objects/info");
    std::fs::write(
        info_dir.join("alternates"),
        format!("{}\n", objects_dir.display()),
    )
    .expect("write objects/info/alternates");
}

/// Build, at `root`, a checkout genuinely DESCENDED from THIS test binary's own build commit
/// (`env!("RIGGER_BUILD_PROVENANCE")`) - the real-ancestry condition `behind_the_tree_
/// advisory`'s `git_is_ancestor` call needs to decide "ahead" - without checking out
/// rigger's own (large, real) tree, which would drag in this project's committed skill/
/// handbook docs and reopen spec 20's unrelated docs-drift gate (`docs_drift_failure`,
/// `cmd_validate`'s own last, HARD-failing check) as an incidental dependency of this test.
///
/// Technique: borrow objects from the real repo (`borrow_objects`, above) so `RIGGER_BUILD_
/// PROVENANCE` resolves as a real commit, then graft a FRESH, fully independent tree - a
/// valid `rigger init` scaffold plus a committed `go-gitsemver.yml` - as one child commit of
/// it via `git commit-tree` (never `git checkout`, which would materialize the real tree
/// instead). The child is also TAGGED, so `go-gitsemver`'s Mainline walk from `HEAD` stops
/// there instead of traversing this project's real, and possibly deep, history looking for a
/// reachable tag - keeping the derived checkout version cheap and fully under this
/// function's own control (`0.9.0`, the tag's own value, since zero commits separate `HEAD`
/// from it).
///
/// Returns `None` when `RIGGER_BUILD_PROVENANCE` is not resolvable through the borrowed
/// objects (the `"unknown"` sentinel `build.rs` falls back to outside a git checkout, or an
/// environment where this test binary's own build commit was pruned/gc'd since) - the
/// caller skips rather than fabricating a scenario that cannot arise for real.
fn scaffold_ahead_checkout(root: &Path) -> Option<()> {
    git(root, &["init", "-q"]);
    borrow_objects(root, &source_objects_dir());
    let provenance = env!("RIGGER_BUILD_PROVENANCE");
    let resolved = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", provenance])
        .current_dir(root)
        .output()
        .ok()?;
    if !resolved.status.success() {
        return None;
    }
    let parent = String::from_utf8(resolved.stdout).ok()?.trim().to_string();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(
        ok,
        "rigger init must succeed while building the fixture; stderr:\n{err}"
    );
    std::fs::write(
        root.join("go-gitsemver.yml"),
        "mode: Mainline\ntag-prefix: v\n",
    )
    .expect("write fixture go-gitsemver.yml");

    git(root, &["add", "-A"]);
    let tree = git_output(root, &["write-tree"]);
    let child = git_output(
        root,
        &[
            "commit-tree",
            &tree,
            "-p",
            &parent,
            "-m",
            "chore: import scaffold",
        ],
    );
    git(root, &["update-ref", "HEAD", &child]);
    git(root, &["tag", "v0.9.0", &child]);
    Some(())
}

// ---------------------------------------------------------------------------------------
// 1. The fire path - a checkout genuinely ahead of the installed binary
// ---------------------------------------------------------------------------------------

#[test]
fn validate_names_the_behind_the_tree_advisory_through_the_real_binary() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let Some(()) = scaffold_ahead_checkout(root) else {
        eprintln!(
            "skipping: RIGGER_BUILD_PROVENANCE ({}) is not resolvable through the borrowed \
             object store in this environment",
            env!("RIGGER_BUILD_PROVENANCE")
        );
        return;
    };
    let installed_version = env!("RIGGER_GITSEMVER_VERSION");

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "an advisory must never fail validate's exit status (Design: advisory tone, never a \
         hard failure); stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("checkout's derived version (0.9.0"),
        "validate must name the checkout's freshly re-derived version (proves the runtime \
         gitsemver::derive_version wiring); stderr:\n{err}"
    );
    assert!(
        err.contains(installed_version),
        "validate must name the installed binary's own version ({installed_version}); \
         stderr:\n{err}"
    );
    assert!(
        err.contains("is 1 commit(s) ahead"),
        "validate must name the exact commit distance (proves git_commit_distance's own \
         wiring through the real CLI, not merely its direct-call unit test); stderr:\n{err}"
    );
    assert!(
        err.contains("rebuild rigger so the binary matches the tree"),
        "validate must name the fix; stderr:\n{err}"
    );
    assert!(
        !err.to_lowercase().contains("fatal:") && !err.contains("Not a valid object name"),
        "the real git_is_ancestor call this advisory makes must never leak a raw git error \
         onto validate's own curated stderr, even on a genuinely decidable ancestor; \
         stderr:\n{err}"
    );
}

// ---------------------------------------------------------------------------------------
// 2. The default path - an ordinary project unrelated to rigger's own history
// ---------------------------------------------------------------------------------------

#[test]
fn validate_stays_silent_on_the_version_advisories_and_leaks_no_raw_git_error_for_an_ordinary_project(
) {
    // Sanity for this test's premise: it asserts SILENCE on the missing-binary advisory,
    // which is only meaningful when this binary's own embedded version is genuinely
    // derived (not `+unversioned`) - true whenever `go-gitsemver` was on PATH when this
    // test binary itself was built, which criterion 1's own derivation-seam tests own
    // proving in general; this is a narrower, environment-specific check.
    assert!(
        !env!("RIGGER_GITSEMVER_VERSION").ends_with("unversioned"),
        "this test's installed-version premise does not hold in this build environment"
    );

    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must succeed on an ordinary project unrelated to rigger's own history; \
         stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains("ahead of the installed rigger binary"),
        "an ordinary project shares no commit history with the installed binary, so the \
         ancestry is undecidable (not merely zero) and the behind-the-tree advisory must \
         stay silent; stderr:\n{err}"
    );
    assert!(
        !err.contains("could not be derived by go-gitsemver"),
        "this environment's installed binary carries a genuinely derived version (asserted \
         above), so the missing-binary advisory must stay silent; stderr:\n{err}"
    );
    assert!(
        !err.to_lowercase().contains("fatal:") && !err.contains("Not a valid object name"),
        "regression guard: behind_the_tree_advisory calls the real git_is_ancestor \
         UNCONDITIONALLY on every validate run, with THIS binary's own build commit as the \
         id to resolve - absent from the git history of every project that is not rigger's \
         own source tree, i.e. nearly every real target project. Before the git_is_ancestor \
         fix (`.output()` instead of `.status()`, so the child's stdio is captured rather \
         than inherited) this leaked git's raw `fatal: Not a valid object name <hash>` onto \
         validate's own curated advisory stderr on every such invocation; stderr:\n{err}"
    );
}
