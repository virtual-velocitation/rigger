//! The Linux keeper: this executable run again as `rigger-keeper`, a process that holds a tree as
//! its child subreaper, so every orphan of the tree is its child and every process the tree starts
//! stays its descendant while it runs. The keeper spawns the tree's main, reaps it and reports its
//! status the moment it exits, and ends the tree when its driver asks or is gone, walking its own
//! descendants until no child is left ([`crate::reap::end_own_descendants`]).
//!
//! The driver holds two pipes to the keeper, passed by descriptor number. The keeper writes
//! [`Record`]s on the status pipe, one a line, and reads the control pipe, which is the driver's
//! whole say over the tree: an `r` byte releases the tree, so the keeper exits once the main has
//! and ends nothing, and the pipe's end without that byte, a close by the driver and the driver's
//! death alike, asks the keeper to end the tree. A TERM to the keeper ends the tree at once,
//! asking no member to exit first, so a sweep that sends the keeper its TERM ends the whole tree
//! within the sweep's grace. INT, HUP and QUIT, which a terminal sends its whole foreground group,
//! leave the keeper running, so the driver's own handling of them decides. The keeper puts
//! `/dev/null` over its standard streams once the main holds them, so it keeps none of the
//! session's streams open.

use super::{line_channel, Contain, Containment, Hold, ReapedChild, Why};
use crate::reap::{self, EndReport, Identity, Leftover, END_BOUND};
use crate::{os_process, subprocess, whole_line};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, PipeReader, PipeWriter, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

/// The keeper's name, its argv[0]: this executable started under it serves a keeper
/// ([`serve_if_asked`]).
const NAME: &str = "rigger-keeper";

/// The keeper's first argument when it is asked only whether it can hold a tree here.
const PROBE: &str = "--probe";

/// How long, in milliseconds, the keeper waits on its control pipe before it reaps and reads its
/// signals again, when no signal and no byte wakes it first.
const TICK_MS: libc::c_int = 100;

/// How long the driver waits past a bound the keeper keeps, and for a keeper to exit once it has
/// written its last record.
const SLACK: Duration = Duration::from_secs(1);

/// Set by a TERM to the keeper.
static TERMED: AtomicBool = AtomicBool::new(false);

/// Serves a keeper and exits with its code when this process was started as one, its argv[0]
/// [`NAME`]; returns at once otherwise, having read nothing but its arguments.
pub(super) fn serve_if_asked() {
    let mut args = std::env::args_os();
    if args.next().as_deref() != Some(OsStr::new(NAME)) {
        return;
    }
    let rest: Vec<OsString> = args.collect();
    std::process::exit(serve(&rest));
}

/// The keeper's run over its arguments after argv[0], answering its exit code: [`PROBE`] and a
/// status descriptor probe ([`probe`]); a status descriptor, a control descriptor, a program and
/// its arguments keep the tree whose main is that program ([`keep`]).
fn serve(args: &[OsString]) -> i32 {
    match args {
        [flag, status] if flag == PROBE => match descriptors([status]) {
            Some([status]) => probe(status),
            None => usage(),
        },
        [status, control, program, rest @ ..] => match descriptors([status, control]) {
            Some([status, control]) => keep(status, control, program, rest),
            None => usage(),
        },
        _ => usage(),
    }
}

/// Says how the keeper is run, and answers the exit code of a run it refuses.
fn usage() -> i32 {
    whole_line::say(format_args!(
        "rigger: {NAME} runs as `{NAME} <status fd> <control fd> <program> [<argument>...]` or \
         `{NAME} {PROBE} <status fd>`, each fd an open descriptor above 2"
    ));
    2
}

