//! The facts memo on disk (addendum section 13): the facts extracted from a worktree file, kept
//! under the code store by the store's fingerprint, the file's path and its content, so a
//! one-shot read extracts each content once.
//!
//! An entry is a one-file snapshot ([`encode`]) at `memo/<fingerprint>-<path hash>-<content
//! id>.facts`, stamped with the store's fingerprint and the content's identity (its content id,
//! crc32 and length), published atomically and unsynced ([`write_atomic_as`]) once it reads back
//! as the facts it holds. A read takes an entry only when it decodes whole with that stamp and
//! holds the facts of that one path; any other entry is absent and removed, and the file is
//! extracted again. The key is the content, so an entry is never stale; it is only unused.
//!
//! The memo is bounded by [`MemoCap::STATED`]: 64 MiB of entries, each counted in whole 4 KiB
//! blocks, none unread for more than three days. Its reaper ([`reap`]) runs with the store's on
//! every advance and after every overlay open that extracts a file. It keeps the entries of the
//! fingerprints the store keeps the snapshots of, so two binaries sharing a store keep each
//! other's, the most recently read first across them, recency being the modification time a read
//! sets; it removes every other file but a temp younger than a minute. `rigger reset --derived`
//! removes the whole memo, and `rigger validate` reports its size.

use crate::code::dir_source::content_id;
use crate::code::snapshot::{encode, Snapshot};
use crate::format::{crc32, write_atomic_as, Durability, TEMP};
use rigger_domain::code_facts::{FileFacts, Stamp, Tip};
use rigger_domain::code_view::{CodeView, FactsView, Groupings};
use rigger_domain::playbooks::fnv1a_64;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The memo's directory under the code store's.
pub(crate) const DIR: &str = "memo";
const FACTS: &str = ".facts";
/// How long a temp is left for its writer to rename: one older is a crashed writer's.
const TEMP_GRACE: Duration = Duration::from_secs(60);
/// The block the cap counts an entry in, the allocation unit of the common file systems: an entry
/// takes its bytes rounded up to it on disk, however few they are.
const BLOCK: u64 = 4096;

/// How much the memo on disk keeps: at most `bytes` of entries, each counted as the 4 KiB blocks it
/// takes, none unread for longer than `age`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoCap {
    pub bytes: u64,
    pub age: Duration,
}

impl MemoCap {
    /// 64 MiB of entries in 4 KiB blocks, none unread for more than three days.
    pub const STATED: MemoCap = MemoCap {
        bytes: 64 << 20,
        age: Duration::from_secs(3 * 24 * 60 * 60),
    };
}

/// The entries of one store fingerprint in the memo directory `dir`.
pub(crate) struct DiskMemo {
    dir: PathBuf,
    fingerprint: u64,
}

impl DiskMemo {
    pub(crate) fn new(dir: PathBuf, fingerprint: u64) -> Self {
        Self { dir, fingerprint }
    }

    /// The facts of the file at `path` holding `bytes`, when an entry holds them, marked read now.
    /// An entry that does not is removed; a file that cannot be read is a miss.
    pub(crate) fn get(&self, path: &str, bytes: &[u8]) -> Option<FileFacts> {
        let at = self.dir.join(self.name(path, bytes));
        let read = std::fs::read(&at).ok()?;
        let facts =
            (Snapshot::decode(read).ok()).and_then(|snap| self.facts_in(&snap, path, bytes));
        match facts {
            Some(facts) => {
                touch(&at);
                Some(facts)
            }
            None => {
                let _ = std::fs::remove_file(&at);
                None
            }
        }
    }

