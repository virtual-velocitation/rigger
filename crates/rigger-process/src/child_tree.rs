//! A process tree started through one handle and ended through it: the session a host spawns, and
//! every process that session starts.
//!
//! A host spawns a session through [`TreeCommand`] and holds the [`ReapedChild`] it answers. The
//! handle reads and waits on the session's main process as a [`Child`] does, and ends what the
//! session started in one of two ways: [`ReapedChild::end`] ends the whole tree, and
//! [`ReapedChild::release`] lets it go and ends nothing. A handle dropped without a release is
//! ended, on a normal scope exit and on an unwinding panic alike, so a driver that crashes leaves
//! no session running behind it.
//!
//! How a tree is held is the host's explicit choice ([`Containment`]). A watched tree
//! ([`Containment::Watch`]) is held through its main process: a watcher thread walks the main's
//! descendants while it runs and remembers each one it finds, waking on a fork or an exit where the
//! OS reports one (a kqueue on macOS) and on a timer elsewhere, and the end
//! ([`crate::reap::end_watched`]) freezes the tree, ends it deepest first and reports what it
//! could not reach: a remembered process that left the tree, whose parent chain no longer reaches
//! the main, is reported and never signalled. A kept tree ([`Containment::Keeper`], on Linux) is
//! held by a keeper process, this executable run again, which spawns the main and is the child
//! subreaper of everything the tree starts, so no process of the tree leaves it by orphaning
//! itself; the handle's process is the keeper, and the handle answers the main's pid and the
//! status the keeper reports for it. [`Containment::detect`] answers the containment this OS gives
//! a tree. Every signal goes through the reaper ([`crate::reap`]), the one place this crate
//! signals a process.

use crate::reap::{self, Identity};
pub use crate::reap::{EndReport, Leftover};
use crate::{os_process, subprocess};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus};
use std::thread::JoinHandle;
use std::time::Duration;

// The one spelling of the OS set for a watcher's wake: macOS wakes the watcher when a process it
// watches forks or exits, and every other OS binds the inline `timed` wake in the `_` arm.
std::cfg_select! {
    target_os = "macos" => {
        mod macos;
        use macos::{Stopper, Wake};
    }
    _ => {
        /// The wake of an OS that reports no fork: the watcher walks again each time its wait
        /// times out.
        mod timed {
            use std::io;
            use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
            use std::time::Duration;

            /// What a watcher waits on: the stop alone.
            pub(super) struct Wake {
                stopped: Receiver<()>,
            }

            /// The watcher's stop: dropping it ends the watcher's next wait.
            pub(super) struct Stopper {
                _sender: Sender<()>,
            }

            impl Wake {
                /// A wake and the stop that ends it.
                pub(super) fn new() -> io::Result<(Wake, Stopper)> {
                    let (sender, stopped) = channel();
                    Ok((Wake { stopped }, Stopper { _sender: sender }))
                }

                /// Watches `pid` for a fork or an exit, which this wake cannot.
                pub(super) fn watch(&mut self, _pid: u32) {}

                /// Waits up to `timeout`; false once the stop is dropped.
                pub(super) fn wait(&mut self, timeout: Duration) -> bool {
                    !matches!(
                        self.stopped.recv_timeout(timeout),
                        Err(RecvTimeoutError::Disconnected)
                    )
                }
            }
        }
        use timed::{Stopper, Wake};
    }
}

// The one spelling of the OS set for a held tree: Linux holds a tree in a keeper process, and
// every other OS watches it, naming why.
std::cfg_select! {
    target_os = "linux" => {
        mod linux;
        use linux::{detect, held, serve_if_asked};
    }
    _ => {
        /// The containment a tree is held in here: a watch, naming why, whatever the keeper.
        fn detect(_keeper: &Path) -> Containment {
            Containment::Watch(NO_KEEPER)
        }

        /// A tree held as `how`, which this OS holds no tree as.
        fn held(how: &Containment, _program: &OsStr) -> io::Result<(Command, Box<dyn Contain>)> {
            Err(unavailable(how))
        }

        /// No keeper runs here, so this process serves none.
        fn serve_if_asked() {}

        std::cfg_select! {
            target_os = "macos" => {
                /// Why a tree is watched here: macOS offers no containment a process can hold a
                /// tree in.
                const NO_KEEPER: Why = Why::NoPublicContainment;
            }
            _ => {
                /// Why a tree is watched here: this OS is none this crate holds a tree on.
                const NO_KEEPER: Why = Why::Unsupported;
            }
        }
    }
}

