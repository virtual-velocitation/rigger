//! The process interface (spec 125, criterion 9): the one place rigger reads a fact about a
//! running process - which processes exist, a process's parent, start time, working directory,
//! command line, run state and open files, and which process listens on a TCP port. Each fact is
//! a function here, answered by the native adapter of the OS the binary is built for; the reaper
//! ([`crate::reap`]), the holder check ([`crate::holders`]) and the dash read processes through
//! these functions alone. Read-only: nothing here signals a process.
//!
//! Every reader is best-effort: a process that is gone, or one this process may not read,
//! answers absent (`None`, an empty list, an empty command), never an error, and on an OS with
//! no native adapter every reader answers absent.

use std::path::PathBuf;

// The one spelling of the OS set: each OS with a native adapter declares its module and binds it
// as `native`, and every other OS binds the inline `unsupported` module in the `_` arm.
std::cfg_select! {
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

/// The pid of a process listening for TCP connections on `port`, over IPv4 or IPv6; `None` when
/// nothing listens there or the listener's process cannot be read.
pub fn listening_pid(port: u16) -> Option<u32> {
    native::listening_pid(port)
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
}