/// The descriptors `fields` name, each taken as this process's own and closed on exec, so no
/// process the keeper spawns holds one; `None` unless each is a number above the standard streams,
/// each differs from the others, and each is open.
fn descriptors<const N: usize>(fields: [&OsString; N]) -> Option<[File; N]> {
    let fds = fields
        .iter()
        .map(|field| {
            let fd = digits(field.to_str()?)?;
            RawFd::try_from(fd).ok().filter(|fd| *fd > 2)
        })
        .collect::<Option<Vec<RawFd>>>()?;
    let distinct = fds.iter().collect::<HashSet<_>>().len() == N;
    if !distinct || !fds.iter().all(|fd| close_on_exec(*fd)) {
        return None;
    }
    let files: Vec<File> = fds
        .into_iter()
        // SAFETY: each descriptor is open (`close_on_exec` read and set its flags), differs from
        // the others and lies above the standard streams, and nothing else in this process owns
        // it: the keeper is served before anything else in `main` runs, and the runtime opens no
        // descriptor above 2 before it.
        .map(|fd| unsafe { File::from_raw_fd(fd) })
        .collect();
    files.try_into().ok()
}

/// Sets close-on-exec on `fd`; whether `fd` is open and took it.
fn close_on_exec(fd: RawFd) -> bool {
    // SAFETY: two `fcntl` calls on a plain number, the first a query: a descriptor that is not
    // open answers an error and nothing changes.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        flags >= 0 && libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) == 0
    }
}

/// Makes this process the child subreaper of its descendants; the OS error number when it cannot.
fn subreaper() -> Result<(), i32> {
    rustix::process::set_child_subreaper(Some(rustix::process::getpid()))
        .map_err(|errno| errno.raw_os_error())
}

/// Answers on `status` whether this process can hold a tree here: [`Record::Probed`] once it is a
/// child subreaper, [`Record::Unavailable`] otherwise.
fn probe(mut status: File) -> i32 {
    match subreaper() {
        Ok(()) => {
            tell(&mut status, &Record::Probed);
            0
        }
        Err(errno) => {
            tell(&mut status, &Record::Unavailable(errno));
            1
        }
    }
}

/// Keeps the tree whose main is `program` run with `args`: becomes its child subreaper, catches
/// its signals ([`catch_signals`]), spawns the main into this process's environment, directory
/// and streams, writes its pid, lets go of the streams ([`let_go_of_the_streams`]) and runs
/// ([`Keeper::run`]). A keeper that cannot be a subreaper, or cannot spawn the main, writes why
/// and exits, having started nothing.
fn keep(mut status: File, control: File, program: &OsStr, args: &[OsString]) -> i32 {
    if let Err(errno) = subreaper() {
        tell(&mut status, &Record::Unavailable(errno));
        return 1;
    }
    catch_signals();
    let main = match subprocess::command(program).args(args).spawn() {
        Ok(main) => main.id(),
        Err(error) => {
            let errno = error.raw_os_error().unwrap_or(libc::EINVAL);
            tell(&mut status, &Record::Failed(errno));
            return 1;
        }
    };
    tell(&mut status, &Record::Spawned(main));
    let_go_of_the_streams();
    Keeper {
        status,
        control: Some(control),
        main,
        main_exited: false,
        released: false,
    }
    .run()
}

/// Catches the keeper's signals: TERM sets [`TERMED`]; INT, HUP and QUIT, unless ignored on entry,
/// and CHLD always, are caught by a handler that does nothing, so each only wakes the keeper's
/// wait. A caught signal reverts to its default in the main at its exec, and an ignored one stays
/// ignored in it, as each would have under the driver.
fn catch_signals() {
    let handlers: [(libc::c_int, extern "C" fn(libc::c_int)); 4] = [
        (libc::SIGTERM, on_term),
        (libc::SIGINT, on_wake),
        (libc::SIGHUP, on_wake),
        (libc::SIGQUIT, on_wake),
    ];
    for (signal, handler) in handlers {
        if crate::interrupt::native::disposition(signal) != libc::SIG_IGN {
            catch(signal, handler);
        }
    }
    catch(libc::SIGCHLD, on_wake);
}

