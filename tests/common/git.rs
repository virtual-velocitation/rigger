//! Git fixtures for the integration suites, re-exported from [`super::fixtures`] - the one
//! definition the crate's own unit tests share - plus the suite-only go-gitsemver fixture.

#![allow(unused_imports)]

use std::path::Path;

pub use super::fixtures::{
    commit_at_fixed_date, git_answer, git_commit_all, git_init_quiet, git_ok, git_ok_with_identity,
    git_out, init_repo, install_refusing_hook, run_git, temp_git_project_with_commit,
    trimmed_stdout,
};

/// A go-gitsemver fixture repository at `root`: `go-gitsemver.yml` matching this repo's own
/// (`mode: Mainline`, `tag-prefix: v`), an initial commit tagged `v1.0.0`, then one more commit
/// with `second_commit_message` - enough history for a real version derivation.
pub fn tagged_gitsemver_repo(root: &Path, second_commit_message: &str) {
    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    std::fs::write(
        root.join("go-gitsemver.yml"),
        "mode: Mainline\ntag-prefix: v\n",
    )
    .expect("write fixture go-gitsemver.yml");
    git_ok(root, &["add", "go-gitsemver.yml"]);
    git_ok(root, &["commit", "-q", "-m", "chore: initial"]);
    git_ok(root, &["tag", "v1.0.0"]);
    std::fs::write(root.join("file.txt"), "second\n").expect("write fixture file");
    git_ok(root, &["add", "file.txt"]);
    git_ok(root, &["commit", "-q", "-m", second_commit_message]);
}
