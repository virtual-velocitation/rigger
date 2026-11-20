//! The process interface (spec 125, criterion 9): the one place rigger reads a fact about a
//! running process - which processes exist, a process's parent, start time, working directory,
//! command line, run state and open files, which process listens on a TCP port, which
//! processes hold a directory, and the path that execs this process's own image. Each fact is a
//! function here, answered by the native adapter of the OS the binary is built for; the reaper
//! ([`crate::reap`]), the holder check ([`crate::holders`]) and the dash read processes through
//! these functions alone. Read-only: nothing here signals a process.
//!
//! Every reader is best-effort: a process that is gone, or one this process may not read,
//! answers absent (`None`, an empty list, an empty command), never an error, and on an OS with
//! no native adapter every reader answers absent. The native adapters are Linux's (`/proc`),
//! which Android, running the same kernel, binds too, macOS's (libproc) and Windows's (the
//! Win32 process, IP Helper and Restart Manager calls);
//! Windows reports no other process's working directory or open files, so there those two readers
//! answer absent, and of a process's run state it reports only that the process has exited.
//!
//! The rules the macOS and Windows adapters apply to what their OS's calls answer - which parent a
//! record is trusted to name, how a table's rows, a socket's port and a start time read, whether a
//! socket listens, how files are batched for registration - are plain functions here, compiled on
//! every host so every lane's tests drive them.

use std::path::{Path, PathBuf};

// The one spelling of the OS set: each OS with a native adapter declares its module and binds it
// as `native`, and every other OS binds the inline `unsupported` module in the `_` arm.
std::cfg_select! {
    any(target_os = "linux", target_os = "android") => {
        pub(crate) mod linux;
        use linux as native;
    }
    target_os = "macos" => {
        mod macos;
        use macos as native;
    }
    target_os = "windows" => {
        pub(crate) mod windows;
        use windows as native;
    }
    _ => {
        /// The OSes with no native adapter, the `_` arm of the dispatch above: every process
        /// answers absent.
        mod unsupported {
            use std::path::PathBuf;

            pub(super) use super::reported_image as running_image;
            pub(super) use super::scan_holders as holders;

            pub(super) fn pids() -> Vec<u32> {
                Vec::new()
            }

            pub(super) fn table() -> Vec<super::Entry> {
                Vec::new()
            }

            pub(super) fn parent(_pid: u32) -> Option<u32> {
                None
            }

            pub(super) fn start_time(_pid: u32) -> Option<u64> {
                None
            }

            pub(super) fn cwd(_pid: u32) -> Option<PathBuf> {
                None
            }

            pub(super) fn command(_pid: u32) -> String {
                String::new()
            }

            pub(super) fn state(_pid: u32) -> Option<char> {
                None
            }

            pub(super) fn open_files(_pid: u32) -> Vec<PathBuf> {
                Vec::new()
            }

            pub(super) fn is_unexeced_fork_of(_pid: u32, _parent: u32) -> bool {
                false
            }

            pub(super) fn listening_pid(_port: u16) -> Option<u32> {
                None
            }

            #[cfg(test)]
            mod tests {
                use crate::os_process::*;

                /// On an OS with no native adapter, this process answers absent from every
                /// reader, a port it listens on names no listener, and no directory has a holder.
                #[test]
                fn every_reader_answers_absent_for_this_process() {
                    let me = std::process::id();
                    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                    let port = listener.local_addr().unwrap().port();
                    assert_eq!(
                        (
                            pids(),
                            table(),
                            parent(me),
                            start_time(me),
                            cwd(me),
                            command(me),
                            state(me),
                            open_files(me),
                            is_unexeced_fork_of(me, me),
                            listening_pid(port),
                            holders(std::path::Path::new("/")),
                        ),
                        (
                            Vec::new(),
                            Vec::new(),
                            None,
                            None,
                            None,
                            String::new(),
                            None,
                            Vec::new(),
                            false,
                            None,
                            Vec::new()
                        )
                    );
                }
            }
        }
        use unsupported as native;
    }
}

/// The pid of every process this process can see, each once.
pub fn pids() -> Vec<u32> {
    native::pids()
}

