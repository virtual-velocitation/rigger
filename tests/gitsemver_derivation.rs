//! Periphery test for spec 74 criterion 1: the build-time `go-gitsemver` delegation and
//! its fallback, proven at the derivation seam with fixture git repositories and the
//! real `go-gitsemver` binary.
//!
//! `build.rs` cannot be exercised by `cargo test` directly - it is compiled and run as
//! its own crate before the package builds, never linked into a test binary. So the
//! derivation logic (spawn `go-gitsemver`, parse its output, decide the fallback) lives
//! in `build/gitsemver.rs`, a plain source file with no `fn main`, included by BOTH
//! `build.rs` (`#[path]`, compiled into the build-script crate) and this file
//! (`#[path]`, compiled into this integration-test binary). That is the ONE derivation
//! seam: this test drives the exact code `build.rs` embeds at compile time, never a
//! reimplementation of it - so a regression here is a regression the shipped binary
//! actually has.
//!
//! Four scenarios, matching the Done-when text verbatim:
//! 1. a fixture repo with a tag and one PLAIN (non-conventional) commit: the patch
//!    increments (Mainline mode's `main` branch default increment).
//! 2. a fixture repo with a tag and one `feat:` commit: the minor increments.
//! 3. the tool absent (a binary name that cannot be found on PATH): the crate semver
//!    plus the explicit `+unversioned` marker, never a panic.
//! 4. a directory outside any git checkout: the same `+unversioned` fallback.
//!
//! Scenarios 1 and 2 additionally pin that `ShortSha` is folded into the reported
//! version's build metadata (the exact-commit identity the spec-74 Problem statement
//! opens with - order from semver, identity from the sha), and that the fallback never
//! carries a stray `ShortSha` fragment.
//!
//! A fifth test pins that a successful derivation never mutates the repository's own
//! `.git/config` as a side effect: `go-gitsemver` silently strips
//! `extensions.worktreeConfig` from it by default unless invoked with
//! `--no-repair-worktree-config`, an undocumented on-disk mutation with no place in what
//! the Design calls compile-time derivation.

#[path = "../build/gitsemver.rs"]
#[allow(dead_code)]
mod gitsemver;

use std::path::Path;
use std::process::Command;

/// Run `git <args>` in `root`, panicking with stderr on failure - fixture setup must
/// never silently half-succeed.
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

/// Build a fixture git repository at `root`: `go-gitsemver.yml` matching the one
/// committed at this repo's own root (`mode: Mainline`, `tag-prefix: v`), an initial
/// commit tagged `v1.0.0`, then one more commit with `second_commit_message`. Mirrors
/// this project's own `init_committed_repo` helper (`src/main.rs`) for git-fixture
/// construction, extended with the tag + second commit criterion 1 needs.
fn fixture_repo(root: &Path, second_commit_message: &str) {
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
    git(root, &["commit", "-q", "-m", second_commit_message]);
}

/// Skip (rather than fail) the success-path scenarios when `go-gitsemver` is not on
/// PATH in this environment: criterion 3 (both feature lanes green) owns provisioning
/// CI with the binary the spec's Notes section documents as installed; this test proves
/// the derivation logic is correct GIVEN the tool, which the fallback scenarios below
/// prove independently of the tool's presence.
fn gitsemver_available() -> bool {
    Command::new("go-gitsemver")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn a_plain_commit_after_a_tag_increments_the_patch() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fixture_repo(dir.path(), "docs: update the readme");

    let version = gitsemver::derive_version("go-gitsemver", dir.path());

    assert!(
        version.starts_with("1.0.1"),
        "a plain commit after v1.0.0 must bump the patch under Mainline mode; got: {version}"
    );
    assert!(
        !version.contains("unversioned"),
        "a successful derivation must never carry the fallback marker; got: {version}"
    );
}

#[test]
fn a_feat_commit_after_a_tag_increments_the_minor() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fixture_repo(dir.path(), "feat: add a thing");

    let version = gitsemver::derive_version("go-gitsemver", dir.path());

    assert!(
        version.starts_with("1.1.0"),
        "a feat: commit after v1.0.0 must bump the minor under Mainline mode; got: {version}"
    );
    assert!(
        !version.contains("unversioned"),
        "a successful derivation must never carry the fallback marker; got: {version}"
    );
}

#[test]
fn a_successful_derivation_folds_the_short_sha_into_build_metadata() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fixture_repo(dir.path(), "docs: update the readme");

    let version = gitsemver::derive_version("go-gitsemver", dir.path());
    let short_sha_out = Command::new("go-gitsemver")
        .arg("-p")
        .arg(dir.path())
        .arg("--show-variable")
        .arg("ShortSha")
        .output()
        .expect("go-gitsemver ShortSha must run in this scenario");
    let short_sha = String::from_utf8(short_sha_out.stdout)
        .unwrap()
        .trim()
        .to_string();

    assert!(
        version.ends_with(&short_sha),
        "the derived version must carry the exact-commit ShortSha as its trailing \
         build-metadata identifier so two binaries built from different commits of the \
         same derived semver stay distinguishable; got version={version} short_sha={short_sha}"
    );
    assert!(
        version.contains('+'),
        "the sha is build metadata, introduced by a `+`; got: {version}"
    );
}

