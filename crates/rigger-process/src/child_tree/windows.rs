//! The Windows containment: a job object this process creates and alone holds, which the tree's
//! main joins before it runs. Every process a job member starts is in the job and in every job
//! the job sits in (MS, `AssignProcessToJobObject`), so every process the tree starts is in it,
//! and none can leave it by asking: the job allows no breakaway, and a breakaway request under a
//! job that allows none fails the start it asks for. The job ends every process in it when its
//! last handle closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), so a driver that dies leaves nothing
//! of the tree running. The main is created suspended, joins the job, and is then resumed, so it
//! starts nothing before it is in the job. The handle's end ends the job
//! ([`crate::reap::end_job`]); its release clears the limit, then closes the handle, ending
//! nothing, or says in its report that the limit could not be cleared.
//!
//! A job that cannot be created, or a main that cannot join one, leaves that launch watched, and
//! the handle says why ([`ReapedChild::degraded`]): the main is still suspended, so its watcher
//! starts before it runs.

use super::{Contain, Containment, Hold, ReapedChild, Why};
use crate::reap::{self, EndReport, Identity};
use crate::subprocess;
use std::ffi::OsStr;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::ptr::null;

use windows_sys::Win32::Foundation::{FALSE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    OpenThread, ResumeThread, CREATE_SUSPENDED, THREAD_SUSPEND_RESUME,
};

/// The containment a tree is held in here: a job, once one can be created and limited, and a
/// watch naming why otherwise.
pub(super) fn detect(_keeper: &Path) -> Containment {
    match Job::new() {
        Ok(_) => Containment::Job,
        Err(error) => Containment::Watch(Why::JobFailed(error.to_string())),
    }
}

/// A command for `program` held as `how`: a job's command creates its main suspended. An error
/// for any other containment, which this OS holds no tree as.
pub(super) fn held(how: &Containment, program: &OsStr) -> io::Result<(Command, Box<dyn Contain>)> {
    if *how != Containment::Job {
        return Err(super::unavailable(how));
    }
    let mut command = subprocess::command(program);
    command.creation_flags(CREATE_SUSPENDED);
    Ok((command, Box::new(JobStart)))
}

/// No keeper runs here, so this process serves none.
pub(super) fn serve_if_asked() {}

/// A jobbed tree's start.
struct JobStart;

impl Contain for JobStart {
    /// Creates the main suspended, puts it in a new job, then resumes it. A job that cannot be
    /// created or joined leaves the main watched, the watcher started before the main runs. A
    /// main that cannot be resumed is ended, with its job, through the handle's drop, and its
    /// start answers the error.
    fn spawn(self: Box<Self>, mut command: Command) -> io::Result<ReapedChild> {
        let mut tree = ReapedChild::new(command.spawn()?);
        let joined = match Job::new() {
            Ok(job) => job
                .assign(&tree.child)
                .map(|()| job)
                .map_err(|error| format!("the session's main could not join its job: {error}")),
            Err(error) => Err(format!("no job could be created: {error}")),
        };
        match joined {
            Ok(job) => tree.hold = Box::new(Jobbed(Some(job))),
            Err(why) => {
                tree.watch()?;
                tree.watched_because(&Why::JobFailed(why));
            }
        }
        resume(tree.child.id())?;
        Ok(tree)
    }
}

/// A job object this process created and alone holds: it is created with no security attributes,
/// so no child inherits its handle.
struct Job(OwnedHandle);

impl Job {
    /// A new, unnamed job that ends every process in it when its handle closes.
    fn new() -> io::Result<Job> {
        // SAFETY: both pointers are null (no attributes, no name), as the call allows, and it
        // answers a null handle on failure.
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the handle is a valid one the call just answered, owned by nothing else.
        let job = Job(unsafe { OwnedHandle::from_raw_handle(handle) });
        job.limit(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)?;
        Ok(job)
    }

