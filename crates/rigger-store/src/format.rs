//! The owned binary format every store file is written in, and the two disciplines that publish
//! one: an atomic write (temp, fsync, rename, directory sync) and an exclusive write lock.
//!
//! A file is a container: a header that names its kind (magic) and format version, a payload the
//! kind defines, then a table of sections, each with its own crc32. All integers are
//! little-endian with fixed widths, so a file reads the same on every platform.
//!
//! ```text
//! magic [4] | u16 version | u16 section count | u32 payload length | payload | u32 crc32
//! [u16 kind | u64 offset | u64 length | u32 crc32] x section count
//! section bytes ...
//! ```
//!
//! The header crc covers everything before it. A section's crc covers its bytes; its offset is
//! from the start of the file.

use crate::sync_dir::sync_dir;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// The CRC-32 (IEEE 802.3, reflected, polynomial 0xEDB88320) of `bytes`: folded with the CPU's
/// carry-less multiply or crc instructions where it has them, else read through tables
/// (`crc32fast` chooses at run time), so a large file's check costs a fraction of a table walk.
pub fn crc32(bytes: &[u8]) -> u32 {
    crc32fast::hash(bytes)
}

pub fn put_u8(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}

pub fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A bounds-checked little-endian reader over a byte slice: every read past the end is an
/// error, never a panic.
pub struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    /// The next `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| format!("truncated: {n} bytes wanted at offset {}", self.at))?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16, String> {
        self.array().map(u16::from_le_bytes)
    }

    pub fn u32(&mut self) -> Result<u32, String> {
        self.array().map(u32::from_le_bytes)
    }

    pub fn u64(&mut self) -> Result<u64, String> {
        self.array().map(u64::from_le_bytes)
    }

    /// How many bytes have been read.
    pub fn position(&self) -> usize {
        self.at
    }

    pub fn is_empty(&self) -> bool {
        self.at == self.bytes.len()
    }
}

/// `n` as the fixed-width field type `T` that stores it, or an error naming `what` when it does
/// not fit: a writer never wraps a count, length or offset into a smaller one.
pub fn narrow<T: TryFrom<usize>>(n: usize, what: &str) -> Result<T, String> {
    T::try_from(n).map_err(|_| {
        format!(
            "{what} {n} does not fit its {}-byte field",
            std::mem::size_of::<T>()
        )
    })
}

/// The size of one section-table row: kind, offset, length, crc.
const TABLE_ROW: usize = 2 + 8 + 8 + 4;

/// The container bytes for `payload` and `sections`, in the order given.
pub fn write_container(
    magic: &[u8; 4],
    version: u16,
    payload: &[u8],
    sections: &[(u16, Vec<u8>)],
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(magic);
    put_u16(&mut out, version);
    put_u16(&mut out, narrow(sections.len(), "section count")?);
    put_u32(&mut out, narrow(payload.len(), "payload length")?);
    out.extend_from_slice(payload);
    let crc = crc32(&out);
    put_u32(&mut out, crc);
    let mut offset: u64 = narrow(out.len() + sections.len() * TABLE_ROW, "section offset")?;
    for (kind, bytes) in sections {
        let length: u64 = narrow(bytes.len(), "section length")?;
        put_u16(&mut out, *kind);
        put_u64(&mut out, offset);
        put_u64(&mut out, length);
        put_u32(&mut out, crc32(bytes));
        offset = offset
            .checked_add(length)
            .ok_or_else(|| "section offset overflows its 8-byte field".to_string())?;
    }
    for (_, bytes) in sections {
        out.extend_from_slice(bytes);
    }
    Ok(out)
}

/// A parsed container whose header and every section passed their crc.
#[derive(Debug)]
pub struct Container<'a> {
    pub payload: &'a [u8],
    bytes: &'a [u8],
    sections: Vec<(u16, Range<usize>)>,
}

