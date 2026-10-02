//! Whether any process still holds a directory - its working directory, or one of its open
//! file descriptors, resolves inside it. The check a reclaim runs before it deletes a
//! directory it counts as dead, so a directory a live process is still using stays where it
//! is. Read-only: it reads `/proc` and never signals anything.
//!
//! This process never counts, and neither does a child of it that has forked but not yet
//! `exec`'d: until its `execve` completes, that child is still this process's own image
//! running the standard library's spawn code over copies of this process's descriptors, and
//! the close-on-exec ones (every descriptor the standard library opens) vanish at exec. The
//! kernel marks such a child with `PF_FORKNOEXEC` in `/proc/<pid>/stat`'s flags and clears
//! the mark inside `execve` before it closes those descriptors, so the flag is read first;
//! whatever the child still holds once it has exec'd counts from then on.

use std::path::{Path, PathBuf};

/// Every process other than this one whose working directory or any open file descriptor
/// resolves to `dir` itself or to a path inside it, as pids. Empty when `dir` cannot be
/// resolved or `/proc` cannot be read (a platform without it) - best-effort, like the
/// reaper's own scan ([`crate::reap::processes_rooted_under`]), whose cwd rule this extends
/// with open descriptors.
pub fn processes_holding(dir: &Path) -> Vec<u32> {
    let Ok(base) = dir.canonicalize() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let self_pid = std::process::id();
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()))
        .filter(|&pid| pid != self_pid && !is_unexeced_fork_of(pid, self_pid) && holds(pid, &base))
        .collect()
}

/// The kernel's per-task "forked but has not exec'd" flag (`PF_FORKNOEXEC`, `proc(5)`).
const PF_FORKNOEXEC: u64 = 0x40;

/// Whether `pid` is a child of `self_pid` still carrying [`PF_FORKNOEXEC`]: a fork of this
/// process whose `execve` has not yet closed the descriptors it inherited.
fn is_unexeced_fork_of(pid: u32, self_pid: u32) -> bool {
    let field = |index| crate::reap::stat_field_after_comm(pid, index);
    field(1).and_then(|ppid| ppid.parse::<u32>().ok()) == Some(self_pid)
        && field(6)
            .and_then(|flags| flags.parse::<u64>().ok())
            .is_some_and(|flags| flags & PF_FORKNOEXEC != 0)
}

/// Whether process `pid`'s cwd or one of its open files ([`open_files`]) resolves inside `base`
/// (already canonical). A link that cannot be read - the process exited, or belongs to
/// another user - holds nothing.
fn holds(pid: u32, base: &Path) -> bool {
    std::fs::read_link(proc_of(pid).join("cwd")).is_ok_and(|cwd| cwd.starts_with(base))
        || open_files(pid).any(|file| file.starts_with(base))
}

/// The files process `pid` holds open, one per open file descriptor, each named as its
/// `/proc/<pid>/fd` link names it: the process adapters' reader of a process's open files, which
/// `holds` and the host test fixtures consume. Nothing when that directory cannot be read - the
/// process is gone, or belongs to another user - and a descriptor whose link cannot be read names
/// nothing.
pub fn open_files(pid: u32) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(proc_of(pid).join("fd"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|fd| std::fs::read_link(fd.path()).ok())
}

/// The `/proc` directory of process `pid`.
fn proc_of(pid: u32) -> PathBuf {
    Path::new("/proc").join(pid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{release, waiting_shell};

    #[test]
    fn a_process_whose_cwd_is_inside_the_dir_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let child = waiting_shell("true", &inner);
        let pid = child.id();
        assert!(processes_holding(dir.path()).contains(&pid));
        release(child);
    }

    #[test]
    fn a_process_with_an_open_descriptor_inside_the_dir_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let child = waiting_shell(&format!("exec 3< '{}'", file.display()), elsewhere.path());
        let pid = child.id();
        assert!(processes_holding(dir.path()).contains(&pid));
        assert!(
            !processes_holding(elsewhere.path()).is_empty(),
            "its cwd holds the other dir"
        );
        release(child);
    }

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
        assert!(!processes_holding(dir.path()).contains(&pid));
        go_tx.write_all(b"g").unwrap();
        assert!(spawner.join().unwrap().success());
    }

    #[test]
    fn a_processes_open_files_name_each_file_it_holds_and_a_gone_process_holds_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().canonicalize().unwrap().join("f");
        std::fs::write(&file, b"x").unwrap();
        let child = waiting_shell(
            &format!("exec 3< '{}' 4< '{}'", file.display(), file.display()),
            dir.path(),
        );
        let pid = child.id();
        let held = open_files(pid).filter(|named| *named == file).count();
        release(child);
        assert_eq!(
            (held, open_files(pid).count()),
            (2, 0),
            "each descriptor on the file names it, and a process that is gone holds nothing"
        );
    }

    #[test]
    fn nothing_holds_a_dir_no_process_uses_and_this_process_never_counts() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let _own = std::fs::File::open(&file).unwrap();
        assert!(processes_holding(dir.path()).is_empty());
        assert!(processes_holding(&dir.path().join("missing")).is_empty());
    }
}