/// The error a tree held as `how` answers where this OS holds no tree as `how`.
fn unavailable(how: &Containment) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("containment {how:?} is not available"),
    )
}

/// Serves the keeper and exits, when this process was started as one; returns at once otherwise.
/// An executable that may be named as a [`Containment::Keeper`] calls this first in its `main`,
/// before it reads its arguments or opens anything.
pub fn serve_keeper_if_asked() {
    serve_if_asked();
}

/// How long a watcher waits for a wake before it walks its tree again.
const TICK: Duration = Duration::from_millis(250);

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

impl Containment {
    /// The containment this OS gives a tree whose keeper would be `keeper`: on Linux the keeper,
    /// once `keeper` answers its probe, and a watch naming why otherwise.
    pub fn detect(keeper: &Path) -> Containment {
        detect(keeper)
    }
}

/// Why a tree is watched rather than held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// A test chose it.
    Chosen,
    /// The host was built through its test-convenience default, which chooses no containment.
    NotChosen,
    /// The keeper did not answer its probe, for the reason given.
    ProbeFailed(String),
    /// A job object could not hold the tree, for the reason given.
    JobFailed(String),
    /// The OS offers no containment a process can hold a tree in.
    NoPublicContainment,
    /// The OS is none this crate holds a tree on.
    Unsupported,
}

/// A command whose process starts a contained tree: the command's own process is the tree's main,
/// and every process it starts is the tree's.
pub struct TreeCommand {
    command: Command,
    contain: Box<dyn Contain>,
}

impl TreeCommand {
    /// A command for `program` ([`crate::subprocess::command`]), contained as `how`. A kept tree's
    /// command runs the keeper, whose arguments after `program` are the main's and whose
    /// environment, directory and streams the main inherits. An error where this OS holds no tree
    /// as `how`.
    pub fn new(program: impl AsRef<OsStr>, how: &Containment) -> io::Result<TreeCommand> {
        let (command, contain): (Command, Box<dyn Contain>) = match how {
            Containment::Watch(_) => (subprocess::command(program), Box::new(Watch)),
            Containment::Keeper(_) | Containment::Job => held(how, program.as_ref())?,
        };
        Ok(TreeCommand { command, contain })
    }

    /// The command, for its arguments, environment, directory and streams.
    pub fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    /// Starts the tree, contained as the command was made.
    pub fn spawn(self) -> io::Result<ReapedChild> {
        self.contain.spawn(self.command)
    }
}

/// How a tree is started: a containment spawns the command it was made with and answers the
/// handle that holds what it started.
trait Contain: Send {
    /// Spawns `command` and answers the handle its tree is held through.
    fn spawn(self: Box<Self>, command: Command) -> io::Result<ReapedChild>;
}

/// A watched tree's start: the command's process is the main, and a watcher walks its tree.
struct Watch;

impl Contain for Watch {
    fn spawn(self: Box<Self>, mut command: Command) -> io::Result<ReapedChild> {
        let (wake, stopper) = Wake::new()?;
        let main = command.spawn()?;
        let start = os_process::start_time(main.id());
        let mut tree = ReapedChild::new(main);
        if let Some(start) = start {
            let main = Identity {
                pid: tree.id(),
                start,
            };
            tree.hold = Box::new(Watched(Some(Watcher::start(main, wake, stopper)?)));
        }
        Ok(tree)
    }
}

/// How a handle holds its tree, given the process the handle spawned: the main of a watched tree,
/// and the keeper of a kept one.
trait Hold: Send {
    /// The main process's pid.
    fn id(&self, child: &Child) -> u32;
    /// The main process's exit status, once it has exited.
    fn try_wait(&mut self, child: &mut Child) -> io::Result<Option<ExitStatus>>;
    /// Waits for the main process to exit, sending it nothing.
    fn wait(&mut self, child: &mut Child) -> io::Result<ExitStatus>;
    /// Ends the tree, answering what the end left running and what it never signalled.
    fn end(&mut self, child: &mut Child) -> EndReport;
    /// Lets the tree go, ending nothing.
    fn release(&mut self, child: &mut Child);
}