/// Catches `signal` with `handler`.
fn catch(signal: libc::c_int, handler: extern "C" fn(libc::c_int)) {
    // SAFETY: each handler only stores to an atomic or does nothing, both async-signal-safe.
    unsafe {
        libc::signal(signal, handler as libc::sighandler_t);
    }
}

/// The keeper's TERM handler.
extern "C" fn on_term(_signal: libc::c_int) {
    TERMED.store(true, Ordering::SeqCst);
}

/// The handler of a signal that only wakes the keeper.
extern "C" fn on_wake(_signal: libc::c_int) {}

/// Puts `/dev/null` over the keeper's standard streams, which the main holds as its own, so the
/// keeper keeps none of the session's streams open; closes them where `/dev/null` cannot be
/// opened.
fn let_go_of_the_streams() {
    // SAFETY: `open`, `dup2` and `close` on plain numbers. The keeper holds no handle to a
    // standard stream, and what it writes to one after this goes nowhere.
    unsafe {
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        for stream in 0..=2 {
            if null < 0 {
                libc::close(stream);
            } else if null != stream {
                libc::dup2(null, stream);
            }
        }
        if null > 2 {
            libc::close(null);
        }
    }
}

/// Writes `record` on `status` as one line in one write ([`crate::whole_line::write_line`]). A
/// failed write is dropped: a driver that is gone reads nothing, and the keeper goes on without
/// it.
fn tell(status: &mut File, record: &Record) {
    let _ = whole_line::write_line(status, format_args!("{}", record.encode()));
}

/// A keeper at work: its status pipe, its control pipe while it is open, its main, and what it has
/// seen.
struct Keeper {
    status: File,
    control: Option<File>,
    main: u32,
    main_exited: bool,
    released: bool,
}

impl Keeper {
    /// Reaps, reads its signals and reads its control pipe until the tree is ended or let go: a
    /// TERM ends the tree at once; the control pipe's end without a release ends it, asking each
    /// member to exit first; and a release lets the keeper exit, ending nothing, once the main has.
    fn run(mut self) -> i32 {
        loop {
            self.reap_ready();
            if TERMED.load(Ordering::SeqCst) {
                return self.end(false);
            }
            match (self.control.is_some(), self.released, self.main_exited) {
                (false, false, _) => return self.end(true),
                (_, true, true) => return 0,
                _ => self.wait_for_control(),
            }
        }
    }

    /// Reaps every child that has exited, writing the main's status the moment the main is
    /// reaped; whether a child is left.
    fn reap_ready(&mut self) -> bool {
        loop {
            match rustix::process::wait(rustix::process::WaitOptions::NOHANG) {
                Ok(Some((pid, status))) => {
                    if u32::try_from(pid.as_raw_nonzero().get()) == Ok(self.main) {
                        self.main_exited = true;
                        tell(&mut self.status, &Record::Exited(status.as_raw()));
                    }
                }
                Ok(None) => return true,
                Err(rustix::io::Errno::INTR) => {}
                Err(_) => return false,
            }
        }
    }

    /// Waits up to [`TICK_MS`] for a byte or the end on the control pipe, or for a signal, then
    /// reads one byte if one is ready: an `r` releases the tree, and the pipe's end or a failed
    /// read closes the pipe. A closed pipe is the descriptor -1, which the wait ignores, so the
    /// wait is then the tick alone.
    fn wait_for_control(&mut self) {
        let mut fds = [libc::pollfd {
            fd: self.control.as_ref().map_or(-1, AsRawFd::as_raw_fd),
            events: libc::POLLIN,
            revents: 0,
        }];
        // SAFETY: `fds` is a live array of one entry, its length passed with it.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 1, TICK_MS) };
        let Some(control) = self.control.as_mut() else {
            return;
        };
        if ready <= 0 || fds[0].revents == 0 {
            return;
        }
        let mut byte = [0];
        match control.read(&mut byte) {
            Ok(0) => self.control = None,
            Ok(_) => self.released |= byte == *b"r",
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => self.control = None,
        }
    }