    /// Keeps `facts`, extracted from `bytes`, once their entry reads back as them. The directory
    /// comes first, so a memo that cannot be written costs no encode. The entry is published
    /// unsynced ([`Durability::Unsynced`]): every read validates it, so one a crash loses or
    /// leaves short is a miss, and the reader waits on no sync.
    pub(crate) fn put(&self, bytes: &[u8], facts: &FileFacts) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("create {}: {e}", self.dir.display()))?;
        let view = FactsView::new(self.stamp(bytes), vec![facts.clone()]);
        let snap = Snapshot::decode(encode(&view, &Groupings::default())?)?;
        if self.facts_in(&snap, &facts.path, bytes).as_ref() != Some(facts) {
            return Err(format!(
                "the memo entry of {} does not read back",
                facts.path
            ));
        }
        let at = self.dir.join(self.name(&facts.path, bytes));
        write_atomic_as(&at, snap.bytes(), Durability::Unsynced)
    }

    /// The entry name of the file at `path` holding `bytes`: one name prefix a fingerprint.
    fn name(&self, path: &str, bytes: &[u8]) -> String {
        let (path, blob) = (fnv1a_64(path.as_bytes()), content_id(bytes));
        format!("{:016x}-{path:016x}-{blob}{FACTS}", self.fingerprint)
    }

    /// The stamp of the entry for `bytes`: the store's fingerprint, and for its tree the content's
    /// identity, so an entry whose name collides with another content's is told apart.
    fn stamp(&self, bytes: &[u8]) -> Stamp {
        let identity = format!("{}-{:08x}-{}", content_id(bytes), crc32(bytes), bytes.len());
        Stamp {
            tip: Tip {
                head: String::new(),
                commit: None,
                tree: identity,
            },
            fingerprint: self.fingerprint,
            seq: 0,
        }
    }

    /// The facts `snap` holds for the file at `path` holding `bytes`: none unless it is stamped
    /// for those bytes and holds that one file alone.
    fn facts_in(&self, snap: &Snapshot, path: &str, bytes: &[u8]) -> Option<FileFacts> {
        let paths: Vec<&str> = snap.files().map(|f| f.path).collect();
        if *snap.stamp() != self.stamp(bytes) || paths != [path] {
            return None;
        }
        let facts = snap.file(path)?.into_owned();
        (facts.blob == content_id(bytes)).then_some(facts)
    }
}

