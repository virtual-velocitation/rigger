//! A process tree started through one handle and ended through it: the session a host spawns, and
//! every process that session starts.

pub use crate::reap::{EndReport, Leftover};
use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};

/// How a tree is contained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Containment {
    /// A keeper process, this executable, holds the tree.
    Keeper(PathBuf),
    /// A job object holds the tree.
    Job,
    /// The tree is watched from its main process, for the reason given.
    Watch(Why),
}

/// Why a tree is watched rather than held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// A test chose it.
    Chosen,
}

/// A command whose process starts a contained tree.
pub struct TreeCommand {
    command: Command,
}

impl TreeCommand {
    /// A command for `program`, contained as `how`.
    pub fn new(program: impl AsRef<OsStr>, how: &Containment) -> io::Result<TreeCommand> {
        let _ = (program.as_ref(), how);
        todo!("the contained command")
    }

    /// The command, for its arguments, environment, directory and streams.
    pub fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    /// Starts the tree.
    pub fn spawn(self) -> io::Result<ReapedChild> {
        todo!("the contained spawn")
    }
}

/// The handle a tree is ended through.
pub struct ReapedChild {
    child: Child,
}

impl ReapedChild {
    /// Adopts `child`.
    pub fn new(child: Child) -> ReapedChild {
        let _ = child;
        todo!("the adopted handle")
    }

    /// The main process's pid.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// The main process's exit status, once it has exited.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Ends the tree.
    pub fn end(&mut self) -> EndReport {
        todo!("the end")
    }

    /// Lets the tree go.
    pub fn release(self) {
        todo!("the release")
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_support::{wait_until, Reading, Witness};
    use std::os::unix::process::ExitStatusExt;

    /// The fixture shell `body` (`Witness::script`), started under `witness` as a watched tree.
    fn watched(witness: &Witness, body: &str) -> ReapedChild {
        let mut command =
            TreeCommand::new("sh", &Containment::Watch(Why::Chosen)).expect("a watched command");
        command
            .command_mut()
            .arg("-c")
            .arg(Witness::script(body))
            .envs(witness.env());
        command.spawn().expect("spawn the watched tree")
    }

    /// A watched tree running `body`, ended once `up` of its processes are up, leaves nothing
    /// running and reports nothing.
    fn ends_the_tree(body: &str, up: usize) {
        let mut witness = Witness::new();
        let mut tree = watched(&witness, body);
        assert!(witness.wait_up(up), "the fixture's {up} processes are up");

        let report = tree.end();

        assert_eq!(
            (witness.read(), report),
            (Reading::Ended, EndReport::default())
        );
    }

    crate::test_cases! {
        a_watched_end_ends_a_tree_that_forks_as_it_ends:
            ends_the_tree(
                "( i=0; while [ $i -lt 100 ]; do holder; sleep 0.02; i=$((i+1)); done; up; held ) & \
                 up; held",
                6,
            );
        a_watched_end_ends_a_process_in_a_session_of_its_own:
            ends_the_tree("setsid sh -c 'printf u >&9; read -r unused <&8' & up; held", 2);
        a_watched_end_ends_a_process_started_in_another_directory:
            ends_the_tree("cd / && holder; up; held", 2);
    }

    #[test]
    fn a_watched_end_reports_a_process_that_left_the_tree_and_never_signals_it() {
        let mut witness = Witness::new();
        let mut tree = watched(&witness, "( holder; sleep 2.5 ); up; held");
        assert!(witness.wait_up(2), "the holder and the main are up");

        let report = tree.end();

        assert_eq!(
            witness.read(),
            Reading::Held,
            "the process that left the tree runs on"
        );
        assert_eq!(
            (report.residue, report.skipped.len()),
            (vec![], 1),
            "the process that left the tree is reported, never signalled"
        );
        assert_eq!(witness.release(), Reading::Ended);
    }

    #[test]
    fn a_released_tree_leaves_what_its_main_started_running() {
        let mut witness = Witness::new();
        let mut tree = watched(&witness, "holder; up");
        assert!(witness.wait_up(2), "the holder and the main are up");
        assert!(
            wait_until(|| matches!(tree.try_wait(), Ok(Some(_)))),
            "the main exits"
        );

        tree.release();

        assert_eq!(
            witness.read(),
            Reading::Held,
            "the holder the main left runs on"
        );
        assert_eq!(witness.release(), Reading::Ended);
    }

    #[test]
    fn a_dropped_tree_is_ended() {
        let mut witness = Witness::new();
        let tree = watched(&witness, "holder; up; held");
        assert!(witness.wait_up(2), "the holder and the main are up");

        drop(tree);

        assert_eq!(witness.read(), Reading::Ended);
    }

    #[test]
    fn an_adopted_child_is_ended_with_its_tree() {
        let mut witness = Witness::new();
        let child = Command::new("sh")
            .arg("-c")
            .arg(Witness::script("holder; up; held"))
            .envs(witness.env())
            .spawn()
            .expect("spawn the fixture shell");
        let mut tree = ReapedChild::new(child);
        assert!(witness.wait_up(2), "the holder and the main are up");

        let report = tree.end();

        assert_eq!(
            (witness.read(), report),
            (Reading::Ended, EndReport::default())
        );
        let status = tree.try_wait().expect("read the main's status");
        assert_eq!(
            status.and_then(|status| status.signal()),
            Some(9),
            "the main is ended hard"
        );
    }
}
