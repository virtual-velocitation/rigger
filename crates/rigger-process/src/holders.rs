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

use std::path::Path;

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

/// Whether process `pid`'s cwd or one of its open file descriptors resolves inside `base`
/// (already canonical). A link that cannot be read - the process exited, or belongs to
/// another user - holds nothing.
fn holds(pid: u32, base: &Path) -> bool {
    let proc = Path::new("/proc").join(pid.to_string());
    let inside =
        |link: &Path| std::fs::read_link(link).is_ok_and(|target| target.starts_with(base));
    inside(&proc.join("cwd"))
        || std::fs::read_dir(proc.join("fd"))
            .is_ok_and(|fds| fds.flatten().any(|fd| inside(&fd.path())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};

    /// A shell that runs `setup`, reports readiness on stdout, then waits for its stdin to
    /// close - so dropping the handle's stdin ends it without any signal.
    fn holder(setup: &str, cwd: &Path) -> Child {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(format!("{setup}; echo ready; read _"))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn the holder");
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(child.stdout.as_mut().unwrap()),
            &mut line,
        )
        .unwrap();
        assert_eq!(line.trim(), "ready");
        child
    }

    /// Close `child`'s stdin so it exits on its own, then reap it.
    fn release(mut child: Child) {
        let mut stdin = child.stdin.take().unwrap();
        let _ = stdin.flush();
        drop(stdin);
        child.wait().unwrap();
    }

    #[test]
    fn a_process_whose_cwd_is_inside_the_dir_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let child = holder("true", &inner);
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
        let child = holder(&format!("exec 3< '{}'", file.display()), elsewhere.path());
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
    fn nothing_holds_a_dir_no_process_uses_and_this_process_never_counts() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let _own = std::fs::File::open(&file).unwrap();
        assert!(processes_holding(dir.path()).is_empty());
        assert!(processes_holding(&dir.path().join("missing")).is_empty());
    }
}
