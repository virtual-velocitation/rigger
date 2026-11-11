//! Making a rename durable: a rename is an entry in its directory, so the directory itself is
//! synced after one. Each OS gets its own implementation behind [`sync_dir`].

use std::path::Path;

/// Flushes `dir`'s entries to disk, so a rename into it survives a crash.
pub fn sync_dir(dir: &Path) -> std::io::Result<()> {
    imp::sync_dir(dir)
}

/// Unix: a directory opens like a file, and its fsync flushes its entries.
#[cfg(unix)]
mod imp {
    use std::path::Path;

    pub fn sync_dir(dir: &Path) -> std::io::Result<()> {
        std::fs::File::open(dir)?.sync_all()
    }
}

/// Windows has no directory fsync (NTFS journals the rename), and other targets offer none: a
/// rename lost to a crash only leaves the previous file current, which the next write repairs.
#[cfg(not(unix))]
mod imp {
    use std::path::Path;

    pub fn sync_dir(_dir: &Path) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncing_a_directory_succeeds_and_a_missing_one_fails_on_unix() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(sync_dir(dir.path()).is_ok());
        let missing = dir.path().join("absent");
        assert_eq!(sync_dir(&missing).is_err(), cfg!(unix));
    }
}
