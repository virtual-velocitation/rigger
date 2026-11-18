//! The process interface's macOS adapter: libproc, the system library's process-information
//! calls (`sys/proc_info.h`), through the declarations the `libc` crate makes for them. Each
//! function answers the fact its namesake in [`super`] documents; a call that fails - the process
//! exited, or belongs to another user - answers absent. The per-descriptor structs `libc` does
//! not declare are declared here from xnu's `bsd/sys/proc_info.h`, their sizes and offsets held to
//! the header's by the `const` assertions below them.

use std::ffi::{c_int, OsStr};
use std::mem::{size_of, MaybeUninit};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

/// `proc_pidinfo`'s `arg` for the BSD-info flavor that also finds a zombie, an exited process its
/// parent has not reaped yet (xnu answers a zombie's BSD info only when `arg` is non-zero).
const FIND_ZOMBIES: u64 = 1;

/// `PROC_FLAG_EXEC`: the BSD-info flag set once a process has called exec.
const PROC_FLAG_EXEC: u32 = 0x4000;

/// `PROC_PIDFDVNODEPATHINFO`: the `proc_pidfdinfo` flavor answering a vnode descriptor's path.
const PROC_PIDFDVNODEPATHINFO: c_int = 2;

/// `PROC_PIDFDSOCKETINFO`: the `proc_pidfdinfo` flavor answering a socket descriptor's state.
const PROC_PIDFDSOCKETINFO: c_int = 3;

/// `SOCKINFO_TCP`: the `soi_kind` of a TCP socket, whose `soi_proto` holds a [`TcpSockInfo`].
const SOCKINFO_TCP: c_int = 2;

/// `struct proc_fileinfo`, which leads every per-descriptor flavor.
#[repr(C)]
struct ProcFileInfo {
    _fi_openflags: u32,
    _fi_status: u32,
    _fi_offset: i64,
    _fi_type: i32,
    _fi_guardflags: u32,
}

/// `struct vnode_fdinfowithpath`, the [`PROC_PIDFDVNODEPATHINFO`] answer.
#[repr(C)]
struct VnodeFdInfoWithPath {
    _pfi: ProcFileInfo,
    pvip: libc::vnode_info_path,
}

/// `struct sockbuf_info`, a socket's receive or send buffer.
#[repr(C)]
struct SockbufInfo {
    _sbi_cc: u32,
    _sbi_hiwat: u32,
    _sbi_mbcnt: u32,
    _sbi_mbmax: u32,
    _sbi_lowat: u32,
    _sbi_flags: i16,
    _sbi_timeo: i16,
}

/// The `insi_v6` member of `struct in_sockinfo`.
#[repr(C)]
#[derive(Clone, Copy)]
struct InSockInfoV6 {
    _in6_hlim: u8,
    _in6_cksum: c_int,
    _in6_ifindex: u16,
    _in6_hops: i16,
}

/// `struct in_sockinfo`, an internet socket's addresses; `insi_lport` holds the local port in
/// network byte order.
#[repr(C)]
#[derive(Clone, Copy)]
struct InSockInfo {
    _insi_fport: c_int,
    insi_lport: c_int,
    _insi_gencnt: u64,
    _insi_flags: u32,
    _insi_flow: u32,
    insi_vflag: u8,
    _insi_ip_ttl: u8,
    _rfu_1: u32,
    _insi_faddr: [u32; 4],
    _insi_laddr: [u32; 4],
    _insi_v4: u8,
    _insi_v6: InSockInfoV6,
}

/// `struct tcp_sockinfo`, the `soi_proto` of a [`SOCKINFO_TCP`] socket.
#[repr(C)]
#[derive(Clone, Copy)]
struct TcpSockInfo {
    tcpsi_ini: InSockInfo,
    tcpsi_state: c_int,
    _tcpsi_timer: [c_int; 4],
    _tcpsi_mss: c_int,
    _tcpsi_flags: u32,
    _rfu_1: u32,
    _tcpsi_tp: u64,
}

