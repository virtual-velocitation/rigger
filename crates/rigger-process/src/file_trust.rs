//! The facts about a file git weighs before it trusts a repository it finds by walking up from a
//! directory: who owns the file, and which device holds it. Each is answered per OS; an OS whose
//! answer this module does not read answers none, and a caller then asks git itself.

use std::path::Path;

/// Whether this process's effective user owns `path` (not following a final symlink); none when
/// it cannot be read or this OS's ownership is not a user id.
#[cfg(unix)]
pub fn owned_by_this_user(path: &Path) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some(meta.uid() == rustix::process::geteuid().as_raw())
}

/// Whether this process's effective user owns `path`: none, since ownership here is not read.
#[cfg(not(unix))]
pub fn owned_by_this_user(_path: &Path) -> Option<bool> {
    None
}

/// The device holding `path`; none when it cannot be read or this OS names no device.
#[cfg(unix)]
pub fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|meta| meta.dev())
}

/// The device holding `path`: none, since devices are not read here.
#[cfg(not(unix))]
pub fn device_of(_path: &Path) -> Option<u64> {
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A file this process makes is its own, on the device its directory is on; a missing path
    /// answers none.
    #[test]
    fn a_file_this_process_makes_is_its_own_on_its_directorys_device() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(owned_by_this_user(&file), Some(true));
        assert_eq!(device_of(&file), device_of(dir.path()));
        assert!(device_of(&file).is_some());
        assert_eq!(owned_by_this_user(&dir.path().join("missing")), None);
        assert_eq!(device_of(&dir.path().join("missing")), None);
    }
}
