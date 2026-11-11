//! The snapshot format: one immutable file holding a stamp, every file's facts, the leaf indexes,
//! the groupings and what the store settles from the tree alone (the test files and the proof
//! calls), and [`Snapshot`], the [`CodeView`] that reads one.
//!
//! The file is a [`crate::format`] container. Its header payload is the stamp: the extractor
//! fingerprint, the tip's head, commit and tree as strings, and the sequence number. A string is
//! a (u32 offset, u32 length) pair into the STRINGS pool, which holds each distinct string once;
//! an absent commit is the offset `u32::MAX`. Each other section is fixed-width rows:
//!
//! ```text
//! FILES        path, blob, lang, half u8, scope u8, flags u8 (partial 1, test file 2),
//!              then [start, end) into DEFS, MENTIONS, MODULES, DOC_CONCEPTS, DOC_LINKS,
//!              WF_NODES, WF_RELATIONS                  (rows sorted by path; one run a file)
//! DEFS         local, leaf, kind, line, end_line, parent (u32::MAX: none), flags (test)
//! MENTIONS     enclosing (u32::MAX: none), name, leaf, kind u8 (0 call, 1 tagged), tag,
//!              line, flags (test)
//! MODULES      name, def (its declaring DEFS row in the file), flags (test),
//!              [start, end) into CANDIDATES
//! CANDIDATES   path
//! DOC_*, WF_*  three strings each, as written
//! IDX_LEAF     u32 key count, keys (leaf, [start, end) into the postings) sorted by leaf,
//!              then u32 postings: DEFS rows, in path then source order
//! IDX_NAME     the same over MENTIONS rows
//! GROUPINGS    u32 count, clusters (id, label, resolution as f64 bits, hash, u32 count,
//!              members); communities then concepts
//! PROOFS       target, caller, file, line: each proof call, in proof order
//! IDX_PROOF_FILE  u32 PROOFS rows, by file then proof order
//! ```
//!
//! A FILES row's test flag and the PROOFS rows are what [`Resolver`] computes over the view, so a
//! read takes them rather than computing them ([`Settled`]).
//!
//! Encoding is a pure function of the view and its groupings: rows follow path then source
//! order, the pool interns in that order and the indexes are built sorted, so the same facts
//! always give the same bytes.

use crate::format::{narrow, put_u32, put_u64, put_u8, write_container, Container, Cursor};
use rigger_domain::code_facts::{
    Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope, Stamp, Tip,
    WfNode, WfRelation,
};
use rigger_domain::code_view::{
    Cluster, CodeView, DefRef, FileRef, Groupings, MentionRef, Proofs, Settled,
};
use rigger_domain::resolve::{ProofCall, Resolver};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, Range};
use std::path::Path;
use std::sync::OnceLock;

pub(crate) const MAGIC: &[u8; 4] = b"RGCS";
pub(crate) const VERSION: u16 = 3;
/// An absent index (parent, enclosing definition) or an absent string (commit).
const NONE: u32 = u32::MAX;

const STRINGS: u16 = 1;
const FILES: u16 = 2;
const DEFS: u16 = 3;
const MENTIONS: u16 = 4;
const MODULES: u16 = 5;
const CANDIDATES: u16 = 6;
const DOC_CONCEPTS: u16 = 7;
const DOC_LINKS: u16 = 8;
const WF_NODES: u16 = 9;
const WF_RELATIONS: u16 = 10;
const IDX_LEAF: u16 = 11;
const IDX_NAME: u16 = 12;
const GROUPINGS: u16 = 13;
const PROOFS: u16 = 14;
const IDX_PROOF_FILE: u16 = 15;

/// The width of a string reference.
const STR: usize = 8;

/// FILES row: three strings, half, scope, flags, then one [start, end) pair per list.
const F_HALF: usize = 3 * STR;
const F_SCOPE: usize = F_HALF + 1;
const F_FLAGS: usize = F_SCOPE + 1;
const F_LISTS: usize = F_FLAGS + 1;
/// The lists a file row ranges into, in row order.
const LISTS: [u16; 7] = [
    DEFS,
    MENTIONS,
    MODULES,
    DOC_CONCEPTS,
    DOC_LINKS,
    WF_NODES,
    WF_RELATIONS,
];
const L_DEFS: usize = 0;
const L_MENTIONS: usize = 1;
const L_MODULES: usize = 2;
/// The lists a file's design and workflow facts sit in.
const L_INTENT: [usize; 4] = [3, 4, 5, 6];
const FILE_ROW: usize = F_LISTS + LISTS.len() * 8;

/// DEFS row.
const D_LINE: usize = 3 * STR;
const D_END: usize = D_LINE + 4;
const D_PARENT: usize = D_END + 4;
const D_FLAGS: usize = D_PARENT + 4;
const DEF_ROW: usize = D_FLAGS + 1;

/// MENTIONS row.
const M_NAME: usize = 4;
const M_LEAF: usize = M_NAME + STR;
const M_KIND: usize = M_LEAF + STR;
const M_TAG: usize = M_KIND + 1;
const M_LINE: usize = M_TAG + STR;
const M_FLAGS: usize = M_LINE + 4;
const MENTION_ROW: usize = M_FLAGS + 1;

/// MODULES row.
const MOD_DEF: usize = STR;
const MOD_FLAGS: usize = MOD_DEF + 4;
const MOD_CANDS: usize = MOD_FLAGS + 1;
const MODULE_ROW: usize = MOD_CANDS + 8;

/// A row of three strings (DOC_*, WF_*), and an index key (leaf, start, end).
const TRIPLE_ROW: usize = 3 * STR;
const KEY_ROW: usize = STR + 8;

/// PROOFS row: target, caller, file, line.
const P_FILE: usize = 2 * STR;
const P_LINE: usize = P_FILE + STR;
const PROOF_ROW: usize = P_LINE + 4;

const KIND_CALL: u8 = 0;
const KIND_TAGGED: u8 = 1;
const FLAG_SET: u8 = 1;
/// A FILES row's flag: test code, by its scope or the test-module closure.
const FLAG_TEST: u8 = 2;

/// The string pool being written: each distinct string once, in first-use order.
#[derive(Default)]
struct Pool {
    text: String,
    at: HashMap<String, (u32, u32)>,
}

