//! The Windows keep-awake adapter (spec 105): a power request that keeps the system required,
//! through `windows-sys`.

use rigger_domain::sleep_hold::{SleepGuard, Unavailable};
use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Power::{
    PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
};
use windows_sys::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
use windows_sys::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};

/// Create a power request whose reason is `reason` and set it to keep the system required. The
/// guard clears the request and closes its handle; a request that cannot be set is closed before
/// the refusal is returned.
pub(super) fn acquire(reason: &str) -> Result<SleepGuard, Unavailable> {
    let mut wide: Vec<u16> = reason.encode_utf16().chain([0]).collect();
    let context = REASON_CONTEXT {
        Version: POWER_REQUEST_CONTEXT_VERSION,
        Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
        Reason: REASON_CONTEXT_0 {
            SimpleReasonString: wide.as_mut_ptr(),
        },
    };
    // SAFETY: `context` and the NUL-terminated reason it points to outlive the call, which copies
    // the reason into the request.
    let request = unsafe { PowerCreateRequest(&context) };
    if request == INVALID_HANDLE_VALUE {
        return Err(Unavailable(format!(
            "Windows made no power request: {}",
            io::Error::last_os_error()
        )));
    }
    // SAFETY: `request` is the live handle just created.
    if unsafe { PowerSetRequest(request, PowerRequestSystemRequired) } == 0 {
        let refusal = io::Error::last_os_error();
        // SAFETY: the handle is closed once, here, on the path that hands it to no guard.
        unsafe { CloseHandle(request) };
        return Err(Unavailable(format!(
            "Windows refused the system-required power request: {refusal}"
        )));
    }
    Ok(SleepGuard::new(move || {
        // SAFETY: `request` is the handle set above, which the guard alone holds; the guard runs
        // this release once, clearing the request before closing its handle.
        unsafe {
            PowerClearRequest(request, PowerRequestSystemRequired);
            CloseHandle(request);
        }
    }))
}
