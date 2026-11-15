//! THE INTERRUPT TRIGGER: what an operator's interactive interrupt of this process (Ctrl-C, or
//! Ctrl-Break on Windows) does besides ending it, through the native implementation of the OS
//! the binary is built for. A driver empties the record of itself it keeps in its step lock as it
//! is interrupted, so a run the operator stopped on purpose is never relaunched as though its
//! driver had died.
//!
//! The process still ends as an interrupt ends it, with the interrupt's conventional status; only
//! the one file is emptied first. Nothing here sends a signal: it only answers the one this
//! process receives.

use std::fs::File;

/// While this lives, an interactive interrupt of this process empties the file
/// [`empty_on_interrupt`] was handed before the process ends; dropped, the interrupt does what it
/// did before. An interrupt the process was started ignoring stays ignored.
#[must_use = "the trigger is removed when the guard drops"]
pub struct InterruptTrigger(());

/// Arm the trigger over `file`, which must stay open for as long as the returned guard lives: an
/// interactive interrupt of this process empties `file`, then ends the process. One trigger is
/// armed at a time; arming another replaces it.
pub fn empty_on_interrupt(file: &File) -> InterruptTrigger {
    native::arm(file);
    InterruptTrigger(())
}

impl Drop for InterruptTrigger {
    fn drop(&mut self) {
        native::disarm();
    }
}

std::cfg_select! {
    unix => {
        /// The unix trigger: a `SIGINT` handler that truncates the armed descriptor and ends the
        /// process with status 130, calling only the async-signal-safe `ftruncate` and `_exit`.
        /// A process started ignoring `SIGINT` (a background start by a shell without job
        /// control) is never ended by one, so it is left ignoring it and nothing is armed.
        mod native {
            use std::fs::File;
            use std::os::fd::AsRawFd;
            use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

            /// The armed descriptor, `-1` while disarmed.
            static ARMED: AtomicI32 = AtomicI32::new(-1);
            /// The disposition `arm` replaced, which `disarm` restores; `SIG_ERR` while nothing
            /// is replaced.
            static REPLACED: AtomicUsize = AtomicUsize::new(libc::SIG_ERR);

            extern "C" fn on_interrupt(_signal: libc::c_int) {
                let fd = ARMED.load(Ordering::SeqCst);
                // SAFETY: `ftruncate` and `_exit` are async-signal-safe, and `fd` is the armed
                // descriptor, open for as long as the trigger is armed.
                unsafe {
                    if fd >= 0 {
                        libc::ftruncate(fd, 0);
                    }
                    libc::_exit(130);
                }
            }

            /// The current disposition of `SIGINT`, read without changing it.
            pub(super) fn current() -> libc::sighandler_t {
                // SAFETY: a query: a null new action changes nothing, and `old` is a valid
                // out-pointer.
                unsafe {
                    let mut old: libc::sigaction = std::mem::zeroed();
                    libc::sigaction(libc::SIGINT, std::ptr::null(), &mut old);
                    old.sa_sigaction
                }
            }

            pub(super) fn arm(file: &File) {
                if current() == libc::SIG_IGN {
                    return;
                }
                ARMED.store(file.as_raw_fd(), Ordering::SeqCst);
                let handler = on_interrupt as extern "C" fn(libc::c_int);
                // SAFETY: the handler calls only async-signal-safe functions.
                let replaced = unsafe { libc::signal(libc::SIGINT, handler as libc::sighandler_t) };
                // A second arm keeps the disposition the first one replaced.
                let _ = REPLACED.compare_exchange(
                    libc::SIG_ERR,
                    replaced,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                );
            }

            pub(super) fn disarm() {
                let replaced = REPLACED.swap(libc::SIG_ERR, Ordering::SeqCst);
                if replaced != libc::SIG_ERR {
                    // SAFETY: restoring the disposition `arm` replaced.
                    unsafe {
                        libc::signal(libc::SIGINT, replaced);
                    }
                }
                ARMED.store(-1, Ordering::SeqCst);
            }
        }
    }
    windows => {
        /// The Windows trigger: a console control handler that, for Ctrl-C and Ctrl-Break,
        /// empties the armed handle's file and then lets the default handler end the process.
        mod native {
            use std::fs::File;
            use std::mem::ManuallyDrop;
            use std::os::windows::io::{AsRawHandle, FromRawHandle};
            use std::sync::atomic::{AtomicPtr, Ordering};
            use windows_sys::core::BOOL;
            use windows_sys::Win32::System::Console::{
                SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT,
            };

            /// The armed handle, null while disarmed.
            static ARMED: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

            unsafe extern "system" fn on_control(event: u32) -> BOOL {
                let handle = ARMED.load(Ordering::SeqCst);
                if (event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT) && !handle.is_null() {
                    // SAFETY: the armed handle stays open while the trigger is armed; the
                    // `ManuallyDrop` keeps this borrowed view from closing it.
                    let file = ManuallyDrop::new(unsafe { File::from_raw_handle(handle) });
                    let _ = file.set_len(0);
                }
                0
            }

            pub(super) fn arm(file: &File) {
                ARMED.store(file.as_raw_handle(), Ordering::SeqCst);
                // SAFETY: registering a handler that only empties the armed file.
                unsafe {
                    SetConsoleCtrlHandler(Some(on_control), 1);
                }
            }

            pub(super) fn disarm() {
                // SAFETY: removing the handler `arm` registered.
                unsafe {
                    SetConsoleCtrlHandler(Some(on_control), 0);
                }
                ARMED.store(std::ptr::null_mut(), Ordering::SeqCst);
            }
        }
    }
    _ => {
        /// The OSes with no interrupt trigger: an interrupt ends the process as it always did.
        mod native {
            pub(super) fn arm(_file: &std::fs::File) {}
            pub(super) fn disarm() {}
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// An interrupt the process was started ignoring (a background start by a shell without job
    /// control) stays ignored, so the driver it would not have ended keeps running and its record;
    /// any other is answered while the trigger lives, and the disposition the trigger replaced
    /// comes back when it drops.
    #[test]
    fn an_ignored_interrupt_stays_ignored_and_any_other_is_answered_until_the_trigger_drops() {
        let file = tempfile::tempfile().unwrap();
        let rows = [
            ("started ignoring interrupts", libc::SIG_IGN, false),
            ("started with the default", libc::SIG_DFL, true),
        ];
        for (case, inherited, answered) in rows {
            // SAFETY: setting `SIGINT` to a standard disposition; no other test here touches it.
            unsafe {
                libc::signal(libc::SIGINT, inherited);
            }
            let trigger = empty_on_interrupt(&file);
            let armed = native::current();
            drop(trigger);
            assert_eq!(
                (armed != inherited, native::current()),
                (answered, inherited),
                "{case}: whether the trigger answers the interrupt, and what its drop leaves"
            );
        }
        // SAFETY: restoring the default disposition.
        unsafe {
            libc::signal(libc::SIGINT, libc::SIG_DFL);
        }
    }
}