impl Pool {
    fn put(&mut self, out: &mut Vec<u8>, s: &str) -> Result<(), String> {
        let (off, len) = match self.at.get(s) {
            Some(&r) => r,
            None => {
                let r = (
                    narrow(self.text.len(), "string offset")?,
                    narrow(s.len(), "string length")?,
                );
                self.text.push_str(s);
                self.at.insert(s.to_owned(), r);
                r
            }
        };
        put_u32(out, off);
        put_u32(out, len);
        Ok(())
    }

    fn put_opt(&mut self, out: &mut Vec<u8>, s: Option<&str>) -> Result<(), String> {
        match s {
            Some(s) => self.put(out, s),
            None => {
                put_u32(out, NONE);
                put_u32(out, 0);
                Ok(())
            }
        }
    }
}

fn flag(set: bool) -> u8 {
    if set {
        FLAG_SET
    } else {
        0
    }
}

fn opt_index(i: Option<u32>) -> u32 {
    i.unwrap_or(NONE)
}

/// The sections being written, one byte buffer per kind.
#[derive(Default)]
struct Encoder {
    pool: Pool,
    sections: BTreeMap<u16, Vec<u8>>,
    /// Leaf to global DEFS rows, and to global MENTIONS rows.
    leaf_defs: BTreeMap<String, Vec<u32>>,
    leaf_mentions: BTreeMap<String, Vec<u32>>,
}

impl Encoder {
    fn rows(&self, kind: u16, width: usize) -> Result<u32, String> {
        narrow(
            self.sections.get(&kind).map_or(0, |s| s.len() / width),
            "row count",
        )
    }

    fn triples<'a>(
        &mut self,
        kind: u16,
        rows: impl Iterator<Item = [&'a str; 3]>,
    ) -> Result<(), String> {
        let mut out = self.sections.remove(&kind).unwrap_or_default();
        for row in rows {
            for s in row {
                self.pool.put(&mut out, s)?;
            }
        }
        self.sections.insert(kind, out);
        Ok(())
    }

    fn file(&mut self, facts: &FileFacts, test: bool) -> Result<(), String> {
        let starts = LISTS
            .iter()
            .map(|&kind| self.rows(kind, width_of(kind)))
            .collect::<Result<Vec<u32>, String>>()?;
        let row = |start: u32, i: usize| -> Result<u32, String> {
            start
                .checked_add(narrow(i, "row")?)
                .ok_or_else(|| format!("row {start} + {i} does not fit its 4-byte field"))
        };
        let mut defs = self.sections.remove(&DEFS).unwrap_or_default();
        for (i, def) in facts.defs.iter().enumerate() {
            for s in [&def.local, &def.leaf, &def.kind] {
                self.pool.put(&mut defs, s)?;
            }
            put_u32(&mut defs, def.line);
            put_u32(&mut defs, def.end_line);
            put_u32(&mut defs, opt_index(def.parent));
            put_u8(&mut defs, flag(def.test));
            self.leaf_defs
                .entry(def.leaf.clone())
                .or_default()
                .push(row(starts[L_DEFS], i)?);
        }
        self.sections.insert(DEFS, defs);
        let mut mentions = self.sections.remove(&MENTIONS).unwrap_or_default();
        for (i, mention) in facts.mentions.iter().enumerate() {
            put_u32(&mut mentions, opt_index(mention.enclosing));
            self.pool.put(&mut mentions, &mention.name)?;
            self.pool.put(&mut mentions, &mention.leaf)?;
            let (kind, tag) = match &mention.kind {
                MentionKind::Call => (KIND_CALL, ""),
                MentionKind::Tagged(tag) => (KIND_TAGGED, tag.as_str()),
            };
            put_u8(&mut mentions, kind);
            self.pool.put(&mut mentions, tag)?;
            put_u32(&mut mentions, mention.line);
            put_u8(&mut mentions, flag(mention.test));
            self.leaf_mentions
                .entry(mention.leaf.clone())
                .or_default()
                .push(row(starts[L_MENTIONS], i)?);
        }
        self.sections.insert(MENTIONS, mentions);
        let mut modules = self.sections.remove(&MODULES).unwrap_or_default();
        let mut candidates = self.sections.remove(&CANDIDATES).unwrap_or_default();
        for module in &facts.modules {
            self.pool.put(&mut modules, &module.name)?;
            put_u32(&mut modules, module.def);
            put_u8(&mut modules, flag(module.test));
            put_u32(
                &mut modules,
                narrow(candidates.len() / STR, "candidate row")?,
            );
            for candidate in &module.candidates {
                self.pool.put(&mut candidates, candidate)?;
            }
            put_u32(
                &mut modules,
                narrow(candidates.len() / STR, "candidate row")?,
            );
        }
        self.sections.insert(MODULES, modules);
        self.sections.insert(CANDIDATES, candidates);
        self.triples(
            DOC_CONCEPTS,
            facts
                .doc_concepts
                .iter()
                .map(|c| [c.kind.as_str(), &c.id, &c.title]),
        )?;
        self.triples(
            DOC_LINKS,
            facts
                .doc_links
                .iter()
                .map(|l| [l.from.as_str(), &l.rel, &l.to]),
        )?;
        self.triples(
            WF_NODES,
            facts
                .wf_nodes
                .iter()
                .map(|n| [n.kind.as_str(), &n.id, &n.title]),
        )?;
        self.triples(
            WF_RELATIONS,
            facts
                .wf_relations
                .iter()
                .map(|r| [r.from.as_str(), &r.rel, &r.to]),
        )?;
        let mut files = self.sections.remove(&FILES).unwrap_or_default();
        for s in [&facts.path, &facts.blob, &facts.lang] {
            self.pool.put(&mut files, s)?;
        }
        put_u8(&mut files, facts.half as u8);
        put_u8(&mut files, facts.scope as u8);
        put_u8(
            &mut files,
            flag(facts.partial) | if test { FLAG_TEST } else { 0 },
        );
        for (&kind, start) in LISTS.iter().zip(starts) {
            put_u32(&mut files, start);
            put_u32(&mut files, self.rows(kind, width_of(kind))?);
        }
        self.sections.insert(FILES, files);
        Ok(())
    }

    fn index(&mut self, kind: u16, leaves: &BTreeMap<String, Vec<u32>>) -> Result<(), String> {
        let mut out = Vec::new();
        put_u32(&mut out, narrow(leaves.len(), "leaf count")?);
        let mut at = 0usize;
        for (leaf, rows) in leaves {
            self.pool.put(&mut out, leaf)?;
            put_u32(&mut out, narrow(at, "index row")?);
            at += rows.len();
            put_u32(&mut out, narrow(at, "index row")?);
        }
        for row in leaves.values().flatten() {
            put_u32(&mut out, *row);
        }
        self.sections.insert(kind, out);
        Ok(())
    }

