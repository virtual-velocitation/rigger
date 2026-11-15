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
/// [`empty_on_interrupt`] was handed before the process ends; dropped, the interrupt ends the
/// process as it did before.
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
        mod native {
            use std::fs::File;
            use std::os::fd::AsRawFd;
            use std::sync::atomic::{AtomicI32, Ordering};

            /// The armed descriptor, `-1` while disarmed.
            static ARMED: AtomicI32 = AtomicI32::new(-1);

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

            pub(super) fn arm(file: &File) {
                ARMED.store(file.as_raw_fd(), Ordering::SeqCst);
                let handler = on_interrupt as extern "C" fn(libc::c_int);
                // SAFETY: the handler calls only async-signal-safe functions.
                unsafe {
                    libc::signal(libc::SIGINT, handler as libc::sighandler_t);
                }
            }

            pub(super) fn disarm() {
                // SAFETY: restoring the default disposition of `SIGINT`.
                unsafe {
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
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