/// A watched tree, held through its main process, and its watcher where it has one.
struct Watched(Option<Watcher>);

impl Hold for Watched {
    fn id(&self, main: &Child) -> u32 {
        main.id()
    }

    fn try_wait(&mut self, main: &mut Child) -> io::Result<Option<ExitStatus>> {
        main.try_wait()
    }

    fn wait(&mut self, main: &mut Child) -> io::Result<ExitStatus> {
        main.wait()
    }

    /// Stops the watcher, then ends the tree ([`crate::reap::end_watched`]), the processes the
    /// watcher remembered among the end's own.
    fn end(&mut self, main: &mut Child) -> EndReport {
        let remembered = self.0.take().map(Watcher::stop).unwrap_or_default();
        reap::end_watched(main, &remembered)
    }

    /// Stops the watcher and reaps the main if it has exited.
    fn release(&mut self, main: &mut Child) {
        if let Some(watcher) = self.0.take() {
            watcher.stop();
        }
        let _ = main.try_wait();
    }
}

/// The handle a tree is ended through: the process it spawned, and how it holds the tree.
///
/// Dropping the handle without [`ReapedChild::release`] ends the tree ([`ReapedChild::end`]). That
/// runs on a normal scope exit and on an unwinding panic alike, so neither a finishing driver nor a
/// crashing one leaves the tree running. A driver that ends with neither (an abort, an exit that
/// skips unwinding, an outside signal) runs no drop; a child that must not outlive it is bound to
/// it by a pipe the handle holds as well, as the auto-started dash is to its stdin.
pub struct ReapedChild {
    child: Child,
    hold: Box<dyn Hold>,
    released: bool,
}

impl ReapedChild {
    /// Adopts `child`, already spawned, as the main of a tree no watcher has seen: the end finds
    /// what it started by its own walk. The caller owns the spawn; this handle owns the end.
    pub fn new(child: Child) -> ReapedChild {
        ReapedChild {
            child,
            hold: Box::new(Watched(None)),
            released: false,
        }
    }

    /// The main process's pid.
    pub fn id(&self) -> u32 {
        self.hold.id(&self.child)
    }

    /// The main process's input, once: taking it and dropping it closes the main's input.
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    /// The main process's output, once.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    /// The main process's error output, once.
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    /// The main process's exit status, once it has exited, reaping it then.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.hold.try_wait(&mut self.child)
    }

    /// Waits for the main process to exit, sending it nothing, and reaps it.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.hold.wait(&mut self.child)
    }

    /// Hand each line of the main process's output to `each` until the process ends the stream,
    /// `each` breaks, a line is no text or `deadline` passes, whichever comes first: a process that
    /// answers nothing holds this read no longer than `deadline`. A main with no output pipe hands
    /// over nothing. The tree is left running; the handle's end or drop still ends it.
    pub fn read_lines_until(
        &mut self,
        deadline: std::time::Instant,
        mut each: impl FnMut(&str) -> std::ops::ControlFlow<()>,
    ) {
        let Some(stdout) = self.child.stdout.take() else {
            return;
        };
        let (lines, _reader) = line_channel(stdout);
        while let Ok(Ok(line)) =
            lines.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            if each(&line).is_break() {
                return;
            }
        }
    }

    /// Ends the tree and reaps its main, answering what the end left running and what it found
    /// outside the tree and never signalled. A watched tree's watcher stops first, and the
    /// processes it remembered are the end's own ([`crate::reap::end_watched`]).
    pub fn end(&mut self) -> EndReport {
        self.hold.end(&mut self.child)
    }

    /// Lets the tree go: ends nothing, and reaps the main if it has exited.
    pub fn release(mut self) {
        self.released = true;
        self.hold.release(&mut self.child);
    }

    /// Ends the main process, and the descendants `descendants` snapshotted before it was asked to
    /// exit ([`crate::reap::end_child`]): the stop's end until the stop ends the tree through
    /// [`ReapedChild::end`].
    #[doc(hidden)]
    pub fn end_from_snapshot(&mut self, descendants: reap::DescendantSnapshot) {
        reap::end_child(&mut self.child, descendants);
    }
}