/// `soi_proto`, whose variant `soi_kind` names. Only the TCP variant is read; the reserve makes
/// the buffer at least as large as the largest variant (the 528-byte `un_sockinfo`), since the
/// kernel refuses a buffer smaller than its own `struct socket_fdinfo`.
#[repr(C)]
union SocketProto {
    pri_tcp: TcpSockInfo,
    _reserve: [u64; 128],
}

/// `struct socket_info`.
#[repr(C)]
struct SocketInfo {
    _soi_stat: libc::vinfo_stat,
    _soi_so: u64,
    _soi_pcb: u64,
    _soi_type: c_int,
    _soi_protocol: c_int,
    _soi_family: c_int,
    _soi_options: i16,
    _soi_linger: i16,
    _soi_state: i16,
    _soi_qlen: i16,
    _soi_incqlen: i16,
    _soi_qlimit: i16,
    _soi_timeo: i16,
    _soi_error: u16,
    _soi_oobmark: u32,
    _soi_rcv: SockbufInfo,
    _soi_snd: SockbufInfo,
    soi_kind: c_int,
    _rfu_1: u32,
    soi_proto: SocketProto,
}

/// `struct socket_fdinfo`, the [`PROC_PIDFDSOCKETINFO`] answer.
#[repr(C)]
struct SocketFdInfo {
    _pfi: ProcFileInfo,
    psi: SocketInfo,
}

// The header's layout, so a field declared out of order or at the wrong width fails the build.
const _: () = assert!(size_of::<ProcFileInfo>() == 24);
const _: () = assert!(size_of::<VnodeFdInfoWithPath>() == 1200);
const _: () = assert!(size_of::<InSockInfo>() == 80);
const _: () = assert!(size_of::<TcpSockInfo>() == 120);
const _: () = assert!(std::mem::offset_of!(SocketInfo, soi_kind) == 232);
const _: () = assert!(std::mem::offset_of!(SocketInfo, soi_proto) == 240);

/// `pid` as libproc's `int`, or `None` for a pid no process can have.
fn raw(pid: u32) -> Option<c_int> {
    c_int::try_from(pid).ok()
}

/// The `T` that `proc_pidinfo`'s `flavor` fills for `pid`, or `None` when the call fills nothing.
///
/// # Safety
/// `T` must be the plain-data struct `flavor` answers (or one at least as large, laid out as it
/// is), for which every byte pattern, the all-zero one included, is a valid value.
unsafe fn pid_info<T>(pid: u32, flavor: c_int, arg: u64) -> Option<T> {
    let pid = raw(pid)?;
    let size = c_int::try_from(size_of::<T>()).ok()?;
    let mut info = MaybeUninit::<T>::zeroed();
    // SAFETY: the buffer is `size` writable bytes of a zeroed `T`, the kernel writes at most
    // `size` bytes into it, and the caller vouches that any bytes it writes make a valid `T`.
    let filled = unsafe { libc::proc_pidinfo(pid, flavor, arg, info.as_mut_ptr().cast(), size) };
    // SAFETY: zeroed, then partly or wholly overwritten by the kernel: a valid `T` either way.
    (filled > 0).then(|| unsafe { info.assume_init() })
}

/// The `T` that `proc_pidfdinfo`'s `flavor` fills for descriptor `fd` of `pid`, or `None` when the
/// call fills nothing.
///
/// # Safety
/// As for [`pid_info`]: `T` is the plain-data struct `flavor` answers, or a larger one laid out as
/// it is.
unsafe fn fd_info<T>(pid: u32, fd: c_int, flavor: c_int) -> Option<T> {
    let pid = raw(pid)?;
    let size = c_int::try_from(size_of::<T>()).ok()?;
    let mut info = MaybeUninit::<T>::zeroed();
    // SAFETY: as in `pid_info`: `size` writable bytes of a zeroed `T`.
    let filled = unsafe { libc::proc_pidfdinfo(pid, fd, flavor, info.as_mut_ptr().cast(), size) };
    // SAFETY: as in `pid_info`.
    (filled > 0).then(|| unsafe { info.assume_init() })
}

