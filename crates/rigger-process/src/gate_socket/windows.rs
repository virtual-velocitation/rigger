//! The gate channel's Windows adapter: a named pipe, `\\.\pipe\rigger-gate-` and the repo root's
//! digest ([`pipe_name`]), that only its owner may open and no remote client may reach. The driver
//! serves it with one overlapped instance listening at a time and each connected instance
//! answered on a thread of its own; `rigger gate` asks over it as a plain file. A pipe is gone
//! when its last instance closes, so a driver that died leaves nothing to reclaim.

use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use rigger_domain::gate_channel::{GateChannel, GateReply, GateRequest};
use windows_sys::core::{w, BOOL};
use windows_sys::Win32::Foundation::{
    LocalFree, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_INCOMPLETE,
    ERROR_IO_PENDING, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, FALSE, HANDLE, INVALID_HANDLE_VALUE,
    TRUE, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
    SECURITY_IDENTIFICATION,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, WaitNamedPipeW, NMPWAIT_WAIT_FOREVER, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResultEx, OVERLAPPED};

use super::serving::{answer_one, exchange, served_already, ACCEPT_POLL, REQUEST_READ_TIMEOUT};
use super::{canonical_root_bytes, pipe_name};
use crate::win32::wide;

/// Each direction's buffer, in bytes: the size std gives its own pipes, so a request line is
/// written whole without waiting on the driver's read.
const PIPE_BUFFER: u32 = 64 * 1024;

/// The gate pipe for the project rooted at `repo_root`, the one rule the driver binds by and
/// `rigger gate` asks by when no driver addressed it ([`socket_to_ask`](super::socket_to_ask)):
/// [`pipe_name`] over the canonical repo root. Errors when the repo root does not resolve.
pub fn socket_path(repo_root: &Path) -> io::Result<PathBuf> {
    Ok(PathBuf::from(pipe_name(&canonical_root_bytes(repo_root)?)))
}

/// `wait` in the milliseconds a Win32 wait takes, a wait too long for them being `INFINITE`.
fn millis(wait: Duration) -> u32 {
    u32::try_from(wait.as_millis()).unwrap_or(INFINITE)
}

/// Whether `e` is the Win32 error `code`.
fn is(e: &io::Error, code: u32) -> bool {
    e.raw_os_error() == Some(code as i32)
}

