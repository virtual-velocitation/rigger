//! The store's operations: reading the current snapshot, advancing it to a tip, reaping what no
//! pointer names, and [`open_current`], the one entry point every caller goes through.
//!
//! The store is a directory of immutable `<seq>.snap` files, one `CURRENT-<fingerprint>` pointer
//! per fingerprint naming the snapshot in force under it, and a `LOCK` every writer holds. A
//! snapshot is published before the pointer to it, each by an atomic rename, so a reader sees the
//! old snapshot or the new one, never a part; a crash between the two leaves the old one current.
//! The reaper keeps the pointers of the two most recently advanced fingerprints and their
//! snapshots, so two binaries sharing a store each keep theirs.

use crate::code::snapshot::{encode, stamp_unchecked, Snapshot};
use crate::format::{write_atomic, WriteLock};
use crate::published::Published;
use rigger_domain::code_facts::{FileFacts, Stamp, Tip};
use rigger_domain::code_view::{
    in_scope, CodeView, Entry, Extract, FactsView, Group, TreeSource, WorktreeDiff,
};
use rigger_domain::config::RIGGER_DIR;
use rigger_domain::playbooks::fnv1a_64;
use rigger_domain::resolve::SETTLED_GENERATION;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// The prefix of a pointer file: `CURRENT-<fingerprint>` names the snapshot in force under that
/// fingerprint, in sixteen lowercase hex digits.
const CURRENT: &str = "CURRENT-";
/// The lock file every writer holds.
pub const LOCK: &str = "LOCK";
const SNAP: &str = ".snap";
const TMP: &str = ".tmp";
/// How many times a reader re-reads its pointer when the snapshot it named was reaped first.
const READ_ATTEMPTS: usize = 3;
/// How many fingerprints' snapshots the store keeps: the two most recently advanced.
const KEPT_FINGERPRINTS: usize = 2;

/// Publishes a file's bytes; the default is [`AtomicPublish`].
pub trait Publish: Send + Sync {
    fn publish(&self, path: &Path, bytes: &[u8]) -> Result<(), String>;
}

/// Publishes through [`write_atomic`]: temp, fsync, rename, directory sync.
pub struct AtomicPublish;

impl Publish for AtomicPublish {
    fn publish(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        write_atomic(path, bytes)
    }
}

/// The fingerprint a snapshot is stamped with: the extractor's fingerprint, the grouper's
/// generation and the generation of what the store settles from the tree alone
/// ([`SETTLED_GENERATION`]) folded together, since all three compute the snapshot, so a change to
/// any computes it anew.
pub fn fingerprint(extract: &dyn Extract, group: &dyn Group) -> u64 {
    folded([
        extract.fingerprint(),
        group.generation(),
        SETTLED_GENERATION,
    ])
}

/// The one fingerprint of the extractor's fingerprint and the two generations, in that order.
fn folded(parts: [u64; 3]) -> u64 {
    fnv1a_64(&parts.map(u64::to_le_bytes).concat())
}

fn snap_name(seq: u64) -> String {
    format!("{seq:06}{SNAP}")
}

/// The pointer file naming the snapshot in force under `fingerprint`.
fn pointer_name(fingerprint: u64) -> String {
    format!("{CURRENT}{fingerprint:016x}")
}

/// The bytes a read of the store file at `path` answered: none when the file is absent, and an
/// error naming the file when it could not be read for another reason.
fn readable<B>(path: &Path, bytes: std::io::Result<B>) -> Result<Option<B>, String> {
    match bytes {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("read {}: {e}", path.display())),
    }
}

/// Whether `name` is a pointer file's.
fn is_pointer(name: &str) -> bool {
    name.strip_prefix(CURRENT)
        .is_some_and(|hex| hex.len() == 16 && u64::from_str_radix(hex, 16).is_ok())
}

