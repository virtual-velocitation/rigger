//! The worktree overlay (addendum section 13): the store's view of the head with the facts of
//! the caller's worktree taken over the files it differs in, so an answer describes the worktree
//! the caller stands in rather than the commit the store was advanced to.
//!
//! An [`Overlay`] decorates the head's view: the head's files the worktree modified or deleted
//! are masked, and the facts extracted from the worktree's bytes of each modified and untracked
//! file in scope take their place, merged by path. What the store settled at advance (the test
//! files and the proof calls) is settled again where a differing file reaches
//! ([`resettle`]), so the resolver reads the worktree's rows. The groupings stay the head's.
//! The facts extracted from a worktree file are kept in a [`Memo`] by content for the life of
//! the process, over a facts memo on disk under the store ([`crate::code::disk_memo`]) that a
//! one-shot reader shares: the one cache between opens, which is never stale because its key is
//! the content.

use crate::code::advance::{extract_parallel, fingerprint, open_compared, Store};
use crate::code::dir_source::content_id;
use crate::code::disk_memo::{DiskMemo, MemoCap};
use rigger_domain::code_facts::{FileFacts, ModuleDecl, Stamp};
use rigger_domain::code_view::{
    CodeView, Compared, DefRef, Entry, Extract, FactsView, FileRef, Group, Groupings, Head,
    MentionRef, Proofs, Settled, TreeSource, WorktreeDiff,
};
use rigger_domain::resolve::{resettle, ProofCall};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::SystemTime;

/// The head of the code store at `root` read for a reader in `worktree`: the view of the head
/// ([`open_compared`]), overlaid with the worktree's differing files when it differs. One
/// comparison and one read of the differing files serve the whole answer. A worktree that cannot
/// be compared, or a differing file that cannot be read, leaves the head's view alone and
/// [`Compared::Uncomparable`], so every code fact is withheld (fail closed).
pub fn open_head(
    root: &Path,
    source: &(dyn TreeSource + Sync),
    extract: &dyn Extract,
    group: &dyn Group,
    worktree: &Path,
    memo: &Memo,
) -> Result<Head, String> {
    let (opened, diff) = open_compared(root, source, extract, group, worktree)?;
    let behind = opened.behind.as_ref().map(ToString::to_string);
    let base: Box<dyn CodeView> = Box::new(opened.view);
    let compared = Compared::of(diff.as_ref().map_err(String::as_str));
    let (view, compared) = match diff {
        Ok(diff) if diff.files_in_scope().is_empty() => (base, compared),
        Ok(diff) => match read_changed(root, source, (extract, group), worktree, &diff, memo) {
            Ok(changed) => {
                let overlay = Overlay::new(base, &diff, changed);
                (Box::new(overlay) as Box<dyn CodeView>, compared)
            }
            Err(_) => (base, Compared::Uncomparable),
        },
        Err(_) => (base, compared),
    };
    Ok(Head {
        view,
        compared,
        behind,
        worktree: worktree.to_path_buf(),
    })
}

/// The facts of each file `diff` reads from the worktree ([`WorktreeDiff::read_from_worktree`]):
/// its bytes read once through `source`, each file's facts taken from `memo` when it holds them
/// for those bytes, then from the facts memo on disk under the store at `root` when `memo` reads
/// one, and the rest extracted in parallel ([`extract_parallel`]) and kept in both, the memo on
/// disk reaped once an entry is written ([`DiskMemo::reap`]). A file that is not read is an
/// error; a memo on disk that cannot be read or written is not, since extracting answers the same.
fn read_changed(
    root: &Path,
    source: &dyn TreeSource,
    (extract, group): (&dyn Extract, &dyn Group),
    worktree: &Path,
    diff: &WorktreeDiff,
    memo: &Memo,
) -> Result<Vec<FileFacts>, String> {
    let paths: Vec<&str> = diff.read_from_worktree().collect();
    let mut bytes: HashMap<String, Vec<u8>> = HashMap::with_capacity(paths.len());
    source.read_worktree(worktree, &paths, &mut |path, read| {
        bytes.insert(path.to_owned(), read.to_vec());
    })?;
    let held_under = extract.fingerprint();
    let disk = (memo.disk).map(|cap| {
        let dir = Store::new(root).memo_dir();
        DiskMemo::new(dir, fingerprint(extract, group), cap)
    });
    let (mut facts, mut missed) = (Vec::with_capacity(paths.len()), Vec::new());
    for path in paths {
        let read = (bytes.get(path)).ok_or_else(|| format!("{path} was not read"))?;
        let entry = Entry {
            path: path.to_owned(),
            blob: content_id(read),
        };
        let on_disk = || {
            let held = disk.as_ref()?.get(path, read)?;
            memo.put(held_under, held.clone());
            Some(held)
        };
        match memo.get(held_under, &entry).or_else(on_disk) {
            Some(held) => facts.push(held),
            None => missed.push(entry),
        }
    }
    let bytes_of = |path: &str| bytes.get(path).map_or(&[][..], Vec::as_slice);
    let extracted = extract_parallel(extract, &missed, &|entry| bytes_of(&entry.path))?;
    let mut wrote = false;
    for f in &extracted {
        memo.put(held_under, f.clone());
        if let Some(disk) = &disk {
            wrote |= disk.put(bytes_of(&f.path), f).is_ok();
        }
    }
    if let Some(disk) = disk.as_ref().filter(|_| wrote) {
        disk.reap(SystemTime::now());
    }
    facts.extend(extracted);
    Ok(facts)
}