#[test]
fn tool_not_found_falls_back_to_the_crate_semver_with_an_unversioned_marker() {
    let dir = tempfile::tempdir().unwrap();
    // A fixture repo that WOULD derive successfully if the tool ran - proves the
    // fallback is driven by the tool being unreachable, not by the repo state.
    fixture_repo(dir.path(), "docs: update the readme");

    let version = gitsemver::derive_version("go-gitsemver-does-not-exist-xyz", dir.path());

    assert_eq!(
        version,
        format!(
            "{}{}",
            env!("CARGO_PKG_VERSION"),
            gitsemver::UNVERSIONED_SUFFIX
        ),
        "a missing binary must fall back to the bare crate semver plus the explicit \
         unversioned marker, never fabricate a version, and never fail the build"
    );
}

#[test]
fn a_successful_derivation_never_mutates_the_repositorys_git_config() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fixture_repo(dir.path(), "docs: update the readme");

    let config_path = dir.path().join(".git").join("config");
    let before = std::fs::read(&config_path).expect("read fixture .git/config before");

    let version = gitsemver::derive_version("go-gitsemver", dir.path());
    assert!(
        !version.contains("unversioned"),
        "the fixture history must derive a real version so this test actually exercises \
         a SUCCESSFUL derivation's side effects, not the tool-absent/non-checkout \
         fallback paths; got: {version}"
    );

    let after = std::fs::read(&config_path).expect("read fixture .git/config after");
    assert_eq!(
        before, after,
        "derive_version must never mutate the repository's own .git/config as a side \
         effect of what the Design calls compile-time derivation"
    );
}

#[test]
fn worktree_config_extension_never_mutated_even_though_it_defeats_derivation() {
    if !gitsemver_available() {
        eprintln!("skipping: go-gitsemver not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fixture_repo(dir.path(), "docs: update the readme");
    // Enable the git extension `go-gitsemver` repairs (silently strips from
    // `.git/config`) by default unless `--no-repair-worktree-config` suppresses it.
    // Verified independently before writing this test: go-gitsemver's own git library
    // flatly refuses to OPEN a repository declaring this extension at all once the
    // repair is suppressed ("core.repositoryformatversion does not support extension:
    // worktreeconfig") - repairing-by-mutation was the ONLY way it could ever read such
    // a repo. So suppressing the mutation and deriving a real version are mutually
    // exclusive for this specific, rare input; `derive_version`'s documented
    // ANY-failure fallback contract (never fabricate, never fail the build) is exactly
    // what covers that trade-off - the same contract the tool-absent and
    // outside-a-checkout scenarios already exercise.
    git(dir.path(), &["config", "extensions.worktreeConfig", "true"]);

    let config_path = dir.path().join(".git").join("config");
    let before = std::fs::read(&config_path).expect("read fixture .git/config before");

    let version = gitsemver::derive_version("go-gitsemver", dir.path());

    let after = std::fs::read(&config_path).expect("read fixture .git/config after");
    assert_eq!(
        before, after,
        "derive_version must never mutate the repository's own .git/config, even on a \
         repo that declares extensions.worktreeConfig - suppressing go-gitsemver's \
         default-on repair-by-mutation must never be silently dropped"
    );
    assert_eq!(
        version,
        format!(
            "{}{}",
            env!("CARGO_PKG_VERSION"),
            gitsemver::UNVERSIONED_SUFFIX
        ),
        "with the mutation suppressed, go-gitsemver cannot open a repo declaring \
         extensions.worktreeConfig at all, so derivation must fall back cleanly rather \
         than fabricate a version or fail the build; got: {version}"
    );
}

#[test]
fn outside_a_git_checkout_falls_back_to_the_crate_semver_with_an_unversioned_marker() {
    // Deliberately NOT `git init`-ed, and under the OS temp dir rather than nested in
    // this project's own checkout, so go-gitsemver's repository discovery cannot walk
    // up into an ENCLOSING real repo and mask the outside-a-checkout case.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("x.txt"), "hi\n").unwrap();

    let version = gitsemver::derive_version("go-gitsemver", dir.path());

    assert_eq!(
        version,
        format!(
            "{}{}",
            env!("CARGO_PKG_VERSION"),
            gitsemver::UNVERSIONED_SUFFIX
        ),
        "a directory outside any git checkout must fall back to the bare crate semver \
         plus the explicit unversioned marker, never fabricate a version, and never fail \
         the build"
    );
}
