//! Spec 74 criterion 1, round 4: the set of files whose change should re-run `build.rs`,
//! extracted into its own file - mirroring `build/gitsemver.rs`'s own module doc comment,
//! which already gives the identical reason for the identical technique - so this seam
//! can be proven from a test binary via the same `#[path]` inclusion, since build scripts
//! cannot be exercised by `cargo test` directly (`build.rs` is compiled and run as its own
//! crate before the package builds, never linked into a test binary).
//!
//! Fixes a defect the round-3 adjudication upheld
//! (`sdet-u74c1r2-tag-only-rebuild-not-triggered` / `adv3-u74c1-tag-rebuild-fell-
//! through-cracks`): the previous, unextracted `git_watch_paths` (still what `HEAD` and
//! the resolved branch ref below cover) watched only `.git/HEAD` and the branch ref it
//! resolves to, so a tag-only change never re-triggered `build.rs` - `go-gitsemver`'s
//! Mainline mode takes the nearest reachable tag as a PRIMARY input to the derived
//! version order (see `build/gitsemver.rs`'s module doc comment), a state the HEAD/ref
//! watch alone cannot see, so `RIGGER_GITSEMVER_VERSION` stayed cached at its pre-tag
//! value until some unrelated watched path happened to change. The loose `refs/tags`
//! directory and the `packed-refs` file (git folds loose refs into this file over time,
//! e.g. after `git gc`, or a fetch/clone can hand you one directly) are now watched
//! unconditionally alongside `HEAD` and the branch ref, so both a fresh `git tag` and a
//! tag that only ever existed packed re-trigger the derivation.
//!
//! All ref-bearing paths resolve against the COMMON git dir via `--path-format=absolute`
//! (never a relative result manually joined against the wrong directory - the same
//! technique `build/gitsemver.rs`'s own `resolve_repo` already uses for the identical
//! resolve-common-dir need), not the possibly-worktree-local `--absolute-git-dir`: tags
//! and branches are shared state across every linked worktree of one repository, so a tag
//! or commit made from ANY worktree re-triggers every worktree's own build, matching
//! git's own shared-ref semantics.
//!
//! Deliberately self-contained (its own small `git_rev_parse`, not a reuse of
//! `build/gitsemver.rs`'s identically-shaped private helper of the same name): the round-1
//! architecture finding proposing that cross-file reuse (`arch-u74c1-git-invoke-trim-
//! triplicated`) was superseded and narrowed to intra-file dedup only
//! (`arch-u74c1-git-invoke-trim-narrowed-to-intra-file`) - reaching across `#[path]`-
//! included sibling modules would need a third shared file with no other purpose, not
//! worth it for a few lines of `git -C <dir> rev-parse <args>` duplicated once more.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The files whose change should re-run `build.rs`: `HEAD` itself; when `HEAD` is a
/// symbolic ref (the normal on-a-branch case) the branch ref it resolves to; and,
/// unconditionally, the loose tag-ref directory and the packed-refs file. Empty when git
/// is unavailable or `dir` is not inside a git checkout, so a non-repo build still
/// succeeds.
pub fn git_watch_paths(dir: &Path) -> Vec<PathBuf> {
    let git_dir = match git_rev_parse(dir, &["--absolute-git-dir"]) {
        Some(d) => PathBuf::from(d),
        None => return Vec::new(),
    };
    let common = match git_rev_parse(dir, &["--path-format=absolute", "--git-common-dir"]) {
        Some(c) => PathBuf::from(c),
        None => git_dir.clone(),
    };
    let mut paths = vec![git_dir.join("HEAD")];
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) {
        if let Some(refname) = head.strip_prefix("ref:").map(str::trim) {
            paths.push(common.join(refname));
        }
    }
    paths.push(common.join("refs/tags"));
    paths.push(common.join("packed-refs"));
    paths
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
