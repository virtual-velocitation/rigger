//! Making a rename durable: a rename is an entry in its directory, so the directory itself is
//! synced after one. Each OS gets its own implementation behind [`sync_dir`].

use std::path::Path;

/// Flushes `dir`'s entries to disk, so a rename into it survives a crash.
pub fn sync_dir(dir: &Path) -> std::io::Result<()> {
    settled(imp::sync_dir(dir))
}

/// What a directory sync that answered `synced` means for the rename before it. A filesystem that
/// cannot sync a directory answers InvalidInput or Unsupported; the rename stands, so, as in
/// SQLite, that is not an error. Every other failure is.
fn settled(synced: std::io::Result<()>) -> std::io::Result<()> {
    match synced {
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::InvalidInput | std::io::ErrorKind::Unsupported
            ) =>
        {
            Ok(())
        }
        other => other,
    }
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

    /// A filesystem that cannot sync a directory answers InvalidInput or Unsupported: the rename
    /// before it stands, so that is not an error, as in SQLite. Every other answer is passed on.
    #[test]
    fn a_directory_sync_the_filesystem_cannot_do_is_not_an_error() {
        use std::io::{Error, ErrorKind};
        for (answer, kept) in [
            (None, true),
            (Some(ErrorKind::InvalidInput), true),
            (Some(ErrorKind::Unsupported), true),
            (Some(ErrorKind::PermissionDenied), false),
            (Some(ErrorKind::NotFound), false),
            (Some(ErrorKind::Other), false),
        ] {
            let synced = answer.map_or(Ok(()), |kind| Err(Error::from(kind)));
            assert_eq!(settled(synced).is_ok(), kept, "{answer:?}");
        }
    }
}