    /// Ends the tree ([`crate::reap::end_own_descendants`]), asking each member to exit first when
    /// `graceful` and reaping as it walks, then writes what the end left running and what it never
    /// signalled, then that it ended.
    fn end(mut self, graceful: bool) -> i32 {
        let report = reap::end_own_descendants(graceful, || self.reap_ready());
        let records = report
            .residue
            .into_iter()
            .map(Record::Residue)
            .chain(report.skipped.into_iter().map(Record::Skipped))
            .chain([Record::Ended]);
        for record in records {
            tell(&mut self.status, &record);
        }
        0
    }
}

/// The command a tree held as `how` starts through, and its start: on Linux a tree is held by a
/// keeper ([`kept`]), and any other containment is unavailable here.
pub(super) fn held(how: &Containment, program: &OsStr) -> io::Result<(Command, Box<dyn Contain>)> {
    match how {
        Containment::Keeper(keeper) => kept(keeper, program),
        _ => Err(super::unavailable(how)),
    }
}

/// The keeper `keeper` over the main `program`, given its ends of a new status pipe and a new
/// control pipe: every argument added to the command after this is the main's, and the command's
/// environment, directory and streams are the keeper's, which the main inherits.
fn kept(keeper: &Path, program: &OsStr) -> io::Result<(Command, Box<dyn Contain>)> {
    let (status, status_end) = io::pipe()?;
    let (control_end, control) = io::pipe()?;
    let keeper_ends = vec![
        File::from(OwnedFd::from(status_end)),
        File::from(OwnedFd::from(control_end)),
    ];
    let mut command = as_keeper(keeper, &[], &keeper_ends);
    command.arg(program);
    let pipes = Pipes {
        status,
        control,
        keeper_ends,
    };
    Ok((command, Box::new(pipes)))
}

/// `keeper` run as [`NAME`] with the arguments `flags`, then the number of each of `ends`, each
/// held open across the keeper's exec.
fn as_keeper(keeper: &Path, flags: &[&str], ends: &[File]) -> Command {
    let mut command = subprocess::command(keeper);
    command.arg0(NAME).args(flags);
    for end in ends {
        command.arg(end.as_raw_fd().to_string());
        subprocess::inherit_on_exec(&mut command, end);
    }
    command
}

/// The keeper's records, as [`line_channel`] forwards them.
type Records = Receiver<io::Result<String>>;

/// Spawns `command`, a keeper given `ends`, closes `ends` here, so the keeper's are the only ones
/// left open, and reads the keeper's first record on `status` within [`END_BOUND`]: answers the
/// keeper, its records after the first, and the first.
fn start(
    mut command: Command,
    ends: Vec<File>,
    status: PipeReader,
) -> io::Result<(Child, Records, Option<Record>)> {
    let spawned = command.spawn();
    drop(ends);
    let keeper = spawned?;
    let (records, _reader) = line_channel(status);
    let first = records
        .recv_timeout(END_BOUND)
        .ok()
        .and_then(Result::ok)
        .as_deref()
        .and_then(Record::decode);
    Ok((keeper, records, first))
}

/// Whether `keeper` exits within `bound`, reaping it then.
fn exits_within(keeper: &mut Child, bound: Duration) -> bool {
    reap::poll_until(Instant::now() + bound, || {
        !matches!(keeper.try_wait(), Ok(None))
    })
}

/// Waits up to [`SLACK`] for `keeper` to exit, reaping it; one still running then is ended with
/// whatever it started ([`crate::reap::end_watched`]), and that end's report answered.
fn settle(keeper: &mut Child) -> Option<EndReport> {
    (!exits_within(keeper, SLACK)).then(|| reap::end_watched(keeper, &[]))
}

/// Why a keeper whose first record was `first` holds no tree.
fn why_unheld(first: &Option<Record>) -> String {
    match first {
        Some(Record::Unavailable(errno)) => format!(
            "it cannot hold a tree here: {}",
            io::Error::from_raw_os_error(*errno)
        ),
        Some(record) => format!("its first record was `{}`", record.encode()),
        None => "it wrote no record".to_string(),
    }
}