    fn groupings(&mut self, groupings: &Groupings) -> Result<(), String> {
        let mut out = Vec::new();
        for clusters in [&groupings.communities, &groupings.concepts] {
            put_u32(&mut out, narrow(clusters.len(), "cluster count")?);
            for c in clusters {
                self.pool.put(&mut out, &c.id)?;
                self.pool.put(&mut out, &c.label)?;
                put_u64(&mut out, c.resolution.to_bits());
                self.pool.put(&mut out, &c.hash)?;
                put_u32(&mut out, narrow(c.members.len(), "member count")?);
                for m in &c.members {
                    self.pool.put(&mut out, m)?;
                }
            }
        }
        self.sections.insert(GROUPINGS, out);
        Ok(())
    }

    /// The PROOFS rows of `calls`, in their (proof) order, and the index of them by file.
    fn proofs(&mut self, calls: &[ProofCall]) -> Result<(), String> {
        let mut out = Vec::new();
        let mut by_file: Vec<(&str, u32)> = Vec::new();
        for (row, call) in calls.iter().enumerate() {
            let (file, line) = (call.evidence.rsplit_once(':'))
                .and_then(|(file, line)| Some((file, line.parse::<u32>().ok()?)))
                .ok_or_else(|| format!("proof evidence {:?} is not file:line", call.evidence))?;
            self.pool.put(&mut out, &call.target)?;
            self.pool.put(&mut out, &call.caller)?;
            self.pool.put(&mut out, file)?;
            put_u32(&mut out, line);
            by_file.push((file, narrow(row, "proof row")?));
        }
        by_file.sort();
        let mut index = Vec::new();
        for (_, row) in by_file {
            put_u32(&mut index, row);
        }
        self.sections.insert(PROOFS, out);
        self.sections.insert(IDX_PROOF_FILE, index);
        Ok(())
    }
}

fn width_of(kind: u16) -> usize {
    match kind {
        FILES => FILE_ROW,
        DEFS => DEF_ROW,
        MENTIONS => MENTION_ROW,
        MODULES => MODULE_ROW,
        CANDIDATES => STR,
        PROOFS => PROOF_ROW,
        IDX_PROOF_FILE => 4,
        _ => TRIPLE_ROW,
    }
}

/// The bytes of `view`'s snapshot with `groupings`: a pure function of the two.
pub fn encode(view: &dyn CodeView, groupings: &Groupings) -> Result<Vec<u8>, String> {
    let mut enc = Encoder::default();
    let resolver = Resolver::new(view);
    let stamp = view.stamp();
    let mut header = Vec::new();
    put_u64(&mut header, stamp.fingerprint);
    enc.pool.put(&mut header, &stamp.tip.head)?;
    enc.pool.put_opt(&mut header, stamp.tip.commit.as_deref())?;
    enc.pool.put(&mut header, &stamp.tip.tree)?;
    put_u64(&mut header, stamp.seq);
    for kind in [FILES, CANDIDATES].into_iter().chain(LISTS) {
        enc.sections.insert(kind, Vec::new());
    }
    for file in view.files() {
        if let Some(facts) = view.file(file.path) {
            enc.file(&facts, resolver.is_test_file(file.path))?;
        }
    }
    enc.proofs(resolver.proof_calls())?;
    let (leaf_defs, leaf_mentions) = (
        std::mem::take(&mut enc.leaf_defs),
        std::mem::take(&mut enc.leaf_mentions),
    );
    enc.index(IDX_LEAF, &leaf_defs)?;
    enc.index(IDX_NAME, &leaf_mentions)?;
    enc.groupings(groupings)?;
    let mut sections = vec![(STRINGS, std::mem::take(&mut enc.pool.text).into_bytes())];
    sections.extend(enc.sections);
    write_container(MAGIC, VERSION, &header, &sections)
}

/// Fixed-width rows at a place in the snapshot's bytes.
#[derive(Clone, Copy, Debug, Default)]
struct Table {
    at: usize,
    rows: usize,
    width: usize,
}

/// A leaf index: sorted keys and the postings they range into.
#[derive(Clone, Copy, Debug, Default)]
struct Index {
    keys: Table,
    postings: Table,
}

fn le32(row: &[u8], at: usize) -> u32 {
    row.get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

fn byte(row: &[u8], at: usize) -> u8 {
    row.get(at).copied().unwrap_or(0)
}

fn opt(i: u32) -> Option<u32> {
    (i != NONE).then_some(i)
}

fn half_of(b: u8) -> Option<Half> {
    [Half::Code, Half::Design, Half::Workflow]
        .into_iter()
        .find(|h| *h as u8 == b)
}

fn scope_of(b: u8) -> Option<Scope> {
    [Scope::Product, Scope::Test]
        .into_iter()
        .find(|s| *s as u8 == b)
}

/// A snapshot's bytes: built in memory by the writer, or a published file mapped read-only.
pub enum SnapshotBytes {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl SnapshotBytes {
    /// The published snapshot at `path`, mapped rather than copied: an open touches the file's
    /// cached pages where a copy would fault in a fresh buffer of the file's size. memmap2 maps
    /// on Unix and on Windows; a file it cannot map (another OS, or a filesystem that does not
    /// map) is read instead.
    pub fn mapped(path: &Path) -> std::io::Result<Self> {
        // SAFETY: a snapshot file is immutable once published. The writer writes a temp file and
        // renames it to a sequence number never used before; the reaper unlinks; nothing writes,
        // truncates or renames over a `.snap` in place. An unlinked file stays mapped whole, and
        // an OS that refuses to unlink a mapped file leaves it to the next reap.
        Self::mapped_with(path, |file| unsafe { memmap2::Mmap::map(file) })
    }

    /// [`Self::mapped`], mapping the opened file through `map`; the file is read when `map`
    /// fails.
    fn mapped_with(
        path: &Path,
        map: impl FnOnce(&std::fs::File) -> std::io::Result<memmap2::Mmap>,
    ) -> std::io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        match map(&file) {
            Ok(map) => Ok(Self::Mapped(map)),
            Err(_) => {
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut bytes)?;
                Ok(Self::Owned(bytes))
            }
        }
    }
}