/// The facts extracted from worktree files, by the extractor's fingerprint, the file's path and
/// its content id, kept for the life of the process and, when it reads one, in the facts memo on
/// disk under the store: an open reads a file read before for the same bytes without extracting
/// it again. It holds at most its bound of files' facts in the process, evicting the least
/// recently read beyond that.
pub struct Memo {
    bound: usize,
    held: Mutex<Held>,
    /// The cap of the facts memo on disk this memo reads and writes under the store; none when it
    /// keeps facts in the process alone.
    disk: Option<MemoCap>,
}

/// A memo's facts, each with the tick it was last read or kept at.
#[derive(Default)]
struct Held {
    tick: u64,
    facts: HashMap<(u64, String, String), (u64, FileFacts)>,
}

impl Memo {
    /// The process memo's bound: 256 files' facts. A worktree's differing files are what a reader
    /// re-reads, and a change set of more than 256 files takes the rest from the memo on disk, or
    /// extracts them again at each open in a memo with none; at tens of kilobytes of facts a file,
    /// the process memo stays within a few megabytes.
    pub const BOUND: usize = 256;

    /// An empty memo holding at most `bound` files' facts.
    pub fn new(bound: usize) -> Self {
        Self {
            bound,
            held: Mutex::new(Held::default()),
            disk: None,
        }
    }

    /// An empty memo holding at most `bound` files' facts in the process, over the facts memo on
    /// disk under the store, kept within `cap`.
    pub fn on_disk(bound: usize, cap: MemoCap) -> Self {
        Self {
            disk: Some(cap),
            ..Self::new(bound)
        }
    }

    /// The process's memo, of [`Self::BOUND`], over the facts memo on disk within
    /// [`MemoCap::STATED`].
    pub fn process() -> &'static Memo {
        static MEMO: LazyLock<Memo> = LazyLock::new(|| Memo::on_disk(Memo::BOUND, MemoCap::STATED));
        &MEMO
    }

    fn get(&self, fingerprint: u64, entry: &Entry) -> Option<FileFacts> {
        let mut held = self.held.lock().ok()?;
        held.tick += 1;
        let tick = held.tick;
        let key = (fingerprint, entry.path.clone(), entry.blob.clone());
        let (read, facts) = held.facts.get_mut(&key)?;
        *read = tick;
        Some(facts.clone())
    }

    fn put(&self, fingerprint: u64, facts: FileFacts) {
        let Ok(mut held) = self.held.lock() else {
            return;
        };
        held.tick += 1;
        let tick = held.tick;
        let key = (fingerprint, facts.path.clone(), facts.blob.clone());
        held.facts.insert(key, (tick, facts));
        while held.facts.len() > self.bound {
            let oldest = (held.facts.iter())
                .min_by_key(|(_, (read, _))| *read)
                .map(|(key, _)| key.clone());
            match oldest {
                Some(key) => held.facts.remove(&key),
                None => break,
            };
        }
    }
}

/// The head's view with a worktree's differing files overlaid: it answers what a view of a
/// commit of the worktree's content answers, but for the groupings, which stay the head's. It
/// holds settled rows when the head's view does, settled again where a differing file reaches.
pub struct Overlay {
    view: Merged,
    /// The test files and the proof calls, settled again on first read.
    settled: OnceLock<Resettled>,
}

/// The test files and the proof calls settled again, with the places among the calls of each
/// definition's and each file's, in proof order: a lookup reads its hits alone, as the snapshot's
/// sorted rows serve it.
struct Resettled {
    test_files: Vec<String>,
    calls: Vec<ProofCall>,
    of: HashMap<String, Vec<usize>>,
    in_file: HashMap<String, Vec<usize>>,
}

impl Resettled {
    fn new((test_files, calls): (Vec<String>, Vec<ProofCall>)) -> Self {
        let (mut of, mut in_file) = (HashMap::new(), HashMap::new());
        for (at, c) in calls.iter().enumerate() {
            let held: &mut Vec<usize> = of.entry(c.target.clone()).or_default();
            held.push(at);
            if let Some(file) = c.file() {
                let held: &mut Vec<usize> = in_file.entry(file.to_owned()).or_default();
                held.push(at);
            }
        }
        Self {
            test_files,
            calls,
            of,
            in_file,
        }
    }
}