/// A kept tree's start: the driver's ends of the keeper's pipes, and the keeper's ends, held open
/// until the keeper is spawned.
struct Pipes {
    status: PipeReader,
    control: PipeWriter,
    keeper_ends: Vec<File>,
}

impl Contain for Pipes {
    /// Spawns the keeper and reads its first record ([`start`]): the main's pid answers the handle
    /// the tree is held through, and the main's spawn error answers that error, the keeper reaped.
    fn spawn(self: Box<Self>, command: Command) -> io::Result<ReapedChild> {
        let Pipes {
            status,
            control,
            keeper_ends,
        } = *self;
        let (mut keeper, records, first) = start(command, keeper_ends, status)?;
        if let Some(Record::Spawned(main)) = first {
            let kept = Kept::new(main, records, control);
            return Ok(ReapedChild::holding(keeper, Box::new(kept)));
        }
        settle(&mut keeper);
        Err(match first {
            Some(Record::Failed(errno)) => io::Error::from_raw_os_error(errno),
            first => io::Error::other(format!(
                "the session keeper did not start the session: {}",
                why_unheld(&first)
            )),
        })
    }
}

/// A kept tree, held through its keeper: the main's pid and start, the keeper's records and its
/// control pipe, and what the records have said.
struct Kept {
    main: u32,
    identity: Option<Identity>,
    records: Records,
    control: Option<PipeWriter>,
    status: Option<ExitStatus>,
    report: EndReport,
    ended: bool,
    finished: bool,
}

impl Kept {
    /// The tree whose keeper spawned `main`, its later records on `records`, its control pipe
    /// `control`.
    fn new(main: u32, records: Records, control: PipeWriter) -> Kept {
        Kept {
            main,
            identity: os_process::start_time(main).map(|start| Identity { pid: main, start }),
            records,
            control: Some(control),
            status: None,
            report: EndReport::default(),
            ended: false,
            finished: false,
        }
    }

    /// Takes in one line the keeper wrote after its spawn record: the main's status, a leftover of
    /// its end, or its end. Any other line is said and ignored.
    fn take(&mut self, line: io::Result<String>) {
        match line.as_deref().ok().and_then(Record::decode) {
            Some(Record::Exited(raw)) => self.status = Some(ExitStatus::from_raw(raw)),
            Some(Record::Residue(left)) => self.report.residue.push(left),
            Some(Record::Skipped(left)) => self.report.skipped.push(left),
            Some(Record::Ended) => self.ended = true,
            _ => whole_line::say(format_args!(
                "rigger: the keeper of session {} wrote {line:?}, which is no record it writes \
                 after its spawn; it is ignored",
                self.main
            )),
        }
    }

    /// Adds `other` to what the end reports.
    fn merge(&mut self, other: EndReport) {
        self.report.residue.extend(other.residue);
        self.report.skipped.extend(other.skipped);
        self.report.faults.extend(other.faults);
    }

    /// The end of a tree whose keeper closed its records before it wrote that it ended the tree.
    /// A keeper that has exited leaves the main, and what runs under it, reported skipped and
    /// never signalled: what the keeper held is no longer this process's to signal, and the main
    /// is not its child. A keeper still running is ended with what it holds.
    fn lost(&mut self, keeper: &mut Child) {
        match settle(keeper) {
            None => {
                whole_line::say(format_args!(
                    "rigger: the keeper of session {} exited before it ended its tree; what still \
                     runs under the session's main is reported, never signalled",
                    self.main
                ));
                let left = self.identity.as_ref().map(reap::left_running);
                self.report.skipped.extend(left.unwrap_or_default());
            }
            Some(report) => {
                whole_line::say(format_args!(
                    "rigger: the keeper of session {} closed its records and did not exit; it is \
                     ended with what it holds",
                    self.main
                ));
                self.merge(report);
            }
        }
    }
}