impl Drop for ReapedChild {
    fn drop(&mut self) {
        if !self.released {
            self.end();
        }
    }
}

/// A watched tree's watcher: a thread that walks the main's descendants and remembers each one,
/// until it is stopped or the main is gone.
struct Watcher {
    stopper: Stopper,
    thread: JoinHandle<Vec<Identity>>,
}

impl Watcher {
    /// Starts watching the tree under `main`, waking through `wake` until `stopper` is dropped.
    fn start(main: Identity, wake: Wake, stopper: Stopper) -> io::Result<Watcher> {
        let thread = std::thread::Builder::new()
            .name("rigger-tree-watch".to_string())
            .spawn(move || watch(main, wake))?;
        Ok(Watcher { stopper, thread })
    }

    /// Stops the watcher and answers every process it remembered.
    fn stop(self) -> Vec<Identity> {
        drop(self.stopper);
        self.thread.join().unwrap_or_default()
    }
}

/// The watcher's walk: while `main` runs, each wake reads the process table once and remembers
/// every descendant of the main, and every descendant of a process it remembered that has left
/// the tree, forgetting each remembered process that has exited. Answers what it remembered when
/// `wake` is stopped or the main is gone.
fn watch(main: Identity, mut wake: Wake) -> Vec<Identity> {
    let mut remembered: HashSet<Identity> = HashSet::new();
    wake.watch(main.pid);
    loop {
        let table = os_process::table();
        let live: HashSet<Identity> = table
            .iter()
            .map(|entry| Identity {
                pid: entry.pid,
                start: entry.start,
            })
            .collect();
        if !live.contains(&main) {
            break;
        }
        remembered.retain(|identity| live.contains(identity));
        let mut found: HashSet<Identity> =
            reap::descendants_in(&table, main.pid).into_iter().collect();
        let left: Vec<Identity> = remembered.difference(&found).copied().collect();
        for identity in left {
            found.extend(reap::descendants_in(&table, identity.pid));
        }
        for identity in found {
            if remembered.insert(identity) {
                wake.watch(identity.pid);
            }
        }
        if !wake.wait(TICK) {
            break;
        }
    }
    remembered.into_iter().collect()
}

/// A stream (a child's stdout or stderr) read line by line on its own thread and forwarded over a
/// channel, so its reader can wait on a line with a timeout where `BufRead::lines` alone would
/// block until the child writes. Each line arrives as `BufRead::lines` yields it, a read error
/// included, and the receiver disconnects at the stream's end. The thread ends there or once
/// the receiver is gone; its handle is the caller's to join or to leave.
pub fn line_channel(
    stream: impl std::io::Read + Send + 'static,
) -> (
    std::sync::mpsc::Receiver<std::io::Result<String>>,
    std::thread::JoinHandle<()>,
) {
    use std::io::BufRead;
    let (line_tx, line_rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stream).lines() {
            if line_tx.send(line).is_err() {
                return; // the reader is gone; nothing left to feed.
            }
        }
    });
    (line_rx, reader)
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

#[cfg(test)]
mod supervised_lifecycle {
    //! Spec 19b, unit 3: a dropped handle ends its tree, so a normally-finishing OR crashing
    //! agent leaves no orphaned `rigger` process. The standalone-`rigger dash` proof the
    //! criterion names lives in `tests/cli.rs` (it needs the compiled binary); this hermetic test
    //! proves the SAME [`ReapedChild`] discipline generically, on a stand-in child on the CRASH
    //! path.
    use super::ReapedChild;
    use std::time::Duration;

    /// A real long-lived child that would outlive the test unless it is reaped. Its
    /// stdout is piped and never written to, so a reader on it blocks until the child
    /// EXITS (the child's write end closes -> EOF). That is a std-only, race-free "is
    /// it still alive?" probe that needs no `libc` (unavailable in the light lane).
    fn spawn_blocking_child() -> (std::process::Child, std::process::ChildStdout) {
        use std::process::{Command, Stdio};
        let mut child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn a long-lived child");
        let out = child.stdout.take().expect("child stdout is piped");
        (child, out)
    }