/// `handle` owned from here, or the calling thread's last error when it is `failed`, the value the
/// call that answered it gives on failure.
fn owned(handle: HANDLE, failed: HANDLE) -> io::Result<OwnedHandle> {
    if handle == failed {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the handle is one the call just opened for this process, owned by nothing else.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// One instance of the pipe and the overlapped structure its connect, read or write runs on.
struct Instance {
    pipe: OwnedHandle,
    /// The manual-reset event `overlapped` names, signalled when its operation ends.
    _event: OwnedHandle,
    /// Boxed, so the address the OS writes an operation's result to holds still as the instance
    /// moves.
    overlapped: Box<OVERLAPPED>,
    /// Whether an operation started on `overlapped` has not been seen to end.
    pending: bool,
}

// SAFETY: the instance owns its handles and its overlapped structure alone, and only the thread
// holding it starts, waits for or cancels an operation on them, so moving it to another thread
// moves the operation's whole state with it.
unsafe impl Send for Instance {}

impl Instance {
    /// A new instance of the pipe named `name`, the first of its name when `first`: overlapped,
    /// byte-mode, local clients only, and open to its owner alone.
    fn create(name: &[u16], first: bool) -> io::Result<Instance> {
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: the string is a NUL-terminated wide literal, and `descriptor` a writable pointer
        // the call sets to a descriptor it allocates, freed below.
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                w!("D:P(A;;GA;;;OW)"),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if converted == FALSE {
            return Err(io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: FALSE,
        };
        let first = if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
        // SAFETY: `name` is NUL-terminated, and `attributes` and the descriptor it points to
        // outlive the call, which copies the descriptor into the pipe.
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | first,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                PIPE_BUFFER,
                PIPE_BUFFER,
                0,
                &attributes,
            )
        };
        let pipe = owned(pipe, INVALID_HANDLE_VALUE);
        // SAFETY: the descriptor the conversion allocated, freed once, after the pipe's last error
        // was read.
        unsafe { LocalFree(descriptor) };
        let pipe = pipe?;
        // SAFETY: the call takes no pointer it writes through and answers null on failure.
        let event = owned(
            unsafe { CreateEventW(null(), TRUE, FALSE, null()) },
            null_mut(),
        )?;
        let overlapped = Box::new(OVERLAPPED {
            hEvent: event.as_raw_handle(),
            ..OVERLAPPED::default()
        });
        Ok(Instance {
            pipe,
            _event: event,
            overlapped,
            pending: false,
        })
    }

    /// The pipe handle the Win32 calls take.
    fn raw(&self) -> HANDLE {
        self.pipe.as_raw_handle()
    }

    /// Mark the operation a call answered `started` for in flight, or answer the call's error
    /// when it started none: an operation that ended at once still sets its overlapped result, so
    /// either way [`Instance::settle`] reads it.
    fn begun(&mut self, started: BOOL) -> io::Result<()> {
        if started == FALSE {
            let e = io::Error::last_os_error();
            if !is(&e, ERROR_IO_PENDING) {
                return Err(e);
            }
        }
        self.pending = true;
        Ok(())
    }

    /// Whether a client connected within `ms` milliseconds, starting the instance's connect when
    /// none is in flight.
    fn connected_within(&mut self, ms: u32) -> io::Result<bool> {
        if !self.pending {
            // SAFETY: the pipe is this instance's, and the boxed overlapped structure outlives
            // the connect, which `settle` sees end or `cancel` ends before the instance drops.
            let started = unsafe { ConnectNamedPipe(self.raw(), &mut *self.overlapped) };
            match self.begun(started) {
                Ok(()) => {}
                // A client that opened the instance before the connect started is connected.
                Err(e) if is(&e, ERROR_PIPE_CONNECTED) => return Ok(true),
                Err(e) => return Err(e),
            }
        }
        Ok(self.settle(ms)?.is_some())
    }

    /// The bytes the operation in flight moved, once it ends within `ms` milliseconds; `None`
    /// while it is still in flight.
    fn settle(&mut self, ms: u32) -> io::Result<Option<u32>> {
        let mut moved = 0;
        // SAFETY: the operation in flight was started on this pipe with this overlapped structure,
        // and `moved` is writable.
        let ended =
            unsafe { GetOverlappedResultEx(self.raw(), &*self.overlapped, &mut moved, ms, FALSE) };
        if ended == FALSE {
            let e = io::Error::last_os_error();
            if is(&e, WAIT_TIMEOUT) || is(&e, ERROR_IO_INCOMPLETE) {
                return Ok(None);
            }
            self.pending = false;
            return Err(e);
        }
        self.pending = false;
        Ok(Some(moved))
    }

    /// Wait for the operation in flight to end: the bytes it moved.
    fn finish(&mut self) -> io::Result<u32> {
        loop {
            if let Some(moved) = self.settle(INFINITE)? {
                return Ok(moved);
            }
        }
    }

    /// End the operation in flight, if any, and wait until the OS writes to the overlapped
    /// structure and the buffer it was given no more.
    fn cancel(&mut self) {
        if self.pending {
            // SAFETY: the operation was started on this pipe with this overlapped structure.
            unsafe { CancelIoEx(self.raw(), &*self.overlapped) };
            let _ = self.finish();
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// A connected instance, the stream [`answer_one`] reads the request from and writes the reply to.
struct Connection(Instance);

impl io::Read for Connection {
    /// Read what the client sent, waiting [`REQUEST_READ_TIMEOUT`] at most; the client gone is the
    /// end of the stream.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let instance = &mut self.0;
        let room = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // SAFETY: `buf` is `room` writable bytes, and it and the overlapped structure outlive the
        // read, which `settle` sees end or `cancel` ends before this returns.
        let started = unsafe {
            ReadFile(
                instance.raw(),
                buf.as_mut_ptr(),
                room,
                null_mut(),
                &mut *instance.overlapped,
            )
        };
        let read = instance
            .begun(started)
            .and_then(|()| instance.settle(millis(REQUEST_READ_TIMEOUT)));
        match read {
            Ok(Some(read)) => Ok(read as usize),
            Ok(None) => {
                instance.cancel();
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "the client sent no gate request in time",
                ))
            }
            Err(e) if is(&e, ERROR_BROKEN_PIPE) => Ok(0),
            Err(e) => Err(e),
        }
    }
}

impl io::Write for Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let instance = &mut self.0;
        let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        // SAFETY: `bytes` is `length` readable bytes, and it and the overlapped structure outlive
        // the write, which `finish` sees end before this returns.
        let started = unsafe {
            WriteFile(
                instance.raw(),
                bytes.as_ptr(),
                length,
                null_mut(),
                &mut *instance.overlapped,
            )
        };
        instance.begun(started)?;
        Ok(instance.finish()? as usize)
    }

    /// Nothing to flush: each write ends once the pipe holds its bytes.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A served gate request pipe: the instance the next client connects to, and the name and path
