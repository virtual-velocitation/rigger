//! Reading a published store file without copying it: mapped read-only where the OS maps files,
//! read where it does not. Each OS gets its own implementation behind [`Published::open`].
//!
//! Mapping is sound because a published file is never written in place. The store publishes a
//! file by writing a temp file and renaming it over a name ([`crate::format::write_atomic`]), and
//! removes one only by unlinking it: nothing writes, truncates or renames over a published file's
//! bytes while it is mapped. An unlinked file stays mapped whole until the mapping drops, and an
//! OS that refuses to unlink a mapped file leaves it to the next sweep, which lists the directory
//! afresh. A file that cannot be mapped (an OS without mapping, or a filesystem that does not
//! map) is read instead.

use std::ops::Deref;
use std::path::Path;

/// A store file's bytes: built in memory, read, or a published file mapped read-only.
pub struct Published(Held);

enum Held {
    Owned(Vec<u8>),
    Mapped(imp::Mapping),
}

impl Published {
    /// The published file at `path`, mapped where the OS can, else read.
    pub(crate) fn open(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mapped = imp::map(&file);
        settled(mapped, file)
    }

    /// Whether the bytes are a mapping of the file rather than a copy.
    pub fn is_mapped(&self) -> bool {
        matches!(self.0, Held::Mapped(_))
    }
}

/// The bytes of `file` given what mapping it answered: the mapping, else the file read whole.
fn settled(
    mapped: std::io::Result<imp::Mapping>,
    mut file: std::fs::File,
) -> std::io::Result<Published> {
    match mapped {
        Ok(map) => Ok(Published(Held::Mapped(map))),
        Err(_) => {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut bytes)?;
            Ok(Published(Held::Owned(bytes)))
        }
    }
}

impl From<Vec<u8>> for Published {
    fn from(bytes: Vec<u8>) -> Self {
        Self(Held::Owned(bytes))
    }
}

impl Deref for Published {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match &self.0 {
            Held::Owned(bytes) => bytes,
            Held::Mapped(map) => map,
        }
    }
}

/// Unix and Windows map a file read-only (mmap, MapViewOfFile).
#[cfg(any(unix, windows))]
mod imp {
    pub type Mapping = memmap2::Mmap;

    pub fn map(file: &std::fs::File) -> std::io::Result<Mapping> {
        // SAFETY: only a published store file is mapped, and a published file is never written,
        // truncated or renamed over in place (the module doc): the mapped bytes cannot change or
        // shrink while the mapping lives.
        unsafe { memmap2::Mmap::map(file) }
    }
}

/// Other targets offer no mapping: every file is read.
#[cfg(not(any(unix, windows)))]
mod imp {
    use std::ops::Deref;

    /// No mapping exists on these targets.
    pub enum Mapping {}

    impl Deref for Mapping {
        type Target = [u8];
        fn deref(&self) -> &[u8] {
            match *self {}
        }
    }

    pub fn map(_file: &std::fs::File) -> std::io::Result<Mapping> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A published file opens mapped where the OS maps files, and its bytes are the file's.
    #[test]
    fn a_published_file_opens_mapped_where_the_os_maps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("000001.snap");
        std::fs::write(&path, b"the published bytes").unwrap();
        let opened = Published::open(&path).unwrap();
        assert_eq!(opened.is_mapped(), cfg!(any(unix, windows)));
        assert_eq!(&*opened, b"the published bytes");
    }

    /// A file the OS cannot map (an answer of Unsupported, or any other failure) is read whole.
    #[test]
    fn a_file_that_cannot_be_mapped_is_read() {
        use std::io::{Error, ErrorKind};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("000001.snap");
        std::fs::write(&path, b"the published bytes").unwrap();
        for kind in [ErrorKind::Unsupported, ErrorKind::Other] {
            let file = std::fs::File::open(&path).unwrap();
            let read = settled(Err(Error::from(kind)), file).unwrap();
            assert!(!read.is_mapped(), "{kind:?}: read");
            assert_eq!(&*read, b"the published bytes", "{kind:?}");
        }
    }
}