    /// Sets the job's limit flags to `flags`, every other limit unset.
    fn limit(&self, flags: JOB_OBJECT_LIMIT) -> io::Result<()> {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = flags;
        // SAFETY: the handle is the job this value holds, and `limits` is the readable structure
        // the information class names, of the size given.
        let set = unsafe {
            SetInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } != FALSE;
        if set {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Puts `main`, created suspended, in the job.
    fn assign(&self, main: &Child) -> io::Result<()> {
        // SAFETY: both handles are open: the job this value holds, and the main's, which its
        // `Child` holds.
        let assigned =
            unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), main.as_raw_handle()) }
                != FALSE;
        if assigned {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Lets the job go: clears its limit, so closing its handle ends nothing, then closes it. A
    /// limit that cannot be cleared is the report's fault, since the close then ends what still
    /// runs in the job.
    fn release(self, main: u32) -> EndReport {
        let mut report = EndReport::default();
        if let Err(error) = self.limit(0) {
            report.faults.push(format!(
                "the job of session {main} could not be let go ({error}); closing it ends what \
                 still runs in it"
            ));
        }
        report
    }
}

/// Resumes each thread of the suspended process `pid`, found in a ToolHelp thread snapshot: no
/// stable std call answers a spawned process's thread. A process with no thread found, or one
/// that cannot be resumed or was not suspended ([`resume_thread`]), answers an error.
fn resume(pid: u32) -> io::Result<()> {
    // SAFETY: the call takes no pointer and answers `INVALID_HANDLE_VALUE` on failure.
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the handle is a valid one the call just answered, owned by nothing else.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut resumed = 0;
    // SAFETY: `entry` is a writable `THREADENTRY32` whose `dwSize` names its size, as the call
    // requires.
    let mut more = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != FALSE;
    while more {
        if entry.th32OwnerProcessID == pid {
            resume_thread(entry.th32ThreadID)?;
            resumed += 1;
        }
        // SAFETY: as for `Thread32First`.
        more = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } != FALSE;
    }
    if resumed == 0 {
        return Err(io::Error::other(format!(
            "no thread of the session's main {pid} was found to resume"
        )));
    }
    Ok(())
}

/// Resumes the thread `id` of the session's main. A thread that was not suspended (a previous
/// suspend count of 0) answers an error: the main then ran before it was held, as when a caller's
/// own creation flags replaced [`CREATE_SUSPENDED`].
fn resume_thread(id: u32) -> io::Result<()> {
    // SAFETY: the call takes no pointer and answers a null handle on failure.
    let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, FALSE, id) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the handle is a valid one the call just answered, owned by nothing else.
    let thread = unsafe { OwnedHandle::from_raw_handle(handle) };
    // SAFETY: the handle is open with the suspend-resume right the call needs.
    match unsafe { ResumeThread(thread.as_raw_handle()) } {
        u32::MAX => Err(io::Error::last_os_error()),
        0 => Err(io::Error::other(format!(
            "thread {id} of the session's main was not held suspended, so the main ran before it \
             was held"
        ))),
        _ => Ok(()),
    }
}

/// A jobbed tree: the handle's process is the main, and the job, until the end or the release
/// takes it, holds every process the tree starts.
struct Jobbed(Option<Job>);

impl Hold for Jobbed {
    /// The job's running processes but the main ([`crate::reap::job_members`]); none once the
    /// end or the release has taken the job.
    fn members(&mut self, main: &mut Child) -> Vec<Identity> {
        let Some(job) = &self.0 else {
            return Vec::new();
        };
        let main = main.id();
        reap::job_members(job.0.as_raw_handle())
            .0
            .into_iter()
            .map(|member| member.identity)
            .filter(|member| member.pid != main)
            .collect()
    }

    /// Ends the job and every process in it ([`crate::reap::end_job`]); an end after the first
    /// ends nothing.
    fn end(&mut self, main: &mut Child) -> EndReport {
        self.0
            .take()
            .map(|job| reap::end_job(job.0, main))
            .unwrap_or_default()
    }