impl Overlay {
    /// `base` with the files `diff` names differing overlaid by `changed`, the facts read from the
    /// worktree's bytes of each file it reads from the worktree.
    pub fn new(base: Box<dyn CodeView>, diff: &WorktreeDiff, changed: Vec<FileFacts>) -> Self {
        let hidden = diff
            .files_in_scope()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let changed = FactsView::new(base.stamp().clone(), changed);
        Self {
            view: Merged {
                base,
                changed,
                hidden,
            },
            settled: OnceLock::new(),
        }
    }

    fn resettled(&self) -> &Resettled {
        self.settled.get_or_init(|| {
            let differing = self.view.hidden.iter().map(String::as_str).collect();
            Resettled::new(resettle(&*self.view.base, &self.view, &differing))
        })
    }
}

impl CodeView for Overlay {
    fn stamp(&self) -> &Stamp {
        self.view.stamp()
    }
    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
        self.view.files()
    }
    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        self.view.file(path)
    }
    fn intent_facts(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        self.view.intent_facts(path)
    }
    fn modules(&self, path: &str) -> Option<Cow<'_, [ModuleDecl]>> {
        self.view.modules(path)
    }
    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
        self.view.defs_named(leaf)
    }
    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
        self.view.mentions_named(leaf)
    }
    fn groupings(&self) -> &Groupings {
        self.view.groupings()
    }
    fn def_leaves(&self) -> Vec<String> {
        self.view.def_leaves()
    }
    fn settled(&self) -> Option<&dyn Settled> {
        self.view.base.settled().map(|_| self as &dyn Settled)
    }
}

impl Settled for Overlay {
    fn test_files(&self) -> Vec<&str> {
        self.resettled()
            .test_files
            .iter()
            .map(String::as_str)
            .collect()
    }

    fn proofs(&self, of: Proofs<'_>) -> Vec<ProofCall> {
        let settled = self.resettled();
        let at = match of {
            Proofs::All => {
                return settled
                    .calls
                    .iter()
                    .inspect(|_| read_proof_row())
                    .cloned()
                    .collect()
            }
            Proofs::Of(id) => settled.of.get(id),
            Proofs::In(path) => settled.in_file.get(path),
        };
        let at = at.map_or(&[][..], Vec::as_slice).iter();
        at.inspect(|_| read_proof_row())
            .map(|&at| settled.calls[at].clone())
            .collect()
    }
}

/// Counts one proof row an overlay's lookup read, so a test sees how many rows a lookup reads.
fn read_proof_row() {
    #[cfg(test)]
    tests::PROOF_ROWS_READ.with(|read| read.set(read.get() + 1));
}

/// The head's view `base` with the files at `hidden` masked and `changed`'s facts merged in by
/// path: the overlay's facts, with nothing settled.
struct Merged {
    base: Box<dyn CodeView>,
    changed: FactsView,
    /// The head's files whose facts no longer describe the worktree: every differing file.
    hidden: BTreeSet<String>,
}

impl Merged {
    /// Whether the file at `path` is read from the worktree's facts rather than the head's.
    fn hides(&self, path: &str) -> bool {
        self.hidden.contains(path)
    }

    /// The view the file at `path` is read from.
    fn holder(&self, path: &str) -> &dyn CodeView {
        match self.hides(path) {
            true => &self.changed,
            false => &*self.base,
        }
    }
}

/// `base` and `changed`, two sequences each in path order with no path in both, merged in path
/// order by `path`.
fn by_path<'a, T: 'a>(
    base: impl Iterator<Item = T> + 'a,
    changed: impl Iterator<Item = T> + 'a,
    path: fn(&T) -> &str,
) -> Box<dyn Iterator<Item = T> + 'a> {
    let (mut base, mut changed) = (base.peekable(), changed.peekable());
    Box::new(std::iter::from_fn(move || {
        match (base.peek(), changed.peek()) {
            (Some(b), Some(c)) if path(c) < path(b) => changed.next(),
            (Some(_), _) => base.next(),
            (None, _) => changed.next(),
        }
    }))
}

