//! The macOS wake of a watched tree: a kqueue that wakes the watcher when a process it watches
//! forks or exits (`EVFILT_PROC` with `NOTE_FORK | NOTE_EXIT`), and when the watcher is stopped,
//! at the end of a pipe whose only write end the [`Stopper`] holds (`EVFILT_READ`). A wake only
//! starts a walk: which processes are the tree's is read from the process table, never from the
//! event.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

/// How many events one wait reads; the next wait reads any still pending.
const EVENTS: usize = 32;

/// What a watcher waits on: the kqueue, and the read end of the stop's pipe.
pub(super) struct Wake {
    queue: OwnedFd,
    stop: io::PipeReader,
}

/// The watcher's stop: dropping it closes the pipe's only write end, which wakes the watcher.
pub(super) struct Stopper {
    _writer: io::PipeWriter,
}

/// One kevent: `filter` on `ident` with `flags` and `fflags`.
fn event(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent {
        ident,
        filter,
        flags,
        fflags,
        data: 0,
        udata: std::ptr::null_mut(),
    }
}

impl Wake {
    /// A kqueue that wakes at the end of a new pipe, and the stop that holds the pipe's write end.
    pub(super) fn new() -> io::Result<(Wake, Stopper)> {
        // SAFETY: `kqueue` takes no argument and answers a new descriptor, or -1.
        let raw = unsafe { libc::kqueue() };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `raw` is the descriptor `kqueue` just opened, which nothing else owns.
        let queue = unsafe { OwnedFd::from_raw_fd(raw) };
        let (stop, writer) = io::pipe()?;
        let wake = Wake { queue, stop };
        wake.add(event(
            wake.stop.as_raw_fd() as usize,
            libc::EVFILT_READ,
            libc::EV_ADD | libc::EV_CLEAR,
            0,
        ))?;
        Ok((wake, Stopper { _writer: writer }))
    }

    /// Registers `change` with the kqueue.
    fn add(&self, change: libc::kevent) -> io::Result<()> {
        // SAFETY: one change is read from `change`, which lives across the call; no event is
        // written and no timeout is read.
        let answer = unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                &change,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if answer < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Wakes the watcher when `pid` forks or exits. A process that has already exited, or that
    /// this process may not watch, is left unwatched: the next timed walk still reads it.
    pub(super) fn watch(&mut self, pid: u32) {
        let _ = self.add(event(
            pid as usize,
            libc::EVFILT_PROC,
            libc::EV_ADD | libc::EV_CLEAR,
            libc::NOTE_FORK | libc::NOTE_EXIT,
        ));
    }

    /// Waits up to `timeout` for a watched process to fork or exit; false once the stop is
    /// dropped. A wait the kqueue fails waits out `timeout` instead, so a failing kqueue never
    /// turns the watcher into a busy loop.
    pub(super) fn wait(&mut self, timeout: Duration) -> bool {
        let limit = libc::timespec {
            tv_sec: libc::time_t::try_from(timeout.as_secs()).unwrap_or(libc::time_t::MAX),
            tv_nsec: libc::c_long::from(timeout.subsec_nanos()),
        };
        let mut events = [event(0, 0, 0, 0); EVENTS];
        // SAFETY: no change is read; at most `EVENTS` events are written into `events`, which
        // lives across the call, and `limit` is read.
        let count = unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                EVENTS as libc::c_int,
                &limit,
            )
        };
        let Ok(count) = usize::try_from(count) else {
            std::thread::sleep(timeout);
            return true;
        };
        let stop = self.stop.as_raw_fd() as usize;
        !events[..count.min(EVENTS)]
            .iter()
            .any(|event| event.filter == libc::EVFILT_READ && event.ident == stop)
    }
}