    /// Lets the job go ([`Job::release`]) and reaps the main if it has exited.
    fn release(&mut self, main: &mut Child) -> EndReport {
        let report = self
            .0
            .take()
            .map(|job| job.release(main.id()))
            .unwrap_or_default();
        let _ = main.try_wait();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::super::{line_channel, TreeCommand};
    use super::*;
    use std::process::Stdio;
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use std::time::{Duration, Instant};

    /// How long a row waits for a line, or for the stream's end.
    const BOUND: Duration = Duration::from_secs(10);

    /// `cmd /c <script>`, started as a jobbed tree, and the lines of its output, a pipe every
    /// process the script starts inherits.
    fn jobbed(script: &str) -> (ReapedChild, Receiver<io::Result<String>>) {
        let mut command = TreeCommand::new("cmd", &Containment::Job).expect("a jobbed command");
        command
            .command_mut()
            .raw_arg(format!("/c {script}"))
            .stdout(Stdio::piped());
        let mut tree = command.spawn().expect("spawn the jobbed tree");
        assert_eq!(tree.degraded(), None, "the tree is held in a job");
        let (lines, _reader) = line_channel(tree.take_stdout().expect("the output pipe"));
        (tree, lines)
    }

    /// Reads `lines` until one contains `text`; whether one did within [`BOUND`].
    fn read_until(lines: &Receiver<io::Result<String>>, text: &str) -> bool {
        let deadline = Instant::now() + BOUND;
        while let Ok(Ok(line)) =
            lines.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            if line.contains(text) {
                return true;
            }
        }
        false
    }