    /// Watch a child's piped stdout on a helper thread: a `recv` that BLOCKS means the
    /// child is still alive (its write end is open); a `recv` that yields `0` means the
    /// child exited and its stdout reached EOF - i.e. it was reaped.
    fn watch_for_exit(mut out: std::process::ChildStdout) -> std::sync::mpsc::Receiver<usize> {
        use std::io::Read;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1];
            let n = out.read(&mut buf).unwrap_or(0);
            let _ = tx.send(n);
        });
        rx
    }

    #[test]
    fn reaped_child_reaps_even_when_the_driver_panics() {
        let (child, out) = spawn_blocking_child();
        let exited = watch_for_exit(out);

        // A CRASHING driver (a panicking agent) still unwinds through the guard's Drop,
        // so the child is reaped on the crash path exactly as on the clean path.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ReapedChild::new(child);
            panic!("the driving agent crashed");
        }));
        assert!(panicked.is_err(), "the closure was expected to panic");

        let n = exited
            .recv_timeout(Duration::from_secs(5))
            .expect("a panic-unwound ReapedChild did not reap its process");
        assert_eq!(n, 0, "a reaped child's stdout should be at EOF");
    }
}

#[cfg(test)]
mod bounded_read {
    //! [`ReapedChild::read_lines_until`]: a read of the child's stdout that hands each line over
    //! until the child ends it, the reader breaks, a line is no text or the deadline passes,
    //! whichever comes first, so a child that answers nothing holds its reader no longer than the
    //! deadline.
    use super::ReapedChild;
    use std::ops::ControlFlow;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// One row of the table below.
    type Row<'a> = (
        &'a str,
        &'a str,
        bool,
        Duration,
        Option<&'a str>,
        &'a [&'a str],
        (Duration, Duration),
    );

    #[cfg(unix)]
    #[test]
    fn a_read_ends_at_the_end_of_the_stream_a_break_a_line_that_is_no_text_or_the_deadline() {
        let long = Duration::from_secs(5);
        let short = Duration::from_millis(600);
        let early = (Duration::ZERO, Duration::from_secs(2));
        let at_the_bound = (short, short + Duration::from_millis(1500));
        // (name, the child's shell, its stdout piped, the bound, the line the reader breaks at,
        // the lines read, elapsed from and below)
        let rows: [Row; 5] = [
            (
                "a child that ends",
                "printf 'a\\nb\\n'",
                true,
                long,
                None,
                &["a", "b"],
                early,
            ),
            (
                "a child still running at the deadline",
                "printf 'a\\nb\\n'; exec sleep 3",
                true,
                short,
                None,
                &["a", "b"],
                at_the_bound,
            ),
            (
                "a reader that breaks",
                "printf 'a\\nb\\n'; exec sleep 3",
                true,
                long,
                Some("a"),
                &["a"],
                early,
            ),
            (
                "a line that is no text",
                "printf 'a\\n\\377\\nb\\n'; exec sleep 3",
                true,
                long,
                None,
                &["a"],
                early,
            ),
            (
                "a child with no stdout pipe",
                "exec sleep 3",
                false,
                long,
                None,
                &[],
                early,
            ),
        ];
        for (name, script, piped, bound, break_at, expected, (from, below)) in rows {
            let child = Command::new("sh")
                .args(["-c", script])
                .stdout(if piped { Stdio::piped() } else { Stdio::null() })
                .spawn()
                .unwrap();
            let mut child = ReapedChild::new(child);
            let mut read = Vec::new();
            let started = Instant::now();
            child.read_lines_until(started + bound, |line| {
                read.push(line.to_string());
                if Some(line) == break_at {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            });
            let elapsed = started.elapsed();
            assert_eq!(read, expected, "{name}");
            assert!(
                from <= elapsed && elapsed < below,
                "{name}: the read ended after {elapsed:?}, outside {from:?}..{below:?}"
            );
        }
    }
}