/// One process of the process table ([`table`]): its pid, the pid its record names as its parent
/// ([`parent`]) and its start time ([`start_time`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The process's pid.
    pub pid: u32,
    /// The pid the process's record names as its parent. On Windows a record keeps a parent's pid
    /// after that parent exits, and a later process can take it, so a walk down the table reads
    /// the parent's start against the child's ([`TIES`]) before it trusts that pid.
    pub parent: u32,
    /// The process's start time, the witness [`start_time`] reads.
    pub start: u64,
}

/// Every process this process can see, each once, with its parent and start time, read in one
/// listing: one `/proc` pass on Linux, one process list and one BSD-info read a process on macOS,
/// one process snapshot and each process's times on Windows. A process whose parent or start time
/// cannot be read (it exited during the listing, or this process may not read it) is left out.
pub fn table() -> Vec<Entry> {
    native::table()
}

/// The pid of process `pid`'s parent; `None` when `pid` is gone or its parent cannot be read.
pub fn parent(pid: u32) -> Option<u32> {
    native::parent(pid)
}

/// Process `pid`'s start time: an opaque value that is the same however often it is read for the
/// same process and differs for a process that later reuses its pid, the witness that a pid read
/// earlier still names the process it named then. `None` when `pid` is gone.
pub fn start_time(pid: u32) -> Option<u64> {
    native::start_time(pid)
}

/// Process `pid`'s working directory, absolute and resolved, named by its path even when the
/// directory has since been removed. `None` when `pid` is gone or its working directory cannot
/// be read.
pub fn cwd(pid: u32) -> Option<PathBuf> {
    native::cwd(pid)
}

/// Whether process `pid`'s working directory ([`cwd`]) is `dir` or lies inside it, matched on
/// path components, so a working directory `/a/bc` never lies in `/a/b`: the safety boundary
/// against a string-prefix match. `dir` is absolute and resolved, as [`cwd`] names a working
/// directory, and a working directory removed under a live process still lies in `dir` by its
/// path. False when `pid` is gone or its working directory cannot be read.
pub(crate) fn cwd_lies_in(pid: u32, dir: &Path) -> bool {
    cwd(pid).is_some_and(|cwd| cwd.starts_with(dir))
}

/// Process `pid`'s command: on Linux its arguments joined by single spaces, or its short name when
/// its arguments cannot be read; on macOS and Windows the full path of its executable.
/// Descriptive only, it names a process in a message and decides nothing. Empty when `pid` is
/// gone.
pub fn command(pid: u32) -> String {
    native::command(pid)
}

/// Process `pid`'s run state as one letter: `R` running, `S` sleeping, `T` stopped, `Z` a zombie
/// (exited, not yet reaped by its parent), or the OS's own letter for another state. `None` when
/// `pid` is gone or its state cannot be read. Windows reads only `Z`, for a process that has exited
/// while a handle still holds it, and `None` for a live one.
pub fn state(pid: u32) -> Option<char> {
    native::state(pid)
}

/// The files process `pid` holds open, one entry per open descriptor, so a file it holds open
/// twice is named twice. Empty when `pid` is gone or its descriptors cannot be read.
pub fn open_files(pid: u32) -> Vec<PathBuf> {
    native::open_files(pid)
}

/// Whether process `pid` is a child of `parent` that has forked but not yet exec'd: until its
/// exec completes it is still `parent`'s own image, holding copies of `parent`'s descriptors.
pub fn is_unexeced_fork_of(pid: u32, parent: u32) -> bool {
    native::is_unexeced_fork_of(pid, parent)
}

/// The pid of a process listening for TCP connections on `port` over IPv4; `None` when nothing
/// listens there or the listener's process cannot be read.
pub fn listening_pid(port: u16) -> Option<u32> {
    native::listening_pid(port)
}

/// Every process other than this one whose working directory or any open file lies in `dir`
/// (absolute and resolved) or inside it, as pids. This process never counts, and neither does a
/// child of it that has forked but not yet exec'd ([`is_unexeced_fork_of`]): until its exec
/// completes it is still this process's own image over copies of this process's descriptors, and
/// the close-on-exec ones (every descriptor the standard library opens) close at that exec;
/// whatever the child still holds once it has exec'd counts from then on. On Windows, which
/// reports no other process's working directory or open files, they are the processes the
/// Restart Manager reports holding a file in `dir` or inside it open. The check a reclaim runs
/// before it deletes a directory it counts as dead.
pub fn holders(dir: &Path) -> Vec<u32> {
    native::holders(dir)
}

