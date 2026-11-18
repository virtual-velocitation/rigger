//! The process interface's Windows adapter: the Win32 calls that report on another process - the
//! ToolHelp process snapshot (pids and parents), the process times (start time), the full image
//! name (command), IP Helper's owner-pid TCP listener table ([`listening_pid`]) and the Restart
//! Manager ([`holders`]) - and the one undocumented call it makes, which reads the command line a
//! process was created with ([`created_line`]), split into its arguments by the documented split
//! ([`arguments`]). Each function answers the fact its namesake in
//! [`super`] documents; a call that fails - the process exited, or this process may not open it -
//! answers absent.
//! Windows documents no call that reads another process's working directory or open files, so
//! [`cwd`] answers `None` and [`open_files`] empty; of a process's run state it reports only that
//! the process has exited ([`state`]); and with no fork [`is_unexeced_fork_of`] answers false.

use std::path::{Path, PathBuf};
use std::ptr::null;

use windows_sys::core::PCWSTR;
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessCommandLineInformation,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA, ERROR_SUCCESS, FALSE,
    FILETIME, HANDLE, INVALID_HANDLE_VALUE, NO_ERROR, UNICODE_STRING, WAIT_OBJECT_0,
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
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
    PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

use crate::win32::wide;

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

/// Process `pid` opened with `access`, or `None` when it cannot be opened (it is gone, or this
/// process may not open it).
fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Option<Owned> {
    // SAFETY: the call takes no pointer and answers a null handle on failure.
    let handle = unsafe { OpenProcess(access, FALSE, pid) };
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

/// Each process of ONE ToolHelp snapshot with the parent its entry names and its [`start_time`];
/// the walk over it, not this listing, judges whether that parent is still the one that started
/// the process.
pub(super) fn table() -> Vec<super::Entry> {
    snapshot()
        .into_iter()
        .filter_map(|(pid, parent)| {
            Some(super::Entry {
                pid,
                parent,
                start: start_time(pid)?,
            })
        })
        .collect()
}

/// The parent `pid`'s snapshot entry names, while [`super::trusts_parent`] trusts it: that parent
/// still opens and started before `pid` ([`super::TIES`]: a tie is no parent here).
pub(super) fn parent(pid: u32) -> Option<u32> {
    let (_, parent) = snapshot().into_iter().find(|&(each, _)| each == pid)?;
    let started = start_time(pid)?;
    super::trusts_parent(super::TIES, start_time(parent), started).then_some(parent)
}

/// The creation time `GetProcessTimes` reports, in 100-nanosecond intervals since 1601.
pub(super) fn start_time(pid: u32) -> Option<u64> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    created(process.0)
}