/// `pid`'s BSD info, a zombie's included.
fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    // SAFETY: `proc_bsdinfo` is the plain-data struct the BSD-info flavor answers.
    unsafe { pid_info(pid, libc::PROC_PIDTBSDINFO, FIND_ZOMBIES) }
}

/// The path a `vnode_info_path` names, its bytes up to the first NUL; `None` when it names none.
fn path_of(vnode: &libc::vnode_info_path) -> Option<PathBuf> {
    let bytes: Vec<u8> = vnode
        .vip_path
        .as_flattened()
        .iter()
        .map(|&c| c as u8)
        .take_while(|&b| b != 0)
        .collect();
    (!bytes.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(&bytes)))
}

/// `pid`'s open descriptors, each with its type: `PROC_PIDLISTFDS` asked first for the size of
/// the list, then for the list.
fn descriptors(pid: u32) -> Vec<libc::proc_fdinfo> {
    let Some(raw_pid) = raw(pid) else {
        return Vec::new();
    };
    let entry = size_of::<libc::proc_fdinfo>();
    // SAFETY: a null buffer of size 0 asks only for the list's size in bytes; nothing is written.
    let needed =
        unsafe { libc::proc_pidinfo(raw_pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    let Ok(needed) = usize::try_from(needed) else {
        return Vec::new();
    };
    let mut fds = vec![
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0
        };
        needed / entry
    ];
    let Ok(size) = c_int::try_from(fds.len() * entry) else {
        return Vec::new();
    };
    // SAFETY: the buffer is `size` writable bytes of `proc_fdinfo` entries, the struct this flavor
    // fills, and the kernel writes at most `size` bytes.
    let filled = unsafe {
        libc::proc_pidinfo(
            raw_pid,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast(),
            size,
        )
    };
    fds.truncate(usize::try_from(filled).unwrap_or(0) / entry);
    fds
}

/// `proc_listallpids`, asked first for the count, then for the list in a buffer with room for
/// the processes started since, grown while the list fills it.
pub(super) fn pids() -> Vec<u32> {
    // SAFETY: a null buffer of size 0 asks only for the number of processes; nothing is written.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let mut capacity = usize::try_from(count).unwrap_or(0) + 64;
    loop {
        let mut pids: Vec<c_int> = vec![0; capacity];
        let Ok(size) = c_int::try_from(capacity * size_of::<c_int>()) else {
            return Vec::new();
        };
        // SAFETY: the buffer is `size` writable bytes of `int`s, the entries this call fills.
        let filled = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
        let Ok(filled) = usize::try_from(filled) else {
            return Vec::new();
        };
        if filled < capacity {
            pids.truncate(filled);
            return pids
                .into_iter()
                .filter_map(|pid| u32::try_from(pid).ok())
                .collect();
        }
        capacity *= 2;
    }
}

/// Each listed process ([`pids`]) with its parent and start time, both read from ONE BSD info.
pub(super) fn table() -> Vec<super::Entry> {
    pids()
        .into_iter()
        .filter_map(|pid| {
            let info = bsd_info(pid)?;
            Some(super::Entry {
                pid,
                parent: info.pbi_ppid,
                start: started(&info),
            })
        })
        .collect()
}

/// The BSD info's `pbi_ppid`.
pub(super) fn parent(pid: u32) -> Option<u32> {
    bsd_info(pid).map(|info| info.pbi_ppid)
}

/// The BSD info's start time ([`started`]).
pub(super) fn start_time(pid: u32) -> Option<u64> {
    bsd_info(pid).map(|info| started(&info))
}

/// A BSD info's start time, in microseconds since the epoch ([`super::micros_since_epoch`]).
fn started(info: &libc::proc_bsdinfo) -> u64 {
    super::micros_since_epoch(info.pbi_start_tvsec, info.pbi_start_tvusec)
}

/// The current directory the vnode path info (`PROC_PIDVNODEPATHINFO`) names.
pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
    // SAFETY: `proc_vnodepathinfo` is the plain-data struct this flavor answers.
    let info: libc::proc_vnodepathinfo = unsafe { pid_info(pid, libc::PROC_PIDVNODEPATHINFO, 0) }?;
    path_of(&info.pvi_cdir)
}