impl CodeView for Merged {
    fn stamp(&self) -> &Stamp {
        self.base.stamp()
    }
    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
        let base = self.base.files().filter(|f| !self.hides(f.path));
        by_path(base, self.changed.files(), |f| f.path)
    }
    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        self.holder(path).file(path)
    }
    fn intent_facts(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        self.holder(path).intent_facts(path)
    }
    fn modules(&self, path: &str) -> Option<Cow<'_, [ModuleDecl]>> {
        self.holder(path).modules(path)
    }
    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
        let base = self
            .base
            .defs_named(leaf)
            .filter(|d| !self.hides(d.file.path));
        by_path(base, self.changed.defs_named(leaf), |d| d.file.path)
    }
    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
        let base = (self.base.mentions_named(leaf)).filter(|m| !self.hides(m.file.path));
        by_path(base, self.changed.mentions_named(leaf), |m| m.file.path)
    }
    fn groupings(&self) -> &Groupings {
        self.base.groupings()
    }
    /// The head's leaves and the worktree's, less each leaf only a masked file defined.
    fn def_leaves(&self) -> Vec<String> {
        let mut leaves: BTreeSet<String> = self.base.def_leaves().into_iter().collect();
        leaves.extend(self.changed.def_leaves());
        for path in &self.hidden {
            for d in self.base.file(path).iter().flat_map(|f| f.defs.iter()) {
                if self.defs_named(&d.leaf).next().is_none() {
                    leaves.remove(&d.leaf);
                }
            }
        }
        leaves.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::doubles::{CountingGroup, MemSource, ToyExtract};
    use crate::code::Store;
    use rigger_domain::code_view::{Tests, ViewFilter};
    use rigger_domain::composed::{Composed, Honesty, Indexed};
    use rigger_domain::contextgraph::{ContextGraph, Graph};

    /// What a worktree changes: each path with its bytes, or none when it deleted the file.
    type Changes<'a> = &'a [(&'a str, Option<&'a str>)];

    thread_local! {
        /// The proof rows the overlays this thread read have read ([`read_proof_row`]).
        pub(super) static PROOF_ROWS_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// A store advanced to a commit of `FILES` in `source`, read by a reader in the worktree `W`.
    struct Fixture {
        root: tempfile::TempDir,
        source: MemSource,
        extract: ToyExtract,
        group: CountingGroup,
        commit: String,
    }

    impl Fixture {
        fn new(files: &[(&str, &str)]) -> Self {
            let (root, source) = (tempfile::tempdir().unwrap(), MemSource::default());
            let (extract, group) = (ToyExtract::new(1), CountingGroup::default());
            let tip = source.commit("c1", files);
            (Store::new(root.path()).advance(&source, &extract, &group, &tip)).unwrap();
            Self {
                root,
                source,
                extract,
                group,
                commit: tip.commit.unwrap(),
            }
        }

        /// The head read once the worktree holds `changes` ([`MemSource::worktree`]).
        fn head(&self, changes: Changes, memo: &Memo) -> Head {
            self.source.worktree(changes);
            self.open(memo).unwrap()
        }

        fn open(&self, memo: &Memo) -> Result<Head, String> {
            let (root, wt) = (self.root.path(), Path::new("W"));
            open_head(root, &self.source, &self.extract, &self.group, wt, memo)
        }

        /// How many files reading the head extracts once the worktree holds `changes`.
        fn extracted(&self, changes: Changes, memo: &Memo) -> usize {
            let before = self.extract.calls();
            self.head(changes, memo);
            self.extract.calls() - before
        }

        /// The files in the store's facts memo on disk, sorted.
        fn entries(&self) -> Vec<std::path::PathBuf> {
            let dir = Store::new(self.root.path()).memo_dir();
            let listed = std::fs::read_dir(dir).into_iter().flatten().flatten();
            let mut entries: Vec<_> = listed.map(|e| e.path()).collect();
            entries.sort();
            entries
        }
    }

    /// The memo a one-shot read opens with: a new process's, over the facts memo on disk.
    fn one_shot() -> Memo {
        Memo::on_disk(Memo::BOUND, MemoCap::STATED)
    }

    /// What a store advanced to a commit of `FILES` with `changes` applied answers whole, less its
    /// groupings: the fresh extraction an overlaid answer of that worktree must equal.
    fn fresh(changes: Changes) -> String {
        let mut files: std::collections::BTreeMap<&str, &str> = FILES.iter().copied().collect();
        for &(path, text) in changes {
            match text {
                Some(text) => files.insert(path, text),
                None => files.remove(path),
            };
        }
        let files: Vec<(&str, &str)> = files.into_iter().collect();
        let (_store, head) = committed(&files);
        ungrouped(&answer(&head).0)
    }

    /// What a store advanced to a commit of `files` answers, with the store it reads from.
    fn committed(files: &[(&str, &str)]) -> (tempfile::TempDir, Head) {
        let (root, source) = (tempfile::tempdir().unwrap(), MemSource::default());
        let tip = source.commit("c2", files);
        let store = Store::new(root.path());
        let group = CountingGroup::default();
        let snap = store.advance(&source, &ToyExtract::new(1), &group, &tip);
        let head = Head {
            view: Box::new(snap.unwrap()),
            compared: Compared::Overlaid(0),
            behind: None,
            worktree: "W".into(),
        };
        (root, head)
    }

    /// The graph `head` answers whole with every test shown, and its label.
    fn answer(head: &Head) -> (Graph, String) {
        let honesty = Honesty {
            worktree: "W".into(),
            compared: head.compared,
            behind: head.behind.clone(),
        };
        let filter = ViewFilter {
            tests: Tests::Include,
        };
        let records = Indexed::default();
        let graph = Composed::new(&records, &*head.view, filter, honesty).unwrap();
        let whole = graph.whole().unwrap();
        (whole, graph.stamp_label().to_string())
    }

    fn ids(graph: &Graph) -> Vec<&str> {
        graph.nodes.iter().map(|n| n.id.as_str()).collect()
    }

    /// The tier of the edge from `from` to `to`, when the graph holds one.
    fn tier<'g>(graph: &'g Graph, from: &str, to: &str) -> Option<&'g str> {
        let edge = graph.edges.iter().find(|e| e.from == from && e.to == to);
        edge.map(|e| e.tier.as_str())
    }

    /// `graph` less its groupings: the nodes and edges a grouping names.
    fn ungrouped(graph: &Graph) -> String {
        let mut graph = graph.clone();
        graph.nodes.retain(|n| !n.id.starts_with("community/"));
        graph.edges.retain(|e| !e.to.starts_with("community/"));
        format!("{graph:#?}")
    }

    const FILES: &[(&str, &str)] = &[
        ("src/a.rs", "def alpha\ncall beta\n"),
        ("src/b.rs", "def beta\n"),
        ("tests/t.rs", "def check\ncall alpha\n"),
    ];

    /// An overlay answers the worktree: GIVEN a worktree that modified `src/a.rs`, WHEN the head
    /// is read for it, THEN the answer holds the worktree's definitions and calls of that file and
    /// not the commit's, the rest is the commit's, the whole is what a commit of the worktree's
    /// content answers but for the groupings, and the label says it describes the worktree with
    /// one file taken from it over the stamp, whose groupings it keeps.
    #[test]
    fn overlay_merges_changed_files_and_describes_the_worktree() {
        let fx = Fixture::new(FILES);
        let changed = "def alpha\ndef gamma\ncall beta\n";
        let head = fx.head(&[("src/a.rs", Some(changed))], &Memo::new(Memo::BOUND));
        assert_eq!(head.compared, Compared::Overlaid(1));
        let (graph, label) = answer(&head);
        let c1 = &fx.commit;
        assert_eq!(
            label,
            format!("describes W ({c1} + 1 file); groupings describe {c1}")
        );
        assert!(
            ids(&graph).contains(&"src/a.rs::gamma"),
            "{:?}",
            ids(&graph)
        );
        assert!(ids(&graph).contains(&"src/b.rs::beta"));
        assert_eq!(
            tier(&graph, "src/a.rs::gamma", "src/a.rs::%beta"),
            Some("inferred")
        );
        assert_eq!(tier(&graph, "src/a.rs::alpha", "src/a.rs::%beta"), None);
        let files: Vec<(&str, &str)> = (FILES.iter())
            .map(|&(p, text)| (p, if p == "src/a.rs" { changed } else { text }))
            .collect();
        let (_store, want) = committed(&files);
        assert_eq!(ungrouped(&graph), ungrouped(&answer(&want).0));
    }

    /// A deleted file is masked: GIVEN a worktree that deleted `src/b.rs`, WHEN the head is read
    /// for it, THEN no node of that file is answered, the call to `beta` resolves as one with no
    /// candidate, nothing is extracted, and the deletion counts as a file taken from the worktree.
    #[test]
    fn overlay_masks_deleted_files() {
        let fx = Fixture::new(FILES);
        let memo = Memo::new(Memo::BOUND);
        assert_eq!(fx.extracted(&[("src/b.rs", None)], &memo), 0);
        let head = fx.head(&[("src/b.rs", None)], &memo);
        assert_eq!(head.compared, Compared::Overlaid(1));
        let (graph, _) = answer(&head);
        let of_b = ids(&graph)
            .into_iter()
            .filter(|id| id.starts_with("src/b.rs"));
        assert_eq!(of_b.count(), 0, "{:?}", ids(&graph));
        let (call, beta) = ("src/a.rs::alpha", "src/a.rs::%beta");
        assert_eq!(tier(&graph, call, beta), Some("ambiguous"));
        assert!(!head.view.def_leaves().contains(&"beta".to_string()));
    }

    /// An untracked file is added: GIVEN a worktree holding `src/c.rs`, which the commit does not,
    /// WHEN the head is read for it, THEN that file's definitions and calls are answered and it
    /// alone is extracted.
    #[test]
    fn overlay_adds_untracked_files() {
        let fx = Fixture::new(FILES);
        let (memo, c) = (
            Memo::new(Memo::BOUND),
            ("src/c.rs", Some("def gamma\ncall alpha\n")),
        );
        assert_eq!(fx.extracted(&[c], &Memo::new(Memo::BOUND)), 1);
        let head = fx.head(&[c], &memo);
        assert_eq!(head.compared, Compared::Overlaid(1));
        let (graph, _) = answer(&head);
        assert!(
            ids(&graph).contains(&"src/c.rs::gamma"),
            "{:?}",
            ids(&graph)
        );
        assert_eq!(
            tier(&graph, "src/c.rs::gamma", "src/c.rs::%alpha"),
            Some("inferred")
        );
        assert_eq!(
            fx.extract.bytes_of("src/c.rs").as_deref(),
            Some(&b"def gamma\ncall alpha\n"[..])
        );
    }

    /// The memo keeps a worktree file's facts by its content: GIVEN a worktree file read once,
    /// WHEN the head is read again for the same bytes, THEN nothing is extracted; WHEN its bytes
    /// change, THEN it is extracted again; and a memo bound to one file evicts the other.
    #[test]
    fn overlay_memoises_by_content_hash() {
        let fx = Fixture::new(FILES);
        let a = ("src/a.rs", Some("def alpha\ndef gamma\n"));
        let c = ("src/c.rs", Some("def delta\n"));
        // (the memo's bound, the worktree's changes, the files the read extracts), in order.
        let cases: [(usize, Changes, usize); 7] = [
            (Memo::BOUND, &[a], 1),
            (Memo::BOUND, &[a], 0),
            (
                Memo::BOUND,
                &[("src/a.rs", Some("def alpha\n def inner\n"))],
                1,
            ),
            (Memo::BOUND, &[a, c], 1),
            (1, &[a], 1),
            (1, &[c], 1),
            (1, &[a], 1),
        ];
        let memos = [(Memo::BOUND, Memo::new(Memo::BOUND)), (1, Memo::new(1))];
        for (bound, changes, want) in cases {
            let memo = &memos.iter().find(|(b, _)| *b == bound).unwrap().1;
            assert_eq!(
                fx.extracted(changes, memo),
                want,
                "bound {bound}: {changes:?}"
            );
        }
    }

    /// The facts memo on disk serves a one-shot read and never answers content the worktree does
    /// not hold: GIVEN a worktree file edited back and forth, each read by a new process's memo,
    /// WHEN the head is read for each content, THEN a content read before is not extracted again,
    /// a content not read before is, the same bytes at another path are extracted for that path,
    /// and every answer is what a fresh extraction of the worktree answers.
    #[test]
    fn a_one_shot_read_extracts_each_content_once_and_answers_the_worktree() {
        let fx = Fixture::new(FILES);
        let (v1, v2, v3) = (
            "def alpha\ndef gamma\n",
            "def alpha\ncall beta\ndef delta\n",
            "def omega\n",
        );
        // (the worktree's changes, the files the read extracts), in order.
        let cases: [(Changes, usize); 7] = [
            (&[("src/a.rs", Some(v1))], 1),
            (&[("src/a.rs", Some(v2))], 1),
            (&[("src/a.rs", Some(v1))], 0),
            (&[("src/a.rs", Some(v2))], 0),
            (&[("src/a.rs", Some(v3))], 1),
            (&[("src/a.rs", Some(v1))], 0),
            (&[("src/a.rs", Some(v1)), ("src/c.rs", Some(v1))], 1),
        ];
        for (changes, want) in cases {
            let before = fx.extract.calls();
            let head = fx.head(changes, &one_shot());
            assert_eq!(
                fx.extract.calls() - before,
                want,
                "extracted for {changes:?}"
            );
            assert_eq!(head.compared, Compared::Overlaid(changes.len()));
            assert_eq!(ungrouped(&answer(&head).0), fresh(changes), "{changes:?}");
        }
    }

    /// An entry that does not read whole as the facts of the path and bytes asked for is absent
    /// and replaced: GIVEN an entry written by a read, then removed (a reaper racing the read),
    /// emptied, truncated, altered, or overwritten by another path's or another content's valid
    /// entry (a colliding name), WHEN a new process reads the head for the same content, THEN the
    /// file is extracted again, the answer is the fresh extraction's, and the entry is rewritten
    /// whole.
    #[test]
    fn a_damaged_entry_reads_as_absent_and_is_replaced() {
        let fx = Fixture::new(FILES);
        let (v1, v2) = ("def alpha\ndef gamma\n", "def alpha\ndef delta\n");
        let a: Changes = &[("src/a.rs", Some(v1))];
        let mut written = Vec::new();
        for changes in [&[("src/b.rs", Some(v1))][..], &[("src/a.rs", Some(v2))], a] {
            fx.head(changes, &one_shot());
            let entries = fx.entries();
            let new = entries
                .into_iter()
                .find(|e| !written.iter().any(|(w, _)| w == e));
            let new = new.unwrap_or_else(|| panic!("a read of {changes:?} writes an entry"));
            written.push((new.clone(), std::fs::read(&new).unwrap()));
        }
        let (entry, whole) = written[2].clone();
        let (of_b, of_v2) = (written[0].1.clone(), written[1].1.clone());
        let cut = |n: usize| Some(whole[..n].to_vec());
        let mut flipped = whole.clone();
        flipped[whole.len() / 2] ^= 0xff;
        // (the damage, the entry's bytes after it: none when it is removed)
        let cases: [(&str, Option<Vec<u8>>); 7] = [
            ("removed", None),
            ("emptied", Some(Vec::new())),
            ("truncated by one byte", cut(whole.len() - 1)),
            ("cut in half", cut(whole.len() / 2)),
            ("a byte flipped", Some(flipped)),
            ("another path's entry", Some(of_b)),
            ("another content's entry", Some(of_v2)),
        ];
        for (case, damaged) in cases {
            match damaged {
                Some(bytes) => std::fs::write(&entry, bytes).unwrap(),
                None => std::fs::remove_file(&entry).unwrap(),
            }
            let before = fx.extract.calls();
            let head = fx.head(a, &one_shot());
            assert_eq!(fx.extract.calls() - before, 1, "{case}: extracted");
            assert_eq!(ungrouped(&answer(&head).0), fresh(a), "{case}: answer");
            assert_eq!(
                std::fs::read(&entry).ok(),
                Some(whole.clone()),
                "{case}: entry"
            );
        }
    }

    /// A memo that cannot be written leaves the answer alone: GIVEN a store whose memo directory
    /// is a file, WHEN the head is read twice for a changed worktree, THEN each read extracts the
    /// file and answers what a fresh extraction answers.
    #[test]
    fn an_unwritable_memo_leaves_the_answer_alone() {
        let fx = Fixture::new(FILES);
        std::fs::write(Store::new(fx.root.path()).memo_dir(), b"a file").unwrap();
        let a: Changes = &[("src/a.rs", Some("def alpha\ndef gamma\n"))];
        for read in 0..2 {
            let before = fx.extract.calls();
            let head = fx.head(a, &one_shot());
            assert_eq!(fx.extract.calls() - before, 1, "read {read}");
            assert_eq!(head.compared, Compared::Overlaid(1), "read {read}");
            assert_eq!(ungrouped(&answer(&head).0), fresh(a), "read {read}");
        }
    }

    /// An open that writes the memo reaps it: GIVEN a memo holding an entry of another
    /// fingerprint, WHEN a one-shot read extracts a changed file, THEN that entry is gone and the
    /// read's own stays.
    #[test]
    fn an_open_that_writes_the_memo_reaps_it() {
        let fx = Fixture::new(FILES);
        let dir = Store::new(fx.root.path()).memo_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let other = fingerprint(&fx.extract, &fx.group) ^ 1;
        let other = dir.join(format!("{other:016x}-other.facts"));
        std::fs::write(&other, b"an entry").unwrap();
        fx.head(&[("src/a.rs", Some("def alpha\ndef gamma\n"))], &one_shot());
        let entries = fx.entries();
        assert!(!entries.contains(&other), "{entries:?}");
        assert_eq!(entries.len(), 1, "{entries:?}");
    }

    /// A reaper racing reads fails closed to extraction: GIVEN a reaper emptying the memo over
    /// and over, WHEN one-shot reads of a file edited back and forth run beside it, THEN every
    /// read is overlaid and answers what a fresh extraction answers, and the reaper's last pass
    /// leaves the memo empty.
    #[test]
    fn a_reaper_racing_reads_fails_closed_to_extraction() {
        let fx = Fixture::new(FILES);
        let empty = MemoCap {
            bytes: 0,
            ..MemoCap::STATED
        };
        let dir = Store::new(fx.root.path()).memo_dir();
        let reaper = DiskMemo::new(dir, fingerprint(&fx.extract, &fx.group), empty);
        let edits: [Changes; 2] = [
            &[("src/a.rs", Some("def alpha\ndef gamma\n"))],
            &[("src/a.rs", Some("def alpha\ncall beta\ndef delta\n"))],
        ];
        let wanted = edits.map(fresh);
        std::thread::scope(|scope| {
            let reads = scope.spawn(|| {
                for round in 0..200 {
                    let head = fx.head(edits[round % 2], &one_shot());
                    assert_eq!(head.compared, Compared::Overlaid(1), "round {round}");
                    let got = ungrouped(&answer(&head).0);
                    assert_eq!(got, wanted[round % 2], "round {round}");
                }
            });
            while !reads.is_finished() {
                reaper.reap(SystemTime::now());
            }
            reaper.reap(SystemTime::now());
            reads.join().unwrap();
        });
        assert_eq!(fx.entries(), Vec::<std::path::PathBuf>::new());
    }

    /// Resolution reads the overlaid view: GIVEN a commit whose `run` calls `helper`, which
    /// nothing defines, and a test that calls `run`, WHEN the worktree adds an untracked file that
    /// defines `helper`, THEN the call's tier moves from ambiguous to inferred and the test's
    /// proof of `run` holds; WHEN it also adds a second candidate `run`, THEN the untouched
    /// test's call proves nothing.
    #[test]
    fn resolution_over_the_overlay_describes_the_worktree() {
        let files = [
            ("src/a.rs", "def run\ncall helper\n"),
            ("tests/t.rs", "def check\ncall run\n"),
        ];
        let fx = Fixture::new(&files);
        let memo = Memo::new(Memo::BOUND);
        let (helper, second) = (
            ("src/h.rs", Some("def helper\n")),
            ("src/r.rs", Some("def run\n")),
        );
        // (the worktree's changes, the tier of the call to `helper`, the proof of `run`)
        let proved = Some(r#"["tests/t.rs:2"]"#);
        let cases: [(Changes, &str, Option<&str>); 3] = [
            (&[], "ambiguous", proved),
            (&[helper], "inferred", proved),
            (&[helper, second], "inferred", None),
        ];
        for (changes, want_tier, want_proof) in cases {
            let (graph, _) = answer(&fx.head(changes, &memo));
            let call = tier(&graph, "src/a.rs::run", "src/a.rs::%helper");
            assert_eq!(call, Some(want_tier), "{changes:?}");
            let run = graph
                .nodes
                .iter()
                .find(|n| n.id == "src/a.rs::run")
                .unwrap();
            let proof = run.attrs.get("proof_evidence").map(String::as_str);
            assert_eq!(proof, want_proof, "{changes:?}");
        }
    }

    /// The groupings stay the stamp's and the label says so: GIVEN a worktree that adds a
    /// definition, WHEN the head is read for it, THEN the community holds the commit's
    /// definitions alone and the label names the stamp as what the groupings describe; with
    /// nothing differing, the label names the stamp alone.
    #[test]
    fn groupings_under_the_overlay_are_labelled_with_the_stamp() {
        let fx = Fixture::new(FILES);
        let (memo, c1) = (Memo::new(Memo::BOUND), &fx.commit);
        let exact = fx.head(&[], &memo);
        assert_eq!(exact.compared, Compared::Overlaid(0));
        assert_eq!(answer(&exact).1, format!("describes {c1}"));
        let head = fx.head(&[("src/b.rs", Some("def beta\ndef zeta\n"))], &memo);
        let members = &head.view.groupings().communities[0].members;
        assert_eq!(members, &exact.view.groupings().communities[0].members);
        assert!(
            !members.iter().any(|m| m == "src/b.rs::zeta"),
            "{members:?}"
        );
        let want = format!("describes W ({c1} + 1 file); groupings describe {c1}");
        assert_eq!(answer(&head).1, want);
    }

    /// A proof lookup over the overlay reads only its hits, as the snapshot's do: GIVEN an overlay
    /// that settled one test file's proofs again beside the head's, WHEN the proofs of one
    /// definition or in one file are read, THEN they are its calls in proof order, and the rows
    /// read are those calls alone.
    #[test]
    fn overlay_proof_lookups_read_only_their_hits() {
        let files = [
            ("src/a.rs", "def alpha\ndef beta\ndef gamma\n"),
            (
                "tests/t.rs",
                "def check\ncall alpha\ncall beta\ncall gamma\n",
            ),
            ("tests/u.rs", "def other\ncall beta\n"),
        ];
        let fx = Fixture::new(&files);
        let u = ("tests/u.rs", Some("def other\ncall beta\ncall gamma\n"));
        let head = fx.head(&[u], &Memo::new(Memo::BOUND));
        let settled = head.view.settled().expect("the overlay settles its proofs");
        assert_eq!(settled.proofs(Proofs::All).len(), 5);
        // (the lookup, the evidence of the calls it answers, in proof order)
        let cases: [(Proofs, &[&str]); 6] = [
            (Proofs::Of("src/a.rs::alpha"), &["tests/t.rs:2"]),
            (
                Proofs::Of("src/a.rs::beta"),
                &["tests/t.rs:3", "tests/u.rs:2"],
            ),
            (
                Proofs::Of("src/a.rs::gamma"),
                &["tests/t.rs:4", "tests/u.rs:3"],
            ),
            (Proofs::Of("src/a.rs::none"), &[]),
            (Proofs::In("tests/u.rs"), &["tests/u.rs:2", "tests/u.rs:3"]),
            (
                Proofs::In("tests/t.rs"),
                &["tests/t.rs:2", "tests/t.rs:3", "tests/t.rs:4"],
            ),
        ];
        for (of, want) in cases {
            let before = PROOF_ROWS_READ.with(std::cell::Cell::get);
            let got: Vec<String> = (settled.proofs(of).into_iter())
                .map(|c| c.evidence)
                .collect();
            let read = PROOF_ROWS_READ.with(std::cell::Cell::get) - before;
            assert_eq!(got, want, "{of:?}");
            assert_eq!(read, want.len(), "rows read for {of:?}");
        }
    }

    /// A worktree whose differing file cannot be read, or that cannot be compared at all, is
    /// uncomparable: GIVEN either, WHEN the head is read for it, THEN no file is overlaid, every
    /// code fact is withheld, and the label says the worktree cannot be compared.
    #[test]
    fn an_unreadable_differing_file_leaves_the_worktree_uncomparable() {
        let fx = Fixture::new(FILES);
        let memo = Memo::new(Memo::BOUND);
        let uncompared = Fixture::new(FILES);
        fx.source.unreadable("src/a.rs");
        fx.source.worktree(&[("src/a.rs", Some("def other\n"))]);
        for (fx, case) in [(&fx, "unreadable"), (&uncompared, "no worktree")] {
            let head = fx.open(&memo).unwrap();
            assert_eq!(head.compared, Compared::Uncomparable, "{case}");
            let (graph, label) = answer(&head);
            let want = format!("describes {}; W cannot be compared; ", fx.commit);
            assert!(label.starts_with(&want), "{case}: {label}");
            let code = ids(&graph).into_iter().filter(|id| id.starts_with("src/"));
            assert_eq!(code.count(), 0, "{case}: {:?}", ids(&graph));
        }
    }
}