impl<'a> Container<'a> {
    /// Parses `bytes`, which must carry `magic` and `version`, verifying every crc.
    pub fn parse(bytes: &'a [u8], magic: &[u8; 4], version: u16) -> Result<Self, String> {
        let mut cur = Cursor::new(bytes);
        if cur.take(4)? != magic {
            return Err("not this kind of file: the magic differs".into());
        }
        let found = cur.u16()?;
        if found != version {
            return Err(format!("format version {found}, expected {version}"));
        }
        let count = cur.u16()?;
        let len = cur.u32()? as usize;
        let payload = cur.take(len)?;
        let covered = &bytes[..cur.position()];
        if cur.u32()? != crc32(covered) {
            return Err("the header crc differs".into());
        }
        let mut sections: Vec<(u16, Range<usize>)> = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let kind = cur.u16()?;
            let offset = usize::try_from(cur.u64()?).map_err(|e| e.to_string())?;
            let length = usize::try_from(cur.u64()?).map_err(|e| e.to_string())?;
            let crc = cur.u32()?;
            let range = offset
                .checked_add(length)
                .filter(|&end| end <= bytes.len())
                .map(|end| offset..end)
                .ok_or_else(|| format!("section {kind} lies outside the file"))?;
            if crc32(&bytes[range.clone()]) != crc {
                return Err(format!("section {kind}'s crc differs"));
            }
            if sections.iter().any(|(k, _)| *k == kind) {
                return Err(format!("section {kind} appears twice"));
            }
            sections.push((kind, range));
        }
        Ok(Self {
            payload,
            bytes,
            sections,
        })
    }

    /// Where section `kind` lies in the file.
    pub fn section_range(&self, kind: u16) -> Result<Range<usize>, String> {
        self.sections
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, range)| range.clone())
            .ok_or_else(|| format!("section {kind} is missing"))
    }

    /// The bytes of section `kind`.
    pub fn section(&self, kind: u16) -> Result<&'a [u8], String> {
        self.section_range(kind).map(|range| &self.bytes[range])
    }
}

/// Distinguishes the temp files of concurrent writers within one process.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Publishes `bytes` at `path` atomically: a reader sees the old file or the new one, never a
/// part. The bytes go to a `.tmp` sibling that is fsynced, renamed over `path`, and the directory
/// is then synced so the rename itself survives a crash. A failed rename removes the temp.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no parent dir", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} has no file name", path.display()))?;
    let tmp = dir.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut f =
            File::create(&tmp).map_err(|e| format!("create temp {}: {e}", tmp.display()))?;
        f.write_all(bytes)
            .map_err(|e| format!("write temp {}: {e}", tmp.display()))?;
        // The bytes reach the disk before the rename publishes them; otherwise a crash right after
        // the rename could leave the new name pointing at empty data.
        f.sync_all()
            .map_err(|e| format!("fsync temp {}: {e}", tmp.display()))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename {} -> {}: {e}", tmp.display(), path.display())
    })?;
    sync_dir(dir).map_err(|e| format!("sync dir {}: {e}", dir.display()))
}

/// An exclusive lock on a lock file, held until dropped. Every process that writes the files
/// the lock guards takes it, so their writes never interleave.
pub struct WriteLock {
    _file: File,
}

/// Releases the lock itself rather than leaving it to the descriptor's close: a child forked
/// while the lock is held shares the descriptor until it execs, and would hold the lock past
/// this drop.
impl Drop for WriteLock {
    fn drop(&mut self) {
        // A failed unlock leaves the lock to the descriptor's close, the release there was before.
        let _ = self._file.unlock();
    }
}

impl WriteLock {
    fn open(path: &Path) -> Result<File, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| format!("open lock {}: {e}", path.display()))
    }

    /// Takes the lock, waiting while another holder has it.
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let file = Self::open(path)?;
        file.lock()
            .map_err(|e| format!("lock {}: {e}", path.display()))?;
        Ok(Self { _file: file })
    }

    /// Takes the lock if it is free; none when another holder has it.
    pub fn try_acquire(path: &Path) -> Result<Option<Self>, String> {
        let file = Self::open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(e)) => Err(format!("lock {}: {e}", path.display())),
        }
    }
}

