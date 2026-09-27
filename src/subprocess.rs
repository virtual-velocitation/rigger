//! THE PROCESS-SPAWN PORT: the one place production code constructs a [`Command`].
//!
//! Every process this crate starts - git, the gate shell, the agent CLI, the Node driver, the
//! dash child - is built here and handed back for the caller to add its own arguments and run.
//! One seam means one place owning how a child is launched: the working-directory rule every
//! caller shares ([`command_in`]), the `git -C <dir>` shape nearly every git call takes
//! ([`git_in`]), and the process-group detachment a long-lived child needs
//! ([`detach_process_group`]). It adds no policy of its own: the caller still picks the
//! arguments, the environment, the stdio and how the child is waited on, and a child is only
//! ever ended through its own spawning handle.

use std::ffi::OsStr;
use std::process::Command;

/// A fresh command for `program`, inheriting this process's environment, working directory
/// and stdio exactly as [`Command::new`] does.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// A fresh command for `program` that runs in `dir`, or in this process's own working
/// directory when `dir` is empty (the caller-supplied "no directory" convention).
pub fn command_in(program: impl AsRef<OsStr>, dir: &str) -> Command {
    let mut cmd = command(program);
    if !dir.is_empty() {
        cmd.current_dir(dir);
    }
    cmd
}

/// `git -C <dir>`: a git command anchored at `dir` rather than the process's working directory.
pub fn git_in(dir: impl AsRef<OsStr>) -> Command {
    let mut cmd = command("git");
    cmd.arg("-C").arg(dir);
    cmd
}

/// Place `cmd`'s spawned child in its OWN process group (a new group whose PGID equals the
/// child's PID, via `process_group(0)`), detached from the parent command's process group.
/// This is the session-detachment that makes the always-on dash actually survive across steps
/// (spec 44): WITHOUT it a detached dash inherits `rigger step`'s process group, and when the
/// workflow courier runs `rigger step` as a foreground command the harness tears down that
/// command's process group on completion and reaps the dash with it - the spec-39 always-on
/// dash then dies the instant every step returns. `process_group(0)` makes the child a group
/// leader in a group the parent's teardown never reaches. Std-only (no `libc`), so it compiles
/// and holds identically on BOTH the default and `--no-default-features` lanes. Non-Unix builds
/// keep the group-inheriting behavior (the always-on dash is a Unix-path feature).
#[cfg(unix)]
pub fn detach_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // `process_group(0)` runs a `setpgid(0, 0)`-equivalent in the child before exec, making it a
    // group leader whose PGID equals its own PID - a brand-new process group the parent command's
    // group teardown never reaches.
    cmd.process_group(0);
}

#[cfg(not(unix))]
pub fn detach_process_group(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd_of(cmd: &Command) -> Option<&std::path::Path> {
        cmd.get_current_dir()
    }

    crate::test_cases! {
        /// An empty directory leaves the child in this process's own working directory.
        command_in_an_empty_dir_sets_no_working_directory:
            assert_eq!(cwd_of(&command_in("sh", "")), None);
        /// A non-empty directory becomes the child's working directory.
        command_in_a_dir_runs_the_child_there:
            assert_eq!(cwd_of(&command_in("sh", "/x")), Some(std::path::Path::new("/x")));
    }

    /// `git_in` is exactly `git -C <dir>`, with nothing else added.
    #[test]
    fn git_in_anchors_git_at_the_directory() {
        let cmd = git_in("/repo");
        assert_eq!(cmd.get_program(), "git");
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, vec![OsStr::new("-C"), OsStr::new("/repo")]);
        assert_eq!(cwd_of(&cmd), None);
    }
}