/// The path that execs the image this process runs: on Linux and Android its `/proc` link, which
/// execs that image even once its file on disk is replaced; elsewhere the path the OS reports for
/// it ([`std::env::current_exe`]), empty when it reports none.
pub fn running_image() -> PathBuf {
    native::running_image()
}

/// [`running_image`] on every OS whose adapter holds no link to it: the path the OS reports.
#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn reported_image() -> PathBuf {
    std::env::current_exe().unwrap_or_default()
}

/// The composed holder scan [`holders`] answers on every OS that reports a process's working
/// directory and open files: each process but this one and its un-exec'd forks that [`holds`]
/// `dir`.
#[cfg(not(target_os = "windows"))]
fn scan_holders(dir: &Path) -> Vec<u32> {
    let me = std::process::id();
    pids()
        .into_iter()
        .filter(|&pid| pid != me && !is_unexeced_fork_of(pid, me) && holds(pid, dir))
        .collect()
}

/// Whether process `pid`'s working directory ([`cwd_lies_in`]) or one of its open files lies in
/// `dir` or inside it, matched on path components.
#[cfg(not(target_os = "windows"))]
fn holds(pid: u32, dir: &Path) -> bool {
    cwd_lies_in(pid, dir) || open_files(pid).iter().any(|file| file.starts_with(dir))
}

/// `path` without the `" (deleted)"` marker linux and android add to the path of a directory or
/// a file that was removed while a process still holds it - a working directory removed under a
/// process, or a running binary an upgrade replaced ([`crate::detached::standing_path`]). The
/// marker is stripped as text, never by touching the filesystem, since the path is already gone:
/// a lossy UTF-8 round-trip, and rigger's own scratch and worktree paths are always valid UTF-8.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn strip_deleted_suffix(path: &Path) -> PathBuf {
    const DELETED_SUFFIX: &str = " (deleted)";
    let text = path.to_string_lossy();
    match text.strip_suffix(DELETED_SUFFIX) {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
}

/// What the start times of a process and of the parent its record names say of that parent
/// ([`Ties::parentage`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Parentage {
    /// The parent started before the process, or in the same tick where a tie is trusted: it may
    /// be the process's parent.
    Parent,
    /// The parent started in the same tick as the process, where a tie is not trusted: the
    /// process's parent, or a later process given a dead parent's pid, which the times cannot
    /// tell apart.
    Tie,
    /// The parent started after the process: a later process given a dead parent's pid, never its
    /// parent.
    Later,
}

/// How a start-time tie between a process and the parent its record names is read: [`TIES`] is
/// this OS's rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ties {
    /// A tie is a parent: a child often starts in its parent's own clock tick.
    Trusted,
    /// A tie is no parent: a pid the OS gives again within the tick could have named the parent.
    Untrusted,
}

impl Ties {
    /// What a parent that started at `parent_started` is to a process that started at `started`.
    pub(crate) fn parentage(self, parent_started: u64, started: u64) -> Parentage {
        match parent_started.cmp(&started) {
            std::cmp::Ordering::Less => Parentage::Parent,
            std::cmp::Ordering::Equal if self == Ties::Trusted => Parentage::Parent,
            std::cmp::Ordering::Equal => Parentage::Tie,
            std::cmp::Ordering::Greater => Parentage::Later,
        }
    }
}

/// This OS's tie rule. Windows reads a tie as no parent: a process snapshot keeps a dead parent's
/// pid and Windows gives a freed pid out again quickly, so a process started in the child's own
/// creation-time tick may hold the pid the child's record names, and a real child started in its
/// parent's tick is left out of a walk and reported, never signalled. Linux and macOS read a tie
/// as a parent: a record's parent pid is rewritten when that parent exits, and pids are given out
/// in sequence, so a tie stands for a child started in its parent's own clock tick, which a
/// forking shell does often; a pid given out again within one tick of the child's start is
/// assumed not to happen, never proved.
pub(crate) const TIES: Ties = if cfg!(target_os = "windows") {
    Ties::Untrusted
} else {
    Ties::Trusted
};

/// Whether the parent a Windows process snapshot names for a process that started at `started`
/// is still its parent: that parent still opens (`parent_started` is its start time) and is a
/// parent by `ties` ([`Ties::parentage`]). A snapshot entry keeps its parent's pid after the
/// parent exits, and Windows may give that pid to a later process.
#[cfg(any(target_os = "windows", test))]
fn trusts_parent(ties: Ties, parent_started: Option<u64>, started: u64) -> bool {
    parent_started
        .is_some_and(|parent_started| ties.parentage(parent_started, started) == Parentage::Parent)
}

