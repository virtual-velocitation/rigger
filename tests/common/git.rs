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
