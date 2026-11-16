//! The process interface's Windows adapter: the Win32 calls that report on another process - the
//! ToolHelp process snapshot (pids and parents), the process times (start time), the full image
//! name (command), IP Helper's owner-pid TCP listener table ([`listening_pid`]) and the Restart
//! Manager ([`holders`]). Each function answers the fact its namesake in [`super`] documents; a
//! call that fails - the process exited, or this process may not open it - answers absent.
//! Windows documents no call that reads another process's working directory, run state or open
//! files, so [`cwd`] and [`state`] answer `None` and [`open_files`] empty, and with no fork
//! [`is_unexeced_fork_of`] answers false.

use std::path::{Path, PathBuf};
use std::ptr::null;

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA, ERROR_SUCCESS, FALSE, FILETIME,
    HANDLE, INVALID_HANDLE_VALUE, NO_ERROR,
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::AF_INET;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
    RM_PROCESS_INFO,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::win32::wide;

/// The most files one Restart Manager registration call is given.
const FILES_PER_REGISTRATION: usize = 256;

/// The longest path, in UTF-16 units, a process's image name can have.
const MAX_IMAGE_NAME: usize = 32_768;

/// A handle this adapter opened, closed once when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle is one this adapter opened and still owns, closed here once.
        unsafe { CloseHandle(self.0) };
    }
}

/// Process `pid` opened to read its limited information, or `None` when it cannot be opened (it
/// is gone, or this process may not open it).
fn open(pid: u32) -> Option<Owned> {
    // SAFETY: the call takes no pointer and answers a null handle on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    (!handle.is_null()).then_some(Owned(handle))
}

/// Each process of a ToolHelp snapshot, as its pid and the pid its entry names as its parent.
fn snapshot() -> Vec<(u32, u32)> {
    // SAFETY: the call takes no pointer and answers `INVALID_HANDLE_VALUE` on failure.
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let snapshot = Owned(handle);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::new();
    // SAFETY: `entry` is a writable `PROCESSENTRY32W` whose `dwSize` names its size, as the call
    // requires.
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != FALSE;
    while more {
        processes.push((entry.th32ProcessID, entry.th32ParentProcessID));
        // SAFETY: as for `Process32FirstW`.
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != FALSE;
    }
    processes
}

/// Every process of a ToolHelp snapshot.
pub(super) fn pids() -> Vec<u32> {
    snapshot().into_iter().map(|(pid, _)| pid).collect()
}

/// The parent `pid`'s snapshot entry names, trusted only while that parent still opens and
/// started no later than `pid`: an entry keeps its parent's pid after the parent exits, and
/// Windows may give that pid to a later process.
pub(super) fn parent(pid: u32) -> Option<u32> {
    let (_, parent) = snapshot().into_iter().find(|&(each, _)| each == pid)?;
    let started = start_time(pid)?;
    start_time(parent)
        .is_some_and(|parent_started| parent_started <= started)
        .then_some(parent)
}

/// The creation time `GetProcessTimes` reports, in 100-nanosecond intervals since 1601.
pub(super) fn start_time(pid: u32) -> Option<u64> {
    let process = open(pid)?;
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: the handle is open with the access the call needs, and each out-pointer is a
    // writable `FILETIME`.
    let read =
        unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) }
            != FALSE;
    read.then(|| u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime))
}

/// No documented call reads another process's working directory.
pub(super) fn cwd(_pid: u32) -> Option<PathBuf> {
    None
}

/// The full path of `pid`'s executable (`QueryFullProcessImageNameW`); empty when it cannot be
/// read.
pub(super) fn command(pid: u32) -> String {
    let Some(process) = open(pid) else {
        return String::new();
    };
    let mut name = vec![0u16; MAX_IMAGE_NAME];
    let mut length = name.len() as u32;
    // SAFETY: `name` is `length` writable UTF-16 units; the call writes at most that many and
    // stores the count it wrote in `length`.
    let read = unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            name.as_mut_ptr(),
            &mut length,
        )
    } != FALSE;
    if !read {
        return String::new();
    }
    name.truncate(length as usize);
    String::from_utf16_lossy(&name)
}

/// No documented call reads another process's run state.
pub(super) fn state(_pid: u32) -> Option<char> {
    None
}

/// No documented call reads another process's open files.
pub(super) fn open_files(_pid: u32) -> Vec<PathBuf> {
    Vec::new()
}

/// A Windows process is never a fork: every process starts from an image.
pub(super) fn is_unexeced_fork_of(_pid: u32, _parent: u32) -> bool {
    false
}