impl From<Vec<u8>> for SnapshotBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Owned(bytes)
    }
}

impl Deref for SnapshotBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Owned(bytes) => bytes,
            Self::Mapped(map) => map,
        }
    }
}

/// A [`CodeView`] over one snapshot's bytes. Every reference in the bytes is checked when the
/// snapshot is opened, so reading it later never fails.
pub struct Snapshot {
    stamp: Stamp,
    groupings: Groupings,
    bytes: SnapshotBytes,
    pool: String,
    tables: BTreeMap<u16, Table>,
    leaf: Index,
    name: Index,
    /// Each file row's facts, decoded the first time this open reads them.
    decoded: Vec<OnceLock<FileFacts>>,
}

impl Snapshot {
    /// Reads `bytes`, verifying every crc and every reference; a damaged snapshot is an error,
    /// never a panic.
    pub fn decode(bytes: impl Into<SnapshotBytes>) -> Result<Self, String> {
        let bytes = bytes.into();
        let container = Container::parse(&bytes, MAGIC, VERSION)?;
        let pool = String::from_utf8(container.section(STRINGS)?.to_vec())
            .map_err(|_| "the string pool is not UTF-8".to_string())?;
        let mut tables = BTreeMap::new();
        for kind in [FILES, CANDIDATES, PROOFS, IDX_PROOF_FILE]
            .into_iter()
            .chain(LISTS)
        {
            let range = container.section_range(kind)?;
            let width = width_of(kind);
            if range.len() % width != 0 {
                return Err(format!("section {kind} is not whole rows"));
            }
            let rows = range.len() / width;
            tables.insert(
                kind,
                Table {
                    at: range.start,
                    rows,
                    width,
                },
            );
        }
        let index = |kind: u16| -> Result<Index, String> {
            let range = container.section_range(kind)?;
            let keys = le32(&bytes[range.clone()], 0) as usize;
            let keys_end = keys
                .checked_mul(KEY_ROW)
                .and_then(|n| n.checked_add(4))
                .filter(|&n| n <= range.len() && (range.len() - n) % 4 == 0)
                .ok_or_else(|| format!("section {kind} is not whole rows"))?;
            Ok(Index {
                keys: Table {
                    at: range.start + 4,
                    rows: keys,
                    width: KEY_ROW,
                },
                postings: Table {
                    at: range.start + keys_end,
                    rows: (range.len() - keys_end) / 4,
                    width: 4,
                },
            })
        };
        let (leaf, name) = (index(IDX_LEAF)?, index(IDX_NAME)?);
        let text = |cur: &mut Cursor<'_>| -> Result<Option<String>, String> {
            let (off, len) = (cur.u32()?, cur.u32()?);
            if off == NONE {
                return Ok(None);
            }
            pool_text(&pool, off, len)
                .map(|s| Some(s.to_owned()))
                .ok_or_else(|| "a string lies outside the pool".to_string())
        };
        let required = |cur: &mut Cursor<'_>| -> Result<String, String> {
            text(cur)?.ok_or_else(|| "a required string is absent".to_string())
        };
        let mut cur = Cursor::new(container.payload);
        let fingerprint = cur.u64()?;
        let tip = Tip {
            head: required(&mut cur)?,
            commit: text(&mut cur)?,
            tree: required(&mut cur)?,
        };
        let stamp = Stamp {
            tip,
            fingerprint,
            seq: cur.u64()?,
        };
        let mut cur = Cursor::new(container.section(GROUPINGS)?);
        let mut clusters = || -> Result<Vec<Cluster>, String> {
            let mut out = Vec::new();
            for _ in 0..cur.u32()? {
                let id = required(&mut cur)?;
                let label = required(&mut cur)?;
                let resolution = f64::from_bits(cur.u64()?);
                let hash = required(&mut cur)?;
                let mut members = Vec::new();
                for _ in 0..cur.u32()? {
                    members.push(required(&mut cur)?);
                }
                out.push(Cluster {
                    id,
                    label,
                    resolution,
                    hash,
                    members,
                });
            }
            Ok(out)
        };
        let groupings = Groupings {
            communities: clusters()?,
            concepts: clusters()?,
        };
        if !cur.is_empty() {
            return Err("the groupings have trailing bytes".into());
        }
        drop(container);
        let files = tables.get(&FILES).map_or(0, |t| t.rows);
        let snapshot = Self {
            stamp,
            groupings,
            bytes,
            pool,
            tables,
            leaf,
            name,
            decoded: (0..files).map(|_| OnceLock::new()).collect(),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn table(&self, kind: u16) -> Table {
        self.tables.get(&kind).copied().unwrap_or_default()
    }

    fn row(&self, table: Table, i: usize) -> &[u8] {
        let start = table.at + i * table.width;
        self.bytes.get(start..start + table.width).unwrap_or(&[])
    }

    fn text(&self, row: &[u8], at: usize) -> &str {
        pool_text(&self.pool, le32(row, at), le32(row, at + 4)).unwrap_or("")
    }

    fn list(&self, file: &[u8], list: usize) -> Range<usize> {
        let at = F_LISTS + list * 8;
        le32(file, at) as usize..le32(file, at + 4) as usize
    }

    /// Checks every string, range and index the rows hold, so the accessors never meet a bad one.
    fn validate(&self) -> Result<(), String> {
        let strings: [(u16, &[usize]); 10] = [
            (FILES, &[0, STR, 2 * STR]),
            (DEFS, &[0, STR, 2 * STR]),
            (MENTIONS, &[M_NAME, M_LEAF, M_TAG]),
            (MODULES, &[0]),
            (CANDIDATES, &[0]),
            (DOC_CONCEPTS, &[0, STR, 2 * STR]),
            (DOC_LINKS, &[0, STR, 2 * STR]),
            (WF_NODES, &[0, STR, 2 * STR]),
            (WF_RELATIONS, &[0, STR, 2 * STR]),
            (PROOFS, &[0, STR, P_FILE]),
        ];
        for (kind, offsets) in strings {
            let table = self.table(kind);
            for i in 0..table.rows {
                let row = self.row(table, i);
                for &at in offsets {
                    if pool_text(&self.pool, le32(row, at), le32(row, at + 4)).is_none() {
                        return Err(format!("section {kind} row {i}: a string outside the pool"));
                    }
                }
            }
        }
        let files = self.table(FILES);
        let mut next = [0usize; LISTS.len()];
        let mut previous: Option<&str> = None;
        for i in 0..files.rows {
            let row = self.row(files, i);
            let path = self.text(row, 0);
            if previous.is_some_and(|p| p >= path) {
                return Err(format!("file {path} is out of path order"));
            }
            previous = Some(path);
            if half_of(byte(row, F_HALF)).is_none() || scope_of(byte(row, F_SCOPE)).is_none() {
                return Err(format!("file {path}: an unknown half or scope"));
            }
            for (l, &kind) in LISTS.iter().enumerate() {
                let range = self.list(row, l);
                if range.start != next[l] || range.end < range.start {
                    return Err(format!("file {path}: section {kind} is not one run a file"));
                }
                next[l] = range.end;
            }
            let defs = self.list(row, L_DEFS).len() as u32;
            for d in self.list(row, L_DEFS) {
                let parent = le32(self.row(self.table(DEFS), d), D_PARENT);
                if parent != NONE && parent >= defs {
                    return Err(format!("file {path}: a parent outside the file"));
                }
            }
            for m in self.list(row, L_MENTIONS) {
                let mention = self.row(self.table(MENTIONS), m);
                let enclosing = le32(mention, 0);
                if enclosing != NONE && enclosing >= defs {
                    return Err(format!("file {path}: an enclosing def outside the file"));
                }
                if byte(mention, M_KIND) > KIND_TAGGED {
                    return Err(format!("file {path}: an unknown mention kind"));
                }
            }
            for m in self.list(row, L_MODULES) {
                if le32(self.row(self.table(MODULES), m), MOD_DEF) >= defs {
                    return Err(format!("file {path}: a module def outside the file"));
                }
            }
        }
        for (l, &kind) in LISTS.iter().enumerate() {
            if next[l] != self.table(kind).rows {
                return Err(format!("section {kind} has rows no file owns"));
            }
        }
        let modules = self.table(MODULES);
        let mut next_candidate = 0;
        for i in 0..modules.rows {
            let row = self.row(modules, i);
            let (start, end) = (
                le32(row, MOD_CANDS) as usize,
                le32(row, MOD_CANDS + 4) as usize,
            );
            if start != next_candidate || end < start {
                return Err("module candidates are not one run a module".into());
            }
            next_candidate = end;
        }
        if next_candidate != self.table(CANDIDATES).rows {
            return Err("candidates no module owns".into());
        }
        let (proofs, by_file) = (self.table(PROOFS), self.table(IDX_PROOF_FILE));
        fn in_order<'s>(key: &dyn Fn(usize) -> &'s str, rows: usize) -> bool {
            (1..rows).all(|i| key(i - 1) <= key(i))
        }
        if !in_order(&|i| self.text(self.row(proofs, i), 0), proofs.rows) {
            return Err("proofs out of target order".into());
        }
        let mut indexed = vec![false; proofs.rows];
        for i in 0..by_file.rows {
            let row = le32(self.row(by_file, i), 0) as usize;
            if indexed.get(row).is_none_or(|seen| *seen) {
                return Err("a proof file index row outside its proofs, or twice".into());
            }
            indexed[row] = true;
        }
        if by_file.rows != proofs.rows || !in_order(&|i| self.proof_file(i), by_file.rows) {
            return Err("the proof file index does not cover its proofs in file order".into());
        }
        for (index, kind) in [(self.leaf, DEFS), (self.name, MENTIONS)] {
            let rows = self.table(kind).rows;
            let mut previous: Option<&str> = None;
            for k in 0..index.keys.rows {
                let key = self.row(index.keys, k);
                let Some(leaf) = pool_text(&self.pool, le32(key, 0), le32(key, 4)) else {
                    return Err("an index key outside the pool".into());
                };
                if previous.is_some_and(|p| p >= leaf) {
                    return Err("index keys out of order".into());
                }
                previous = Some(leaf);
                let (start, end) = (le32(key, STR) as usize, le32(key, STR + 4) as usize);
                if start > end || end > index.postings.rows {
                    return Err("an index key outside its postings".into());
                }
            }
            for p in 0..index.postings.rows {
                if le32(self.row(index.postings, p), 0) as usize >= rows {
                    return Err("a posting outside its section".into());
                }
            }
        }
        Ok(())
    }

    fn file_ref(&self, row: &[u8]) -> FileRef<'_> {
        FileRef {
            path: self.text(row, 0),
            lang: self.text(row, 2 * STR),
            half: half_of(byte(row, F_HALF)).unwrap_or_default(),
            scope: scope_of(byte(row, F_SCOPE)).unwrap_or_default(),
            intent: L_INTENT
                .into_iter()
                .any(|list| !self.list(row, list).is_empty()),
            modules: !self.list(row, L_MODULES).is_empty(),
        }
    }

    fn triples(&self, file: &[u8], list: usize) -> impl Iterator<Item = [String; 3]> + '_ {
        let table = self.table(LISTS[list]);
        self.list(file, list).map(move |i| {
            let row = self.row(table, i);
            [0, STR, 2 * STR].map(|at| self.text(row, at).to_owned())
        })
    }

    /// The bytes this snapshot was decoded from, as a publish writes them.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether this snapshot reads a mapped file rather than bytes of its own.
    #[cfg(test)]
    pub(crate) fn is_mapped(&self) -> bool {
        matches!(self.bytes, SnapshotBytes::Mapped(_))
    }

    fn facts(&self, row: &[u8]) -> FileFacts {
        let defs = self.table(DEFS);
        let mentions = self.table(MENTIONS);
        let modules = self.table(MODULES);
        let candidates = self.table(CANDIDATES);
        FileFacts {
            path: self.text(row, 0).to_owned(),
            blob: self.text(row, STR).to_owned(),
            lang: self.text(row, 2 * STR).to_owned(),
            half: half_of(byte(row, F_HALF)).unwrap_or_default(),
            scope: scope_of(byte(row, F_SCOPE)).unwrap_or_default(),
            partial: byte(row, F_FLAGS) & FLAG_SET != 0,
            defs: self
                .list(row, L_DEFS)
                .map(|d| {
                    let r = self.row(defs, d);
                    Def {
                        local: self.text(r, 0).to_owned(),
                        leaf: self.text(r, STR).to_owned(),
                        kind: self.text(r, 2 * STR).to_owned(),
                        line: le32(r, D_LINE),
                        end_line: le32(r, D_END),
                        parent: opt(le32(r, D_PARENT)),
                        test: byte(r, D_FLAGS) & FLAG_SET != 0,
                    }
                })
                .collect(),
            mentions: self
                .list(row, L_MENTIONS)
                .map(|m| {
                    let r = self.row(mentions, m);
                    Mention {
                        enclosing: opt(le32(r, 0)),
                        name: self.text(r, M_NAME).to_owned(),
                        leaf: self.text(r, M_LEAF).to_owned(),
                        kind: self.mention_kind(r),
                        line: le32(r, M_LINE),
                        test: byte(r, M_FLAGS) & FLAG_SET != 0,
                    }
                })
                .collect(),
            modules: self
                .list(row, L_MODULES)
                .map(|m| {
                    let r = self.row(modules, m);
                    ModuleDecl {
                        name: self.text(r, 0).to_owned(),
                        def: le32(r, MOD_DEF),
                        test: byte(r, MOD_FLAGS) & FLAG_SET != 0,
                        candidates: (le32(r, MOD_CANDS) as usize..le32(r, MOD_CANDS + 4) as usize)
                            .map(|c| self.text(self.row(candidates, c), 0).to_owned())
                            .collect(),
                    }
                })
                .collect(),
            doc_concepts: self
                .triples(row, 3)
                .map(|[kind, id, title]| DocConcept { kind, id, title })
                .collect(),
            doc_links: self
                .triples(row, 4)
                .map(|[from, rel, to]| DocLink { from, rel, to })
                .collect(),
            wf_nodes: self
                .triples(row, 5)
                .map(|[kind, id, title]| WfNode { kind, id, title })
                .collect(),
            wf_relations: self
                .triples(row, 6)
                .map(|[from, rel, to]| WfRelation { from, rel, to })
                .collect(),
        }
    }

    fn mention_kind(&self, row: &[u8]) -> MentionKind {
        match byte(row, M_KIND) {
            KIND_TAGGED => MentionKind::Tagged(self.text(row, M_TAG).to_owned()),
            _ => MentionKind::Call,
        }
    }

    /// The file row holding global row `g` of `list`: the runs are contiguous and in file order.
    fn owner(&self, list: usize, g: usize) -> (usize, &[u8]) {
        let files = self.table(FILES);
        let (mut lo, mut hi) = (0, files.rows);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.list(self.row(files, mid), list).end <= g {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let row = self.row(files, lo);
        (g - self.list(row, list).start.min(g), row)
    }

    /// The postings of `leaf` in `index`.
    fn postings(&self, index: Index, leaf: &str) -> impl Iterator<Item = usize> + '_ {
        let (mut lo, mut hi) = (0, index.keys.rows);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.text(self.row(index.keys, mid), 0) < leaf {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let range = if lo < index.keys.rows && self.text(self.row(index.keys, lo), 0) == leaf {
            let key = self.row(index.keys, lo);
            le32(key, STR) as usize..le32(key, STR + 4) as usize
        } else {
            0..0
        };
        range.map(move |p| le32(self.row(index.postings, p), 0) as usize)
    }
}