/// The full path of `pid`'s executable (`proc_pidpath`); empty when it cannot be read.
pub(super) fn command(pid: u32) -> String {
    let Some(raw_pid) = raw(pid) else {
        return String::new();
    };
    let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: the buffer is `path.len()` writable bytes, the most the call writes.
    let filled =
        unsafe { libc::proc_pidpath(raw_pid, path.as_mut_ptr().cast(), path.len() as u32) };
    path.truncate(usize::try_from(filled).unwrap_or(0));
    String::from_utf8_lossy(&path).into_owned()
}

/// No arguments are read here. macOS keeps a process's arguments readable only through the
/// `KERN_PROCARGS2` sysctl, and the one caller that needs them, a tree's ordinary end looking for
/// a process still writing for its session, finds none to look at on macOS: a tree there is
/// watched, and a watched tree whose main has exited signals nothing.
pub(super) fn arguments(_pid: u32) -> Option<Vec<String>> {
    None
}

/// The BSD info's run status as one letter: `SIDL` (being created) and `SRUN` are `R`, `SSLEEP`
/// is `S`, `SSTOP` is `T` and `SZOMB` is `Z`.
pub(super) fn state(pid: u32) -> Option<char> {
    match bsd_info(pid)?.pbi_status {
        libc::SIDL | libc::SRUN => Some('R'),
        libc::SSLEEP => Some('S'),
        libc::SSTOP => Some('T'),
        libc::SZOMB => Some('Z'),
        _ => None,
    }
}

/// The path of each vnode (file or directory) descriptor `pid` holds; a socket, pipe or other
/// descriptor names nothing.
pub(super) fn open_files(pid: u32) -> Vec<PathBuf> {
    descriptors(pid)
        .iter()
        .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_VNODE as u32)
        .filter_map(|fd| {
            // SAFETY: `VnodeFdInfoWithPath` is `struct vnode_fdinfowithpath`, the plain-data
            // struct this flavor answers.
            let info: VnodeFdInfoWithPath =
                unsafe { fd_info(pid, fd.proc_fd, PROC_PIDFDVNODEPATHINFO) }?;
            path_of(&info.pvip)
        })
        .collect()
}

/// Whether `pid`'s BSD info names `parent` as its parent and lacks [`PROC_FLAG_EXEC`].
pub(super) fn is_unexeced_fork_of(pid: u32, parent: u32) -> bool {
    bsd_info(pid)
        .is_some_and(|info| info.pbi_ppid == parent && info.pbi_flags & PROC_FLAG_EXEC == 0)
}

/// The first process holding an IPv4 TCP socket listening on `port`.
pub(super) fn listening_pid(port: u16) -> Option<u32> {
    pids().into_iter().find(|&pid| listens_on(pid, port))
}

/// Whether one of `pid`'s socket descriptors is an IPv4 TCP socket listening on `port`.
fn listens_on(pid: u32, port: u16) -> bool {
    descriptors(pid)
        .iter()
        .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
        .any(|fd| {
            // SAFETY: `SocketFdInfo` is `struct socket_fdinfo` with a larger `soi_proto`, the
            // plain-data struct this flavor answers.
            let Some(info) =
                (unsafe { fd_info::<SocketFdInfo>(pid, fd.proc_fd, PROC_PIDFDSOCKETINFO) })
            else {
                return false;
            };
            if info.psi.soi_kind != SOCKINFO_TCP {
                return false;
            }
            // SAFETY: `soi_kind` names the TCP variant as the one the kernel filled, and every
            // byte pattern is a valid `TcpSockInfo`.
            let tcp = unsafe { info.psi.soi_proto.pri_tcp };
            super::listens_over_ipv4(
                tcp.tcpsi_state,
                tcp.tcpsi_ini.insi_vflag,
                tcp.tcpsi_ini.insi_lport,
                port,
            )
        })
}

/// The one scan [`super`] composes over the readers above.
pub(super) use super::scan_holders as holders;

/// The path the OS reports for the image this process runs.
pub(super) use super::command_file_name as executable_name;
pub(super) use super::reported_image as running_image;
