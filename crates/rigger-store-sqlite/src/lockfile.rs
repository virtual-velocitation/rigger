//! THE LOCK FILE: the one guard every rigger advisory lock is held through - the step lock
//! `rigger run` holds for its whole life and the graph rebuild lock alike.
//!
//! A lock is the OS advisory lock on a lock file, taken without waiting, and released when its
//! [`HeldLock`] drops - explicitly, never by closing its descriptor alone - or when its process
//! ends however it ends, so a holder that dies never leaves a stale lock.
//!
//! A lock file may carry what its holder says of itself ([`HeldLock::contents`],
//! [`HeldLock::replace`]), read and written only through the held lock, so no reader ever sees a
//! half-written record and the read and the write go through the one handle that holds the lock
//! on every OS.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// An exclusive advisory lock on a lock file, held for as long as this lives.
#[derive(Debug)]
pub struct HeldLock {
    /// The locked file, held open for as long as the lock is.
    file: File,
}

impl HeldLock {
    /// Take the exclusive lock on the lock file at `path` without waiting; the file is created if
    /// absent and never truncated. `Ok(None)` while another holder has it; a failure to open or to
    /// lock the file is the error, never read as "held".
    pub fn try_take(path: &Path) -> io::Result<Option<HeldLock>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(HeldLock { file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    /// The locked file, for a caller that needs a copy of its descriptor.
    pub fn file(&self) -> &File {
        &self.file
    }

    /// What the lock file holds, read whole through the held lock.
    pub fn contents(&self) -> io::Result<String> {
        let mut file = &self.file;
        file.seek(SeekFrom::Start(0))?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        Ok(text)
    }

    /// Make the lock file hold `text` and nothing else, written through the held lock and synced
    /// to disk, so what it says outlives a power loss the moment this returns. An empty `text`
    /// empties the file.
    pub fn replace(&self, text: &str) -> io::Result<()> {
        let mut file = &self.file;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(text.as_bytes())?;
        file.sync_data()
    }
}

impl Drop for HeldLock {
    /// Release the lock explicitly ([`File::unlock`]) before the lock file's descriptor closes.
    /// The OS lock ([`File::try_lock`], `flock`) belongs to the lock file's OPEN FILE DESCRIPTION,
    /// not to this descriptor, so closing this descriptor releases it only once no other
    /// descriptor shares that description - and a child process forked by any thread of this
    /// process holds a copy of every descriptor from its fork until its exec closes the
    /// close-on-exec ones. Closing alone inside that window would leave the lock held by the
    /// child's copy, and the next take would be refused as held. Unlocking releases the
    /// description's lock whatever copies of it stand. A failed unlock is ignored: closing the
    /// descriptor, and the process ending, still release the lock.
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// While one holder has the lock, a second take is refused without waiting; once the holder
    /// drops it - even while a dup of its descriptor stays open, the copy a child forked by any
    /// thread of this process holds until it execs - the next take succeeds at once.
    #[test]
    fn a_held_lock_refuses_a_second_take_and_is_free_at_once_when_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let held = HeldLock::try_take(&path).unwrap().expect("the first take");
        assert!(
            HeldLock::try_take(&path).unwrap().is_none(),
            "a second take is refused while the lock is held"
        );
        let forked_copy = held.file().try_clone().unwrap();
        drop(held);
        let next = HeldLock::try_take(&path).unwrap();
        drop(forked_copy);
        assert!(
            next.is_some(),
            "the dropped lock is released, whatever other descriptor shares its description"
        );
    }

    /// What a holder writes through its lock is what the next holder reads, a shorter text leaving
    /// nothing of a longer one behind and an empty one emptying the file, and the lock file a
    /// holder finds keeps what the last holder left.
    #[test]
    fn a_held_lock_reads_back_what_its_holder_wrote_and_the_next_holder_finds_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let held = HeldLock::try_take(&path).unwrap().unwrap();
        assert_eq!(held.contents().unwrap(), "");
        for text in ["a longer record", "short", ""] {
            held.replace(text).unwrap();
            assert_eq!(
                held.contents().unwrap(),
                text,
                "the holder reads back {text:?}"
            );
        }
        held.replace("left behind").unwrap();
        drop(held);
        let next = HeldLock::try_take(&path).unwrap().unwrap();
        assert_eq!(next.contents().unwrap(), "left behind");
    }

    /// A lock file that cannot be opened - a directory stands at its path - is an error, never a
    /// lock read as held by another.
    #[test]
    fn a_lock_file_that_cannot_be_opened_is_an_error_never_held() {
        let dir = tempfile::tempdir().unwrap();
        assert!(HeldLock::try_take(dir.path()).is_err());
    }
}
