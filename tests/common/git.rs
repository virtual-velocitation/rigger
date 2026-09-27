//! Git fixtures for the integration suites: the shared git commands (defined once in
//! `fixtures/git.rs`, which the crate's unit tests reach too) plus the suite-only hook fixture.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

// A suite uses the subset it needs, like every other `tests/common` item.
#[allow(unused_imports)]
pub use super::fixtures::{
    commit_at_fixed_date, git_init_quiet, git_ok, git_out, git_stdout, init_repo, run_git,
};

/// Install a `pre-commit` hook in the repository at `repo_path` that refuses every commit.
pub fn install_refusing_hook(repo_path: &str) {
    let hooks = Path::new(repo_path).join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'hook: refusing' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
}

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