/// The error a read of the main's status answers once its keeper is gone without writing it.
fn lost_status() -> io::Error {
    io::Error::other("the session keeper exited without the session's status")
}

impl Hold for Kept {
    fn id(&self, _keeper: &Child) -> u32 {
        self.main
    }

    fn try_wait(&mut self, _keeper: &mut Child) -> io::Result<Option<ExitStatus>> {
        while self.status.is_none() {
            match self.records.try_recv() {
                Ok(line) => self.take(line),
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => return Err(lost_status()),
            }
        }
        Ok(self.status)
    }

    fn wait(&mut self, _keeper: &mut Child) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.status {
                return Ok(status);
            }
            let line = self.records.recv().map_err(|_| lost_status())?;
            self.take(line);
        }
    }

    /// Closes the control pipe, asking the keeper to end the tree, and reads its records until it
    /// has ended it, within the keeper's bound and [`SLACK`] past it, then reaps the keeper. A
    /// keeper that does not end the tree within that is ended with what it holds
    /// ([`crate::reap::end_watched`]), and one gone without ending it is reported
    /// ([`Kept::lost`]). An end after the first ends nothing more.
    fn end(&mut self, keeper: &mut Child) -> EndReport {
        if std::mem::replace(&mut self.finished, true) {
            return EndReport::default();
        }
        self.control = None;
        let deadline = Instant::now() + END_BOUND + SLACK;
        while !self.ended {
            match self
                .records
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => self.take(line),
                Err(RecvTimeoutError::Disconnected) => {
                    self.lost(keeper);
                    return std::mem::take(&mut self.report);
                }
                Err(RecvTimeoutError::Timeout) => {
                    whole_line::say(format_args!(
                        "rigger: the keeper of session {} did not end its tree within {:?}; it is \
                         ended with what it holds",
                        self.main,
                        END_BOUND + SLACK
                    ));
                    let report = reap::end_watched(keeper, &[]);
                    self.merge(report);
                    return std::mem::take(&mut self.report);
                }
            }
        }
        if let Some(report) = settle(keeper) {
            whole_line::say(format_args!(
                "rigger: the keeper of session {} ended its tree and did not exit; it is ended",
                self.main
            ));
            self.merge(report);
        }
        std::mem::take(&mut self.report)
    }

    /// Writes the release on the control pipe and closes it, so the keeper exits once the main
    /// has, ending nothing, and reaps the keeper when the main's status is already read. A keeper
    /// released before the main's exit is left to exit on its own.
    fn release(&mut self, keeper: &mut Child) -> EndReport {
        while let Ok(line) = self.records.try_recv() {
            self.take(line);
        }
        if let Some(mut control) = self.control.take() {
            let _ = control.write_all(b"r");
        }
        if self.status.is_some() && !exits_within(keeper, SLACK) {
            whole_line::say(format_args!(
                "rigger: the keeper of session {} did not exit at its release; it is left running",
                self.main
            ));
        }
        EndReport::default()
    }
}

/// The containment a tree is held in here, given the keeper `keeper`: the keeper once it answers
/// its probe that it can hold a tree, and a watch naming the failed probe otherwise.
pub(super) fn detect(keeper: &Path) -> Containment {
    match probe_keeper(keeper) {
        Ok(Some(Record::Probed)) => Containment::Keeper(keeper.to_path_buf()),
        Ok(first) => Containment::Watch(Why::ProbeFailed(why_unheld(&first))),
        Err(error) => Containment::Watch(Why::ProbeFailed(format!("it did not start: {error}"))),
    }
}

/// Runs `keeper` as [`NAME`] with [`PROBE`] and a status pipe, its standard streams null, and
/// answers its first record once it has exited.
fn probe_keeper(keeper: &Path) -> io::Result<Option<Record>> {
    let (status, status_end) = io::pipe()?;
    let ends = vec![File::from(OwnedFd::from(status_end))];
    let mut command = as_keeper(keeper, &[PROBE], &ends);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let (mut prober, _records, first) = start(command, ends, status)?;
    settle(&mut prober);
    Ok(first)
}

