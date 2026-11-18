//! A process tree started through one handle and ended through it: the session a host spawns, and
//! every process that session starts.
//!
//! A host spawns a session through [`TreeCommand`] and holds the [`ReapedChild`] it answers. The
//! handle reads and waits on the session's main process as a [`Child`] does, and ends everything
//! the session started: [`ReapedChild::end`] ends the whole tree at once, as a stop does, and
//! [`ReapedChild::finish`] ends it once its host is done reading it, whether its main has exited
//! or still runs. A dropped handle is ended, on a normal scope exit and on an unwinding panic
//! alike, so a driver that crashes leaves no session running behind it.
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
//! status the keeper reports for it. A jobbed tree ([`Containment::Job`], on Windows) is held in a
//! job object this process creates and alone holds, which the main joins before it runs, so every
//! process the tree starts is in the job and none can leave it. [`Containment::detect`] answers
//! the containment this OS gives a tree. Every signal, and the end of a job, goes through the
//! reaper ([`crate::reap`]), the one place this crate ends a process.

use crate::reap::{self, Identity};
pub use crate::reap::{EndReport, Leftover};
use crate::{os_process, subprocess, whole_line};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output};
use std::sync::{mpsc, Mutex, Once, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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

// The one spelling of the OS set for a held tree: Linux holds a tree in a keeper process, Windows
// in a job object, and every other OS watches it, naming why.
std::cfg_select! {
    target_os = "linux" => {
        mod linux;
        use linux::{detect, held, serve_if_asked};
    }
    target_os = "windows" => {
        mod windows;
        use windows::{detect, held, serve_if_asked};
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
    /// once `keeper` answers its probe; on Windows a job, once one can be created; and a watch
    /// naming why otherwise.
    pub fn detect(keeper: &Path) -> Containment {
        detect(keeper)
    }

    /// The containment this OS gives a tree whose keeper is the running executable, whose `main`
    /// calls [`serve_keeper_if_asked`] first ([`Containment::detect`]). The keeper is named by the
    /// path that execs the running image ([`os_process::running_image`]), so on Linux, the one OS
    /// that reads a keeper's path, it is the very build that answered the probe.
    pub fn of_running_binary() -> Containment {
        detect(&os_process::running_image())
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

impl Why {
    /// Why a tree is watched where a containment was wanted: a keeper that failed its probe, a job
    /// that could not hold the tree, or a host that chose none. `None` where the watch is the
    /// design (macOS, an OS this crate holds no tree on) or a test's choice.
    fn degradation(&self) -> Option<String> {
        match self {
            Why::ProbeFailed(reason) => {
                Some(format!("the session keeper failed its probe: {reason}"))
            }
            Why::JobFailed(reason) => Some(format!("no job object holds the session: {reason}")),
            Why::NotChosen => Some("the host chose no containment".to_string()),
            Why::Chosen | Why::NoPublicContainment | Why::Unsupported => None,
        }
    }
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
    /// environment, directory and streams the main inherits; a jobbed tree's command starts its
    /// main suspended, so it runs only once it is in the job. An error where this OS holds no tree
    /// as `how`.
    pub fn new(program: impl AsRef<OsStr>, how: &Containment) -> io::Result<TreeCommand> {
        let (command, contain): (Command, Box<dyn Contain>) = match how {
            Containment::Watch(why) => (subprocess::command(program), Box::new(Watch(why.clone()))),
            Containment::Keeper(_) | Containment::Job => held(how, program.as_ref())?,
        };
        Ok(TreeCommand { command, contain })
    }

    /// The command, for its arguments, environment, directory and streams. On Windows its creation
    /// flags are the containment's: a jobbed tree's command is created suspended, and setting them
    /// again would start the main before it is in the job.
    pub fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    /// Starts the tree, contained as the command was made. Where this process signals a
    /// session's processes by pid because process descriptors are unavailable, it says so once
    /// for each reason ([`reap::say_signalled_by_pid`]).
    pub fn spawn(self) -> io::Result<ReapedChild> {
        if let Some(reason) = reap::no_process_descriptors() {
            reap::say_signalled_by_pid(&reason);
        }
        self.contain.spawn(self.command)
    }
}

/// How a tree is started: a containment spawns the command it was made with and answers the
/// handle that holds what it started.
trait Contain: Send {
    /// Spawns `command` and answers the handle its tree is held through.
    fn spawn(self: Box<Self>, command: Command) -> io::Result<ReapedChild>;
}

/// A watched tree's start, and why it is watched: the command's process is the main, and a
/// watcher walks its tree.
struct Watch(Why);

impl Contain for Watch {
    fn spawn(self: Box<Self>, mut command: Command) -> io::Result<ReapedChild> {
        let mut tree = ReapedChild::new(command.spawn()?);
        tree.watch()?;
        tree.watched_because(&self.0);
        Ok(tree)
    }
}

/// How a handle holds its tree, given the process the handle spawned: the main of a watched or a
/// jobbed tree, and the keeper of a kept one. A hold whose process is the main reads and waits on
/// that process itself, as each method's default does.
trait Hold: Send {
    /// The main process's pid.
    fn id(&self, child: &Child) -> u32 {
        child.id()
    }
    /// The main process's exit status, once it has exited.
    fn try_wait(&mut self, child: &mut Child) -> io::Result<Option<ExitStatus>> {
        child.try_wait()
    }
    /// Waits for the main process to exit, sending it nothing.
    fn wait(&mut self, child: &mut Child) -> io::Result<ExitStatus> {
        child.wait()
    }
    /// The processes the tree's end would reach now that are still running, its main aside.
    fn members(&mut self, child: &mut Child) -> Vec<Identity>;
    /// Ends the tree, answering what the end signalled, what it left running and what it never
    /// signalled.
    fn end(&mut self, child: &mut Child) -> EndReport;
}

/// A watched tree, held through its main process, and its watcher where it has one.
struct Watched(Option<Watcher>);

impl Hold for Watched {
    /// The main's running descendants while the main runs, and none once it has exited, when the
    /// end signals nothing ([`crate::reap::end_watched`]).
    fn members(&mut self, main: &mut Child) -> Vec<Identity> {
        if !matches!(main.try_wait(), Ok(None)) {
            return Vec::new();
        }
        reap::descendants_of(main.id())
            .into_iter()
            .filter(reap::running)
            .collect()
    }

    /// Stops the watcher, then ends the tree ([`crate::reap::end_watched`]), the processes the
    /// watcher remembered among the end's own.
    fn end(&mut self, main: &mut Child) -> EndReport {
        let remembered = self.0.take().map(Watcher::stop).unwrap_or_default();
        reap::end_watched(main, &remembered)
    }
}

/// The handle a tree is ended through: the process it spawned, and how it holds the tree.
///
/// Dropping the handle ends the tree ([`ReapedChild::end`]). That runs on a normal scope exit and
/// on an unwinding panic alike, so neither a finishing driver nor a crashing one leaves the tree
/// running. A driver that ends with neither (an abort, an exit that skips unwinding, an outside
/// signal) runs no drop; a child that must not outlive it is bound to it by a pipe the handle
/// holds as well, as the auto-started dash is to its stdin.
pub struct ReapedChild {
    child: Child,
    hold: Box<dyn Hold>,
    degraded: Option<String>,
}

impl ReapedChild {
    /// Adopts `child`, already spawned, as the main of a tree no watcher has seen: the end finds
    /// what it started by its own walk. The caller owns the spawn; this handle owns the end.
    pub fn new(child: Child) -> ReapedChild {
        ReapedChild::holding(child, Box::new(Watched(None)))
    }

    /// The handle to the tree `hold` holds, whose process is `child`.
    fn holding(child: Child, hold: Box<dyn Hold>) -> ReapedChild {
        ReapedChild {
            child,
            hold,
            degraded: None,
        }
    }

    /// Starts a watcher on this tree's main, which this handle then holds the tree through. A main
    /// whose start time cannot be read is left to the end's own walk.
    fn watch(&mut self) -> io::Result<()> {
        let pid = self.child.id();
        let Some(start) = os_process::start_time(pid) else {
            return Ok(());
        };
        let (wake, stopper) = Wake::new()?;
        let main = Identity { pid, start };
        self.hold = Box::new(Watched(Some(Watcher::start(main, wake, stopper)?)));
        Ok(())
    }

    /// Notes that this tree is watched because `why`. Where that is a degradation
    /// ([`ReapedChild::degraded`]) the handle names it, and this process says it once on its error
    /// output.
    fn watched_because(&mut self, why: &Why) {
        static SAID: Once = Once::new();
        let Some(reason) = why.degradation() else {
            return;
        };
        SAID.call_once(|| {
            whole_line::say(format_args!(
                "rigger: session containment unavailable: {reason}; a session's processes are \
                 watched, and one whose parent exits unseen can outlive the session"
            ))
        });
        self.degraded = Some(reason);
    }

    /// Why this tree is watched where a containment was wanted: its host's containment was a
    /// watch for a keeper that failed its probe or for no choice at all, or on Windows its launch
    /// fell back to a watch for a job that could not be created or joined. The host says so in a
    /// progress line.
    pub fn degraded(&self) -> Option<&str> {
        self.degraded.as_deref()
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

    /// Closes the main process's input, as [`Child::wait`] does, then waits for the main to exit,
    /// sending it nothing, and reaps it.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        drop(self.take_stdin());
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

    /// Ends the tree and reaps its main, answering what the end signalled besides the main
    /// ([`EndReport::sent`]), what it left running and what it found outside the tree and never
    /// signalled. A watched tree's watcher stops first, and the
    /// processes it remembered are the end's own ([`crate::reap::end_watched`]). An end that
    /// signalled by pid, in this process or in the tree's keeper, says why once for each reason
    /// ([`reap::say_signalled_by_pid`]).
    pub fn end(&mut self) -> EndReport {
        let report = self.hold.end(&mut self.child);
        if let Some(reason) = &report.by_pid_because {
            reap::say_signalled_by_pid(reason);
        }
        report
    }

    /// Finishes with a tree its host is done reading by ending it ([`ReapedChild::end`]), whether
    /// its main has exited or still runs, so nothing the session started outlives it. Where the
    /// main has exited and a `writer` is given, the end first waits, for at most
    /// [`crate::reap::END_BOUND`], while a process it would reach is a writer: one whose
    /// arguments, read while its pid still names it ([`os_process::arguments_of`]), `writer`
    /// accepts, as a hook or a tool server still writing the spawn's record is, so that write
    /// finishes before the end. The bound holds whatever OS call a read of arguments waits in:
    /// processes whose arguments were not read by then are no writers, and the end reports why
    /// ([`ArgumentReads`]). A main that still runs is ended at once with all it started, as a
    /// stop ends it. A watched tree whose main has exited signals nothing, and answers what the
    /// main left running as never signalled ([`crate::reap::end_watched`]). Answers what the end
    /// signalled and what it left ([`ReapedChild::end`]).
    pub fn finish(self, writer: Option<&IsWriter<'_>>) -> EndReport {
        self.finish_with(writer, &ARGUMENTS, reap::END_BOUND)
    }

    /// [`ReapedChild::finish`], reading the arguments of the processes its end would reach
    /// through `reads` and waiting for a writer for at most `bound`.
    fn finish_with(
        mut self,
        writer: Option<&IsWriter<'_>>,
        reads: &ArgumentReads,
        bound: Duration,
    ) -> EndReport {
        let mut unread = None;
        if let Some(writer) = writer {
            if matches!(self.try_wait(), Ok(Some(_))) {
                let deadline = Instant::now() + bound;
                reap::poll_until(deadline, || {
                    match reads.of(self.hold.members(&mut self.child), deadline) {
                        Ok(arguments) => !arguments.iter().flatten().any(|words| writer(words)),
                        Err(why) => {
                            unread = Some(why);
                            true
                        }
                    }
                });
            }
        }
        let mut report = self.end();
        report.faults.extend(unread);
        report
    }

    /// Waits for the main process to exit and answers its status and all of its output, as
    /// [`Child::wait_with_output`] does: its input is closed first, and each output pipe is read
    /// to its end on a thread of its own. Once the main exits the tree is finished, for no writer
    /// ([`ReapedChild::finish`]), before the pipes are read to their end, so a process
    /// the main left holding an output pipe is ended rather than waited for; one the finish never
    /// signalled, or could not end, holds this read until it exits, as it holds
    /// `Child::wait_with_output`'s. Answers the output and what the finish ended and left. A wait
    /// that fails ends the tree.
    pub fn wait_with_output(mut self) -> io::Result<(Output, EndReport)> {
        let stdout = self.take_stdout().map(read_to_end);
        let stderr = self.take_stderr().map(read_to_end);
        let status = self.wait()?;
        let report = self.finish(None);
        let output = Output {
            status,
            stdout: joined(stdout)?,
            stderr: joined(stderr)?,
        };
        Ok((output, report))
    }
}

/// Whether a process, by its arguments ([`os_process::arguments`]), is a writer an ordinary end
/// waits for ([`ReapedChild::finish`]).
pub type IsWriter<'a> = dyn Fn(&[String]) -> bool + 'a;

/// How a finish reads the arguments of the processes its end would reach, to find a writer
/// among them ([`ReapedChild::finish`]): each process's by `read`, on a thread of its own, so the
/// finish gives the read up at the end of its wait whatever OS call the read waits in, as a read
/// of a process stuck in the kernel waits on that process's memory. A read given up runs on until
/// its call returns, and then its answer is dropped and its thread exits; while one has not
/// returned, no finish starts another, so given-up reads never pile up behind a process that
/// stays stuck.
struct ArgumentReads {
    /// Reads one process's arguments; `None` where they cannot be read, which no writer's are.
    read: fn(&Identity) -> Option<Vec<String>>,
    /// The thread of each read a finish gave up, kept until it is seen to have returned.
    given_up: Mutex<Vec<JoinHandle<()>>>,
}

/// The reads every finish makes: each process's arguments while its pid still names it
/// ([`os_process::arguments_of`]), so a pid that names another process by now is no writer's.
static ARGUMENTS: ArgumentReads = ArgumentReads {
    read: arguments_while_named,
    given_up: Mutex::new(Vec::new()),
};

/// `member`'s arguments while its pid still names it ([`os_process::arguments_of`]).
fn arguments_while_named(member: &Identity) -> Option<Vec<String>> {
    os_process::arguments_of(member.pid, member.start)
}

/// The fault a finish reports where the arguments of the processes its end would reach were not
/// read by the end of its wait for a writer ([`ArgumentReads::of`]).
const ARGUMENTS_GIVEN_UP: &str = "the arguments of the processes this end would reach were not \
     read by the end of its wait for a writer, as when one of them is stuck in the kernel; it \
     took none of them for a writer, and no end reads arguments until that read returns";

/// The fault a finish reports where the arguments of the processes its end would reach could not
/// be read, for the reason `why` ([`ArgumentReads::of`]).
fn arguments_unread(why: &str) -> String {
    format!(
        "the arguments of the processes this end would reach could not be read ({why}), so it \
         took none of them for a writer"
    )
}

/// The fault a finish reports where a read of arguments an earlier finish gave up had not
/// returned, so it read none ([`ArgumentReads::of`]).
const ARGUMENTS_HELD: &str = "a read of arguments an earlier end gave up has not returned, so \
     this end read none and took none of the processes it would reach for a writer";

impl ArgumentReads {
    /// The arguments of each of `members`, in their order, read ([`ArgumentReads::read`]) on a
    /// thread of its own by `deadline`, the end of the finish's wait for a writer; none once
    /// `deadline` has passed, and none where there are no members. Where they were not read by
    /// `deadline`, the read is given up ([`ArgumentReads::given_up`]); where a read given up
    /// before has not returned, none is started; and where no thread could be started, or the
    /// read ended without an answer, none is read. Each answers why.
    fn of(
        &self,
        members: Vec<Identity>,
        deadline: Instant,
    ) -> Result<Vec<Option<Vec<String>>>, String> {
        if members.is_empty() || Instant::now() >= deadline {
            return Ok(Vec::new());
        }
        if self.still_given_up() > 0 {
            return Err(ARGUMENTS_HELD.to_string());
        }
        let read = self.read;
        let (answer, answered) = mpsc::channel();
        let reader = std::thread::Builder::new()
            .name("rigger-tree-arguments".to_string())
            .spawn(move || {
                let _ = answer.send(members.iter().map(read).collect());
            })
            .map_err(|error| {
                arguments_unread(&format!("no thread to read them started: {error}"))
            })?;
        match answered.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(arguments) => Ok(arguments),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(arguments_unread("their read ended without an answer"))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.given_up
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(reader);
                Err(ARGUMENTS_GIVEN_UP.to_string())
            }
        }
    }

    /// How many reads a finish gave up have not returned yet; the threads of those that have
    /// returned are let go.
    fn still_given_up(&self) -> usize {
        let mut given_up = self.given_up.lock().unwrap_or_else(PoisonError::into_inner);
        given_up.retain(|read| !read.is_finished());
        given_up.len()
    }
}

/// Reads `pipe` to its end on a thread of its own.
fn read_to_end(mut pipe: impl Read + Send + 'static) -> JoinHandle<io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).map(|_| bytes)
    })
}

/// What a [`read_to_end`] read, empty where there was no pipe to read.
fn joined(reader: Option<JoinHandle<io::Result<Vec<u8>>>>) -> io::Result<Vec<u8>> {
    reader.map_or(Ok(Vec::new()), |reader| {
        reader.join().unwrap_or_else(|_| {
            Err(io::Error::other(
                "a reader of the session's output panicked",
            ))
        })
    })
}

impl Drop for ReapedChild {
    fn drop(&mut self) {
        self.end();
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

    /// The pids `report` names as sent the end, none of them the main's `main`, and the rest of
    /// the report.
    fn sent_besides(main: u32, report: EndReport) -> (Vec<u32>, EndReport) {
        let sent: Vec<u32> = report.sent.iter().map(|left| left.pid).collect();
        assert!(
            !sent.contains(&main),
            "the main is never named as sent the end"
        );
        let rest = EndReport {
            sent: Vec::new(),
            ..report
        };
        (sent, rest)
    }

    /// A watched tree running `body`, ended once `up` of its processes are up, leaves nothing
    /// running and reports only the processes besides the main it sent its end to.
    fn ends_the_tree(body: &str, up: usize) {
        let mut witness = Witness::new();
        let mut tree = watched(&witness, body);
        assert!(witness.wait_up(up), "the fixture's {up} processes are up");
        let main = tree.id();

        let report = tree.end();

        let (sent, rest) = sent_besides(main, report);
        assert_eq!(
            (witness.read(), sent.is_empty(), rest),
            (Reading::Ended, false, EndReport::default())
        );
    }

    crate::test_cases! {
        a_watched_end_ends_a_tree_that_forks_as_it_ends:
            ends_the_tree(
                "( i=0; while [ $i -lt 100 ]; do holder; sleep 0.02; i=$((i+1)); done; \
                 up; held ) & up; held",
                6,
            );
        // Linux only: util-linux `setsid`, which the fixture starts its process with.
        #[cfg(target_os = "linux")]
        a_watched_end_ends_a_process_in_a_session_of_its_own:
            ends_the_tree("holder setsid; up; held", 2);
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

    /// A watched tree running `body`, finished for the `writer` given once `up` of its
    /// processes are up and, where `exited`, once its main has exited, leaves the witness reading
    /// `left`, reports `sent` processes besides the main it sent its end to and `skipped` it found
    /// and never signalled, leaves nothing it signalled running, and never waits out the writers'
    /// bound.
    fn finishes(
        body: &str,
        up: usize,
        exited: bool,
        writer: Option<&IsWriter<'_>>,
        left: Reading,
        sent: usize,
        skipped: usize,
    ) {
        let mut witness = Witness::new();
        let mut tree = watched(&witness, body);
        assert!(witness.wait_up(up), "the fixture's {up} processes are up");
        if exited {
            assert!(
                wait_until(|| matches!(tree.try_wait(), Ok(Some(_)))),
                "the main exits"
            );
        }

        let started = std::time::Instant::now();

        let report = tree.finish(writer);

        let elapsed = started.elapsed();
        assert!(
            elapsed < crate::reap::END_BOUND,
            "the finish took {elapsed:?}"
        );
        assert_eq!(
            (
                witness.read(),
                report.sent.len(),
                report.residue,
                report.skipped.len()
            ),
            (left, sent, vec![], skipped)
        );
        assert_eq!(witness.release(), Reading::Ended);
    }

    crate::test_cases! {
        // The main outlives the holder's start by a second, four of the watch's walks, so the
        // watch has seen the holder before the main exits; the end then signals nothing.
        a_finished_tree_whose_main_exited_reports_what_it_left_and_signals_nothing:
            finishes("holder; up; sleep 1", 2, true, None, Reading::Held, 0, 1);
        a_finished_tree_whose_main_runs_is_ended:
            finishes("holder; up; held", 2, false, None, Reading::Ended, 1, 0);
        // A watched end signals nothing once the main has exited, so it waits for no writer.
        a_finished_tree_whose_main_exited_waits_for_no_writer:
            finishes(
                "sh -c \"$witness_up; $witness_held\" u-late-writer & up; sleep 1",
                2,
                true,
                Some(&|arguments: &[String]| {
                    arguments.iter().any(|word| word.contains("u-late-writer"))
                }),
                Reading::Held,
                0,
                1,
            );
    }

    /// A hold that names `member` among its tree's processes, by a start time one later than its
    /// own where `reused`, as a hold that read the member before another process took its pid
    /// would name it, and whose end signals nothing: the end closes the member's input and waits
    /// for it, so the member exits on its own while the end runs, and then reaps the main.
    struct EndsBySelf {
        member: Child,
        input: Option<ChildStdin>,
        reused: bool,
    }

    impl Hold for EndsBySelf {
        fn members(&mut self, _main: &mut Child) -> Vec<Identity> {
            let pid = self.member.id();
            os_process::start_time(pid)
                .map(|start| Identity {
                    pid,
                    start: start + u64::from(self.reused),
                })
                .into_iter()
                .collect()
        }

        fn end(&mut self, main: &mut Child) -> EndReport {
            drop(self.input.take());
            let _ = self.member.wait();
            let _ = main.wait();
            EndReport::default()
        }
    }

    /// A process the tree held when the finish began, that exits on its own while the end runs
    /// and is never signalled by it, is not named as one the end was sent to.
    #[test]
    fn a_finish_names_no_process_its_end_did_not_signal() {
        let mut member = Command::new("cat")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("spawn a member that exits at its input's end");
        let input = member.stdin.take();
        let main = Command::new("true").spawn().expect("spawn the main");
        let tree = ReapedChild::holding(
            main,
            Box::new(EndsBySelf {
                member,
                input,
                reused: false,
            }),
        );

        let report = tree.finish(None);

        assert_eq!(report, EndReport::default());
    }

    /// A finish for a writer reads a process its tree held only while the process's pid still
    /// names it: a member named by a start time its pid does not read, as when another process
    /// took the pid, is no writer whatever its pid's process runs, so the finish waits for none.
    #[test]
    fn a_finish_reads_no_writer_through_a_pid_its_member_no_longer_holds() {
        let mut member = Command::new("cat")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("spawn a member that exits at its input's end");
        let input = member.stdin.take();
        let main = Command::new("true").spawn().expect("spawn the main");
        let mut tree = ReapedChild::holding(
            main,
            Box::new(EndsBySelf {
                member,
                input,
                reused: true,
            }),
        );
        assert!(
            wait_until(|| matches!(tree.try_wait(), Ok(Some(_)))),
            "the main exits"
        );
        let started = std::time::Instant::now();

        let report = tree.finish(Some(&|_: &[String]| true));

        let elapsed = started.elapsed();
        assert_eq!(
            (elapsed < crate::reap::END_BOUND / 2, report),
            (true, EndReport::default()),
            "the finish took {elapsed:?}"
        );
    }

    /// Whether a read of [`HUNG`] may return, and the wake its reads wait on.
    static RELEASED: (Mutex<bool>, std::sync::Condvar) =
        (Mutex::new(false), std::sync::Condvar::new());

    /// Reads that do not return until [`release`]d, as a read of the arguments of a process stuck
    /// in the kernel does not, and then answer a writer's arguments.
    static HUNG: ArgumentReads = ArgumentReads {
        read: hangs,
        given_up: Mutex::new(Vec::new()),
    };

    /// [`HUNG`]'s read: waits until [`release`]d, and then answers a writer's arguments.
    fn hangs(_member: &Identity) -> Option<Vec<String>> {
        let (released, wake) = &RELEASED;
        let mut released = released.lock().unwrap_or_else(PoisonError::into_inner);
        while !*released {
            released = wake.wait(released).unwrap_or_else(PoisonError::into_inner);
        }
        Some(vec!["a writer".to_string()])
    }

    /// Lets every read of [`HUNG`] return.
    fn release() {
        let (released, wake) = &RELEASED;
        *released.lock().unwrap_or_else(PoisonError::into_inner) = true;
        wake.notify_all();
    }

    /// A hold that names this process among its tree's processes and whose end only reaps the
    /// main.
    struct NamesThisProcess;

    impl Hold for NamesThisProcess {
        fn members(&mut self, _main: &mut Child) -> Vec<Identity> {
            vec![Identity {
                pid: std::process::id(),
                start: 0,
            }]
        }

        fn end(&mut self, main: &mut Child) -> EndReport {
            let _ = main.wait();
            EndReport::default()
        }
    }

    /// A finish for any writer of a tree whose main has exited, reading through [`HUNG`] and
    /// waiting for a writer for at most `bound`, run on a thread of its own: how long it took and
    /// its faults, or `None` where it had not answered within `within`.
    fn finish_hung(bound: Duration, within: Duration) -> Option<(Duration, Vec<String>)> {
        let (answer, answered) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let main = Command::new("true").spawn().expect("spawn the main");
            let mut tree = ReapedChild::holding(main, Box::new(NamesThisProcess));
            assert!(
                wait_until(|| matches!(tree.try_wait(), Ok(Some(_)))),
                "the main exits"
            );
            let started = Instant::now();
            let report = tree.finish_with(Some(&|_: &[String]| true), &HUNG, bound);
            let _ = answer.send((started.elapsed(), report.faults));
        });
        answered.recv_timeout(within).ok()
    }

    /// A finish whose read of its members' arguments does not return, as a read of a process
    /// stuck in the kernel does not, gives that read up at the end of its wait for a writer and
    /// takes none of them for a writer; a finish while that read has not returned starts no read
    /// and waits for no writer, so given-up reads never pile up; once the read returns, its thread
    /// is let go.
    #[test]
    fn a_finish_gives_up_a_read_of_arguments_that_does_not_return_by_its_bound() {
        let bound = Duration::from_millis(300);
        let slack = Duration::from_secs(2);

        let first = finish_hung(bound, bound + slack);
        let second = finish_hung(bound, slack);
        let held = HUNG.still_given_up();
        release();

        let let_go = wait_until(|| HUNG.still_given_up() == 0);
        assert_eq!(
            (
                first.map(|(took, faults)| ((bound..bound + slack).contains(&took), faults)),
                second.map(|(took, faults)| (took < bound, faults)),
                held,
                let_go
            ),
            (
                Some((true, vec![ARGUMENTS_GIVEN_UP.to_string()])),
                Some((true, vec![ARGUMENTS_HELD.to_string()])),
                1,
                true
            )
        );
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
        let main = tree.id();

        let report = tree.end();

        let (sent, rest) = sent_besides(main, report);
        assert_eq!(
            (witness.read(), sent.len(), rest),
            (Reading::Ended, 1, EndReport::default())
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

#[cfg(all(test, unix))]
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