/// The sequence number a snapshot file name carries; none for any other name.
fn seq_of(name: &str) -> Option<u64> {
    let digits = name.strip_suffix(SNAP)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The code store of the project at a root: `<root>/.rigger/store/code/`.
pub struct Store {
    dir: PathBuf,
    publish: Box<dyn Publish>,
}

impl Store {
    /// The store of the project whose root (the directory owning `.rigger/`) is `root`.
    pub fn new(root: &Path) -> Self {
        Self::with_publish(root, Box::new(AtomicPublish))
    }

    /// The store at `root`, publishing through `publish`.
    pub(crate) fn with_publish(root: &Path, publish: Box<dyn Publish>) -> Self {
        Self {
            dir: root.join(RIGGER_DIR).join("store").join("code"),
            publish,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn lock_path(&self) -> PathBuf {
        self.dir.join(LOCK)
    }

    /// The directory of the facts memo on disk.
    pub fn memo_dir(&self) -> PathBuf {
        self.dir.join(crate::code::disk_memo::DIR)
    }

    /// The snapshot `fingerprint`'s pointer names, when it reads whole and was extracted under
    /// `fingerprint`; none when there is no such snapshot (no pointer, a pointer that names no
    /// snapshot, a snapshot that fails to decode or was extracted under another fingerprint),
    /// which callers treat as an empty store. A store file that cannot be read for a reason other
    /// than its absence is an error.
    pub fn current(&self, fingerprint: u64) -> Result<Option<Snapshot>, String> {
        self.current_with(fingerprint, &Published::open)
    }

    /// [`Store::current`], reading snapshot files through `read`. When the file the pointer named
    /// is gone, the reaper won the race: the pointer has moved on, so it is read again, a bounded
    /// number of times.
    pub(crate) fn current_with(
        &self,
        fingerprint: u64,
        read: &dyn Fn(&Path) -> std::io::Result<Published>,
    ) -> Result<Option<Snapshot>, String> {
        for _ in 0..READ_ATTEMPTS {
            let Some((path, seq)) = self.named(fingerprint)? else {
                return Ok(None);
            };
            let Some(bytes) = readable(&path, read(&path))? else {
                continue;
            };
            return Ok(Snapshot::decode(bytes).ok().filter(|snap| {
                snap.stamp().fingerprint == fingerprint && snap.stamp().seq == seq
            }));
        }
        Ok(None)
    }

    /// The snapshot file `fingerprint`'s pointer names, with its sequence number; none when there
    /// is no pointer or it names no snapshot file.
    fn named(&self, fingerprint: u64) -> Result<Option<(PathBuf, u64)>, String> {
        let current = self.dir.join(pointer_name(fingerprint));
        let Some(pointer) = readable(&current, std::fs::read(&current))? else {
            return Ok(None);
        };
        let pointer = String::from_utf8_lossy(&pointer);
        let name = pointer.trim();
        Ok(seq_of(name).map(|seq| (self.dir.join(name), seq)))
    }

    /// The tree the snapshot `fingerprint`'s pointer names describes, read from that snapshot's
    /// header without checking the rest: a guess, which [`open_compared`] confirms against the
    /// snapshot it opens.
    fn named_tree(&self, fingerprint: u64) -> Option<String> {
        let (path, _) = self.named(fingerprint).ok()??;
        let bytes = Published::open(&path).ok()?;
        stamp_unchecked(&bytes).map(|stamp| stamp.tip.tree)
    }

    /// Advances the store to `target`, waiting for the lock, and returns the snapshot in force.
    pub fn advance(
        &self,
        source: &dyn TreeSource,
        extract: &dyn Extract,
        group: &dyn Group,
        target: &Tip,
    ) -> Result<Snapshot, String> {
        let _lock = WriteLock::acquire(&self.lock_path())?;
        self.advance_locked(source, extract, group, target)
    }

    /// The advance proper, run holding the lock. The tree's in-scope files are listed and
    /// compared with the snapshot's own (path, blob) pairs: a file whose pair is unchanged keeps its facts, every
    /// other file is extracted from the blob the source reads.
    fn advance_locked(
        &self,
        source: &dyn TreeSource,
        extract: &dyn Extract,
        group: &dyn Group,
        target: &Tip,
    ) -> Result<Snapshot, String> {
        let fingerprint = fingerprint(extract, group);
        let base = match self.current(fingerprint)? {
            Some(snap) if snap.stamp().tip == *target => {
                self.reap();
                return Ok(snap);
            }
            other => other,
        };
        let mut entries = source.list(&target.tree)?;
        entries.retain(|e| in_scope(&e.path));
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries.dedup_by(|a, b| a.path == b.path);
        let mut files = Vec::with_capacity(entries.len());
        let mut changed = Vec::new();
        for entry in entries {
            match base.as_ref().and_then(|b| b.file(&entry.path)) {
                Some(facts) if facts.blob == entry.blob => files.push(facts.into_owned()),
                _ => changed.push(entry),
            }
        }
        let reuse_groupings = changed.is_empty()
            && base
                .as_ref()
                .is_some_and(|b| b.files().count() == files.len());
        files.extend(extract_all(source, extract, &changed)?);
        let stamp = Stamp {
            tip: target.clone(),
            fingerprint,
            seq: self.next_seq(),
        };
        let view = FactsView::new(stamp, files);
        let groupings = match base.as_ref().filter(|_| reuse_groupings) {
            Some(b) => b.groupings().clone(),
            None => group.group(&view),
        };
        // Decoded, and so validated, before anything is published: a snapshot that would not
        // read back is an error, and the store keeps the one in force.
        let snap = Snapshot::decode(encode(&view, &groupings)?)?;
        let name = snap_name(snap.stamp().seq);
        self.publish.publish(&self.dir.join(&name), snap.bytes())?;
        self.publish
            .publish(&self.dir.join(pointer_name(fingerprint)), name.as_bytes())?;
        self.reap();
        Ok(snap)
    }

    /// One past every sequence number in use: every pointer's, and every snapshot file's, so a
    /// name is never reused while a reader may hold the file it named.
    fn next_seq(&self) -> u64 {
        let pointed = self.pointers().filter_map(|(_, seq)| seq).max();
        let listed = self.names().filter_map(|n| seq_of(&n)).max();
        pointed.max(listed).unwrap_or(0) + 1
    }

    /// Every pointer file that reads, with the sequence number of the snapshot it names; none for
    /// a pointer that names no snapshot. A pointer that cannot be read is not listed.
    fn pointers(&self) -> impl Iterator<Item = (String, Option<u64>)> + '_ {
        self.names().filter(|n| is_pointer(n)).filter_map(|name| {
            let pointer = std::fs::read(self.dir.join(&name)).ok()?;
            let seq = seq_of(String::from_utf8_lossy(&pointer).trim());
            Some((name, seq))
        })
    }

    fn names(&self) -> impl Iterator<Item = String> {
        std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
    }

    /// Keeps the pointers of the [`KEPT_FINGERPRINTS`] most recently advanced fingerprints and
    /// the snapshots they name, and removes every other pointer, snapshot and temp. Recency is the
    /// order the pointers were written in, which is the order of the sequence numbers they name,
    /// since every pointer is written holding the lock with the next number. A pointer is removed
    /// before its snapshot, so a crash between leaves an unnamed snapshot the next reap removes,
    /// never a pointer naming nothing. A pointer that cannot be read is left for the next advance,
    /// though the snapshot it names is not kept, since which one it names is unknown; so is a
    /// failed delete (a reader holding the file open on an OS that refuses it).
    fn reap(&self) {
        let mut ranked: Vec<(String, Option<u64>)> = self.pointers().collect();
        ranked.sort_by_key(|(_, seq)| std::cmp::Reverse(*seq));
        let stale = ranked.split_off(KEPT_FINGERPRINTS.min(ranked.len()));
        let kept: Vec<String> = ranked
            .iter()
            .filter_map(|(_, seq)| seq.map(snap_name))
            .collect();
        let unnamed: Vec<String> = self
            .names()
            .filter(|name| (seq_of(name).is_some() && !kept.contains(name)) || name.ends_with(TMP))
            .collect();
        for name in stale.into_iter().map(|(name, _)| name).chain(unnamed) {
            let _ = std::fs::remove_file(self.dir.join(name));
        }
    }
}

/// Extracts each entry from its blob, in parallel. The bytes come from the source's objects,
/// never a worktree, and each blob is read once however many paths share it.
fn extract_all(
    source: &dyn TreeSource,
    extract: &dyn Extract,
    entries: &[Entry],
) -> Result<Vec<FileFacts>, String> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let mut wanted: Vec<String> = entries.iter().map(|e| e.blob.clone()).collect();
    wanted.sort();
    wanted.dedup();
    let mut blobs: HashMap<String, Vec<u8>> = HashMap::with_capacity(wanted.len());
    source.read_blobs(&wanted, &mut |id, bytes| {
        blobs.insert(id.to_owned(), bytes.to_vec());
    })?;
    if let Some(missing) = entries.iter().find(|e| !blobs.contains_key(&e.blob)) {
        return Err(format!(
            "blob {} of {} was not read",
            missing.blob, missing.path
        ));
    }
    extract_parallel(extract, entries, &|entry| {
        blobs.get(&entry.blob).map_or(&[][..], Vec::as_slice)
    })
}

/// The facts of each of `entries`, extracted from the bytes `bytes_of` hands for it in parallel
/// across the machine's cores, each with the path and blob its entry names: the one extraction
/// pass an advance and a worktree overlay both run.
pub(crate) fn extract_parallel<'b>(
    extract: &dyn Extract,
    entries: &[Entry],
    bytes_of: &(dyn Fn(&Entry) -> &'b [u8] + Sync),
) -> Result<Vec<FileFacts>, String> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(entries.len());
    let chunk = entries.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let workers: Vec<_> = entries
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|entry| FileFacts {
                            path: entry.path.clone(),
                            blob: entry.blob.clone(),
                            ..extract.extract(entry, bytes_of(entry))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut out = Vec::with_capacity(entries.len());
        for worker in workers {
            out.extend(
                worker
                    .join()
                    .map_err(|_| "an extractor panicked".to_string())?,
            );
        }
        Ok(out)
    })
}

/// How a caller waits on the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A writer (the conductor after a merge, `graph build`, `setup`): waits for the lock.
    Write,
    /// A reader: tries the lock once, and answers from the snapshot in force when it is held.
    Read,
}

/// Why an answer describes an older stamp than the head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Behind {
    /// Another process holds the lock, advancing.
    AdvanceInFlight,
    /// The store could not be written; the error says why.
    CannotWrite(String),
}

impl fmt::Display for Behind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AdvanceInFlight => write!(f, "advance in flight"),
            Self::CannotWrite(e) => write!(f, "cannot write the code store: {e}"),
        }
    }
}

