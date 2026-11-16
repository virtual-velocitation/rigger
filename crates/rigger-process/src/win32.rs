//! The conversions this crate's Windows adapters share for the Win32 calls they make.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

/// `path` as the NUL-terminated UTF-16 string the Win32 calls take.
pub(crate) fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}