/// The owning pid of the first row of IP Helper's IPv4 TCP listener table bound to `port`.
pub(super) fn listening_pid(port: u16) -> Option<u32> {
    let table = listener_table();
    let (&count, rows) = table.split_first()?;
    let words = size_of::<MIB_TCPROW_OWNER_PID>() / size_of::<u32>();
    let count = (count as usize).min(rows.len() / words);
    // SAFETY: the table is a row count followed by its rows, each a `MIB_TCPROW_OWNER_PID` of
    // `u32` fields and so `u32`-aligned like `rows`, and `count` rows lie inside `rows`.
    let rows =
        unsafe { std::slice::from_raw_parts(rows.as_ptr().cast::<MIB_TCPROW_OWNER_PID>(), count) };
    rows.iter()
        .find(|row| u16::from_be(row.dwLocalPort as u16) == port)
        .map(|row| row.dwOwningPid)
}

/// IP Helper's IPv4 TCP listener table with each row's owning pid (`GetExtendedTcpTable`), as
/// `u32` words; empty when it cannot be read.
fn listener_table() -> Vec<u32> {
    let mut table: Vec<u32> = Vec::new();
    let mut size = 0u32;
    loop {
        // SAFETY: `table` is `size` writable bytes (none on the first call, which asks only for
        // the size); the call writes at most `size` bytes and stores the size it needs in `size`.
        let status = unsafe {
            GetExtendedTcpTable(
                table.as_mut_ptr().cast(),
                &mut size,
                FALSE,
                u32::from(AF_INET),
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        match status {
            NO_ERROR => return table,
            ERROR_INSUFFICIENT_BUFFER => {
                table = vec![0; (size as usize).div_ceil(size_of::<u32>())]
            }
            _ => return Vec::new(),
        }
    }
}

/// The processes the Restart Manager reports holding a regular file in `dir` or inside it, this
/// process excluded: one session registers every such file, at most [`FILES_PER_REGISTRATION`]
/// per call, and a failed start, registration or list call answers no holder.
pub(super) fn holders(dir: &Path) -> Vec<u32> {
    let mut files = Vec::new();
    regular_files(dir, &mut files);
    if files.is_empty() {
        return Vec::new();
    }
    let mut session = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `session` is a writable handle and `key` the `CCH_RM_SESSION_KEY + 1` writable
    // units the call requires.
    if unsafe { RmStartSession(&mut session, 0, key.as_mut_ptr()) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let holders = session_holders(session, &files);
    // SAFETY: the session started above and is ended here once.
    unsafe { RmEndSession(session) };
    let me = std::process::id();
    holders.into_iter().filter(|&pid| pid != me).collect()
}

/// The pids `session` lists once every one of `files` (NUL-terminated wide paths) is registered
/// with it.
fn session_holders(session: u32, files: &[Vec<u16>]) -> Vec<u32> {
    for chunk in files.chunks(FILES_PER_REGISTRATION) {
        let names: Vec<PCWSTR> = chunk.iter().map(|file| file.as_ptr()).collect();
        // SAFETY: `names` holds `names.len()` pointers to NUL-terminated wide paths that outlive
        // the call, and no applications or services are registered.
        let registered = unsafe {
            RmRegisterResources(
                session,
                names.len() as u32,
                names.as_ptr(),
                0,
                null(),
                0,
                null(),
            )
        };
        if registered != ERROR_SUCCESS {
            return Vec::new();
        }
    }
    // A list too small for the processes holding a file (none on the first call, which asks only
    // for the count) is grown to the count the call reports and read again.
    let mut list: Vec<RM_PROCESS_INFO> = Vec::new();
    loop {
        let (mut needed, mut count, mut reasons) = (0u32, list.len() as u32, 0u32);
        // SAFETY: `list` is `count` writable entries; the call writes at most that many, stores
        // the count it wrote in `count` and the count it needs in `needed`.
        let status = unsafe {
            RmGetList(
                session,
                &mut needed,
                &mut count,
                list.as_mut_ptr(),
                &mut reasons,
            )
        };
        match status {
            ERROR_SUCCESS => {
                list.truncate(count as usize);
                return list.iter().map(|info| info.Process.dwProcessId).collect();
            }
            ERROR_MORE_DATA => list = vec![RM_PROCESS_INFO::default(); needed as usize],
            _ => return Vec::new(),
        }
    }
}

/// Each regular file in `dir` or inside it, as a NUL-terminated wide path, appended to `files`;
/// a symbolic link is neither followed nor named.
fn regular_files(dir: &Path, files: &mut Vec<Vec<u16>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            regular_files(&path, files);
        } else if kind.is_file() {
            files.push(wide(&path));
        }
    }
}