/// The snapshot a caller reads, the head it was asked for, and why the two differ when they do.
pub struct Opened {
    pub view: Snapshot,
    pub head: Tip,
    /// None when the view describes the head.
    pub behind: Option<Behind>,
}

/// The view of the head of the project at `root`, advancing the store first when the snapshot
/// in force describes another tip or none. A snapshot the source says the checkout is still at
/// answers without asking the source for its head. A reader never blocks on a writer while a
/// snapshot exists: it answers from it, saying why ([`Behind`]). With no snapshot it waits, since
/// it has nothing else to answer from.
pub fn open_current(
    root: &Path,
    source: &dyn TreeSource,
    extract: &dyn Extract,
    group: &dyn Group,
    mode: Mode,
) -> Result<Opened, String> {
    open_current_in(&Store::new(root), source, extract, group, mode)
}

/// [`open_current`] for a reader in `worktree`: the view of the head, with how `worktree`
/// differs from the tree that view describes.
///
/// The two overlap: `worktree` is compared with the tree the pointer names (read from that
/// snapshot's header alone) while the snapshot is read and checked whole. That comparison is the
/// answer only when it succeeded against the very tree the opened view describes; when the view
/// describes another (the store advanced, or the pointer moved on) or the comparison failed,
/// `worktree` is compared again with the view's tree.
pub fn open_compared(
    root: &Path,
    source: &(dyn TreeSource + Sync),
    extract: &dyn Extract,
    group: &dyn Group,
    worktree: &Path,
) -> Result<(Opened, Result<WorktreeDiff, String>), String> {
    let store = Store::new(root);
    let named = fingerprint(extract, group);
    std::thread::scope(|scope| {
        let early = scope.spawn(|| {
            let tree = store.named_tree(named)?;
            let diff = source.changed_against_worktree(&tree, worktree).ok()?;
            Some((tree, diff))
        });
        let opened = open_current_in(&store, source, extract, group, Mode::Read)?;
        let tree = &opened.view.stamp().tip.tree;
        let diff = match early.join() {
            Ok(Some((compared, diff))) if compared == *tree => Ok(diff),
            _ => source.changed_against_worktree(tree, worktree),
        };
        Ok((opened, diff))
    })
}