/// One line the keeper writes on its status pipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Record {
    /// The keeper spawned the tree's main, whose pid this is.
    Spawned(u32),
    /// The keeper could not spawn the main, for this OS error number.
    Failed(i32),
    /// The keeper cannot hold a tree on this system, for this OS error number.
    Unavailable(i32),
    /// The keeper's probe holds a tree on this system.
    Probed,
    /// The main exited, with this raw wait status.
    Exited(i32),
    /// A process the keeper's end signalled is still running past its bound.
    Residue(Leftover),
    /// A process the keeper's end found and never signalled.
    Skipped(Leftover),
    /// The keeper's end left no process of the tree running but those its residue and skipped
    /// records named.
    Ended,
}

impl Record {
    /// The record as one line, without its line end: its tag, then its fields, each after one
    /// space. A leftover's program is its last field, everything after its pid's space, with each
    /// line end in it written as a space.
    pub(super) fn encode(&self) -> String {
        match self {
            Record::Spawned(pid) => format!("spawned {pid}"),
            Record::Failed(errno) => format!("failed {errno}"),
            Record::Unavailable(errno) => format!("unavailable {errno}"),
            Record::Probed => "ok".to_string(),
            Record::Exited(raw) => format!("exited {raw}"),
            Record::Residue(left) => format!("residue {}", Record::fields(left)),
            Record::Skipped(left) => format!("skipped {}", Record::fields(left)),
            Record::Ended => "ended".to_string(),
        }
    }

    /// The record `line` encodes, without its line end; `None` for a line that is no record.
    pub(super) fn decode(line: &str) -> Option<Record> {
        let (tag, rest) = match line.split_once(' ') {
            Some((tag, rest)) => (tag, Some(rest)),
            None => (line, None),
        };
        match (tag, rest) {
            ("ok", None) => Some(Record::Probed),
            ("ended", None) => Some(Record::Ended),
            ("spawned", Some(pid)) => digits(pid).map(Record::Spawned),
            ("failed", Some(errno)) => errno.parse().ok().map(Record::Failed),
            ("unavailable", Some(errno)) => errno.parse().ok().map(Record::Unavailable),
            ("exited", Some(raw)) => raw.parse().ok().map(Record::Exited),
            ("residue", Some(rest)) => Record::leftover(rest).map(Record::Residue),
            ("skipped", Some(rest)) => Record::leftover(rest).map(Record::Skipped),
            _ => None,
        }
    }

    /// A leftover's fields: its pid, a space, and its program on one line.
    fn fields(left: &Leftover) -> String {
        format!("{} {}", left.pid, left.program.replace(['\n', '\r'], " "))
    }

    /// The leftover `fields` names ([`Record::fields`]); `None` unless they are a pid, a space
    /// and a program.
    fn leftover(fields: &str) -> Option<Leftover> {
        let (pid, program) = fields.split_once(' ')?;
        Some(Leftover {
            pid: digits(pid)?,
            program: program.to_string(),
            stopped: false,
        })
    }
}

