//! Build script: embed a build-provenance identifier so the compiled binary can report
//! WHICH source it was built from.
//!
//! An agent cannot otherwise tell whether an installed `rigger` binary matches the source,
//! which is what makes the workflow-drift warning ambiguous. We resolve a git commit/describe
//! id at build time and hand it to the compiler as `RIGGER_BUILD_PROVENANCE`, so `main.rs`
//! (and the drift diagnostic that consumes it) can read it with `env!`. The value is always
//! non-empty: outside a git checkout (e.g. a build from a published tarball with no `.git`) it
//! falls back to a sentinel rather than failing the build.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Re-run when the build script itself changes.
    println!("cargo:rerun-if-changed=build.rs");

    // Re-embed the id whenever the checked-out commit moves, so a rebuilt binary carries the
    // provenance of the source it was actually built from. Watching git HEAD and the ref it
    // resolves to is resolved THROUGH git, so it works both in a plain clone and in a linked
    // worktree (where `.git` is a file, not a directory).
    for path in git_watch_paths() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let provenance = git_provenance().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=RIGGER_BUILD_PROVENANCE={provenance}");
}

/// `git describe --always --abbrev=12`, trimmed: a stable, commit-determined identifier that
/// prefers a reachable tag and otherwise the abbreviated commit hash. Deliberately WITHOUT
/// `--dirty` so the value is fully determined by the committed state the HEAD/ref watch tracks
/// (a `--dirty` flag would go stale unless the script also re-ran on every uncommitted edit).
/// Returns `None` when git is not runnable or the source is not a git repository.
fn git_provenance() -> Option<String> {
    let out = Command::new("git")
        .args(["describe", "--always", "--abbrev=12"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let id = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// The files whose change should re-run this script: git HEAD, plus - when HEAD is a symbolic
/// ref (the normal on-a-branch case) - the branch ref it resolves to, so a new commit on the
/// branch re-embeds the id. The ref lives in the COMMON git dir (shared across linked
/// worktrees). Empty when git is unavailable, so a non-repo build still succeeds.
fn git_watch_paths() -> Vec<PathBuf> {
    let git_dir = match git_output(&["rev-parse", "--absolute-git-dir"]) {
        Some(d) => PathBuf::from(d),
        None => return Vec::new(),
    };
    let mut paths = vec![git_dir.join("HEAD")];
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) {
        if let Some(refname) = head.strip_prefix("ref:").map(str::trim) {
            if let Some(common) = git_output(&["rev-parse", "--git-common-dir"]) {
                paths.push(PathBuf::from(common).join(refname));
            }
        }
    }
    paths
}

/// Run `git <args...>` and return its trimmed stdout, or `None` when git is unavailable, the
/// command fails, or the output is empty.
fn git_output(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
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