pub(crate) fn open_current_in(
    store: &Store,
    source: &dyn TreeSource,
    extract: &dyn Extract,
    group: &dyn Group,
    mode: Mode,
) -> Result<Opened, String> {
    let current = match store.current(fingerprint(extract, group))? {
        Some(view) if source.still_at(&view.stamp().tip) => {
            let head = view.stamp().tip.clone();
            return Ok(Opened {
                view,
                head,
                behind: None,
            });
        }
        other => other,
    };
    let head = source.head()?;
    let current = match current {
        Some(view) if view.stamp().tip == head => {
            return Ok(Opened {
                view,
                head,
                behind: None,
            })
        }
        other => other,
    };
    let lock = match mode {
        Mode::Read if current.is_some() => WriteLock::try_acquire(&store.lock_path()),
        _ => WriteLock::acquire(&store.lock_path()).map(Some),
    };
    let advanced = match lock {
        Ok(Some(_held)) => store
            .advance_locked(source, extract, group, &head)
            .map_err(Behind::CannotWrite),
        Ok(None) => Err(Behind::AdvanceInFlight),
        Err(e) => Err(Behind::CannotWrite(e)),
    };
    match (advanced, current) {
        (Ok(view), _) => Ok(Opened {
            view,
            head,
            behind: None,
        }),
        (Err(behind), Some(view)) => Ok(Opened {
            view,
            head,
            behind: Some(behind),
        }),
        (Err(behind), None) => Err(behind.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::disk_memo::MemoCap;
    use crate::code::doubles::{CountingGroup, MemSource, ToyExtract};
    use crate::format::{Container, WriteLock};
    use rigger_domain::code_facts::FileFacts;
    use rigger_domain::code_view::{CodeView, Groupings};
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime};

    const FP: u64 = 1;

    /// The fingerprint a snapshot is stamped with when the toy extractor of `extracted_under`
    /// reads it and the counting grouper groups it.
    fn stamped(extracted_under: u64) -> u64 {
        fingerprint(&ToyExtract::new(extracted_under), &CountingGroup::default())
    }

    /// A commit's files as (path, text), and a list of paths.
    type Files = &'static [(&'static str, &'static str)];
    type Paths = &'static [&'static str];
    type Commit = Vec<(&'static str, &'static str)>;

    /// The files of commit A, and B: `src/b.rs` modified, `tests/t.rs` deleted, `src/c.rs` added.
    const A: Files = &[
        ("src/a.rs", "def alpha\ncall beta\n"),
        ("src/b.rs", "def beta\n def inner\ncall alpha\n"),
        ("tests/t.rs", "def check\ncall alpha\n"),
        ("docs/one.txt", "def twin\n"),
        ("docs/two.txt", "def twin\n"),
    ];
    const B: Files = &[
        ("src/a.rs", "def alpha\ncall beta\n"),
        (
            "src/b.rs",
            "def beta\n def inner\ncall alpha\nuse type Alpha\n",
        ),
        ("src/c.rs", "def gamma\n"),
        ("docs/one.txt", "def twin\n"),
        ("docs/two.txt", "def twin\n"),
    ];

    fn snap_names(store: &Store) -> BTreeSet<String> {
        std::fs::read_dir(store.dir())
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|n| (*n).to_owned()).collect()
    }

    /// The pointer file of the snapshots the toy extractor of `extracted_under` reads.
    fn pointer(extracted_under: u64) -> String {
        pointer_name(stamped(extracted_under))
    }

    /// Each row: two commits advanced one after the other in one store, and in another store
    /// from empty under another extractor first, so both reach the same sequence number. The two
    /// snapshot files must be byte-identical.
    #[test]
    fn advance_is_path_independent() {
        let cases: &[(&str, Files, Files)] =
            &[("A then B", A, B), ("B then A", B, A), ("A then A", A, A)];
        for (name, first, second) in cases {
            let (x, y) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
            let source = MemSource::default();
            let group = CountingGroup::default();
            let incremental = Store::new(x.path());
            let tip = source.commit("c1", first);
            incremental
                .advance(&source, &ToyExtract::new(FP), &group, &tip)
                .unwrap();
            let target = source.commit("c2", second);
            let walked = incremental
                .advance(&source, &ToyExtract::new(FP), &group, &target)
                .unwrap();
            let cold = Store::new(y.path());
            cold.advance(&source, &ToyExtract::new(FP + 1), &group, &target)
                .unwrap();
            let direct = cold
                .advance(&source, &ToyExtract::new(FP), &group, &target)
                .unwrap();
            assert_eq!(walked.stamp(), direct.stamp(), "{name}: stamps");
            let file = format!("{:06}.snap", direct.stamp().seq);
            assert_eq!(
                std::fs::read(incremental.dir().join(&file)).unwrap(),
                std::fs::read(cold.dir().join(&file)).unwrap(),
                "{name}: the two snapshots' bytes"
            );
        }
    }

    /// Each row: a second commit, the paths advancing to it must extract, how many blobs it
    /// reads, and whether groupings are recomputed.
    #[test]
    fn advance_reextracts_only_changed_blobs() {
        let modified: Vec<(&str, &str)> = A
            .iter()
            .map(|&(p, t)| {
                if p == "src/b.rs" {
                    (p, "def beta2\n")
                } else {
                    (p, t)
                }
            })
            .collect();
        let added: Vec<(&str, &str)> = A.iter().copied().chain([("src/c.rs", "def c\n")]).collect();
        let deleted: Vec<(&str, &str)> = A
            .iter()
            .copied()
            .filter(|(p, _)| *p != "tests/t.rs")
            .collect();
        let renamed: Vec<(&str, &str)> = A
            .iter()
            .map(|&(p, t)| {
                if p == "src/a.rs" {
                    ("src/z.rs", t)
                } else {
                    (p, t)
                }
            })
            .collect();
        let shared: Vec<(&str, &str)> = A
            .iter()
            .map(|&(p, t)| {
                if p.starts_with("docs/") {
                    (p, "def twin2\n")
                } else {
                    (p, t)
                }
            })
            .collect();
        let cases: Vec<(&str, Commit, Paths, usize, usize)> = vec![
            ("modified", modified, &["src/b.rs"], 1, 1),
            ("added", added, &["src/c.rs"], 1, 1),
            ("deleted", deleted, &[], 0, 1),
            ("renamed", renamed, &["src/z.rs"], 1, 1),
            (
                "one new blob two paths share",
                shared,
                &["docs/one.txt", "docs/two.txt"],
                1,
                1,
            ),
            ("the same tree under a new commit", A.to_vec(), &[], 0, 0),
        ];
        for (name, files, extracted, blobs, groupings) in cases {
            let root = tempfile::tempdir().unwrap();
            let store = Store::new(root.path());
            let source = MemSource::default();
            let group = CountingGroup::default();
            let first = source.commit("c1", A);
            store
                .advance(&source, &ToyExtract::new(FP), &group, &first)
                .unwrap();
            let (read_before, grouped_before) = (source.blobs_read(), group.calls());
            let extract = ToyExtract::new(FP);
            let target = source.commit("c2", &files);
            let snap = store.advance(&source, &extract, &group, &target).unwrap();
            assert_eq!(extract.paths(), *extracted, "{name}: extracted");
            assert_eq!(
                source.blobs_read() - read_before,
                blobs,
                "{name}: blobs read"
            );
            assert_eq!(
                group.calls() - grouped_before,
                groupings,
                "{name}: groupings"
            );
            assert_eq!(snap.stamp().tip, target, "{name}: stamp");
            let paths: Vec<&str> = snap.files().map(|f| f.path).collect();
            let mut expected: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
            expected.sort_unstable();
            assert_eq!(paths, expected, "{name}: files");
        }
    }

    /// Each row: the fingerprint of the second advance to the same tip, and how many files it
    /// must extract: none when it matches, every file when it differs.
    #[test]
    fn a_fingerprint_change_advances_from_empty() {
        let cases = [
            ("same fingerprint", FP, 0),
            ("new fingerprint", FP + 7, A.len()),
        ];
        for (name, extracted_under, extracted) in cases {
            let root = tempfile::tempdir().unwrap();
            let store = Store::new(root.path());
            let source = MemSource::default();
            let tip = source.commit("c1", A);
            let group = CountingGroup::default();
            store
                .advance(&source, &ToyExtract::new(FP), &group, &tip)
                .unwrap();
            let extract = ToyExtract::new(extracted_under);
            let snap = store.advance(&source, &extract, &group, &tip).unwrap();
            assert_eq!(extract.calls(), extracted, "{name}: extracted");
            assert_eq!(
                snap.stamp().fingerprint,
                stamped(extracted_under),
                "{name}: fingerprint"
            );
            assert!(
                store.current(stamped(extracted_under)).unwrap().is_some(),
                "{name}: current"
            );
        }
    }

    /// One open of the shared store: its name, which fingerprint opens it, the commit it moves
    /// HEAD to first if any, how many files that open extracts, and which fingerprints then hold a
    /// snapshot.
    type Open = (&'static str, usize, Option<Files>, usize, [bool; 3]);

    /// Each row: one open of the store, under the extractor of a fingerprint, after checking out
    /// the commit named, if any. Two fingerprints sharing one store each keep their snapshot, so
    /// neither re-extracts after its first open; a third reaps the snapshot of the one advanced
    /// least recently, and the store never holds more than two.
    #[test]
    fn the_two_most_recently_advanced_fingerprints_keep_their_snapshots() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path());
        let source = MemSource::default();
        let group = CountingGroup::default();
        let extractors = [FP, FP + 1, FP + 2].map(ToyExtract::new);
        let [a, b, c] = [0, 1, 2];
        let rows: [Open; 8] = [
            ("a's first open", a, Some(A), A.len(), [true, false, false]),
            ("b's first open", b, None, A.len(), [true, true, false]),
            ("a again", a, None, 0, [true, true, false]),
            ("b again", b, None, 0, [true, true, false]),
            (
                "a advances to a new commit",
                a,
                Some(B),
                2,
                [true, true, false],
            ),
            (
                "c reaps b, advanced least recently",
                c,
                None,
                B.len(),
                [true, false, true],
            ),
            ("a again after c", a, None, 0, [true, false, true]),
            (
                "b re-extracts and reaps a",
                b,
                None,
                B.len(),
                [false, true, true],
            ),
        ];
        for (step, (name, opener, checkout, extracted, held)) in rows.into_iter().enumerate() {
            if let Some(files) = checkout {
                source.commit(&format!("c{step}"), files);
            }
            let extract = &extractors[opener];
            let before = extract.calls();
            open_current_in(&store, &source, extract, &group, Mode::Write).unwrap();
            assert_eq!(extract.calls() - before, extracted, "{name}: extracted");
            let holds =
                [FP, FP + 1, FP + 2].map(|fp| store.current(stamped(fp)).unwrap().is_some());
            assert_eq!(holds, held, "{name}: which fingerprints hold a snapshot");
            let listed = snap_names(&store);
            let (snapshots, pointers): (Vec<&String>, Vec<&String>) = listed
                .iter()
                .filter(|n| n.as_str() != LOCK)
                .partition(|n| seq_of(n).is_some());
            let held_count = held.iter().filter(|h| **h).count();
            assert_eq!(snapshots.len(), held_count, "{name}: snapshot files");
            assert_eq!(
                pointers.len(),
                held_count,
                "{name}: pointer files, one per fingerprint held"
            );
        }
    }

    /// The counting grouper, at a generation of its own.
    struct GroupedAt {
        generation: u64,
        counting: CountingGroup,
    }

    impl Group for GroupedAt {
        fn generation(&self) -> u64 {
            self.generation
        }

        fn group(&self, view: &dyn CodeView) -> Groupings {
            self.counting.group(view)
        }
    }

    /// Each row: the grouper's generation at a second advance of the same tip. A new generation
    /// is a new fingerprint, so the snapshot is computed anew and regrouped; the same one keeps
    /// the snapshot in force.
    #[test]
    fn a_grouping_generation_change_regroups_the_snapshot() {
        let cases = [("the same generation", 1, 0), ("a new generation", 2, 1)];
        for (name, generation, regrouped) in cases {
            let root = tempfile::tempdir().unwrap();
            let store = Store::new(root.path());
            let source = MemSource::default();
            let tip = source.commit("c1", A);
            let extract = ToyExtract::new(FP);
            let first = GroupedAt {
                generation: 1,
                counting: CountingGroup::default(),
            };
            store.advance(&source, &extract, &first, &tip).unwrap();
            let second = GroupedAt {
                generation,
                counting: CountingGroup::default(),
            };
            let snap = store.advance(&source, &extract, &second, &tip).unwrap();
            assert_eq!(second.counting.calls(), regrouped, "{name}: regrouped");
            assert_eq!(
                snap.stamp().fingerprint,
                fingerprint(&extract, &second),
                "{name}: the stamp folds both generations"
            );
        }
    }

    /// Each row: one part of the fingerprint changed. Each part changes the fingerprint, so a
    /// snapshot computed under another extractor, grouping or settled generation is not trusted.
    #[test]
    fn every_part_of_the_fingerprint_changes_it() {
        let base = [FP, 1, SETTLED_GENERATION];
        let cases = [
            ("the extractor's fingerprint", 0),
            ("the grouping generation", 1),
            ("the settled generation", 2),
        ];
        for (name, part) in cases {
            let mut changed = base;
            changed[part] += 1;
            assert_ne!(folded(changed), folded(base), "{name}");
        }
        assert_eq!(
            fingerprint(&ToyExtract::new(FP), &CountingGroup::default()),
            folded([
                FP,
                CountingGroup::default().generation(),
                SETTLED_GENERATION
            ]),
        );
    }

    /// Each row: a damage to the published store. The damaged snapshot reads as absent, and the
    /// next advance re-extracts every file and repairs it.
    #[test]
    fn a_corrupt_snapshot_is_treated_as_absent() {
        type Damage = Box<dyn Fn(&Path)>;
        let snap = |dir: &Path| dir.join("000001.snap");
        let flip = |at: fn(&[u8]) -> usize| -> Damage {
            Box::new(move |dir: &Path| {
                let path = dir.join("000001.snap");
                let mut bytes = std::fs::read(&path).unwrap();
                let i = at(&bytes);
                bytes[i] ^= 0x20;
                std::fs::write(&path, bytes).unwrap();
            })
        };
        let mut cases: Vec<(String, Damage)> = vec![
            (
                "truncated".into(),
                Box::new(move |dir: &Path| {
                    let bytes = std::fs::read(snap(dir)).unwrap();
                    std::fs::write(snap(dir), &bytes[..bytes.len() / 2]).unwrap();
                }),
            ),
            ("magic".into(), flip(|_| 0)),
            ("version".into(), flip(|_| 4)),
            ("header".into(), flip(|_| 14)),
            (
                "the pointer names a missing file".into(),
                Box::new(|dir: &Path| {
                    std::fs::write(dir.join(pointer(FP)), "000007.snap").unwrap()
                }),
            ),
            (
                "the pointer names a path outside the store".into(),
                Box::new(|dir: &Path| {
                    std::fs::write(dir.join(pointer(FP)), "../000001.snap").unwrap()
                }),
            ),
            (
                "the pointer is empty".into(),
                Box::new(|dir: &Path| std::fs::write(dir.join(pointer(FP)), "").unwrap()),
            ),
            (
                "the snapshot's seq differs from its name".into(),
                Box::new(move |dir: &Path| {
                    std::fs::rename(snap(dir), dir.join("000003.snap")).unwrap();
                    std::fs::write(dir.join(pointer(FP)), "000003.snap").unwrap();
                }),
            ),
        ];
        for kind in 1..=13u16 {
            cases.push((
                format!("section {kind}"),
                Box::new(move |dir: &Path| {
                    let path = dir.join("000001.snap");
                    let mut bytes = std::fs::read(&path).unwrap();
                    let range = Container::parse(
                        &bytes,
                        crate::code::snapshot::MAGIC,
                        crate::code::snapshot::VERSION,
                    )
                    .unwrap()
                    .section_range(kind)
                    .unwrap();
                    let at = if range.is_empty() {
                        range.start - 1
                    } else {
                        range.start
                    };
                    bytes[at] ^= 0x20;
                    std::fs::write(&path, bytes).unwrap();
                }),
            ));
        }
        let rich: Vec<(&str, &str)> = A
            .iter()
            .copied()
            .chain([("src/u.rs", "def u\nuse type U\n")])
            .collect();
        for (name, damage) in cases {
            let root = tempfile::tempdir().unwrap();
            let store = Store::new(root.path());
            let source = MemSource::default();
            let group = CountingGroup::default();
            let tip = source.commit("c1", &rich);
            store
                .advance(&source, &ToyExtract::new(FP), &group, &tip)
                .unwrap();
            damage(store.dir());
            assert!(
                store.current(stamped(FP)).unwrap().is_none(),
                "{name}: read as absent"
            );
            let extract = ToyExtract::new(FP);
            let repaired = store.advance(&source, &extract, &group, &tip).unwrap();
            assert_eq!(extract.calls(), rich.len(), "{name}: advanced from empty");
            assert_eq!(repaired.stamp().tip, tip, "{name}: repaired stamp");
            assert!(
                store.current(stamped(FP)).unwrap().is_some(),
                "{name}: repaired"
            );
        }
    }

    /// Each row: a store file replaced by a directory, which every OS refuses to read as a file
    /// with an error other than not-found. Opening the store, in either mode, is that error,
    /// never an empty store advanced over.
    #[test]
    fn a_store_file_that_cannot_be_read_is_an_error_never_an_empty_store() {
        let cases = [
            ("the snapshot the pointer names", "000001.snap".to_owned()),
            ("the pointer", pointer(FP)),
        ];
        for (name, file) in cases {
            for mode in [Mode::Read, Mode::Write] {
                let root = tempfile::tempdir().unwrap();
                let store = Store::new(root.path());
                let source = MemSource::default();
                let group = CountingGroup::default();
                let first = source.commit("c1", A);
                store
                    .advance(&source, &ToyExtract::new(FP), &group, &first)
                    .unwrap();
                std::fs::remove_file(store.dir().join(&file)).unwrap();
                std::fs::create_dir(store.dir().join(&file)).unwrap();
                let before = snap_names(&store);
                source.commit("c2", B);
                let opened = open_current_in(&store, &source, &ToyExtract::new(FP), &group, mode);
                assert!(opened.is_err(), "{name}, {mode:?}: the read error surfaces");
                assert_eq!(
                    snap_names(&store),
                    before,
                    "{name}, {mode:?}: nothing published or reaped"
                );
            }
        }
    }

    /// An advance reaps the facts memo: GIVEN a memo holding a fresh entry of the store's
    /// fingerprint, one unread for longer than the stated age, and one of another fingerprint,
    /// WHEN the store advances, THEN the fresh entry alone stays.
    #[test]
    fn an_advance_reaps_the_memo() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path());
        let (source, group) = (MemSource::default(), CountingGroup::default());
        let memo = store.memo_dir();
        std::fs::create_dir_all(&memo).unwrap();
        let entry = |fp: u64, name: &str| format!("{fp:016x}-{name}.facts");
        let (fresh, old) = (entry(stamped(FP), "fresh"), entry(stamped(FP), "old"));
        let other = entry(stamped(FP + 1), "other");
        for name in [&fresh, &old, &other] {
            std::fs::write(memo.join(name), b"an entry").unwrap();
        }
        let unread = MemoCap::STATED.age + Duration::from_secs(60);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(memo.join(&old));
        file.unwrap()
            .set_modified(SystemTime::now() - unread)
            .unwrap();
        let tip = source.commit("c1", A);
        (store.advance(&source, &ToyExtract::new(FP), &group, &tip)).unwrap();
        let left: Vec<String> = (std::fs::read_dir(&memo).unwrap().flatten())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, [fresh]);
    }

    /// Fails one publish, as a crash at that step would: a torn temp, or no pointer rename.
    struct Crash {
        torn_snapshot: bool,
    }

    impl Publish for Crash {
        fn publish(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
            let is_pointer = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_pointer);
            match (self.torn_snapshot, is_pointer) {
                (true, false) => {
                    let tmp = path.with_extension("snap.4242.0.tmp");
                    std::fs::write(tmp, &bytes[..bytes.len() / 2]).unwrap();
                    Err("crashed mid-write".into())
                }
                (false, true) => Err("crashed between the renames".into()),
                _ => AtomicPublish.publish(path, bytes),
            }
        }
    }

    /// Each row: where the crash falls. The pointer keeps the old stamp, and the next advance
    /// publishes past the debris and reaps it.
    #[test]
    fn a_crash_between_renames_leaves_the_old_stamp_current() {
        let cases = [
            ("crash after the snapshot rename", false, "000003.snap"),
            ("crash while writing the snapshot", true, "000002.snap"),
        ];
        for (name, torn_snapshot, next) in cases {
            let root = tempfile::tempdir().unwrap();
            let source = MemSource::default();
            let group = CountingGroup::default();
            let first = source.commit("c1", A);
            Store::new(root.path())
                .advance(&source, &ToyExtract::new(FP), &group, &first)
                .unwrap();
            let target = source.commit("c2", B);
            let crashing = Store::with_publish(root.path(), Box::new(Crash { torn_snapshot }));
            assert!(
                crashing
                    .advance(&source, &ToyExtract::new(FP), &group, &target)
                    .is_err(),
                "{name}: the crash surfaces"
            );
            let store = Store::new(root.path());
            assert_eq!(
                store
                    .current(stamped(FP))
                    .unwrap()
                    .map(|s| s.stamp().tip.clone()),
                Some(first),
                "{name}: the old stamp stays current"
            );
            let snap = store
                .advance(&source, &ToyExtract::new(FP), &group, &target)
                .unwrap();
            assert_eq!(snap.stamp().tip, target, "{name}: advanced");
            assert_eq!(
                snap_names(&store),
                names(&[&pointer(FP), LOCK, next]),
                "{name}: debris reaped"
            );
        }
    }

    /// A damage done to one file's facts.
    type Damage = fn(&mut FileFacts);

    /// The toy extractor, with each file's facts damaged so they encode but fail to validate.
    struct Damaging {
        toy: ToyExtract,
        damage: Damage,
    }

    impl Extract for Damaging {
        fn fingerprint(&self) -> u64 {
            self.toy.fingerprint()
        }

        fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts {
            let mut facts = self.toy.extract(entry, bytes);
            (self.damage)(&mut facts);
            facts
        }
    }

    /// Each row: facts that encode but fail the snapshot's validation. The advance is an error,
    /// and nothing is published or reaped: the old snapshot stays current beside the debris.
    #[test]
    fn a_snapshot_that_fails_validation_is_an_error_and_never_published() {
        let cases: [(&str, Damage); 2] = [
            ("a parent outside the file", |facts| {
                if let Some(def) = facts.defs.first_mut() {
                    def.parent = Some(99);
                }
            }),
            ("an enclosing def outside the file", |facts| {
                if let Some(mention) = facts.mentions.first_mut() {
                    mention.enclosing = Some(99);
                }
            }),
        ];
        for (name, damage) in cases {
            let root = tempfile::tempdir().unwrap();
            let source = MemSource::default();
            let group = CountingGroup::default();
            let store = Store::new(root.path());
            let first = source.commit("c1", A);
            store
                .advance(&source, &ToyExtract::new(FP), &group, &first)
                .unwrap();
            std::fs::write(store.dir().join("000000.snap.4242.0.tmp"), b"debris").unwrap();
            let before = snap_names(&store);
            let target = source.commit("c2", B);
            let damaging = Damaging {
                toy: ToyExtract::new(FP),
                damage,
            };
            assert!(
                store.advance(&source, &damaging, &group, &target).is_err(),
                "{name}: the advance fails"
            );
            assert_eq!(
                snap_names(&store),
                before,
                "{name}: nothing published or reaped"
            );
            assert_eq!(
                store
                    .current(stamped(FP))
                    .unwrap()
                    .map(|s| s.stamp().tip.clone()),
                Some(first),
                "{name}: the old stamp stays current"
            );
        }
    }

    /// Opens the store before and after every publish, recording the tree it reads.
    struct Probe {
        root: PathBuf,
        seen: Mutex<Vec<Option<String>>>,
    }

    impl Probe {
        fn look(&self) {
            let tree = Store::new(&self.root)
                .current(stamped(FP))
                .unwrap()
                .map(|s| s.stamp().tip.tree.clone());
            self.seen.lock().unwrap().push(tree);
        }
    }

    impl Publish for &'static Probe {
        fn publish(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
            self.look();
            let published = AtomicPublish.publish(path, bytes);
            self.look();
            published
        }
    }

    /// Each row: a transition. A reader opening the store at every publish step sees the whole
    /// old snapshot, then the whole new one, never a part or nothing.
    #[test]
    fn a_reader_never_sees_a_partial_snapshot() {
        let cases: &[(&str, Files, Files)] = &[("A to B", A, B), ("B to A", B, A)];
        for (name, first, second) in cases {
            let root = tempfile::tempdir().unwrap();
            let source = MemSource::default();
            let group = CountingGroup::default();
            let old = source.commit("c1", first);
            Store::new(root.path())
                .advance(&source, &ToyExtract::new(FP), &group, &old)
                .unwrap();
            let probe: &'static Probe = Box::leak(Box::new(Probe {
                root: root.path().to_path_buf(),
                seen: Mutex::new(Vec::new()),
            }));
            let new = source.commit("c2", second);
            Store::with_publish(root.path(), Box::new(probe))
                .advance(&source, &ToyExtract::new(FP), &group, &new)
                .unwrap();
            let (o, n) = (Some(old.tree.clone()), Some(new.tree.clone()));
            assert_eq!(
                *probe.seen.lock().unwrap(),
                vec![o.clone(), o.clone(), o, n],
                "{name}: what the reader saw"
            );
        }
    }

    /// Each row: how many advancers race to one tip. One extracts, the others find it done, and
    /// one snapshot remains.
    #[test]
    fn two_advancers_converge_on_one_snapshot() {
        for advancers in [2usize, 4] {
            let root = tempfile::tempdir().unwrap();
            let source = MemSource::default();
            let tip = source.commit("c1", A);
            let barrier = std::sync::Barrier::new(advancers);
            let extracts: Vec<ToyExtract> = (0..advancers).map(|_| ToyExtract::new(FP)).collect();
            let stamps: Vec<_> = std::thread::scope(|s| {
                let handles: Vec<_> = extracts
                    .iter()
                    .map(|extract| {
                        let (source, tip, barrier, root) = (&source, &tip, &barrier, root.path());
                        s.spawn(move || {
                            barrier.wait();
                            Store::new(root)
                                .advance(source, extract, &CountingGroup::default(), tip)
                                .map(|snap| snap.stamp().clone())
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap().unwrap())
                    .collect()
            });
            assert!(
                stamps.iter().all(|s| s == &stamps[0]),
                "{advancers}: one stamp"
            );
            assert_eq!(stamps[0].seq, 1, "{advancers}: one advance");
            let extracted: usize = extracts.iter().map(ToyExtract::calls).sum();
            assert_eq!(extracted, A.len(), "{advancers}: extracted once");
            assert_eq!(
                snap_names(&Store::new(root.path())),
                names(&[&pointer(FP), LOCK, "000001.snap"]),
                "{advancers}: one snapshot"
            );
        }
    }

    /// Each row: debris planted beside the current snapshot, and whether the advance that follows
    /// moves the tip. Either way every unreferenced snapshot and temp goes, and other files stay.
    #[test]
    fn the_reaper_removes_unreferenced_snapshots_and_temps() {
        let cases = [
            ("an advance to a new tip", true, "000010.snap"),
            ("an advance with nothing to do", false, "000001.snap"),
        ];
        for (name, moves, kept) in cases {
            let root = tempfile::tempdir().unwrap();
            let store = Store::new(root.path());
            let source = MemSource::default();
            let group = CountingGroup::default();
            let first = source.commit("c1", A);
            store
                .advance(&source, &ToyExtract::new(FP), &group, &first)
                .unwrap();
            let pointer_temp = format!(".{}.77.1.tmp", pointer(FP));
            for debris in [
                "000000.snap",
                "000009.snap",
                ".000004.snap.77.0.tmp",
                &pointer_temp,
                "notes.txt",
            ] {
                std::fs::write(store.dir().join(debris), b"x").unwrap();
            }
            let target = if moves { source.commit("c2", B) } else { first };
            store
                .advance(&source, &ToyExtract::new(FP), &group, &target)
                .unwrap();
            assert_eq!(
                snap_names(&store),
                names(&[&pointer(FP), LOCK, kept, "notes.txt"]),
                "{name}"
            );
        }
    }

    /// The snapshot in force is read through a mapping of its file, never a copy. Later advances
    /// publish new files and the reaper unlinks the mapped one, never writing it in place, so
    /// the mapping reads whole after.
    #[test]
    fn the_snapshot_in_force_is_mapped_and_outlives_its_unlink() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path());
        let source = MemSource::default();
        let group = CountingGroup::default();
        let tip = source.commit("c1", A);
        let written = store
            .advance(&source, &ToyExtract::new(FP), &group, &tip)
            .unwrap();
        let read = store
            .current(stamped(FP))
            .unwrap()
            .expect("a snapshot in force");
        assert!(read.is_mapped(), "read through a mapping");
        let mapped: Vec<String> = snap_names(&store)
            .into_iter()
            .filter(|n| n.ends_with(SNAP))
            .collect();
        for (name, files) in [("c2", B), ("c3", A)] {
            let tip = source.commit(name, files);
            store
                .advance(&source, &ToyExtract::new(FP), &group, &tip)
                .unwrap();
        }
        let left = snap_names(&store);
        assert!(
            mapped.iter().all(|name| !left.contains(name)),
            "the reaper unlinked {mapped:?}: {left:?}"
        );
        assert_eq!(
            read.bytes(),
            written.bytes(),
            "the unlinked file reads whole"
        );
        for file in written.files() {
            assert_eq!(
                read.file(file.path),
                written.file(file.path),
                "{}",
                file.path
            );
        }
    }

    /// Each row: how many reads the reaper wins. The reader re-reads its pointer and retries, up to
    /// its bound; past it, the store reads as absent.
    #[test]
    fn a_reader_retries_when_the_reaper_wins() {
        let cases = [
            ("the reaper wins once", 1usize, true),
            ("the reaper always wins", 99, false),
        ];
        for (name, wins, found) in cases {
            let root = tempfile::tempdir().unwrap();
            let store = Store::new(root.path());
            let source = MemSource::default();
            let group = CountingGroup::default();
            let first = source.commit("c1", A);
            store
                .advance(&source, &ToyExtract::new(FP), &group, &first)
                .unwrap();
            let target = source.commit("c2", B);
            let reads = Mutex::new(0usize);
            let read = |path: &Path| -> std::io::Result<Published> {
                let mut n = reads.lock().unwrap();
                *n += 1;
                if *n == 1 {
                    Store::new(root.path())
                        .advance(&source, &ToyExtract::new(FP), &group, &target)
                        .unwrap();
                }
                if *n <= wins {
                    return Err(std::io::ErrorKind::NotFound.into());
                }
                Published::open(path)
            };
            let snap = store.current_with(stamped(FP), &read).unwrap();
            assert_eq!(
                snap.map(|s| s.stamp().tip.clone()),
                found.then(|| target.clone()),
                "{name}: what the reader got"
            );
            assert_eq!(
                *reads.lock().unwrap(),
                wins.min(3) + usize::from(found),
                "{name}: reads"
            );
        }
    }

    /// Each row: a change to the file `src/b.rs` that `src/a.rs` calls into. `src/a.rs`'s facts
    /// stay exactly as they were and it is not extracted again.
    #[test]
    fn a_file_change_never_rewrites_another_files_facts() {
        let cases = [
            ("the callee renamed", "def gamma\n"),
            ("the callee deleted", ""),
            ("the callee nested", "def outer\n def beta\n"),
        ];
        for (name, callee) in cases {
            let root = tempfile::tempdir().unwrap();
            let store = Store::new(root.path());
            let source = MemSource::default();
            let group = CountingGroup::default();
            let before = store
                .advance(
                    &source,
                    &ToyExtract::new(FP),
                    &group,
                    &source.commit("c1", A),
                )
                .unwrap();
            let files: Vec<(&str, &str)> = A
                .iter()
                .map(|&(p, t)| if p == "src/b.rs" { (p, callee) } else { (p, t) })
                .collect();
            let extract = ToyExtract::new(FP);
            let after = store
                .advance(&source, &extract, &group, &source.commit("c2", &files))
                .unwrap();
            assert_eq!(extract.paths(), vec!["src/b.rs"], "{name}: extracted");
            for path in ["src/a.rs", "tests/t.rs"] {
                assert_eq!(
                    after.file(path),
                    before.file(path),
                    "{name}: {path}'s facts"
                );
            }
        }
    }

    /// Fails every publish, as a read-only store would.
    struct ReadOnly;

    impl Publish for ReadOnly {
        fn publish(&self, path: &Path, _: &[u8]) -> Result<(), String> {
            Err(format!("{} is read-only", path.display()))
        }
    }

    /// A snapshot the source says the checkout is still at answers without the source being
    /// asked for its head; once the checkout moves, the head is asked and the store advances.
    #[test]
    fn a_snapshot_the_checkout_is_still_at_answers_without_asking_the_head() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(root.path());
        let source = MemSource::default();
        let (extract, group) = (ToyExtract::new(FP), CountingGroup::default());
        let first = source.commit("c1", A);
        let opened = open_current_in(&store, &source, &extract, &group, Mode::Read).unwrap();
        assert_eq!((opened.head, source.heads_asked()), (first.clone(), 1));
        let again = open_current_in(&store, &source, &extract, &group, Mode::Read).unwrap();
        assert_eq!(
            (&again.head, again.view.stamp().tip.clone()),
            (&first, first.clone())
        );
        assert!(again.behind.is_none());
        assert_eq!(
            source.heads_asked(),
            1,
            "the head is not asked while still at it"
        );
        let second = source.commit("c2", B);
        let moved = open_current_in(&store, &source, &extract, &group, Mode::Read).unwrap();
        assert_eq!(
            (moved.view.stamp().tip.clone(), source.heads_asked()),
            (second, 2)
        );
    }

    /// A source over a [`MemSource`] that answers a comparison with a worktree by naming the tree
    /// it compared against, and refuses its first comparison when `refuse_first`. Asked whether
    /// the checkout is still at a tip, which an open asks once it has read the snapshot, it first
    /// waits a bounded time for a comparison to start, and records whether one had.
    struct Comparing {
        inner: MemSource,
        refuse_first: std::sync::atomic::AtomicBool,
        compared: Mutex<Vec<String>>,
        started: std::sync::Condvar,
        overlapped: Mutex<Option<bool>>,
    }

    impl TreeSource for Comparing {
        fn head(&self) -> Result<Tip, String> {
            self.inner.head()
        }

        fn still_at(&self, tip: &Tip) -> bool {
            let compared = self.compared.lock().unwrap();
            let wait = std::time::Duration::from_secs(2);
            let (compared, _) = self
                .started
                .wait_timeout_while(compared, wait, |c| c.is_empty())
                .unwrap();
            let overlapped = !compared.is_empty();
            drop(compared);
            self.overlapped.lock().unwrap().get_or_insert(overlapped);
            self.inner.still_at(tip)
        }

        fn list(&self, tree: &str) -> Result<Vec<Entry>, String> {
            self.inner.list(tree)
        }

        fn changed_against_worktree(&self, tree: &str, _: &Path) -> Result<WorktreeDiff, String> {
            self.compared.lock().unwrap().push(tree.to_owned());
            self.started.notify_all();
            if self
                .refuse_first
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err("refused".into());
            }
            Ok(WorktreeDiff {
                modified: vec![tree.to_owned()],
                ..WorktreeDiff::default()
            })
        }

        fn read_worktree(
            &self,
            worktree: &Path,
            paths: &[&str],
            sink: &mut dyn FnMut(&str, &[u8]),
        ) -> Result<(), String> {
            self.inner.read_worktree(worktree, paths, sink)
        }

        fn read_blobs(
            &self,
            blobs: &[String],
            sink: &mut dyn FnMut(&str, &[u8]),
        ) -> Result<(), String> {
            self.inner.read_blobs(blobs, sink)
        }
    }

    /// Each row: whether the head moves past the snapshot in force and whether the first
    /// comparison is refused. The worktree is compared while the snapshot is read, and the answer
    /// is the comparison with the tree the opened view describes: the early one when it answered
    /// for that tree, else one made after.
    #[test]
    fn a_reader_compares_its_worktree_while_the_snapshot_is_read() {
        let cases = [
            (
                "the head the store describes",
                false,
                false,
                "c1",
                &["c1"][..],
            ),
            (
                "a head the store advances to",
                true,
                false,
                "c2",
                &["c1", "c2"][..],
            ),
            (
                "a refused early comparison",
                false,
                true,
                "c1",
                &["c1", "c1"][..],
            ),
        ];
        for (name, moves, refuse_first, answered, compared) in cases {
            let root = tempfile::tempdir().unwrap();
            let source = Comparing {
                inner: MemSource::default(),
                refuse_first: refuse_first.into(),
                compared: Mutex::default(),
                started: std::sync::Condvar::new(),
                overlapped: Mutex::default(),
            };
            let group = CountingGroup::default();
            let extract = ToyExtract::new(FP);
            let mut trees = HashMap::from([("c1", source.inner.commit("c1", A).tree)]);
            Store::new(root.path())
                .advance(
                    &source.inner,
                    &extract,
                    &group,
                    &source.inner.head().unwrap(),
                )
                .unwrap();
            if moves {
                trees.insert("c2", source.inner.commit("c2", B).tree);
            }
            let (opened, diff) = open_compared(
                root.path(),
                &source,
                &extract,
                &group,
                Path::new("worktree"),
            )
            .unwrap();
            assert_eq!(
                opened.view.stamp().tip.tree,
                trees[answered],
                "{name}: view"
            );
            assert_eq!(
                diff.map(|d| d.modified),
                Ok(vec![trees[answered].clone()]),
                "{name}: the comparison answered"
            );
            let asked: Vec<String> = compared.iter().map(|c| trees[c].clone()).collect();
            assert_eq!(*source.compared.lock().unwrap(), asked, "{name}: compared");
            assert_eq!(
                *source.overlapped.lock().unwrap(),
                Some(true),
                "{name}: a comparison started while the snapshot was read"
            );
        }
    }

    /// Each row: the mode, whether another writer holds the lock, whether writes fail, and the
    /// stamp and label the reader answers with.
    #[test]
    fn open_current_falls_back_to_the_labelled_stamp_when_it_cannot_write() {
        let in_flight = Some(Behind::AdvanceInFlight);
        let cannot = Some(Behind::CannotWrite("read-only".into()));
        let cases = [
            (
                "a reader while an advance is in flight",
                Mode::Read,
                true,
                false,
                "c1",
                in_flight,
            ),
            (
                "a reader on a read-only store",
                Mode::Read,
                false,
                true,
                "c1",
                cannot.clone(),
            ),
            (
                "a writer on a read-only store",
                Mode::Write,
                false,
                true,
                "c1",
                cannot,
            ),
            (
                "a reader with the lock free",
                Mode::Read,
                false,
                false,
                "c2",
                None,
            ),
            (
                "a writer with the lock free",
                Mode::Write,
                false,
                false,
                "c2",
                None,
            ),
        ];
        for (name, mode, held, read_only, answered, behind) in cases {
            let root = tempfile::tempdir().unwrap();
            let source = MemSource::default();
            let group = CountingGroup::default();
            Store::new(root.path())
                .advance(
                    &source,
                    &ToyExtract::new(FP),
                    &group,
                    &source.commit("c1", A),
                )
                .unwrap();
            let head = source.commit("c2", B);
            let store = if read_only {
                Store::with_publish(root.path(), Box::new(ReadOnly))
            } else {
                Store::new(root.path())
            };
            let _held = held.then(|| WriteLock::acquire(&store.lock_path()).unwrap());
            let opened =
                open_current_in(&store, &source, &ToyExtract::new(FP), &group, mode).unwrap();
            assert_eq!(
                opened.view.stamp().tip.commit.as_deref(),
                Some(answered),
                "{name}: the stamp answered from"
            );
            assert_eq!(opened.head, head, "{name}: head");
            let label = match opened.behind {
                Some(Behind::CannotWrite(e)) if e.contains("read-only") => {
                    Some(Behind::CannotWrite("read-only".into()))
                }
                other => other,
            };
            assert_eq!(label, behind, "{name}: label");
        }
        let empty = tempfile::tempdir().unwrap();
        let source = MemSource::default();
        source.commit("c1", A);
        let store = Store::with_publish(empty.path(), Box::new(ReadOnly));
        assert!(
            open_current_in(
                &store,
                &source,
                &ToyExtract::new(FP),
                &CountingGroup::default(),
                Mode::Read
            )
            .is_err(),
            "no snapshot and no write: an error"
        );
    }
}
