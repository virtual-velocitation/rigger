//! THE PROCESS-SPAWN PORT: the one place production code constructs a [`Command`].
//!
//! Every process this crate starts - git, the gate shell, the agent CLI, the dash child - is built
//! here and handed back for the caller to add its own arguments and run. One seam means one place
//! owning how a child is launched: the working-directory rule every caller shares ([`command_in`])
//! and the `git -C <dir>` shape nearly every git call takes, its repository found from `dir` alone
//! ([`git_in`]). Beyond that repository rule it adds no policy of its own: the caller still picks
//! the arguments, the environment, the stdio and how the child is waited on, and a child is only
//! ever ended through its own spawning handle.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// A fresh command for `program`, inheriting this process's environment, working directory
/// and stdio exactly as [`Command::new`] does.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// The directory a child given `dir` runs in: `dir` itself, or this process's own working
/// directory, `.`, when `dir` is empty (the caller-supplied "no directory" convention).
pub fn session_dir(dir: &str) -> &Path {
    Path::new(if dir.is_empty() { "." } else { dir })
}

/// A fresh command for `program` that runs in [`session_dir`] of `dir`.
pub fn command_in(program: impl AsRef<OsStr>, dir: &str) -> Command {
    let mut cmd = command(program);
    cmd.current_dir(session_dir(dir));
    cmd
}

/// The variables through which an environment names the repository git operates on, apart from
/// the directory git starts in: the repository itself, its work tree, its index, its object
/// databases and its common directory. A git hook runs with some of them set.
const GIT_REPOSITORY_VARIABLES: [&str; 6] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_COMMON_DIR",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

/// `git -C <dir>`: a git command anchored at `dir` rather than the process's working directory,
/// with none of [`GIT_REPOSITORY_VARIABLES`] passed on, so the repository git finds from `dir` is
/// the one it operates on whatever repository this process's environment names (the convention
/// git documents for a hook that operates on another repository). A caller may still set one of
/// them on the command it is handed.
pub fn git_in(dir: impl AsRef<OsStr>) -> Command {
    let mut cmd = command("git");
    cmd.arg("-C").arg(dir);
    for variable in GIT_REPOSITORY_VARIABLES {
        cmd.env_remove(variable);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd_of(cmd: &Command) -> Option<&std::path::Path> {
        cmd.get_current_dir()
    }

    crate::test_cases! {
        /// An empty directory runs the child in this process's own working directory, `.`.
        command_in_an_empty_dir_runs_the_child_in_this_processs_own:
            assert_eq!(cwd_of(&command_in("sh", "")), Some(std::path::Path::new(".")));
        /// A non-empty directory becomes the child's working directory.
        command_in_a_dir_runs_the_child_there:
            assert_eq!(cwd_of(&command_in("sh", "/x")), Some(std::path::Path::new("/x")));
    }

    /// `git_in` is exactly `git -C <dir>` with every variable naming a repository removed from
    /// the child's environment, and nothing else added.
    #[test]
    fn git_in_anchors_git_at_the_directory() {
        let cmd = git_in("/repo");
        assert_eq!(cmd.get_program(), "git");
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, vec![OsStr::new("-C"), OsStr::new("/repo")]);
        assert_eq!(cwd_of(&cmd), None);
        let envs: Vec<(&OsStr, Option<&OsStr>)> = cmd.get_envs().collect();
        let mut removed: Vec<(&OsStr, Option<&OsStr>)> = GIT_REPOSITORY_VARIABLES
            .iter()
            .map(|variable| (OsStr::new(variable), None))
            .collect();
        removed.sort();
        assert_eq!(envs, removed);
    }
}