/// Runs `write` holding the exclusive lock at `lock_path`, waiting for it if it is held.
pub fn with_write_lock<T, F>(lock_path: &Path, write: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let _lock = WriteLock::acquire(lock_path)?;
    write()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each row: the input and its CRC-32 (IEEE) check value.
    #[test]
    fn crc32_matches_the_ieee_check_values() {
        let ramp: Vec<u8> = (0..4099u32).map(|i| (i % 251) as u8).collect();
        let cases: &[(&[u8], u32)] = &[
            (b"", 0),
            (b"a", 0xE8B7_BE43),
            (b"123456789", 0xCBF4_3926),
            (b"The quick brown fox jumps over the lazy dog", 0x414F_A339),
            (&[0xFF; 64], 0x0F61_87BA),
            (&ramp, 0x48D1_721D),
        ];
        for (input, expected) in cases {
            assert_eq!(crc32(input), *expected, "{input:?}");
        }
    }

    #[test]
    fn a_container_round_trips_its_payload_and_sections() {
        let bytes = write_container(
            b"TEST",
            3,
            b"payload",
            &[
                (7, b"seven".to_vec()),
                (2, Vec::new()),
                (9, b"nine".to_vec()),
            ],
        )
        .expect("writes");
        let parsed = Container::parse(&bytes, b"TEST", 3).expect("parses");
        assert_eq!(parsed.payload, b"payload");
        assert_eq!(parsed.section(7), Ok(&b"seven"[..]));
        assert_eq!(parsed.section(2), Ok(&b""[..]));
        assert_eq!(parsed.section(9), Ok(&b"nine"[..]));
        assert!(parsed.section(1).is_err());
    }

    /// Each row: how the bytes are damaged; every damage is refused, never a panic.
    #[test]
    fn a_damaged_container_is_refused() {
        let good =
            write_container(b"TEST", 1, b"payload", &[(1, b"section".to_vec())]).expect("writes");
        let flip = |at: usize| {
            let mut b = good.clone();
            b[at] ^= 0x40;
            b
        };
        let last = good.len() - 1;
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", Vec::new()),
            ("truncated", good[..good.len() / 2].to_vec()),
            ("magic", flip(0)),
            ("version", flip(4)),
            ("payload", flip(12)),
            ("table offset", flip(good.len() - 7 - 4 - 4 - 8)),
            ("section byte", flip(last)),
        ];
        for (name, bytes) in cases {
            assert!(
                Container::parse(&bytes, b"TEST", 1).is_err(),
                "{name} damage must be refused"
            );
        }
    }

    /// Each row: how many sections, and whether their count fits the header's u16 field. A count
    /// that does not fit is an error, never a header that wraps to a smaller one.
    #[test]
    fn a_section_count_that_overflows_its_field_is_an_error() {
        let fits = usize::from(u16::MAX);
        for (count, written) in [(fits, true), (fits + 1, false)] {
            let sections = vec![(1, Vec::new()); count];
            assert_eq!(
                write_container(b"TEST", 1, b"", &sections).is_ok(),
                written,
                "{count} sections"
            );
        }
    }

    #[test]
    fn write_atomic_publishes_and_leaves_no_temp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("f");
        write_atomic(&path, b"one").expect("first write");
        write_atomic(&path, b"two").expect("second write");
        assert_eq!(std::fs::read(&path).expect("read"), b"two");
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .expect("list")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["f".to_string()]);
    }

    #[test]
    fn a_held_write_lock_is_not_taken_twice() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sub").join("LOCK");
        let held = WriteLock::try_acquire(&path).expect("open").expect("free");
        assert!(WriteLock::try_acquire(&path).expect("open").is_none());
        drop(held);
        assert!(WriteLock::try_acquire(&path).expect("open").is_some());
        assert_eq!(with_write_lock(&path, || Ok::<_, String>(5)), Ok(5));
    }

    /// A child process forked while the lock is held keeps a duplicate of its descriptor until it
    /// execs; dropping the lock frees it all the same.
    #[test]
    fn a_dropped_lock_is_free_while_a_duplicate_of_its_descriptor_lives() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("LOCK");
        let held = WriteLock::try_acquire(&path).expect("open").expect("free");
        let forked_copy = held._file.try_clone().expect("duplicate");
        drop(held);
        assert!(WriteLock::try_acquire(&path).expect("open").is_some());
        drop(forked_copy);
    }
}
