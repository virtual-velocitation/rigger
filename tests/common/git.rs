//! Git fixtures for the integration suites, re-exported from [`super::fixtures`] - the one
//! definition the crate's own unit tests share.

#![allow(unused_imports)]

pub use super::fixtures::{
    git_answer, git_commit_all, git_ok, git_ok_with_identity, git_out, git_out_with_identity,
    git_stdout, init_repo, install_refusing_hook, run_git, temp_git_project_with_commit,
};