    /// Whether every process holding the pipe behind `lines` is gone within [`BOUND`]: the stream
    /// ends only once none holds it.
    fn ends(lines: &Receiver<io::Result<String>>) -> bool {
        let deadline = Instant::now() + BOUND;
        loop {
            match lines.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
    }

    /// `cmd /c <script>` created suspended, put in `job` and resumed, and the lines of its output,
    /// a pipe every process the script starts inherits.
    fn in_job(job: &Job, script: &str) -> (Child, Receiver<io::Result<String>>) {
        let mut main = Command::new("cmd")
            .raw_arg(format!("/c {script}"))
            .creation_flags(CREATE_SUSPENDED)
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn the main");
        job.assign(&main).expect("the main joins the job");
        resume(main.id()).expect("the main is resumed");
        let (lines, _reader) = line_channel(main.stdout.take().expect("the output pipe"));
        (main, lines)
    }

    /// A second handle to `job` that may only query it, so the OS refuses any call on it that
    /// needs another right.
    fn query_only(job: &Job) -> OwnedHandle {
        use windows_sys::Win32::Foundation::{DuplicateHandle, HANDLE};
        use windows_sys::Win32::System::SystemServices::JOB_OBJECT_QUERY;
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: both process handles are this process's own pseudo handle, the source is the
        // open job `job` holds, and `handle` is a writable handle the call fills.
        let duplicated = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                job.0.as_raw_handle(),
                GetCurrentProcess(),
                &mut handle,
                JOB_OBJECT_QUERY,
                FALSE,
                0,
            )
        } != FALSE;
        assert!(
            duplicated,
            "a query-only handle: {}",
            io::Error::last_os_error()
        );
        // SAFETY: the handle is the one the call just answered, owned by nothing else.
        unsafe { OwnedHandle::from_raw_handle(handle) }
    }

    /// Closing a job's only handle ends every process in it, a child of a child of its main among
    /// them: the inner shell starts the background ping and exits, so the ping's parent is gone
    /// before the close.
    #[test]
    fn closing_a_job_ends_a_child_of_a_child_of_its_main() {
        let job = Job::new().expect("a job");
        let (mut main, lines) = in_job(
            &job,
            "cmd /c start /b ping -n 60 127.0.0.1 & echo up& ping -n 60 127.0.0.1 >NUL",
        );
        assert!(read_until(&lines, "127.0.0.1"), "the background ping is up");

        drop(job);

        assert!(ends(&lines), "nothing the main started holds its output");
        let _ = main.wait();
    }

    /// A release through a handle the OS will not clear the job's limit through answers the
    /// refusal and that closing the handle ends what still runs in the job; the job's own handle
    /// still holds the main until it closes.
    #[test]
    fn a_job_the_os_will_not_let_go_says_so_in_the_report() {
        let job = Job::new().expect("a job");
        let (mut main, lines) = in_job(&job, "echo up& ping -n 60 127.0.0.1 >NUL");
        assert!(read_until(&lines, "up"), "the main is up");

        let report = Job(query_only(&job)).release(main.id());

        assert_eq!((report.residue, report.skipped), (vec![], vec![]));
        assert!(
            matches!(&report.faults[..], [fault] if fault.contains("could not be let go")),
            "the report says the job could not be let go: {:?}",
            report.faults
        );
        drop(job);
        assert!(ends(&lines), "closing the job's own handle ends the main");
        let _ = main.wait();
    }

    /// An end through a handle the OS will not end the job through reports the refusal and
    /// returns without waiting the bound, naming no process as left running; closing the job's
    /// own handle then ends what it holds.
    #[test]
    fn an_end_the_os_refuses_says_why_without_waiting_for_the_job() {
        let job = Job::new().expect("a job");
        let (mut main, lines) = in_job(&job, "echo up& ping -n 60 127.0.0.1 >NUL");
        assert!(read_until(&lines, "up"), "the main is up");
        let started = Instant::now();

        let report = reap::end_job(query_only(&job), &mut main);

        assert!(
            started.elapsed() < reap::END_BOUND,
            "the end waits for nothing"
        );
        assert_eq!((report.residue, report.skipped), (vec![], vec![]));
        assert!(
            matches!(&report.faults[..], [fault] if fault.contains("could not be ended")),
            "the report says the job could not be ended: {:?}",
            report.faults
        );
        drop(job);
        assert!(ends(&lines), "closing the job's own handle ends the main");
        let _ = main.wait();
    }

    /// An end returns only once each process its job held has exited, so none the job listed
    /// before the end still runs after it: the background ping holds the output and the main's
    /// own ping keeps the main running.
    #[test]
    fn a_job_end_returns_once_each_process_it_held_has_exited() {
        let job = Job::new().expect("a job");
        let (mut main, lines) = in_job(
            &job,
            "start /b ping -n 60 127.0.0.1 & echo up& ping -n 60 127.0.0.1 >NUL",
        );
        assert!(read_until(&lines, "127.0.0.1"), "the background ping is up");
        let (held, _) = reap::job_members(job.0.as_raw_handle());
        let started = Instant::now();

        let report = reap::end_job(job.0, &mut main);

        let elapsed = started.elapsed();
        let running: Vec<u32> = held
            .iter()
            .filter(|member| member.running())
            .map(|member| member.identity.pid)
            .collect();
        assert!(
            held.len() >= 2,
            "the job lists the main and the background ping: {} listed",
            held.len()
        );
        assert!(
            elapsed < reap::END_BOUND,
            "the end returns within its bound: {elapsed:?}"
        );
        assert_eq!((running, report), (vec![], EndReport::default()));
    }

    /// An end whose bound passes before the job's processes have exited closes the job, which ends
    /// them, and says so: none is named as left running, each fault names the close, and none the
    /// job held still runs once the end returns.
    #[test]
    fn an_end_past_its_bound_names_the_close_that_ended_what_still_ran() {
        let job = Job::new().expect("a job");
        let (mut main, lines) = in_job(
            &job,
            "start /b ping -n 60 127.0.0.1 & echo up& ping -n 60 127.0.0.1 >NUL",
        );
        assert!(read_until(&lines, "127.0.0.1"), "the background ping is up");
        let (held, _) = reap::job_members(job.0.as_raw_handle());

        let report = reap::end_job_with(job.0, &mut main, Duration::ZERO);

        let running: Vec<u32> = held
            .iter()
            .filter(|member| member.running())
            .map(|member| member.identity.pid)
            .collect();
        assert!(
            report
                .faults
                .iter()
                .all(|fault| fault.contains("closing the job ended them")),
            "each fault names the close: {:?}",
            report.faults
        );
        assert_eq!(
            (running, report.residue, report.skipped),
            (vec![], vec![], vec![])
        );
        assert!(ends(&lines), "nothing the job held holds the output");
    }

    /// A process is read as a job's member only while that job holds it: the job's main is read,
    /// and a process this test started outside the job is not.
    #[test]
    fn a_process_is_read_as_a_member_only_of_the_job_that_holds_it() {
        let job = Job::new().expect("a job");
        let (mut main, lines) = in_job(&job, "echo up& ping -n 60 127.0.0.1 >NUL");
        assert!(read_until(&lines, "up"), "the main is up");
        let mut outside = Command::new("cmd")
            .raw_arg("/c ping -n 60 127.0.0.1 >NUL")
            .spawn()
            .expect("spawn a process outside the job");

        let read = [main.id(), outside.id()].map(|pid| {
            reap::job_member(job.0.as_raw_handle(), pid).map(|member| member.identity.pid)
        });

        crate::test_support::cleanup(&mut outside);
        drop(job);
        let _ = main.wait();
        assert_eq!(read, [Some(main.id()), None]);
    }

    /// A jobbed command whose caller's own creation flags replace [`CREATE_SUSPENDED`] starts its
    /// main running before the job holds it, and its start is refused for it. The main runs a
    /// ping long enough that its thread is there to resume.
    #[test]
    fn a_main_started_running_before_its_job_holds_it_is_refused() {
        let mut command = TreeCommand::new("cmd", &Containment::Job).expect("a jobbed command");
        command
            .command_mut()
            .raw_arg("/c ping -n 30 127.0.0.1 >NUL")
            .creation_flags(0);

        let started = command.spawn().map(|_| ());

        let error = started.expect_err("a main that was not held suspended is refused");
        assert!(
            error.to_string().contains("was not held suspended"),
            "the refusal says why: {error}"
        );
    }

    /// A jobbed main is resumed once its job holds it: it writes, and exits on its own.
    #[test]
    fn a_jobbed_main_runs_once_its_job_holds_it() {
        let (mut tree, lines) = jobbed("echo up");
        assert!(read_until(&lines, "up"), "the main runs");
        let deadline = Instant::now() + BOUND;
        while matches!(tree.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            matches!(tree.try_wait(), Ok(Some(_))),
            "the main exits on its own"
        );
    }

    #[test]
    fn this_os_holds_a_tree_in_a_job() {
        assert_eq!(
            Containment::detect(Path::new("no keeper runs here")),
            Containment::Job
        );
    }

    // A ping names its address in each line it writes, in any language, so a line with the
    // address in it proves the background ping is up and holds the pipe.
    #[test]
    fn a_jobbed_end_ends_what_its_main_started() {
        let (mut tree, lines) =
            jobbed("start /b ping -n 60 127.0.0.1 & echo up& ping -n 60 127.0.0.1 >NUL");
        assert!(read_until(&lines, "127.0.0.1"), "the background ping is up");

        let report = tree.end();

        assert_eq!(report, EndReport::default());
        assert!(ends(&lines), "nothing the tree started holds its output");
    }

    // The writer names the spawn on its command line and writes its line two seconds after the
    // main has exited, once its ping ends: the line is read only if the finish waited for it.
    #[test]
    fn a_jobbed_finish_waits_for_a_writer_its_exited_main_left() {
        let (mut tree, lines) = jobbed(
            "start /b cmd /c \"ping -n 3 127.0.0.1 >NUL & echo written u-late-writer\" & echo up",
        );
        assert!(read_until(&lines, "up"), "the main is up");
        let deadline = Instant::now() + BOUND;
        while matches!(tree.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            matches!(tree.try_wait(), Ok(Some(_))),
            "the main exits on its own"
        );

        let report = tree.finish(Some("u-late-writer"));

        assert_eq!(report, EndReport::default());
        assert!(read_until(&lines, "written"), "the writer wrote its line");
        assert!(ends(&lines), "nothing the tree started holds its output");
    }

    // The ping replies once a second, so two replies read after the lines already sent are drained
    // were written after the release, by a ping the release left running.
    #[test]
    fn a_released_jobbed_tree_leaves_what_its_main_started_running() {
        let (mut tree, lines) = jobbed("start /b ping -n 8 127.0.0.1 & echo up");
        assert!(read_until(&lines, "up"), "the main is up");
        let deadline = Instant::now() + BOUND;
        while matches!(tree.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            matches!(tree.try_wait(), Ok(Some(_))),
            "the main exits on its own"
        );

        tree.release();

        while lines.try_recv().is_ok() {}
        assert!(
            read_until(&lines, "127.0.0.1") && read_until(&lines, "127.0.0.1"),
            "the background ping writes on after the release"
        );
        assert!(ends(&lines), "the background ping ends on its own");
    }
}
