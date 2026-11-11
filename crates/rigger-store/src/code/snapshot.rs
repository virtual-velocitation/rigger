//! The snapshot format: one immutable file holding a stamp, every file's facts, the leaf indexes
//! and the groupings, and [`Snapshot`], the [`CodeView`] that reads one.
//!
//! The file is a [`crate::format`] container. Its header payload is the stamp: the extractor
//! fingerprint, the tip's head, commit and tree as strings, and the sequence number. A string is
//! a (u32 offset, u32 length) pair into the STRINGS pool, which holds each distinct string once;
//! an absent commit is the offset `u32::MAX`. Each other section is fixed-width rows:
//!
//! ```text
//! FILES        path, blob, lang, half u8, scope u8, flags u8 (partial),
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
//! ```
//!
//! Encoding is a pure function of the view and its groupings: rows follow path then source
//! order, the pool interns in that order and the indexes are built sorted, so the same facts
//! always give the same bytes.

use crate::format::{narrow, put_u32, put_u64, put_u8, write_container, Container, Cursor};
use rigger_domain::code_facts::{
    Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope, Stamp, Tip,
    WfNode, WfRelation,
};
use rigger_domain::code_view::{Cluster, CodeView, DefRef, FileRef, Groupings, MentionRef};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

pub(crate) const MAGIC: &[u8; 4] = b"RGCS";
pub(crate) const VERSION: u16 = 2;
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

const KIND_CALL: u8 = 0;
const KIND_TAGGED: u8 = 1;
const FLAG_SET: u8 = 1;

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

    fn file(&mut self, facts: &FileFacts) -> Result<(), String> {
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
        put_u8(&mut files, flag(facts.partial));
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
}

fn width_of(kind: u16) -> usize {
    match kind {
        FILES => FILE_ROW,
        DEFS => DEF_ROW,
        MENTIONS => MENTION_ROW,
        MODULES => MODULE_ROW,
        CANDIDATES => STR,
        _ => TRIPLE_ROW,
    }
}

/// The bytes of `view`'s snapshot with `groupings`: a pure function of the two.
pub fn encode(view: &dyn CodeView, groupings: &Groupings) -> Result<Vec<u8>, String> {
    let mut enc = Encoder::default();
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
            enc.file(&facts)?;
        }
    }
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

/// A [`CodeView`] over one snapshot's bytes. Every reference in the bytes is checked when the
/// snapshot is opened, so reading it later never fails.
pub struct Snapshot {
    stamp: Stamp,
    groupings: Groupings,
    bytes: Vec<u8>,
    pool: String,
    tables: BTreeMap<u16, Table>,
    leaf: Index,
    name: Index,
}

impl Snapshot {
    /// Reads `bytes`, verifying every crc and every reference; a damaged snapshot is an error,
    /// never a panic.
    pub fn decode(bytes: Vec<u8>) -> Result<Self, String> {
        let container = Container::parse(&bytes, MAGIC, VERSION)?;
        let pool = String::from_utf8(container.section(STRINGS)?.to_vec())
            .map_err(|_| "the string pool is not UTF-8".to_string())?;
        let mut tables = BTreeMap::new();
        for kind in [FILES, CANDIDATES].into_iter().chain(LISTS) {
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
        let snapshot = Self {
            stamp,
            groupings,
            bytes,
            pool,
            tables,
            leaf,
            name,
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
        let strings: [(u16, &[usize]); 9] = [
            (FILES, &[0, STR, 2 * STR]),
            (DEFS, &[0, STR, 2 * STR]),
            (MENTIONS, &[M_NAME, M_LEAF, M_TAG]),
            (MODULES, &[0]),
            (CANDIDATES, &[0]),
            (DOC_CONCEPTS, &[0, STR, 2 * STR]),
            (DOC_LINKS, &[0, STR, 2 * STR]),
            (WF_NODES, &[0, STR, 2 * STR]),
            (WF_RELATIONS, &[0, STR, 2 * STR]),
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
        }
    }

    fn triples(&self, file: &[u8], list: usize) -> impl Iterator<Item = [String; 3]> + '_ {
        let table = self.table(LISTS[list]);
        self.list(file, list).map(move |i| {
            let row = self.row(table, i);
            [0, STR, 2 * STR].map(|at| self.text(row, at).to_owned())
        })
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
                    return Some(Cow::Owned(self.facts(self.row(files, mid))))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigger_domain::code_facts::{
        Def, DocConcept, DocLink, Half, Mention, MentionKind, ModuleDecl, Scope, Tip, WfNode,
        WfRelation,
    };
    use rigger_domain::code_view::{Cluster, FactsView};

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