/// it was bound at.
pub struct GateSocket {
    path: PathBuf,
    name: Vec<u16>,
    listening: Mutex<Instance>,
    closed: AtomicBool,
}

impl GateSocket {
    /// Bind the pipe named `path`, the name [`socket_path`] gives, as its first instance: open to
    /// its owner alone and to local clients only. A pipe of that name another process already
    /// serves is refused with `AddrInUse`.
    pub fn bind(path: &Path) -> io::Result<GateSocket> {
        let name = wide(path);
        let listening = Instance::create(&name, true).map_err(|e| {
            if is(&e, ERROR_ACCESS_DENIED) {
                served_already(path)
            } else {
                e
            }
        })?;
        Ok(GateSocket {
            path: path.to_path_buf(),
            name,
            listening: Mutex::new(listening),
            closed: AtomicBool::new(false),
        })
    }
}

impl GateChannel for GateSocket {
    fn serve(&self, answer: &(dyn Fn(GateRequest) -> GateReply + Sync)) -> io::Result<()> {
        let mut listening = self
            .listening
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // The connect is waited for one poll at a time, so the loop sees the flag within one poll
        // whether or not any client connects.
        std::thread::scope(|s| {
            while !self.closed.load(Ordering::SeqCst) {
                let connected = match listening.connected_within(millis(ACCEPT_POLL)) {
                    Ok(false) => continue,
                    Ok(true) => true,
                    // A client gone before its connect ended leaves the instance unusable.
                    Err(_) => false,
                };
                // The next instance listens before this one is handed on, so a client never finds
                // the pipe gone between two connections.
                match Instance::create(&self.name, false) {
                    Ok(next) => {
                        let used = std::mem::replace(&mut *listening, next);
                        if connected {
                            s.spawn(move || answer_one(Connection(used), answer));
                        }
                    }
                    Err(_) => std::thread::sleep(ACCEPT_POLL),
                }
            }
        });
        Ok(())
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// The pipe's name; `None` for a name that is not UTF-8, which no spawn env can carry, so a
    /// spawn's verb derives the same name itself.
    fn address(&self) -> Option<String> {
        self.path.to_str().map(str::to_owned)
    }
}

/// Ask the driver serving the pipe named `path` to answer `request`: its reply, or `None` only
/// when no pipe of that name exists. A pipe whose every instance is busy is waited for; one whose
/// server closed before reading the request, which [`exchange`] answers `None`, is an error here,
/// as is any other failure to open or exchange. The pipe is opened at the identification level, so
/// a server that is not the driver cannot act as this user.
pub fn ask(path: &Path, request: &GateRequest) -> io::Result<Option<GateReply>> {
    loop {
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(path);
        match opened {
            Ok(pipe) => {
                return match exchange(pipe, request)? {
                    Some(reply) => Ok(Some(reply)),
                    None => Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "the driver closed the gate pipe before reading the request",
                    )),
                }
            }
            Err(e) if is(&e, ERROR_FILE_NOT_FOUND) => return Ok(None),
            Err(e) if is(&e, ERROR_PIPE_BUSY) => {
                let name = wide(path);
                // SAFETY: `name` is a NUL-terminated wide string that outlives the call; a pipe
                // gone meanwhile fails the wait, and the next open answers it.
                unsafe { WaitNamedPipeW(name.as_ptr(), NMPWAIT_WAIT_FOREVER) };
            }
            Err(e) => return Err(e),
        }
    }
}
