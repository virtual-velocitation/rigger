//! The process interface's Linux adapter, bound on Linux and on Android, which runs the same
//! kernel: the one production file that reads `/proc`, the kernel's own view of every process
//! (`proc(5)`). Each function answers the fact its namesake in
//! [`super`] documents, through `std` alone (no `libc`), so it reads identically on both feature
//! lanes; a read that fails - the process exited, or belongs to another user - answers absent.

use std::path::{Path, PathBuf};

pub(super) use super::scan_holders as holders;

/// The `/proc` directory of process `pid`.
fn proc_of(pid: u32) -> PathBuf {
    Path::new("/proc").join(pid.to_string())
}

/// The numeric entries of `/proc`, one per process; `/proc/self`, `/proc/meminfo` and the other
/// named entries are skipped.
pub(super) fn pids() -> Vec<u32> {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok())
        })
        .collect()
}

/// `/proc/<pid>/stat`'s `ppid` field, the parent ([`stat_field_after_comm`] index `1`): the one
/// read of a process's parent in this adapter.
pub(super) fn parent(pid: u32) -> Option<u32> {
    stat_field_after_comm(pid, 1)?.parse().ok()
}

/// The ONE `/proc/<pid>/stat` parser: the `index`-th (0-based) whitespace-separated field
/// AFTER `comm` - `0` is the state, `1` the parent, `6` the flags, `19` the start time (see
/// `proc(5)`). Fields are located from the LAST `)` in the line rather than by naive
/// whitespace-splitting, because field 2 (`comm`, the process name in parens) may itself contain
/// spaces or parens. `None` when the process has already exited, `/proc` is unavailable, or the
/// line has no such field. Public so the Linux test fixtures read a process's state or group
/// through this same parser (`crate::reap` re-exports it).
pub fn stat_field_after_comm(pid: u32, index: usize) -> Option<String> {
    let stat = std::fs::read_to_string(proc_of(pid).join("stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    after_comm.split_whitespace().nth(index).map(String::from)
}

/// `/proc/<pid>/stat` field 22 (`starttime`, clock ticks since boot), which the kernel keeps
/// for the life of a pid and which repeats only when the pid itself is reused.
pub(super) fn start_time(pid: u32) -> Option<u64> {
    // Fields after `comm`, 1-indexed: state, ppid, pgrp, session, tty_nr, tpgid, flags,
    // minflt, cminflt, majflt, cmajflt, utime, stime, cutime, cstime, priority, nice,
    // num_threads, itrealvalue, starttime - the 20th, so index 19 (0-based).
    stat_field_after_comm(pid, 19)?.parse().ok()
}

/// `/proc/<pid>/cwd`, the link the kernel resolves to the process's absolute, canonical working
/// directory, without the `" (deleted)"` marker the kernel adds once that directory is removed
/// ([`super::strip_deleted_suffix`]), so a process holding a removed directory is still named by
/// that directory's path.
pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(proc_of(pid).join("cwd"))
        .ok()
        .map(|cwd| super::strip_deleted_suffix(&cwd))
}

/// `/proc/<pid>/cmdline` (the NUL-separated arguments) joined by spaces, falling back to
/// `/proc/<pid>/comm` (the short name) and finally an empty string.
pub(super) fn command(pid: u32) -> String {
    let dir = proc_of(pid);
    if let Ok(bytes) = std::fs::read(dir.join("cmdline")) {
        let joined = bytes
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            return joined;
        }
    }
    std::fs::read_to_string(dir.join("comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The first letter of `/proc/<pid>/stat`'s state field: `R` running, `S` sleeping, `D`
/// uninterruptible sleep, `T` stopped by job control, `t` stopped under a tracer, `Z` zombie, and
/// so on.
pub(super) fn state(pid: u32) -> Option<char> {
    stat_field_after_comm(pid, 0)?.chars().next()
}

/// Each `/proc/<pid>/fd` link, named as the link names it: a file by its path, a socket as
/// `socket:[<inode>]`. A descriptor whose link cannot be read names nothing.
pub(super) fn open_files(pid: u32) -> Vec<PathBuf> {
    std::fs::read_dir(proc_of(pid).join("fd"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|fd| std::fs::read_link(fd.path()).ok())
        .collect()
}

/// The kernel's per-task "forked but has not exec'd" flag (`PF_FORKNOEXEC`, `proc(5)`), which it
/// clears inside `execve` before it closes the close-on-exec descriptors.
const PF_FORKNOEXEC: u64 = 0x40;

/// Whether `pid`'s [`parent`] is `parent_pid` and its `/proc/<pid>/stat` flags carry
/// [`PF_FORKNOEXEC`].
pub(super) fn is_unexeced_fork_of(pid: u32, parent_pid: u32) -> bool {
    parent(pid) == Some(parent_pid)
        && stat_field_after_comm(pid, 6)
            .and_then(|flags| flags.parse::<u64>().ok())
            .is_some_and(|flags| flags & PF_FORKNOEXEC != 0)
}

/// The process holding the socket [`tcp_listen_inode_for_port`] names: the first process whose open
/// files ([`open_files`]) hold `socket:[<inode>]`, the same technique `lsof` and `ss` use.
pub(super) fn listening_pid(port: u16) -> Option<u32> {
    let socket = PathBuf::from(format!("socket:[{}]", tcp_listen_inode_for_port(port)?));
    pids()
        .into_iter()
        .find(|&pid| open_files(pid).contains(&socket))
}

/// The kernel's `st` value for a listening socket (`TCP_LISTEN`), in `/proc/net/tcp`'s hex.
const TCP_LISTEN_STATE: &str = "0A";

/// The inode of the socket listening on `port` in `/proc/net/tcp`, the kernel's IPv4 TCP socket
/// table. One row per socket: `sl local_address rem_address st tx:rx tr:tm retrnsmt uid timeout
/// inode ...`, whitespace-separated, header row skipped; `local_address` is `<hex-ip>:<hex-port>`
/// in the kernel's own hex formatting, and `st` is [`TCP_LISTEN_STATE`] for a listener. A
/// connection's row is never the answer, though its own end is bound to `port`, as an accepted
/// connection's is. Matching on the PORT alone (not the ip half) is deliberate: a bind to
/// `0.0.0.0:<port>` reserves every interface including loopback, so it holds a loopback port
/// exactly as a same-address listener does. `None` when the table cannot be read or no
/// listening row's port matches.
fn tcp_listen_inode_for_port(port: u16) -> Option<u64> {
    let text = std::fs::read_to_string("/proc/net/tcp").ok()?;
    let port_hex = format!("{port:04X}");
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(local_address) = fields.get(1) else {
            continue;
        };
        let Some((_, local_port)) = local_address.split_once(':') else {
            continue;
        };
        if !local_port.eq_ignore_ascii_case(&port_hex) || fields.get(3) != Some(&TCP_LISTEN_STATE) {
            continue;
        }
        if let Some(inode) = fields.get(9).and_then(|s| s.parse().ok()) {
            return Some(inode);
        }
    }
    None
}
