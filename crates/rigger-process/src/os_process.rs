//! The process interface (spec 125, criterion 9): the one place rigger reads a fact about a
//! running process - which processes exist, a process's parent, start time, working directory,
//! command line, run state and open files, which process listens on a TCP port, and which
//! processes hold a directory. Each fact is a function here, answered by the native adapter of
//! the OS the binary is built for; the reaper ([`crate::reap`]), the holder check
//! ([`crate::holders`]) and the dash read processes through these functions alone. Read-only:
//! nothing here signals a process.
//!
//! Every reader is best-effort: a process that is gone, or one this process may not read,
//! answers absent (`None`, an empty list, an empty command), never an error, and on an OS with
//! no native adapter every reader answers absent.

use std::path::{Path, PathBuf};

// The one spelling of the OS set: each OS with a native adapter declares its module and binds it
// as `native`, and every other OS binds the inline `unsupported` module in the `_` arm.
std::cfg_select! {
    target_os = "linux" => {
        pub(crate) mod linux;
        use linux as native;
    }
    _ => {
        /// The OSes with no native adapter, the `_` arm of the dispatch above: every process
        /// answers absent.
        mod unsupported {
            use std::path::PathBuf;

            pub(super) fn pids() -> Vec<u32> {
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
                /// reader.
                #[test]
                fn every_reader_answers_absent_for_this_process() {
                    let me = std::process::id();
                    assert_eq!(
                        (
                            pids(),
                            parent(me),
                            start_time(me),
                            cwd(me),
                            command(me),
                            state(me),
                            open_files(me),
                            is_unexeced_fork_of(me, me),
                        ),
                        (Vec::new(), None, None, None, String::new(), None, Vec::new(), false)
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

/// Process `pid`'s command line, its arguments joined by single spaces, or its short name when
/// its arguments cannot be read: descriptive only, it names a process in a message and decides
/// nothing. Empty when `pid` is gone.
pub fn command(pid: u32) -> String {
    native::command(pid)
}

/// Process `pid`'s run state as one letter: `R` running, `S` sleeping, `T` stopped, `Z` a zombie
/// (exited, not yet reaped by its parent), or the OS's own letter for another state. `None` when
/// `pid` is gone or its state cannot be read.
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
/// whatever the child still holds once it has exec'd counts from then on. The check a reclaim
/// runs before it deletes a directory it counts as dead.
pub fn holders(dir: &Path) -> Vec<u32> {
    let me = std::process::id();
    pids()
        .into_iter()
        .filter(|&pid| pid != me && !is_unexeced_fork_of(pid, me) && holds(pid, dir))
        .collect()
}

/// Whether process `pid`'s working directory or one of its open files lies in `dir` or inside
/// it, matched on path components.
fn holds(pid: u32, dir: &Path) -> bool {
    cwd(pid).is_some_and(|cwd| cwd.starts_with(dir))
        || open_files(pid).iter().any(|file| file.starts_with(dir))
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

    /// The process list names this process, and names it once.
    #[cfg(unix)]
    #[test]
    fn the_process_list_names_this_process_once() {
        let me = std::process::id();
        assert_eq!(pids().into_iter().filter(|&pid| pid == me).count(), 1);
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

    /// A process's command is its arguments joined by single spaces.
    #[cfg(unix)]
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

    /// This process, which is running a test, is running or sleeping, and a child that exited
    /// but is not yet reaped is a zombie.
    #[cfg(unix)]
    #[test]
    fn a_live_process_runs_or_sleeps_and_an_unreaped_exit_is_a_zombie() {
        let mut child = crate::subprocess::command("true").spawn().unwrap();
        let zombie = wait_until(|| state(child.id()) == Some('Z'));
        child.wait().unwrap();
        let mine = state(std::process::id());
        assert!(
            matches!(mine, Some('R' | 'S')),
            "this process runs or sleeps: {mine:?}"
        );
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
    /// listens there once the listener is closed.
    #[cfg(unix)]
    #[test]
    fn the_listener_on_a_port_is_the_process_holding_it() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let listening = listening_pid(port);
        drop(listener);
        assert_eq!(listening, Some(std::process::id()));
        // A sibling test's fork that has not exec'd yet holds a copy of the listener until its
        // exec closes it, so the port is given that moment to close.
        assert!(
            wait_until(|| listening_pid(port).is_none()),
            "nothing listens on a closed port"
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
}