/// The number `field` names in decimal digits alone, no sign.
fn digits(field: &str) -> Option<u32> {
    if field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    field.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `record` decodes from its own line.
    fn round_trips(record: Record) {
        assert_eq!(Record::decode(&record.encode()), Some(record));
    }

    /// A leftover named `program`, as the keeper reports one.
    fn leftover(pid: u32, program: &str) -> Leftover {
        Leftover {
            pid,
            program: program.to_string(),
            stopped: false,
        }
    }

    crate::test_cases! {
        a_spawn_record_round_trips: round_trips(Record::Spawned(4242));
        a_spawn_failure_round_trips: round_trips(Record::Failed(2));
        an_unavailable_record_round_trips: round_trips(Record::Unavailable(1));
        a_probe_answer_round_trips: round_trips(Record::Probed);
        an_exit_status_round_trips: round_trips(Record::Exited(15));
        a_residue_record_round_trips: round_trips(Record::Residue(leftover(7, "sh -c sleep 30")));
        a_skipped_record_round_trips: round_trips(Record::Skipped(leftover(9, " sh  -c x ")));
        a_leftover_with_no_program_round_trips: round_trips(Record::Residue(leftover(11, "")));
        an_end_record_round_trips: round_trips(Record::Ended);
        a_signalled_by_pid_record_round_trips: round_trips(Record::ByPid(
            "opening one fails here: Too many open files (os error 24)".to_string()
        ));
    }

    #[test]
    fn a_reason_with_line_ends_is_written_on_one_line() {
        let line = Record::ByPid("fails\nhere\r".to_string()).encode();

        assert!(!line.contains(['\n', '\r']), "one line: {line:?}");
        assert_eq!(
            Record::decode(&line),
            Some(Record::ByPid("fails here ".to_string()))
        );
    }

    /// A kept tree whose keeper wrote `lines` after its spawn record, and `merged` added.
    fn kept_report(lines: &[String], merged: EndReport) -> EndReport {
        let (_keeper, records) = std::sync::mpsc::channel();
        let (_read, control) = std::io::pipe().expect("a control pipe");
        let mut kept = Kept::new(std::process::id(), records, control);
        for line in lines {
            kept.take(Ok(line.clone()));
        }
        kept.merge(merged);
        kept.report
    }

    /// Why the end that a kept tree's keeper relayed, or one merged into it, signalled by pid.
    fn by_pid_because(lines: &[String], merged: Option<&str>) -> Option<String> {
        let merged = EndReport {
            by_pid_because: merged.map(str::to_string),
            ..EndReport::default()
        };
        kept_report(lines, merged).by_pid_because
    }

    crate::test_cases! {
        a_keeper_that_signalled_by_pid_says_why_in_the_end_report: assert_eq!(
            by_pid_because(
                &[Record::ByPid("why".to_string()).encode(), Record::Ended.encode()],
                None
            )
            .as_deref(),
            Some("why")
        );
        an_end_merged_in_that_signalled_by_pid_says_why_in_the_end_report: assert_eq!(
            by_pid_because(&[Record::Ended.encode()], Some("why")).as_deref(),
            Some("why")
        );
        a_keeper_that_signalled_through_descriptors_says_nothing: assert_eq!(
            by_pid_because(&[Record::Ended.encode()], None),
            None
        );
    }

    #[test]
    fn a_program_with_line_ends_is_written_on_one_line() {
        let line = Record::Residue(leftover(7, "sh -c 'a\nb\rc'")).encode();

        assert!(!line.contains(['\n', '\r']), "one line: {line:?}");
        assert_eq!(
            Record::decode(&line),
            Some(Record::Residue(leftover(7, "sh -c 'a b c'")))
        );
    }

    /// `line` decodes to no record.
    fn refused(line: &str) {
        assert_eq!(Record::decode(line), None, "{line:?}");
    }

    crate::test_cases! {
        an_empty_line_is_no_record: refused("");
        an_unknown_tag_is_no_record: refused("launched 12");
        a_spawn_record_with_no_pid_is_no_record: refused("spawned");
        a_spawn_record_with_a_negative_pid_is_no_record: refused("spawned -1");
        a_spawn_record_with_a_word_for_its_pid_is_no_record: refused("spawned twelve");
        a_spawn_record_with_two_pids_is_no_record: refused("spawned 12 13");
        an_exit_record_with_no_status_is_no_record: refused("exited");
        a_probe_answer_with_more_is_no_record: refused("ok now");
        an_end_record_with_more_is_no_record: refused("ended 1");
        a_residue_record_with_no_pid_is_no_record: refused("residue");
        a_residue_record_with_a_word_for_its_pid_is_no_record: refused("residue sh -c x");
        a_tag_in_capitals_is_no_record: refused("SPAWNED 12");
    }
}
