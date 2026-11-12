//! The facts memo on disk (addendum section 13): the facts extracted from a worktree file, kept
//! under the code store by the store's fingerprint, the file's path and its content, so a
//! one-shot read extracts each content once.
//!
//! An entry is a one-file snapshot ([`encode`]) at `memo/<fingerprint>-<path hash>-<content
//! id>.facts`, stamped with the store's fingerprint and the content's identity (its content id,
//! crc32 and length), published atomically ([`write_atomic`]) once it reads back as the facts it
//! holds. A read takes an entry only when it decodes whole with that stamp and holds the facts of
//! that one path; any other entry is absent and removed, and the file is extracted again. The key
//! is the content, so an entry is never stale; it is only unused.

use crate::code::dir_source::content_id;
use crate::code::snapshot::{encode, Snapshot};
use crate::format::{crc32, write_atomic, TEMP};
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

/// How much the memo on disk keeps: at most `bytes` of entries, none unread for longer than `age`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoCap {
    pub bytes: u64,
    pub age: Duration,
}

impl MemoCap {
    /// 64 MiB of entries, none unread for more than three days.
    pub const STATED: MemoCap = MemoCap {
        bytes: 64 << 20,
        age: Duration::from_secs(3 * 24 * 60 * 60),
    };
}

/// The entries of one store fingerprint in the memo directory `dir`, kept within `cap`.
pub(crate) struct DiskMemo {
    dir: PathBuf,
    fingerprint: u64,
    cap: MemoCap,
}

impl DiskMemo {
    pub(crate) fn new(dir: PathBuf, fingerprint: u64, cap: MemoCap) -> Self {
        Self {
            dir,
            fingerprint,
            cap,
        }
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

    /// Keeps `facts`, extracted from `bytes`, once their entry reads back as them.
    pub(crate) fn put(&self, bytes: &[u8], facts: &FileFacts) -> Result<(), String> {
        let view = FactsView::new(self.stamp(bytes), vec![facts.clone()]);
        let snap = Snapshot::decode(encode(&view, &Groupings::default())?)?;
        if self.facts_in(&snap, &facts.path, bytes).as_ref() != Some(facts) {
            return Err(format!(
                "the memo entry of {} does not read back",
                facts.path
            ));
        }
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("create {}: {e}", self.dir.display()))?;
        write_atomic(&self.dir.join(self.name(&facts.path, bytes)), snap.bytes())
    }

    /// Keeps the entries of this fingerprint read within the cap's age, the most recently read
    /// first up to the cap's bytes, and removes every other file in the directory but a temp a
    /// writer may still rename. Recency is an entry's modification time, which a read sets
    /// ([`touch`]). Best-effort: a file that cannot be listed or removed is left for the next
    /// reap, and a reader whose entry goes extracts its file again.
    pub(crate) fn reap(&self, now: SystemTime) {
        let own = format!("{:016x}-", self.fingerprint);
        let (mut kept, mut gone) = (Vec::new(), Vec::new());
        for file in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let (Ok(name), Ok(meta)) = (file.file_name().into_string(), file.metadata()) else {
                continue;
            };
            let modified = meta.modified().ok();
            let unread = modified.and_then(|at| now.duration_since(at).ok());
            let unread = unread.unwrap_or_default();
            let is_entry = name.starts_with(&own) && name.ends_with(FACTS);
            match name.ends_with(TEMP) {
                true if unread <= TEMP_GRACE => {}
                false if is_entry && unread <= self.cap.age => {
                    kept.push((unread, name, meta.len()))
                }
                _ => gone.push(name),
            }
        }
        kept.sort();
        let mut total = 0u64;
        for (_, name, len) in kept {
            total = total.saturating_add(len);
            if total > self.cap.bytes {
                gone.push(name);
            }
        }
        for name in gone {
            let _ = std::fs::remove_file(self.dir.join(name));
        }
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

    /// Two writers of one entry leave one whole entry: GIVEN the same facts put by several
    /// threads at once, THEN the memo holds that one entry and no temp, and it reads back as the
    /// facts.
    #[test]
    fn writers_of_one_entry_leave_it_whole() {
        let root = tempfile::tempdir().unwrap();
        let memo = DiskMemo::new(root.path().join(DIR), 7, MemoCap::STATED);
        let bytes = b"alpha\nbeta\n";
        let facts = facts_of("src/a.rs", bytes);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..16 {
                        memo.put(bytes, &facts).unwrap();
                    }
                });
            }
        });
        assert_eq!(names(&root.path().join(DIR)).len(), 1);
        assert_eq!(memo.get("src/a.rs", bytes), Some(facts));
    }

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
    /// a reap of the fingerprint 7 within 100 bytes and a day leaves.
    #[test]
    fn a_reap_keeps_the_recently_read_entries_of_its_fingerprint_within_the_cap() {
        const DAY: u64 = 24 * 60 * 60;
        let cap = MemoCap {
            bytes: 100,
            age: Duration::from_secs(DAY),
        };
        let (own, other) = (|n| entry(7, n), |n| entry(8, n));
        let (old_temp, new_temp) = (".x.facts.1.0.tmp".to_owned(), ".y.facts.1.1.tmp".to_owned());
        let cases: [(&str, Vec<Seeded>, Vec<String>); 7] = [
            (
                "a fresh entry of its fingerprint stays",
                vec![(own("a"), 10, 0)],
                vec![own("a")],
            ),
            (
                "an entry of another fingerprint goes",
                vec![(other("a"), 10, 0), (own("b"), 10, 0)],
                vec![own("b")],
            ),
            (
                "an entry unread for longer than the age goes",
                vec![(own("a"), 10, DAY + 60), (own("b"), 10, DAY - 60)],
                vec![own("b")],
            ),
            (
                "the least recently read go first beyond the bytes",
                vec![(own("a"), 40, 30), (own("b"), 40, 20), (own("c"), 40, 10)],
                vec![own("b"), own("c")],
            ),
            (
                "an entry larger than the bytes goes",
                vec![(own("a"), 101, 0)],
                vec![],
            ),
            (
                "a temp older than a writer takes goes, a younger one stays",
                vec![(old_temp, 10, 600), (new_temp.clone(), 10, 0)],
                vec![new_temp],
            ),
            (
                "a name the memo never writes goes",
                vec![("notes.txt".to_owned(), 10, 0), (own("a"), 10, 0)],
                vec![own("a")],
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
            DiskMemo::new(dir.clone(), 7, cap).reap(now);
            let mut want = want;
            want.sort();
            assert_eq!(names(&dir), want, "{case}");
        }
    }

    /// A read is what keeps an entry: GIVEN two entries, the one read longer ago read again,
    /// WHEN the memo is reaped to the bytes of one of them, THEN the entry read again stays.
    #[test]
    fn a_read_entry_outlives_one_read_before_it() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(DIR);
        let (a, b) = (b"alpha\n".as_slice(), b"beta\n".as_slice());
        let (of_a, of_b) = (facts_of("src/a.rs", a), facts_of("src/b.rs", b));
        let memo = DiskMemo::new(dir.clone(), 7, MemoCap::STATED);
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
        let cap = MemoCap {
            bytes: size(&at_a).max(size(&at_b)),
            ..MemoCap::STATED
        };
        DiskMemo::new(dir.clone(), 7, cap).reap(SystemTime::now());
        assert_eq!(names(&dir), [at_a]);
    }
}
