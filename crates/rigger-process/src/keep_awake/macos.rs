//! The macOS keep-awake adapter (spec 105): an IOKit power assertion that prevents idle system
//! sleep, reached through `extern` declarations linking IOKit and CoreFoundation, so the adapter
//! adds no crate.

use rigger_domain::sleep_hold::{SleepGuard, Unavailable};
use std::ffi::c_void;

/// The assertion type IOKit's `kIOPMAssertPreventUserIdleSystemSleep` names. IOKit spells that
/// name as a `CFSTR` macro, never a symbol a program can link, so the adapter makes the string.
const PREVENT_USER_IDLE_SYSTEM_SLEEP: &str = "PreventUserIdleSystemSleep";

/// IOKit's `kIOPMAssertionLevelOn`: the assertion holds from its creation.
const ASSERTION_LEVEL_ON: u32 = 255;

/// IOKit's `kIOReturnSuccess`.
const IO_RETURN_SUCCESS: i32 = 0;

/// CoreFoundation's `kCFStringEncodingUTF8`.
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: *const c_void,
        level: u32,
        name: *const c_void,
        id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        is_external_representation: u8,
    ) -> *const c_void;
    fn CFRelease(object: *const c_void);
}

/// Assert, under the name `reason`, that the system must not idle-sleep. The guard releases the
/// assertion; the two strings the call takes are released on return, IOKit keeping its own
/// copies.
pub(super) fn acquire(reason: &str) -> Result<SleepGuard, Unavailable> {
    let assertion_type = CfString::new(PREVENT_USER_IDLE_SYSTEM_SLEEP)?;
    let name = CfString::new(reason)?;
    let mut id = 0;
    // SAFETY: both strings are live CoreFoundation strings for the whole call, and `id` is a
    // writable `IOPMAssertionID` the call fills on success.
    let code = unsafe {
        IOPMAssertionCreateWithName(assertion_type.0, ASSERTION_LEVEL_ON, name.0, &mut id)
    };
    if code != IO_RETURN_SUCCESS {
        return Err(Unavailable(format!(
            "IOKit refused the power assertion: IOReturn {code:#010x}"
        )));
    }
    Ok(SleepGuard::new(move || {
        // SAFETY: `id` names the assertion this acquire created, and the guard runs its release
        // once.
        unsafe { IOPMAssertionRelease(id) };
    }))
}

/// A CoreFoundation string this adapter made, released when it drops.
struct CfString(*const c_void);

impl CfString {
    /// The CoreFoundation string of `text`, or unavailable when CoreFoundation makes none.
    fn new(text: &str) -> Result<Self, Unavailable> {
        let length = isize::try_from(text.len())
            .map_err(|_| Unavailable(format!("{} bytes overflow a CFIndex", text.len())))?;
        // SAFETY: `text` is `length` bytes of UTF-8, read for the call alone; a null allocator
        // selects CoreFoundation's default.
        let string = unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                text.as_ptr(),
                length,
                CF_STRING_ENCODING_UTF8,
                0,
            )
        };
        if string.is_null() {
            Err(Unavailable(format!(
                "CoreFoundation made no string of {text:?}"
            )))
        } else {
            Ok(Self(string))
        }
    }
}

impl Drop for CfString {
    fn drop(&mut self) {
        // SAFETY: the string is this value's own, created non-null and released once, here.
        unsafe { CFRelease(self.0) };
    }
}