/// The creation time of the process `process` holds ([`start_time`]), read through that handle,
/// which this process opened with at least `PROCESS_QUERY_LIMITED_INFORMATION` and keeps open
/// across the call: a handle keeps its process's pid that process's own while it is open, so the
/// time read is the time of the process the handle was opened on.
pub(crate) fn created(process: HANDLE) -> Option<u64> {
    let (mut creation, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: the handle is open with the access the call needs, and each out-pointer is a
    // writable `FILETIME`.
    let read =
        unsafe { GetProcessTimes(process, &mut creation, &mut exited, &mut kernel, &mut user) }
            != FALSE;
    read.then(|| u64::from(creation.dwHighDateTime) << 32 | u64::from(creation.dwLowDateTime))
}

/// No documented call reads another process's working directory.
pub(super) fn cwd(_pid: u32) -> Option<PathBuf> {
    None
}

/// The full path of `pid`'s executable (`QueryFullProcessImageNameW`); empty when it cannot be
/// read.
pub(super) fn command(pid: u32) -> String {
    let Some(process) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
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

/// The arguments `pid` was created with: the command line it was created with
/// ([`created_line`]), split into its arguments ([`split_command_line`]).
pub(super) fn arguments(pid: u32) -> Option<Vec<String>> {
    split_command_line(&created_line(pid)?)
}

/// `line`, a command line as a process is created with, split into its arguments, its program
/// first, by `CommandLineToArgvW`, the split Windows documents for a command line: an argument
/// holding a space, or nothing, is one in double quotes, a quote inside one follows a backslash,
/// and backslashes before a quote are doubled. `None` for an empty line, which the call would
/// answer with this process's own path, and when the call fails.
pub(super) fn split_command_line(line: &[u16]) -> Option<Vec<String>> {
    if line.is_empty() {
        return None;
    }
    let ended: Vec<u16> = line.iter().copied().chain([0]).collect();
    let mut count = 0i32;
    // SAFETY: `ended` is a NUL-terminated wide string that outlives the call, and `count` is a
    // writable `i32`.
    let list = unsafe { CommandLineToArgvW(ended.as_ptr(), &mut count) };
    if list.is_null() {
        return None;
    }
    let arguments = (0..usize::try_from(count).unwrap_or_default())
        .map(|index| {
            // SAFETY: the call answered `count` pointers, each to a NUL-terminated wide string,
            // all in the one allocation `list` names until it is freed below.
            unsafe {
                let word = *list.add(index);
                let length = (0..).take_while(|&at| *word.add(at) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(word, length))
            }
        })
        .collect();
    // SAFETY: `list` is the one allocation the call answered, which its caller frees with
    // `LocalFree`, and nothing reads it after.
    unsafe { LocalFree(list.cast()) };
    Some(arguments)
}

/// The command line `pid` was created with, read by `NtQueryInformationProcess` with
/// `ProcessCommandLineInformation`, the call Task Manager reads it by and the one Windows offers:
/// no documented call reads another process's command line. The call answers a `UNICODE_STRING`
/// naming the line, followed by the line itself, into a buffer given room for the longest line a
/// `UNICODE_STRING` can name, so one read answers it. `None` when the process cannot be opened,
/// the call fails (a Windows that drops the class among its causes), the structure names bytes
/// outside the buffer ([`super::command_line_span`]), or the line is empty.
fn created_line(pid: u32) -> Option<Vec<u16>> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let header = size_of::<UNICODE_STRING>();
    // Words of eight bytes, so the buffer is aligned for the structure at its start.
    let mut buffer = vec![0u64; (header + usize::from(u16::MAX) + 1).div_ceil(8)];
    let size = buffer.len() * size_of::<u64>();
    // SAFETY: `buffer` is `size` writable bytes, aligned for the structure the class names; the
    // call writes at most that many, and no returned length is asked for.
    let status = unsafe {
        NtQueryInformationProcess(
            process.0,
            ProcessCommandLineInformation,
            buffer.as_mut_ptr().cast(),
            size as u32,
            std::ptr::null_mut(),
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: `buffer` holds at least the structure and is aligned for it.
    let line = unsafe { buffer.as_ptr().cast::<UNICODE_STRING>().read() };
    // SAFETY: `buffer` is `size` initialized bytes, read here as bytes.
    let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), size) };
    let span = super::command_line_span(
        buffer.as_ptr() as usize,
        size,
        header,
        line.Buffer as usize,
        usize::from(line.Length),
    )?;
    let units: Vec<u16> = bytes[span]
        .chunks_exact(2)
        .map(|unit| u16::from_ne_bytes([unit[0], unit[1]]))
        .collect();
    (!units.is_empty()).then_some(units)
}

/// `Z` once `pid` has exited while a handle still holds it: a process handle is signalled when
/// the process exits, so a wait on it for no time (`WaitForSingleObject`) answers
/// `WAIT_OBJECT_0`. `None` while it runs, or when it cannot be opened; Windows documents no call
/// that reads another process's other run states.
pub(super) fn state(pid: u32) -> Option<char> {
    let process = open(pid, PROCESS_SYNCHRONIZE)?;
    // SAFETY: the handle is open with the synchronize right a wait needs, and a zero timeout
    // returns at once.
    let exited = unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0;
    exited.then_some('Z')
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
    let (&claimed, rows) = table.split_first()?;
    let row_words = size_of::<MIB_TCPROW_OWNER_PID>() / size_of::<u32>();
    let count = super::whole_rows(claimed, rows.len(), row_words);
    // SAFETY: the table is a row count followed by its rows, each a `MIB_TCPROW_OWNER_PID` of
    // `u32` fields and so `u32`-aligned like `rows`, and `super::whole_rows` caps `count` at the
    // whole rows that lie inside `rows`.
    let rows =
        unsafe { std::slice::from_raw_parts(rows.as_ptr().cast::<MIB_TCPROW_OWNER_PID>(), count) };
    rows.iter()
        .find(|row| super::port_of(row.dwLocalPort) == port)
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

/// The path the OS reports for the image this process runs.
pub(super) use super::command_file_name as executable_name;
pub(super) use super::reported_image as running_image;

/// The processes the Restart Manager reports holding a regular file in `dir` or inside it, this
/// process excluded: one session registers every such file, at most
/// [`FILES_PER_REGISTRATION`](super::FILES_PER_REGISTRATION) per call
/// ([`super::registrations`]), and a failed start, registration or list call answers no holder.
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
    for chunk in super::registrations(files) {
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