/// Reaps the memo directory `dir` to the entries of the fingerprints in `kept` read within the
/// cap's age, the most recently read first up to the cap's bytes across them, each entry counted
/// in whole 4 KiB blocks ([`allocated`]), and removes every other file in it but a temp a writer
/// may still rename. Recency is an entry's modification time, which a read sets ([`touch`]).
/// Best-effort: a file that cannot be listed or removed is left for the next reap, and a reader
/// whose entry goes extracts its file again.
pub(crate) fn reap(dir: &Path, kept: &[u64], cap: MemoCap, now: SystemTime) {
    let kept: Vec<String> = kept.iter().map(|fp| format!("{fp:016x}-")).collect();
    let (mut held, mut gone) = (Vec::new(), Vec::new());
    for file in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let (Ok(name), Ok(meta)) = (file.file_name().into_string(), file.metadata()) else {
            continue;
        };
        let modified = meta.modified().ok();
        let unread = modified.and_then(|at| now.duration_since(at).ok());
        let unread = unread.unwrap_or_default();
        let is_kept = name.ends_with(FACTS) && kept.iter().any(|fp| name.starts_with(fp));
        match name.ends_with(TEMP) {
            true if unread <= TEMP_GRACE => {}
            false if is_kept && unread <= cap.age => held.push((unread, name, allocated(&meta))),
            _ => gone.push(name),
        }
    }
    held.sort();
    let mut total = 0u64;
    for (_, name, len) in held {
        total = total.saturating_add(len);
        if total > cap.bytes {
            gone.push(name);
        }
    }
    for name in gone {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// The bytes the entry `meta` describes takes on disk, counted in whole [`BLOCK`]s.
fn allocated(meta: &std::fs::Metadata) -> u64 {
    meta.len().div_ceil(BLOCK) * BLOCK
}

/// Marks the entry at `at` read now, its recency for the reaper. Best-effort: a mark that fails
/// only lets the entry be reaped sooner. The file is opened to write without creating it, so an
/// entry reaped since it was read is never made again.
fn touch(at: &Path) {
    let opened = std::fs::OpenOptions::new().write(true).open(at);
    let _ = opened.and_then(|f| f.set_modified(SystemTime::now()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigger_domain::code_facts::Def;

    /// The facts of the file at `path` holding `bytes`, defining one function per line.
    fn facts_of(path: &str, bytes: &[u8]) -> FileFacts {
        let defs = String::from_utf8_lossy(bytes)
            .lines()
            .enumerate()
            .map(|(at, leaf)| Def {
                local: leaf.to_owned(),
                leaf: leaf.to_owned(),
                kind: "fn".into(),
                line: at as u32 + 1,
                end_line: at as u32 + 1,
                parent: None,
                test: false,
            })
            .collect();
        FileFacts {
            path: path.to_owned(),
            blob: content_id(bytes),
            lang: "toy".into(),
            defs,
            ..Default::default()
        }
    }

    /// The names in `dir`, sorted.
    fn names(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = (std::fs::read_dir(dir).into_iter().flatten().flatten())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect();
        names.sort();
        names
    }

    /// A memo that cannot be written costs no encode: GIVEN a memo directory under a regular
    /// file, WHEN facts are put that would not read back, THEN the put fails making the directory,
    /// before it encodes the entry.
    #[test]
    fn an_unwritable_memo_fails_before_it_encodes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file"), b"not a directory").unwrap();
        let memo = DiskMemo::new(root.path().join("file").join(DIR), 7);
        let put = memo.put(b"beta\n", &facts_of("src/a.rs", b"alpha\n"));
        let err = put.expect_err("no directory is made under a file");
        assert!(err.starts_with("create "), "{err}");
    }

    /// Writers of one entry never show a reader part of it: GIVEN the same facts put over and
    /// over by several threads while others read the entry, THEN every read begun after a put
    /// returned answers the facts, and the memo ends holding that one entry and no temp.
    #[test]
    fn writers_of_one_entry_leave_it_whole() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
        const WRITERS: usize = 4;
        const READERS: usize = 4;
        let root = tempfile::tempdir().unwrap();
        let memo = DiskMemo::new(root.path().join(DIR), 7);
        // Enough lines that an entry is written in more than one piece.
        let bytes: String = (0..2000).map(|n| format!("def_{n}\n")).collect();
        let bytes = bytes.as_bytes();
        let facts = facts_of("src/a.rs", bytes);
        let (published, writing) = (AtomicBool::new(false), AtomicUsize::new(WRITERS));
        let checked = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..WRITERS {
                scope.spawn(|| {
                    let put = (0..16).try_for_each(|_| {
                        memo.put(bytes, &facts)?;
                        published.store(true, SeqCst);
                        Ok::<_, String>(())
                    });
                    writing.fetch_sub(1, SeqCst);
                    put.unwrap();
                });
            }
            for _ in 0..READERS {
                scope.spawn(|| {
                    while writing.load(SeqCst) > 0 {
                        let after = published.load(SeqCst);
                        let read = memo.get("src/a.rs", bytes);
                        if after {
                            assert_eq!(read.as_ref(), Some(&facts), "a read after a put");
                            checked.fetch_add(1, SeqCst);
                        }
                    }
                });
            }
        });
        assert!(checked.load(SeqCst) > 0, "no read began after a put");
        let entry = memo.name("src/a.rs", bytes);
        assert_eq!(names(&root.path().join(DIR)), vec![entry]);
        assert_eq!(memo.get("src/a.rs", bytes), Some(facts));
    }

    /// The block the reap counts an entry's bytes in.
    const FOUR_KIB: usize = 4096;

    /// The name of an entry of the fingerprint `fp`.
    fn entry(fp: u64, name: &str) -> String {
        format!("{fp:016x}-{name}{FACTS}")
    }

    /// A file seeded in the memo directory: its name, its length, and how many seconds unread.
    type Seeded = (String, usize, u64);

    /// Writes `len` bytes at `at`, last read `unread` seconds before `now`.
    fn seed(at: &std::path::Path, len: usize, unread: u64, now: SystemTime) {
        std::fs::write(at, vec![b'x'; len]).unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(at).unwrap();
        file.set_modified(now - Duration::from_secs(unread))
            .unwrap();
    }

    /// Each row: the files in the memo directory as (name, bytes, seconds unread), and the names
    /// a reap keeping the fingerprints 7 and 9 within three 4 KiB blocks and a day leaves.
    #[test]
    fn a_reap_keeps_the_recently_read_entries_of_the_kept_fingerprints_within_the_cap() {
        const DAY: u64 = 24 * 60 * 60;
        let cap = MemoCap {
            bytes: 3 * FOUR_KIB as u64,
            age: Duration::from_secs(DAY),
        };
        let (first, second, other) = (|n| entry(7, n), |n| entry(9, n), |n| entry(8, n));
        let (old_temp, new_temp) = (".x.facts.1.0.tmp".to_owned(), ".y.facts.1.1.tmp".to_owned());
        let cases: [(&str, Vec<Seeded>, Vec<String>); 8] = [
            (
                "a fresh entry of each kept fingerprint stays",
                vec![(first("a"), 10, 0), (second("b"), 10, 0)],
                vec![first("a"), second("b")],
            ),
            (
                "an entry of a fingerprint not kept goes",
                vec![(other("a"), 10, 0), (first("b"), 10, 0)],
                vec![first("b")],
            ),
            (
                "an entry unread for longer than the age goes, of either kept fingerprint",
                vec![
                    (first("a"), 10, DAY + 60),
                    (second("b"), 10, DAY + 60),
                    (first("c"), 10, DAY - 60),
                ],
                vec![first("c")],
            ),
            (
                "the least recently read go first beyond the bytes, across the kept fingerprints",
                vec![
                    (first("a"), 2 * FOUR_KIB, 30),
                    (second("b"), FOUR_KIB, 20),
                    (first("c"), FOUR_KIB, 10),
                ],
                vec![second("b"), first("c")],
            ),
            (
                "an entry counts the 4 KiB blocks it takes, so four small ones pass three blocks",
                vec![
                    (first("a"), 10, 40),
                    (first("b"), 10, 30),
                    (second("c"), 10, 20),
                    (first("d"), 10, 10),
                ],
                vec![first("b"), second("c"), first("d")],
            ),
            (
                "an entry larger than the bytes goes",
                vec![(first("a"), 3 * FOUR_KIB + 1, 0)],
                vec![],
            ),
            (
                "a temp older than a writer takes goes, a younger one stays",
                vec![(old_temp, 10, 600), (new_temp.clone(), 10, 0)],
                vec![new_temp],
            ),
            (
                "a name the memo never writes goes",
                vec![("notes.txt".to_owned(), 10, 0), (first("a"), 10, 0)],
                vec![first("a")],
            ),
        ];
        for (case, files, want) in cases {
            let root = tempfile::tempdir().unwrap();
            let dir = root.path().join(DIR);
            std::fs::create_dir(&dir).unwrap();
            let now = SystemTime::now();
            for (name, len, unread) in &files {
                seed(&dir.join(name), *len, *unread, now);
            }
            reap(&dir, &[7, 9], cap, now);
            let mut want = want;
            want.sort();
            assert_eq!(names(&dir), want, "{case}");
        }
    }

    /// A read is what keeps an entry: GIVEN two entries, the one read longer ago read again,
    /// WHEN the memo is reaped to the one block either takes, THEN the entry read again stays.
    #[test]
    fn a_read_entry_outlives_one_read_before_it() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR);
        let (a, b) = (b"alpha\n".as_slice(), b"beta\n".as_slice());
        let (of_a, of_b) = (facts_of("src/a.rs", a), facts_of("src/b.rs", b));
        let memo = DiskMemo::new(dir.clone(), 7);
        memo.put(a, &of_a).unwrap();
        memo.put(b, &of_b).unwrap();
        let (at_a, at_b) = (memo.name("src/a.rs", a), memo.name("src/b.rs", b));
        let now = SystemTime::now();
        for (name, unread) in [(&at_a, 20), (&at_b, 10)] {
            let file = std::fs::OpenOptions::new().write(true).open(dir.join(name));
            let mtime = now - Duration::from_secs(unread);
            file.unwrap().set_modified(mtime).unwrap();
        }
        assert_eq!(memo.get("src/a.rs", a), Some(of_a));
        let size = |name: &String| std::fs::metadata(dir.join(name)).unwrap().len();
        assert!(size(&at_a).max(size(&at_b)) <= FOUR_KIB as u64);
        let cap = MemoCap {
            bytes: FOUR_KIB as u64,
            ..MemoCap::STATED
        };
        reap(&dir, &[7], cap, SystemTime::now());
        assert_eq!(names(&dir), [at_a]);
    }
}
