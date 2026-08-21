//! Spec 74: version derivation is DELEGATED to the `go-gitsemver` binary
//! (github.com/MyCarrier-DevOps/go-gitsemver, GitVersion-compatible) - rigger never
//! reimplements the bump algorithm, only invokes the tool and folds its output into the
//! string the binary embeds. This file has no `fn main` and is included by `#[path]`
//! from BOTH `build.rs` (compiled into the build-script crate, invoked at COMPILE time)
//! and `tests/gitsemver_derivation.rs` (compiled into that integration-test binary,
//! driven against fixture git repositories and the real binary). That is the ONE
//! derivation seam: whatever the test proves is exactly the code the shipped binary
//! runs, never a parallel reimplementation of it.
//!
//! The reported version is `go-gitsemver`'s `FullSemVer` for the built commit, with
//! `ShortSha` folded into its build-metadata segment. Semver ignores build metadata for
//! ordering, so this adds exact-commit identity on top of the derived order without
//! disturbing it - the Problem spec 74 opens with ("only the build hash - identity
//! without order - tells two binaries apart") stays solved even as the reported version
//! now also carries real, comparable order.
//!
//! Falls back to the bare crate semver plus the explicit [`UNVERSIONED_SUFFIX`] marker
//! whenever `go-gitsemver` cannot produce a `FullSemVer` for ANY reason - the binary
//! missing from PATH, the source not a git checkout, a non-zero exit, or empty output.
//! One uniform "could not derive" signal regardless of cause, mirroring criterion 2's
//! validate advisory (which folds every `+unversioned` cause into one message for the
//! same reason: the embedded marker itself cannot distinguish its cause). The build
//! must NEVER fail because the derivation tool could not run.
//!
//! `dir` is resolved through real `git rev-parse` (never through `go-gitsemver` itself)
//! before the tool is invoked, mirroring `build.rs`'s own `git_watch_paths` pattern for
//! `BUILD_PROVENANCE`. `go-gitsemver`'s git library cannot resolve `HEAD` through a
//! LINKED git worktree's `gitdir:` indirection (`.git` a file, not a directory) - this
//! project's own primary way of building every spec unit - so a bare `-p dir` there
//! fails and silently falls back to [`UNVERSIONED_SUFFIX`] even though the worktree is a
//! real checkout with real history. Resolving `dir`'s PRIMARY checkout root (the parent
//! of the shared common `.git` directory) and this checkout's own resolved `HEAD` commit
//! through real git first, then handing `go-gitsemver` `-p <primary root> -c <commit>`,
//! gives it coordinates it CAN resolve - delegating 100% of the version computation to
//! the tool still, only correctly locating its input. For a plain (non-worktree)
//! repository this resolves to `dir` and its own `HEAD`: the same inputs `go-gitsemver`
//! would have used unassisted, so the common case is unchanged.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Appended to the bare crate semver whenever the real derivation is unavailable, so a
/// reader can never mistake the fallback for a genuinely derived version.
pub const UNVERSIONED_SUFFIX: &str = "+unversioned";

/// Resolve `dir`'s primary checkout root and its own `HEAD` commit via real `git`,
/// exactly as `build.rs`'s `git_watch_paths` resolves a linked worktree's shared git
/// state. `--path-format=absolute` makes `--git-common-dir` return an absolute path
/// regardless of whether `dir` is already the process's cwd, so no manual
/// relative-to-`dir` joining is needed. `None` when `dir` is not inside a git checkout
/// at all (git itself is unavailable, or `dir` truly is outside any repository) - the
/// caller falls back to invoking the tool directly on `dir` and lets IT report that
/// failure, preserving the existing outside-a-checkout fallback behavior.
fn resolve_repo(dir: &Path) -> Option<(PathBuf, String)> {
    let common_dir = git_rev_parse(dir, &["--path-format=absolute", "--git-common-dir"])?;
    let repo_root = PathBuf::from(common_dir).parent()?.to_path_buf();
    let commit = git_rev_parse(dir, &["HEAD"])?;
    Some((repo_root, commit))
}

/// Run `git -C <dir> rev-parse <args...>`, trimmed. `None` on any failure: git
/// unavailable, `dir` not inside a git checkout, a non-zero exit, or empty output.
fn git_rev_parse(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("rev-parse")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Run `<bin> --no-repair-worktree-config -p <path> [-c <commit>] --show-variable
/// <variable>`, trimmed. `None` on ANY failure: the binary missing from PATH
/// (`Command::output` errors with `NotFound`), `path` not a git checkout, a non-zero
/// exit, or empty output. `--no-repair-worktree-config` suppresses `go-gitsemver`'s
/// default-on behavior of silently deleting `extensions.worktreeConfig` from
/// `.git/config` as a side effect of reading it - a mutation with no place in what the
/// Design calls compile-time derivation, undocumented, and unrelated to computing a
/// version string.
fn show_variable(bin: &str, path: &Path, commit: Option<&str>, variable: &str) -> Option<String> {
    let mut cmd = Command::new(bin);
    cmd.arg("--no-repair-worktree-config").arg("-p").arg(path);
    if let Some(commit) = commit {
        cmd.arg("-c").arg(commit);
    }
    cmd.arg("--show-variable").arg(variable);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Folds `short_sha` into `full_semver`'s build-metadata segment: a new dot-separated
/// identifier when metadata already exists (go-gitsemver's own commits-since-tag
/// count), else the sole `+` metadata.
fn append_build_metadata(full_semver: &str, short_sha: &str) -> String {
    if full_semver.contains('+') {
        format!("{full_semver}.{short_sha}")
    } else {
        format!("{full_semver}+{short_sha}")
    }
}

/// The version rigger reports: `go-gitsemver`'s `FullSemVer` for the commit checked out
/// at `dir`, under whatever `go-gitsemver.yml` `dir` has committed, with `ShortSha`
/// folded in as build metadata. Falls back to the crate's own semver
/// (`CARGO_PKG_VERSION`, resolved for whichever crate includes this file - build.rs and
/// this test binary are both compiled as part of the SAME package, so the value is
/// identical either way) plus [`UNVERSIONED_SUFFIX`] whenever `go-gitsemver` cannot
/// produce a `FullSemVer`. `bin` is the executable name/path to invoke (parameterized
/// so tests can force a not-found tool without touching PATH).
///
/// `dir` is resolved through [`resolve_repo`] first so a LINKED git worktree derives its
/// real version instead of falling back (see the module doc comment); when `dir` is not
/// inside any git checkout at all, resolution yields `None` and `go-gitsemver` is
/// invoked on `dir` directly, unassisted, so it reports the same "not a checkout"
/// failure it always has.
pub fn derive_version(bin: &str, dir: &Path) -> String {
    let (path, commit) = match resolve_repo(dir) {
        Some((repo_root, commit)) => (repo_root, Some(commit)),
        None => (dir.to_path_buf(), None),
    };
    let Some(full_semver) = show_variable(bin, &path, commit.as_deref(), "FullSemVer") else {
        return format!("{}{UNVERSIONED_SUFFIX}", env!("CARGO_PKG_VERSION"));
    };
    match show_variable(bin, &path, commit.as_deref(), "ShortSha") {
        Some(short_sha) => append_build_metadata(&full_semver, &short_sha),
        None => full_semver,
    }
}