/// The rows a table that claims `claimed` rows of `row_words` words each holds in its `words`
/// words: the claim, capped at the whole rows present, so no row is read past the table's end.
#[cfg(any(target_os = "windows", test))]
fn whole_rows(claimed: u32, words: usize, row_words: usize) -> usize {
    (claimed as usize).min(words / row_words)
}

/// The port a socket record holds in network byte order in the low 16 bits of a wider field
/// (Windows's listener-table `dwLocalPort`, macOS's `insi_lport`), whose upper bits carry nothing.
#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn port_of(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}

/// The most files one Windows Restart Manager registration call is given.
#[cfg(any(target_os = "windows", test))]
const FILES_PER_REGISTRATION: usize = 256;

/// `files` in the Windows Restart Manager registration calls that register them, each given at
/// most [`FILES_PER_REGISTRATION`].
#[cfg(any(target_os = "windows", test))]
fn registrations<T>(files: &[T]) -> std::slice::Chunks<'_, T> {
    files.chunks(FILES_PER_REGISTRATION)
}

/// A macOS process's start time from its BSD info's `pbi_start_tvsec` (`secs`) and
/// `pbi_start_tvusec` (`micros`), in microseconds since the epoch.
#[cfg(any(target_os = "macos", test))]
fn micros_since_epoch(secs: u64, micros: u64) -> u64 {
    secs * 1_000_000 + micros
}

/// `TSI_S_LISTEN`: the `tcpsi_state` of a listening macOS TCP socket.
#[cfg(any(target_os = "macos", test))]
const TSI_S_LISTEN: std::ffi::c_int = 1;

/// `INI_IPV4`: the `insi_vflag` bit of a macOS IPv4 socket.
#[cfg(any(target_os = "macos", test))]
const INI_IPV4: u8 = 0x1;