impl Snapshot {
    /// The PROOFS row the `i`th entry of the file index names.
    fn proof_at(&self, i: usize) -> &[u8] {
        let row = le32(self.row(self.table(IDX_PROOF_FILE), i), 0) as usize;
        self.row(self.table(PROOFS), row)
    }

    /// The file of the PROOFS row the `i`th entry of the file index names.
    fn proof_file(&self, i: usize) -> &str {
        self.text(self.proof_at(i), P_FILE)
    }

    fn proof(&self, row: &[u8]) -> ProofCall {
        ProofCall {
            target: self.text(row, 0).to_owned(),
            caller: self.text(row, STR).to_owned(),
            evidence: format!("{}:{}", self.text(row, P_FILE), le32(row, P_LINE)),
        }
    }

    /// The first of `rows` rows whose `key` is not less than `want`, the keys ascending.
    fn lower_bound<'s>(rows: usize, want: &str, key: impl Fn(usize) -> &'s str) -> usize {
        let (mut lo, mut hi) = (0, rows);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if key(mid) < want {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

impl Settled for Snapshot {
    fn test_files(&self) -> Vec<&str> {
        let files = self.table(FILES);
        (0..files.rows)
            .map(|i| self.row(files, i))
            .filter(|row| byte(row, F_FLAGS) & FLAG_TEST != 0)
            .map(|row| self.text(row, 0))
            .collect()
    }

    fn proofs(&self, of: Proofs<'_>) -> Vec<ProofCall> {
        let proofs = self.table(PROOFS);
        match of {
            Proofs::All => (0..proofs.rows)
                .map(|i| self.proof(self.row(proofs, i)))
                .collect(),
            Proofs::Of(id) => {
                let target = |i| self.text(self.row(proofs, i), 0);
                (Self::lower_bound(proofs.rows, id, target)..proofs.rows)
                    .map(|i| self.row(proofs, i))
                    .take_while(|row| self.text(row, 0) == id)
                    .map(|row| self.proof(row))
                    .collect()
            }
            Proofs::In(path) => {
                let rows = self.table(IDX_PROOF_FILE).rows;
                let file = |i| self.proof_file(i);
                (Self::lower_bound(rows, path, file)..rows)
                    .map(|i| self.proof_at(i))
                    .take_while(|row| self.text(row, P_FILE) == path)
                    .map(|row| self.proof(row))
                    .collect()
            }
        }
    }
}

fn pool_text(pool: &str, off: u32, len: u32) -> Option<&str> {
    let start = off as usize;
    pool.get(start..start.checked_add(len as usize)?)
}

impl CodeView for Snapshot {
    fn stamp(&self) -> &Stamp {
        &self.stamp
    }

    fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
        let files = self.table(FILES);
        Box::new((0..files.rows).map(move |i| self.file_ref(self.row(files, i))))
    }

    fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        let files = self.table(FILES);
        let (mut lo, mut hi) = (0, files.rows);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.text(self.row(files, mid), 0).cmp(path) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let facts = self.decoded[mid].get_or_init(|| self.facts(self.row(files, mid)));
                    return Some(Cow::Borrowed(facts));
                }
            }
        }
        None
    }

    fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
        let defs = self.table(DEFS);
        Box::new(self.postings(self.leaf, leaf).map(move |g| {
            let (index, file) = self.owner(L_DEFS, g);
            let r = self.row(defs, g);
            DefRef {
                file: self.file_ref(file),
                index: index as u32,
                local: self.text(r, 0),
                leaf: self.text(r, STR),
                kind: self.text(r, 2 * STR),
                line: le32(r, D_LINE),
                end_line: le32(r, D_END),
                parent: opt(le32(r, D_PARENT)),
                test: byte(r, D_FLAGS) & FLAG_SET != 0,
            }
        }))
    }

    fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
        let mentions = self.table(MENTIONS);
        Box::new(self.postings(self.name, leaf).map(move |g| {
            let (_, file) = self.owner(L_MENTIONS, g);
            let r = self.row(mentions, g);
            MentionRef {
                file: self.file_ref(file),
                enclosing: opt(le32(r, 0)),
                name: self.text(r, M_NAME),
                leaf: self.text(r, M_LEAF),
                kind: self.mention_kind(r),
                line: le32(r, M_LINE),
                test: byte(r, M_FLAGS) & FLAG_SET != 0,
            }
        }))
    }

    fn groupings(&self) -> &Groupings {
        &self.groupings
    }

    fn def_leaves(&self) -> Vec<String> {
        let keys = self.leaf.keys;
        (0..keys.rows)
            .map(|k| self.text(self.row(keys, k), 0).to_owned())
            .collect()
    }

    fn settled(&self) -> Option<&dyn Settled> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigger_domain::code_facts::{
        Def, DocConcept, DocLink, Half, Mention, MentionKind, ModuleDecl, Scope, Tip, WfNode,
        WfRelation,
    };
    use rigger_domain::code_view::{Cluster, FactsView};

    /// Each row: whether the file can be mapped. A file that can is mapped; one that cannot is
    /// read; either way the bytes are the file's.
    #[test]
    fn a_snapshot_file_that_cannot_be_mapped_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("000001.snap");
        std::fs::write(&path, b"the published bytes").unwrap();
        let cases: [(&str, bool); 2] = [("a file that maps", true), ("one that cannot", false)];
        for (name, maps) in cases {
            let bytes = SnapshotBytes::mapped_with(&path, |file| match maps {
                // SAFETY: the file is this test's own and nothing writes it while mapped.
                true => unsafe { memmap2::Mmap::map(file) },
                false => Err(std::io::Error::other("this filesystem does not map")),
            })
            .unwrap();
            assert_eq!(
                matches!(bytes, SnapshotBytes::Mapped(_)),
                maps,
                "{name}: mapped"
            );
            assert_eq!(&*bytes, b"the published bytes", "{name}: the bytes");
        }
    }

    fn stamp(commit: Option<&str>, seq: u64) -> Stamp {
        Stamp {
            tip: Tip {
                head: "main".into(),
                commit: commit.map(Into::into),
                tree: "tree-1".into(),
            },
            fingerprint: 0xFEED_F00D_0000_0001,
            seq,
        }
    }

    fn code_file() -> FileFacts {
        FileFacts {
            path: "src/ünï.rs".into(),
            blob: "b1".into(),
            lang: "rust".into(),
            half: Half::Code,
            scope: Scope::Product,
            partial: true,
            defs: vec![
                Def {
                    local: "Outer".into(),
                    leaf: "Outer".into(),
                    kind: "struct".into(),
                    line: 1,
                    end_line: 9,
                    parent: None,
                    test: false,
                },
                Def {
                    local: "Outer::run".into(),
                    leaf: "run".into(),
                    kind: "fn".into(),
                    line: 3,
                    end_line: 5,
                    parent: Some(0),
                    test: true,
                },
            ],
            mentions: vec![
                Mention {
                    enclosing: Some(1),
                    name: "crate::x::run".into(),
                    leaf: "run".into(),
                    kind: MentionKind::Call,
                    line: 4,
                    test: true,
                },
                Mention {
                    enclosing: None,
                    name: "Outer".into(),
                    leaf: "Outer".into(),
                    kind: MentionKind::Tagged("type".into()),
                    line: 11,
                    test: false,
                },
            ],
            modules: vec![ModuleDecl {
                name: "inner".into(),
                def: 1,
                test: true,
                candidates: vec!["src/inner.rs".into(), "src/inner/mod.rs".into()],
            }],
            ..Default::default()
        }
    }

    fn design_file() -> FileFacts {
        FileFacts {
            path: "docs/a.md".into(),
            blob: "b2".into(),
            lang: "markdown".into(),
            half: Half::Design,
            doc_concepts: vec![DocConcept {
                kind: "design-doc".into(),
                id: "doc/a".into(),
                title: "Ä design".into(),
            }],
            doc_links: vec![DocLink {
                from: "doc/a".into(),
                rel: "GOVERNS".into(),
                to: "src/ünï.rs".into(),
            }],
            ..Default::default()
        }
    }

    fn workflow_file() -> FileFacts {
        FileFacts {
            path: ".rigger/workflow.toml".into(),
            blob: "b3".into(),
            lang: "toml".into(),
            half: Half::Workflow,
            scope: Scope::Test,
            wf_nodes: vec![WfNode {
                kind: "stage".into(),
                id: "stage/build".into(),
                title: "Build".into(),
            }],
            wf_relations: vec![WfRelation {
                from: "stage/build".into(),
                rel: "PRECEDES".into(),
                to: "stage/review".into(),
            }],
            ..Default::default()
        }
    }

    fn empty_file(path: &str) -> FileFacts {
        FileFacts {
            path: path.into(),
            blob: "b4".into(),
            ..Default::default()
        }
    }

    fn groupings() -> Groupings {
        let cluster = |id: &str, members: &[&str]| Cluster {
            id: id.into(),
            label: format!("label of {id}"),
            resolution: 1.5,
            hash: "h".into(),
            members: members.iter().map(|m| (*m).into()).collect(),
        };
        Groupings {
            communities: vec![
                cluster(
                    "community/1.5/0",
                    &["src/ünï.rs::Outer", "src/ünï.rs::Outer::run"],
                ),
                cluster("community/1.5/1", &[]),
            ],
            concepts: vec![cluster("concept/1.5/0", &["doc/a"])],
        }
    }

    /// A code file of `path` and `scope` holding `defs` (local = leaf, a function each), the
    /// calls `calls` (leaf, enclosing def, line) and the module declarations `modules` (name,
    /// declaring def, test, candidates).
    fn proving_file(
        path: &str,
        scope: Scope,
        defs: &[&str],
        calls: &[(&str, Option<u32>, u32)],
        modules: &[(&str, u32, bool, &str)],
    ) -> FileFacts {
        FileFacts {
            path: path.into(),
            blob: format!("b-{path}"),
            lang: "rust".into(),
            half: Half::Code,
            scope,
            partial: false,
            defs: (defs.iter().enumerate())
                .map(|(i, leaf)| Def {
                    local: leaf.to_string(),
                    leaf: leaf.to_string(),
                    kind: "function".into(),
                    line: i as u32 + 1,
                    end_line: i as u32 + 1,
                    parent: None,
                    test: false,
                })
                .collect(),
            mentions: (calls.iter())
                .map(|&(leaf, enclosing, line)| Mention {
                    enclosing,
                    name: leaf.into(),
                    leaf: leaf.into(),
                    kind: MentionKind::Call,
                    line,
                    test: false,
                })
                .collect(),
            modules: (modules.iter())
                .map(|&(name, def, test, candidate)| ModuleDecl {
                    name: name.into(),
                    def,
                    test,
                    candidates: vec![candidate.into()],
                })
                .collect(),
            ..empty_file(path)
        }
    }

    /// WHAT A STORE SETTLES READS BACK AS THE RESOLVER COMPUTES IT: over a view holding a test
    /// file by its scope, one by the test-module closure alone and calls from both (at file
    /// level and inside a definition, on lines whose text order is not their number order), the
    /// snapshot's test files, its proof calls (every one, those of each definition, those in each
    /// file) and its definition leaves are what a resolver computes over the facts.
    #[test]
    fn a_snapshot_settles_the_test_files_and_proofs_the_resolver_computes() {
        let files = vec![
            proving_file(
                "src/lib.rs",
                Scope::Product,
                &["run", "stop", "checks"],
                &[("run", Some(1), 3)],
                &[("checks", 2, true, "src/checks.rs")],
            ),
            proving_file(
                "src/checks.rs",
                Scope::Product,
                &["verify"],
                &[("run", Some(0), 9), ("stop", Some(0), 10), ("run", None, 2)],
                &[],
            ),
            proving_file(
                "tests/it.rs",
                Scope::Test,
                &[],
                &[("stop", None, 4), ("missing", None, 5)],
                &[],
            ),
        ];
        let view = FactsView::new(stamp(Some("c"), 1), files);
        let snap = Snapshot::decode(encode(&view, &Groupings::default()).unwrap()).unwrap();
        let resolver = Resolver::new(&view);
        let want_tests: Vec<&str> = (view.files())
            .map(|f| f.path)
            .filter(|p| resolver.is_test_file(p))
            .collect();
        assert_eq!(want_tests, ["src/checks.rs", "tests/it.rs"], "the fixture");
        let settled = snap.settled().expect("a snapshot settles");
        assert_eq!(settled.test_files(), want_tests, "test files");
        let all = resolver.proof_calls();
        assert_eq!(all.len(), 4, "the fixture's proofs: {all:#?}");
        assert_eq!(settled.proofs(Proofs::All), all, "every proof");
        for id in [
            "src/lib.rs::run",
            "src/lib.rs::stop",
            "src/lib.rs::checks",
            "x",
        ] {
            let want: Vec<ProofCall> = all.iter().filter(|c| c.target == id).cloned().collect();
            assert_eq!(settled.proofs(Proofs::Of(id)), want, "proofs of {id}");
        }
        for path in ["src/checks.rs", "src/lib.rs", "tests/it.rs", "src/a", "z"] {
            assert_eq!(
                settled.proofs(Proofs::In(path)),
                resolver.proofs_in(path),
                "proofs in {path}"
            );
        }
        assert_eq!(snap.def_leaves(), view.def_leaves(), "definition leaves");
    }

    /// Each row: a stamp, the files and the groupings a view holds. Every section must read back
    /// as written: the stamp, the file list, each file's facts, both leaf indexes and the
    /// groupings; and re-encoding the snapshot yields the same bytes.
    #[test]
    fn snapshot_round_trips_every_section() {
        let rich = || vec![code_file(), design_file(), workflow_file(), empty_file("z")];
        let cases: Vec<(&str, Stamp, Vec<FileFacts>, Groupings)> = vec![
            (
                "empty view",
                stamp(None, 1),
                Vec::new(),
                Groupings::default(),
            ),
            ("every half, no commit", stamp(None, 2), rich(), groupings()),
            (
                "every half, a commit",
                stamp(Some("c0ffee"), 7),
                rich(),
                groupings(),
            ),
            (
                "only empty files",
                stamp(Some("c"), 3),
                vec![empty_file("a"), empty_file("b")],
                Groupings::default(),
            ),
        ];
        for (name, stamp, files, groupings) in cases {
            let view = FactsView::new(stamp.clone(), files.clone());
            let bytes = encode(&view, &groupings).expect("encodes");
            let snap = Snapshot::decode(bytes.clone())
                .unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
            assert_eq!(snap.stamp(), &stamp, "{name}: stamp");
            assert_eq!(snap.groupings(), &groupings, "{name}: groupings");
            assert_eq!(
                snap.files().collect::<Vec<_>>(),
                view.files().collect::<Vec<_>>(),
                "{name}: file list"
            );
            for file in &files {
                assert_eq!(
                    snap.file(&file.path).as_deref(),
                    Some(file),
                    "{name}: facts of {}",
                    file.path
                );
                for leaf in file.defs.iter().map(|d| &d.leaf) {
                    assert_eq!(
                        snap.defs_named(leaf).collect::<Vec<_>>(),
                        view.defs_named(leaf).collect::<Vec<_>>(),
                        "{name}: defs named {leaf}"
                    );
                }
                for leaf in file.mentions.iter().map(|m| &m.leaf) {
                    assert_eq!(
                        snap.mentions_named(leaf).collect::<Vec<_>>(),
                        view.mentions_named(leaf).collect::<Vec<_>>(),
                        "{name}: mentions named {leaf}"
                    );
                }
            }
            assert!(snap.file("absent").is_none(), "{name}: absent file");
            assert_eq!(snap.defs_named("absent").count(), 0, "{name}: absent leaf");
            assert_eq!(
                encode(&snap, snap.groupings()),
                Ok(bytes),
                "{name}: re-encode"
            );
        }
    }
}