/// Whether a macOS TCP socket whose `tcpsi_state` is `state`, whose `insi_vflag` is `vflag` and
/// whose `insi_lport` is `raw_port` listens over IPv4 on `port`.
#[cfg(any(target_os = "macos", test))]
fn listens_over_ipv4(
    state: std::ffi::c_int,
    vflag: u8,
    raw_port: std::ffi::c_int,
    port: u16,
) -> bool {
    state == TSI_S_LISTEN && vflag & INI_IPV4 != 0 && port_of(raw_port as u32) == port
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::{cleanup, sleeper_in, wait_until};
    #[cfg(unix)]
    use std::path::Path;

    /// The pid of a child that has exited and been reaped, so it names no process.
    #[cfg(unix)]
    fn gone_pid() -> u32 {
        let mut child = crate::subprocess::command("true").spawn().unwrap();
        child.wait().unwrap();
        child.id()
    }

    /// The running image is the image this process runs: it resolves to this test's executable.
    #[test]
    fn the_running_image_resolves_to_this_executable() {
        assert_eq!(
            running_image().canonicalize().unwrap(),
            std::env::current_exe().unwrap().canonicalize().unwrap()
        );
    }

    /// The process list names this process, and names it once.
    #[cfg(unix)]
    #[test]
    fn the_process_list_names_this_process_once() {
        let me = std::process::id();
        assert_eq!(pids().into_iter().filter(|&pid| pid == me).count(), 1);
    }

    /// The table names this process and a child it spawned, each once, with the parent and start
    /// time the one-process readers report, and names no process that is gone.
    #[cfg(unix)]
    #[test]
    fn the_table_names_each_process_once_with_its_parent_and_start_time() {
        let me = std::process::id();
        let mut child = sleeper_in(Path::new("/"));
        let gone = gone_pid();
        let listed = table();
        let rows_of = |pid: u32| -> Vec<Entry> {
            listed
                .iter()
                .filter(|row| row.pid == pid)
                .copied()
                .collect()
        };
        let read = (rows_of(me), rows_of(child.id()), rows_of(gone));
        let expected = (
            vec![Entry {
                pid: me,
                parent: std::os::unix::process::parent_id(),
                start: start_time(me).unwrap(),
            }],
            vec![Entry {
                pid: child.id(),
                parent: me,
                start: start_time(child.id()).unwrap(),
            }],
            Vec::new(),
        );
        cleanup(&mut child);
        assert_eq!(read, expected);
    }

    /// This process's parent is the parent the OS reports to it, and a child it spawned names it
    /// as its parent.
    #[cfg(unix)]
    #[test]
    fn a_processes_parent_is_the_process_that_spawned_it() {
        let mut child = sleeper_in(Path::new("/"));
        let child_parent = parent(child.id());
        cleanup(&mut child);
        assert_eq!(
            (parent(std::process::id()), child_parent),
            (
                Some(std::os::unix::process::parent_id()),
                Some(std::process::id())
            )
        );
    }

    /// This process's start time reads one value twice, and a child it starts later reads a
    /// later one.
    #[cfg(unix)]
    #[test]
    fn a_processes_start_time_is_one_value_and_a_later_process_starts_later() {
        let me = std::process::id();
        let first = start_time(me);
        let second = start_time(me);
        // Past one clock tick, so a child started now cannot share this process's start tick.
        std::thread::sleep(std::time::Duration::from_millis(30));
        let mut child = sleeper_in(Path::new("/"));
        let child_started = start_time(child.id());
        cleanup(&mut child);
        assert_eq!(
            second, first,
            "a start time is one value however often it is read"
        );
        assert!(
            first.is_some_and(|me| child_started.is_some_and(|child| child > me)),
            "a child started later starts later: {first:?} then {child_started:?}"
        );
    }

    /// This process's working directory is the one it reports, and a child's is the directory it
    /// was started in.
    #[cfg(unix)]
    #[test]
    fn a_processes_cwd_is_its_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let started_in = dir.path().canonicalize().unwrap();
        let mut child = sleeper_in(&started_in);
        let child_cwd = cwd(child.id());
        cleanup(&mut child);
        assert_eq!(
            (cwd(std::process::id()), child_cwd),
            (
                Some(std::env::current_dir().unwrap().canonicalize().unwrap()),
                Some(started_in)
            )
        );
    }

    /// A WORKING DIRECTORY LIES IN A DIRECTORY BY PATH COMPONENTS: a child started in `<tmp>/ab`
    /// lies in `<tmp>/ab` and in `<tmp>`, never in `<tmp>/a`, which its path extends only as
    /// text, nor in `<tmp>/ab/c`, inside it; a gone process lies in nothing, not even `/`.
    #[cfg(unix)]
    #[test]
    fn a_working_directory_lies_in_a_directory_by_path_components_never_by_text() {
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path().canonicalize().unwrap();
        let started_in = tmp.join("ab");
        std::fs::create_dir(&started_in).unwrap();
        let mut child = sleeper_in(&started_in);
        let gone = gone_pid();
        let rows = [
            ("its own directory", child.id(), started_in.clone()),
            ("its parent", child.id(), tmp.clone()),
            ("a text prefix", child.id(), tmp.join("a")),
            ("a directory inside it", child.id(), started_in.join("c")),
            ("a gone process", gone, PathBuf::from("/")),
        ];
        let lies: Vec<(&str, bool)> = rows
            .iter()
            .map(|(what, pid, dir)| (*what, cwd_lies_in(*pid, dir)))
            .collect();
        cleanup(&mut child);
        assert_eq!(
            lies,
            [
                ("its own directory", true),
                ("its parent", true),
                ("a text prefix", false),
                ("a directory inside it", false),
                ("a gone process", false),
            ]
        );
    }

    /// On Linux a process's command is its arguments joined by single spaces.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_processes_command_is_its_arguments_joined_by_spaces() {
        let mut child = sleeper_in(Path::new("/"));
        // `spawn` returns while the child's exec may still be laying out its arguments, when the
        // OS reports none yet, so the child's command is read once its exec has finished.
        wait_until(|| command(child.id()) == "sleep 300");
        let child_command = command(child.id());
        cleanup(&mut child);
        assert_eq!(
            (command(std::process::id()), child_command),
            (
                std::env::args().collect::<Vec<_>>().join(" "),
                "sleep 300".to_string()
            )
        );
    }

    /// On macOS a process's command is the full path of its executable.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_processes_command_is_its_executables_path() {
        let mut child = sleeper_in(Path::new("/"));
        // `spawn` returns while the child's exec may still be under way, when the OS reports this
        // test's own executable for it, so the child's command is read once its exec has finished.
        wait_until(|| command(child.id()) == "/bin/sleep");
        let child_command = command(child.id());
        cleanup(&mut child);
        assert_eq!(
            (command(std::process::id()), child_command),
            (
                std::env::current_exe()
                    .unwrap()
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                "/bin/sleep".to_string()
            )
        );
    }

    /// This process, which is running a test, is running or sleeping, and a child that exited
    /// but is not yet reaped is a zombie. On Linux a process's state is its main thread's, here the
    /// test harness's, which this test does not drive: under load the kernel reports it in
    /// uninterruptible sleep (`D`) for the instants it waits on the process's address-space lock,
    /// so the state is read until it runs or sleeps.
    #[cfg(unix)]
    #[test]
    fn a_live_process_runs_or_sleeps_and_an_unreaped_exit_is_a_zombie() {
        let mut child = crate::subprocess::command("true").spawn().unwrap();
        let zombie = wait_until(|| state(child.id()) == Some('Z'));
        child.wait().unwrap();
        let mut mine = None;
        let runs_or_sleeps = wait_until(|| {
            mine = state(std::process::id());
            matches!(mine, Some('R' | 'S'))
        });
        assert!(runs_or_sleeps, "this process runs or sleeps: {mine:?}");
        assert!(zombie, "an exited child not yet reaped is a zombie");
    }

    /// This process's open files name a file it holds open once it opened it, once, and no
    /// longer once it closed it.
    #[cfg(unix)]
    #[test]
    fn a_processes_open_files_name_a_file_while_it_holds_it_open() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().canonicalize().unwrap().join("held");
        std::fs::write(&file, b"x").unwrap();
        let naming = |file: &Path| {
            open_files(std::process::id())
                .iter()
                .filter(|named| *named == file)
                .count()
        };
        let held = std::fs::File::open(&file).unwrap();
        let while_held = naming(&file);
        drop(held);
        assert_eq!((while_held, naming(&file)), (1, 0));
    }

    /// The process listening on a port is this process while it holds the listener, and nothing
    /// listens there once the listener is closed, though a connection it accepted, whose own end
    /// is bound to that same port, stays open.
    #[cfg(unix)]
    #[test]
    fn the_listener_on_a_port_is_the_process_holding_it() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (accepted, _) = listener.accept().unwrap();
        let listening = listening_pid(port);
        drop(listener);
        assert_eq!(listening, Some(std::process::id()));
        // A sibling test's fork that has not exec'd yet holds a copy of the listener until its
        // exec closes it, so the port is given that moment to close.
        let closed = wait_until(|| listening_pid(port).is_none());
        drop((client, accepted));
        assert!(
            closed,
            "nothing listens on a closed port, though a connection it accepted is open"
        );
    }

    /// A process that is gone answers absent from every reader.
    #[cfg(unix)]
    #[test]
    fn a_gone_process_answers_absent_from_every_reader() {
        let gone = gone_pid();
        assert_eq!(
            (
                pids().contains(&gone),
                parent(gone),
                start_time(gone),
                cwd(gone),
                command(gone),
                state(gone),
                open_files(gone),
                is_unexeced_fork_of(gone, std::process::id()),
            ),
            (
                false,
                None,
                None,
                None,
                String::new(),
                None,
                Vec::new(),
                false
            )
        );
    }

    /// A process whose working directory was removed under it names that directory by its path,
    /// without the marker the kernel adds to it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_removed_cwd_is_named_by_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let removed = dir.path().canonicalize().unwrap().join("removed");
        std::fs::create_dir(&removed).unwrap();
        let mut child = sleeper_in(&removed);
        std::fs::remove_dir(&removed).unwrap();
        let child_cwd = cwd(child.id());
        cleanup(&mut child);
        assert_eq!(child_cwd, Some(removed));
    }

    /// A fork of this process that has not exec'd holds the descriptors it copied from this
    /// process, and is never counted a holder for them.
    #[cfg(unix)]
    #[test]
    fn a_fork_of_this_process_that_has_not_execd_never_counts() {
        use std::io::{Read, Write};
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::process::CommandExt;

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let _own = std::fs::File::open(&file).unwrap();
        let (mut pid_rx, pid_tx) = std::io::pipe().unwrap();
        let (go_rx, mut go_tx) = std::io::pipe().unwrap();
        let go_tx_fd = go_tx.as_raw_fd();
        let mut cmd = crate::subprocess::command("true");
        // SAFETY: the closure runs in the forked child before exec and only makes raw
        // read/write/close calls on descriptors it inherited.
        unsafe {
            cmd.pre_exec(move || {
                // Close the child's copy of the go pipe's write end, so this test dropping
                // its own copy (on success or on a panic) always releases the child.
                drop(OwnedFd::from_raw_fd(go_tx_fd));
                (&pid_tx).write_all(&std::process::id().to_ne_bytes())?;
                let _ = (&go_rx).read(&mut [0]);
                Ok(())
            });
        }
        // `spawn` returns only once the child has exec'd, so it runs off this thread.
        let spawner = std::thread::spawn(move || cmd.spawn().unwrap().wait().unwrap());
        let mut pid = [0; 4];
        pid_rx.read_exact(&mut pid).unwrap();
        let pid = u32::from_ne_bytes(pid);
        let base = dir.path().canonicalize().unwrap();
        assert!(
            holds(pid, &base),
            "the paused fork holds the inherited descriptor"
        );
        assert!(is_unexeced_fork_of(pid, std::process::id()));
        assert!(!holders(&base).contains(&pid));
        assert_eq!(
            crate::test_support::files_open_by(pid, &base.join("f")),
            0,
            "the host fixture counts no open by a fork whose descriptors are this process's own"
        );
        go_tx.write_all(b"g").unwrap();
        assert!(spawner.join().unwrap().success());
    }

    /// A parent a Windows snapshot names is trusted while it still opens and is a parent by the
    /// rule it is read with: never once it started after the child (a later process given the
    /// dead parent's pid) or no longer opens, and when it started with the child only where a tie
    /// is trusted.
    #[test]
    fn a_named_parent_is_trusted_only_while_it_opens_and_its_start_makes_it_a_parent() {
        let started = 1_000;
        let rows = [
            ("started before", Ties::Untrusted, Some(999), true),
            ("a tie", Ties::Untrusted, Some(1_000), false),
            ("a tie, ties trusted", Ties::Trusted, Some(1_000), true),
            ("started after", Ties::Untrusted, Some(1_001), false),
            ("no longer opens", Ties::Untrusted, None, false),
        ];
        let trusted: Vec<(&str, bool)> = rows
            .iter()
            .map(|&(what, ties, parent_started, _)| {
                (what, trusts_parent(ties, parent_started, started))
            })
            .collect();
        let expected: Vec<(&str, bool)> = rows
            .iter()
            .map(|&(what, _, _, trusted)| (what, trusted))
            .collect();
        assert_eq!(trusted, expected);
    }

    /// A parent that started after its child is never its parent and one that started before it
    /// always is, under either rule; one that started in the child's own tick is a parent where a
    /// tie is trusted (Linux and macOS) and a tie where it is not (Windows).
    #[test]
    fn a_parents_start_against_its_childs_reads_by_the_tie_rule() {
        use Parentage::{Later, Parent, Tie};
        let started = 1_000;
        // What the parent's start is under each rule: ties trusted, then ties untrusted.
        let rows = [
            ("a later parent", 1_001, Later, Later),
            ("an earlier parent", 999, Parent, Parent),
            ("a tie", 1_000, Parent, Tie),
        ];
        let read: Vec<(&str, Parentage, Parentage)> = rows
            .iter()
            .map(|&(what, parent_started, _, _)| {
                (
                    what,
                    Ties::Trusted.parentage(parent_started, started),
                    Ties::Untrusted.parentage(parent_started, started),
                )
            })
            .collect();
        let expected: Vec<(&str, Parentage, Parentage)> = rows
            .iter()
            .map(|&(what, _, trusted, untrusted)| (what, trusted, untrusted))
            .collect();
        assert_eq!(read, expected);
    }

    /// Windows reads a tie as no parent and every other OS as a parent.
    #[test]
    fn this_oss_tie_rule_is_its_own() {
        let expected = std::cfg_select! {
            target_os = "windows" => { Ties::Untrusted }
            _ => { Ties::Trusted }
        };
        assert_eq!(TIES, expected);
    }

    /// A table's rows are the rows it claims, capped at the whole rows its words hold: a partial
    /// row at its end is never a row.
    #[test]
    fn a_tables_rows_are_its_claim_capped_at_the_whole_rows_it_holds() {
        // Six words a row, as Windows's owner-pid TCP table has.
        let row_words = 6;
        let rows = [
            ("claims none", 0, 12, 0),
            ("claims fewer than it holds", 1, 12, 1),
            ("claims what it holds", 2, 12, 2),
            ("claims more than it holds", 3, 12, 2),
            ("ends in a partial row", 3, 17, 2),
        ];
        let held: Vec<(&str, usize)> = rows
            .iter()
            .map(|&(what, claimed, words, _)| (what, whole_rows(claimed, words, row_words)))
            .collect();
        let expected: Vec<(&str, usize)> = rows
            .iter()
            .map(|&(what, _, _, held)| (what, held))
            .collect();
        assert_eq!(held, expected);
    }

    /// A port held in network byte order in a wider field's low 16 bits reads as that port,
    /// whatever the field's upper bits hold.
    #[test]
    fn a_socket_records_port_is_its_low_half_in_network_byte_order() {
        let network = |port: u16| u32::from(port.to_be());
        let rows = [
            (network(8080), 8080),
            (network(1), 1),
            (0xDEAD_0000 | network(443), 443),
        ];
        let read: Vec<u16> = rows.iter().map(|&(raw, _)| port_of(raw)).collect();
        assert_eq!(read, rows.map(|(_, port)| port));
    }

    /// Files are registered with the Restart Manager at most 256 a call: no call for no file,
    /// one call up to 256, and another from the 257th file on.
    #[test]
    fn files_are_registered_at_most_256_a_call() {
        let rows: [(usize, &[usize]); 5] = [
            (0, &[]),
            (1, &[1]),
            (256, &[256]),
            (257, &[256, 1]),
            (513, &[256, 256, 1]),
        ];
        let calls: Vec<(usize, Vec<usize>)> = rows
            .iter()
            .map(|&(files, _)| {
                let calls = registrations(&vec![(); files]).map(<[()]>::len).collect();
                (files, calls)
            })
            .collect();
        let expected: Vec<(usize, Vec<usize>)> = rows
            .iter()
            .map(|&(files, calls)| (files, calls.to_vec()))
            .collect();
        assert_eq!(calls, expected);
    }

    /// A macOS start time is its seconds and microseconds read as microseconds since the epoch.
    #[test]
    fn a_macos_start_time_is_its_seconds_and_microseconds_in_microseconds() {
        let rows = [
            (0, 0, 0),
            (0, 999_999, 999_999),
            (2, 3, 2_000_003),
            (1_700_000_000, 123_456, 1_700_000_000_123_456),
        ];
        let read: Vec<u64> = rows
            .iter()
            .map(|&(secs, micros, _)| micros_since_epoch(secs, micros))
            .collect();
        assert_eq!(read, rows.map(|(_, _, started)| started));
    }

    /// A macOS TCP socket listens over IPv4 on a port only while it is listening, carries the
    /// IPv4 flag (alone or beside IPv6's) and is bound to that port. The rows pin that
    /// conjunction over the flag and state names, not the kernel's values for them.
    #[test]
    fn a_macos_socket_listens_over_ipv4_only_when_listening_ipv4_and_on_the_port() {
        // `INI_IPV6` and `TSI_S_ESTABLISHED`.
        let (ipv6, established) = (0x2, 4);
        let port = |port: u16| i32::from(port.to_be());
        let rows = [
            (
                "listening, IPv4, on the port",
                TSI_S_LISTEN,
                INI_IPV4,
                port(8080),
                true,
            ),
            (
                "listening, IPv4 and IPv6, on the port",
                TSI_S_LISTEN,
                INI_IPV4 | ipv6,
                port(8080),
                true,
            ),
            (
                "listening, IPv6 alone",
                TSI_S_LISTEN,
                ipv6,
                port(8080),
                false,
            ),
            (
                "established, IPv4, on the port",
                established,
                INI_IPV4,
                port(8080),
                false,
            ),
            (
                "listening, IPv4, on another port",
                TSI_S_LISTEN,
                INI_IPV4,
                port(8081),
                false,
            ),
        ];
        let listens: Vec<(&str, bool)> = rows
            .iter()
            .map(|&(what, state, vflag, raw_port, _)| {
                (what, listens_over_ipv4(state, vflag, raw_port, 8080))
            })
            .collect();
        let expected: Vec<(&str, bool)> = rows
            .iter()
            .map(|&(what, _, _, _, listens)| (what, listens))
            .collect();
        assert_eq!(listens, expected);
    }
}
