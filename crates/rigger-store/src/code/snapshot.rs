//! The snapshot format: one immutable file holding a stamp, every file's facts, the leaf indexes,
//! the groupings and what the store settles from the tree alone (the test files, the proof calls
//! and the code half resolved whole), and [`Snapshot`], the [`CodeView`] that reads one.
//!
//! The file is a [`crate::format`] container. Its header payload is the stamp: the extractor
//! fingerprint, the tip's head, commit and tree as strings, and the sequence number. A string is
//! a (u32 offset, u32 length) pair into the STRINGS pool, which holds each distinct string once;
//! an absent commit is the offset `u32::MAX`. A doc span is the u32 first and last line of a doc
//! comment; lines count from 1, so 0, 0 is no doc comment. Each other section is fixed-width rows:
//!
//! ```text
//! FILES        path, blob, lang, half u8, scope u8, flags u8 (partial 1, test file 2),
//!              doc span, then [start, end) into DEFS, MENTIONS, MODULES, DOC_CONCEPTS,
//!              DOC_LINKS, WF_NODES, WF_RELATIONS       (rows sorted by path; one run a file)
//! DEFS         local, leaf, kind, line, end_line, parent (u32::MAX: none), flags (test),
//!              doc span
//! MENTIONS     enclosing (u32::MAX: none), name, leaf, kind u8 (0 call, 1 tagged), tag,
//!              line, flags (test)
//! MODULES      name, def (its declaring DEFS row in the file), flags (test 1, path
//!              attribute 2), then start, mid, end into CANDIDATES: as_root is [start, mid),
//!              as_child [mid, end)
//! CANDIDATES   path
//! DOC_*, WF_*  three strings each, as written
//! IDX_LEAF     u32 key count, keys (leaf, [start, end) into the postings) sorted by leaf,
//!              then u32 postings: DEFS rows, in path then source order
//! IDX_NAME     the same over MENTIONS rows
//! GROUPINGS    u32 count, clusters (id, label, resolution as f64 bits, hash, u32 count,
//!              members); communities then concepts
//! PROOFS       target, caller, file, line: each proof call, in proof order
//! IDX_PROOF_FILE  u32 PROOFS rows, by file then proof order
//! ROW_NODES    id, kind, then [start, end) into ROW_ATTRS      (rows sorted by id; one a node)
//! ROW_ATTRS    key, value                                       (one run a node, in key order)
//! ROW_EDGES    from, to, rel, tier, valid_from as i64 bits, source u64
//!                                                    (sorted by from, to, rel; one each)
//! IDX_ROW_IN   the IDX_LEAF format: ROW_EDGES rows by the id they lead to, in edge order
//! IDX_ROW_FILE the same: ROW_NODES rows by the file they are members of, in given order
//! IDX_ROW_LEAF the same: ROW_NODES rows by each leaf that binds them, in id order
//! ```
//!
//! A FILES row's test flag and the PROOFS rows are what
//! [`Resolver`](rigger_domain::resolve::Resolver) computes over the view, and
//! the ROW_* sections the code half [`Indexed::resolved`] resolves under the default filter, so a
//! read takes them rather than computing them ([`Settled`], [`SettledHalf`]). A facts memo entry
//! ([`encode_without_half`]) holds the ROW_* sections empty, since nothing reads its half.
//!
//! Encoding is a pure function of the view and its groupings: rows follow path then source
//! order, the pool interns in that order and the indexes are built sorted, so the same facts
//! always give the same bytes.

use crate::format::{narrow, put_u32, put_u64, put_u8, write_container, Container, Cursor};
use crate::published::Published;
use rigger_domain::code_facts::{
    Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, ModuleDecl, Scope, Stamp, Tip,
    WfNode, WfRelation,
};
use rigger_domain::code_graph::CodeGraph;
use rigger_domain::code_view::{
    Cluster, CodeView, DefRef, FileRef, Groupings, MentionRef, Proofs, Settled, SettledHalf,
    SETTLED_FILTER,
};
use rigger_domain::composed::{HalfRows, Indexed};
use rigger_domain::contextgraph::{Edge, Graph, Node};
use rigger_domain::resolve::ProofCall;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::OnceLock;

pub(crate) const MAGIC: &[u8; 4] = b"RGCS";
pub(crate) const VERSION: u16 = 6;
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
const ROW_NODES: u16 = 16;
const ROW_ATTRS: u16 = 17;
const ROW_EDGES: u16 = 18;
const IDX_ROW_IN: u16 = 19;
const IDX_ROW_FILE: u16 = 20;
const IDX_ROW_LEAF: u16 = 21;
/// The last section kind: every kind from [`STRINGS`] to it is written.
#[cfg(test)]
pub(crate) const LAST: u16 = IDX_ROW_LEAF;

/// The width of a string reference.
const STR: usize = 8;

/// FILES row: three strings, half, scope, flags, the doc span, then one [start, end) pair per
/// list.
const F_HALF: usize = 3 * STR;
const F_SCOPE: usize = F_HALF + 1;
const F_FLAGS: usize = F_SCOPE + 1;
const F_DOC: usize = F_FLAGS + 1;
const F_LISTS: usize = F_DOC + 8;
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
const D_DOC: usize = D_FLAGS + 1;
const DEF_ROW: usize = D_DOC + 8;

/// MENTIONS row.
const M_NAME: usize = 4;
const M_LEAF: usize = M_NAME + STR;
const M_KIND: usize = M_LEAF + STR;
const M_TAG: usize = M_KIND + 1;
const M_LINE: usize = M_TAG + STR;
const M_FLAGS: usize = M_LINE + 4;
const MENTION_ROW: usize = M_FLAGS + 1;

/// MODULES row: name, def, flags, then start, mid and end into CANDIDATES.
const MOD_DEF: usize = STR;
const MOD_FLAGS: usize = MOD_DEF + 4;
const MOD_CANDS: usize = MOD_FLAGS + 1;
const MODULE_ROW: usize = MOD_CANDS + 12;

/// A row of three strings (DOC_*, WF_*), and an index key (leaf, start, end).
const TRIPLE_ROW: usize = 3 * STR;
const KEY_ROW: usize = STR + 8;

/// PROOFS row: target, caller, file, line.
const P_FILE: usize = 2 * STR;
const P_LINE: usize = P_FILE + STR;
const PROOF_ROW: usize = P_LINE + 4;

/// ROW_NODES row: id, kind, then [start, end) into ROW_ATTRS.
const N_ATTRS: usize = 2 * STR;
const NODE_ROW: usize = N_ATTRS + 8;
/// ROW_ATTRS row: key, value.
const ATTR_ROW: usize = 2 * STR;
/// ROW_EDGES row: from, to, rel, tier, then valid_from as i64 bits and source.
const E_VALID: usize = 4 * STR;
const E_SOURCE: usize = E_VALID + 8;
const EDGE_ROW: usize = E_SOURCE + 8;

const KIND_CALL: u8 = 0;
const KIND_TAGGED: u8 = 1;
const FLAG_SET: u8 = 1;
/// A FILES row's flag: test code, by its scope or the test-module closure.
const FLAG_TEST: u8 = 2;
/// A MODULES row's flag: the declaration carries an explicit path attribute.
const FLAG_PATH_ATTR: u8 = 2;

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

/// The flag bit `mask` when `set`, else no bit.
fn bit(set: bool, mask: u8) -> u8 {
    if set {
        mask
    } else {
        0
    }
}

fn flag(set: bool) -> u8 {
    bit(set, FLAG_SET)
}

fn opt_index(i: Option<u32>) -> u32 {
    i.unwrap_or(NONE)
}

/// Writes a doc span: its first and last line, or 0, 0 when there is none.
fn put_span(out: &mut Vec<u8>, span: Option<(u32, u32)>) {
    let (first, last) = span.unwrap_or((0, 0));
    put_u32(out, first);
    put_u32(out, last);
}

/// The sections being written, one byte buffer per kind.
#[derive(Default)]
struct Encoder {
    pool: Pool,
    sections: BTreeMap<u16, Vec<u8>>,
    /// Leaf to global DEFS rows, and to global MENTIONS rows.
    leaf_defs: BTreeMap<String, Vec<usize>>,
    leaf_mentions: BTreeMap<String, Vec<usize>>,
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
        let row = |start: u32, i: usize| start as usize + i;
        let mut defs = self.sections.remove(&DEFS).unwrap_or_default();
        for (i, def) in facts.defs.iter().enumerate() {
            for s in [&def.local, &def.leaf, &def.kind] {
                self.pool.put(&mut defs, s)?;
            }
            put_u32(&mut defs, def.line);
            put_u32(&mut defs, def.end_line);
            put_u32(&mut defs, opt_index(def.parent));
            put_u8(&mut defs, flag(def.test));
            put_span(&mut defs, def.doc);
            self.leaf_defs
                .entry(def.leaf.clone())
                .or_default()
                .push(row(starts[L_DEFS], i));
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
                .push(row(starts[L_MENTIONS], i));
        }
        self.sections.insert(MENTIONS, mentions);
        let mut modules = self.sections.remove(&MODULES).unwrap_or_default();
        let mut candidates = self.sections.remove(&CANDIDATES).unwrap_or_default();
        for module in &facts.modules {
            self.pool.put(&mut modules, &module.name)?;
            put_u32(&mut modules, module.def);
            put_u8(
                &mut modules,
                flag(module.test) | bit(module.path_attr, FLAG_PATH_ATTR),
            );
            put_u32(
                &mut modules,
                narrow(candidates.len() / STR, "candidate row")?,
            );
            for list in [&module.as_root, &module.as_child] {
                for candidate in list {
                    self.pool.put(&mut candidates, candidate)?;
                }
                put_u32(
                    &mut modules,
                    narrow(candidates.len() / STR, "candidate row")?,
                );
            }
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
        put_u8(&mut files, flag(facts.partial) | bit(test, FLAG_TEST));
        put_span(&mut files, facts.doc);
        for (&kind, start) in LISTS.iter().zip(starts) {
            put_u32(&mut files, start);
            put_u32(&mut files, self.rows(kind, width_of(kind))?);
        }
        self.sections.insert(FILES, files);
        Ok(())
    }

    /// The index `kind` of `keys`: each key's rows, in key order.
    fn index<K: AsRef<str>>(
        &mut self,
        kind: u16,
        keys: &BTreeMap<K, Vec<usize>>,
    ) -> Result<(), String> {
        let mut out = Vec::new();
        put_u32(&mut out, narrow(keys.len(), "key count")?);
        let mut at = 0usize;
        for (key, rows) in keys {
            self.pool.put(&mut out, key.as_ref())?;
            put_u32(&mut out, narrow(at, "index row")?);
            at += rows.len();
            put_u32(&mut out, narrow(at, "index row")?);
        }
        for &row in keys.values().flatten() {
            put_u32(&mut out, narrow(row, "posting")?);
        }
        self.sections.insert(kind, out);
        Ok(())
    }

    /// The code half `half` settles as: its nodes, their attributes, its edges, and its indexes
    /// of incoming edges, file members and leaves, each naming its rows by place.
    fn half(&mut self, half: &HalfRows<'_>) -> Result<(), String> {
        let (mut nodes, mut attrs, mut edges) = (Vec::new(), Vec::new(), Vec::new());
        for node in half.nodes {
            self.pool.put(&mut nodes, &node.id)?;
            self.pool.put(&mut nodes, &node.kind)?;
            put_u32(&mut nodes, narrow(attrs.len() / ATTR_ROW, "attribute row")?);
            for (key, value) in &node.attrs {
                self.pool.put(&mut attrs, key)?;
                self.pool.put(&mut attrs, value)?;
            }
            put_u32(&mut nodes, narrow(attrs.len() / ATTR_ROW, "attribute row")?);
        }
        for edge in half.edges {
            for s in [&edge.from, &edge.to, edge.rel.as_str(), edge.tier.as_str()] {
                self.pool.put(&mut edges, s)?;
            }
            put_u64(&mut edges, edge.valid_from.cast_unsigned());
            put_u64(&mut edges, edge.source);
        }
        self.sections
            .extend([(ROW_NODES, nodes), (ROW_ATTRS, attrs), (ROW_EDGES, edges)]);
        self.index(IDX_ROW_IN, &half.incoming)?;
        self.index(IDX_ROW_FILE, half.members)?;
        self.index(IDX_ROW_LEAF, half.leaves)
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
            self.pool.put_opt(&mut out, call.caller.as_deref())?;
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
        ROW_NODES => NODE_ROW,
        ROW_ATTRS => ATTR_ROW,
        ROW_EDGES => EDGE_ROW,
        _ => TRIPLE_ROW,
    }
}

/// The bytes of `view`'s snapshot with `groupings`: a pure function of the two.
pub fn encode(view: &dyn CodeView, groupings: &Groupings) -> Result<Vec<u8>, String> {
    let graph = CodeGraph::new(view, SETTLED_FILTER);
    let half = Indexed::resolved(&graph, groupings);
    encode_with(view, &graph, groupings, &half.half_rows())
}

/// The bytes of `view`'s snapshot with no groupings and an empty code half: an entry of the facts
/// memo, which nothing reads but its files' facts.
pub(crate) fn encode_without_half(view: &dyn CodeView) -> Result<Vec<u8>, String> {
    let graph = CodeGraph::new(view, SETTLED_FILTER);
    let none = Indexed::new(Graph::default(), Vec::new());
    encode_with(view, &graph, &Groupings::default(), &none.half_rows())
}

/// The bytes of `view`'s snapshot, `graph` read over it, with `groupings` and the code half
/// `half`.
fn encode_with(
    view: &dyn CodeView,
    graph: &CodeGraph<'_>,
    groupings: &Groupings,
    half: &HalfRows<'_>,
) -> Result<Vec<u8>, String> {
    let mut enc = Encoder::default();
    let resolver = graph.resolver();
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
    enc.half(half)?;
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

fn le64(row: &[u8], at: usize) -> u64 {
    row.get(at..at + 8)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u64::from_le_bytes)
}

fn byte(row: &[u8], at: usize) -> u8 {
    row.get(at).copied().unwrap_or(0)
}

fn opt(i: u32) -> Option<u32> {
    (i != NONE).then_some(i)
}

/// The doc span at `at` in `row`; none when it is 0, 0 ([`put_span`]).
fn span(row: &[u8], at: usize) -> Option<(u32, u32)> {
    let lines = (le32(row, at), le32(row, at + 4));
    (lines != (0, 0)).then_some(lines)
}

/// A MODULES row's start, mid and end into CANDIDATES: its `as_root` list is [start, mid), its
/// `as_child` list [mid, end).
fn candidate_bounds(row: &[u8]) -> [usize; 3] {
    [0, 4, 8].map(|at| le32(row, MOD_CANDS + at) as usize)
}

/// The runs a child table is cut into, taken one per parent row in parent order: each run starts
/// where the one before it ended and never runs backwards, and the last ends at the table's row
/// count, so every child row has exactly one owner.
#[derive(Clone, Copy, Default)]
struct Runs {
    next: usize,
}

impl Runs {
    /// Whether `run` starts where the runs taken before it ended and does not run backwards. The
    /// runs after it start from its end either way.
    fn take(&mut self, run: Range<usize>) -> bool {
        let placed = run.start == self.next && run.end >= run.start;
        self.next = run.end;
        placed
    }

    /// Whether the runs taken end at `rows`, the child table's row count.
    fn cover(&self, rows: usize) -> bool {
        self.next == rows
    }
}

/// Whether the keys `key` reads for rows `0..rows` ascend in row order: strictly when `strict`,
/// so no key repeats, else with equal neighbours allowed.
fn ascending<K: PartialOrd>(rows: usize, strict: bool, key: impl Fn(usize) -> K) -> bool {
    (1..rows).all(|i| {
        let (before, at) = (key(i - 1), key(i));
        before < at || (!strict && before == at)
    })
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
    bytes: Published,
    pool: String,
    tables: BTreeMap<u16, Table>,
    leaf: Index,
    name: Index,
    /// The settled code half's ROW_EDGES rows by the id they lead into, and its ROW_NODES rows
    /// by member file and by leaf.
    into_id: Index,
    of_file: Index,
    by_leaf: Index,
    /// Each file row's facts, decoded the first time this open reads them.
    decoded: Vec<OnceLock<FileFacts>>,
}

impl Snapshot {
    /// Reads `bytes`, verifying every crc and every reference; a damaged snapshot is an error,
    /// never a panic.
    pub fn decode(bytes: impl Into<Published>) -> Result<Self, String> {
        let bytes = bytes.into();
        let container = Container::parse(&bytes, MAGIC, VERSION)?;
        let pool = String::from_utf8(container.section(STRINGS)?.to_vec())
            .map_err(|_| "the string pool is not UTF-8".to_string())?;
        let mut tables = BTreeMap::new();
        let rows = [
            FILES,
            CANDIDATES,
            PROOFS,
            IDX_PROOF_FILE,
            ROW_NODES,
            ROW_ATTRS,
            ROW_EDGES,
        ];
        for kind in rows.into_iter().chain(LISTS) {
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
        let (into_id, of_file, by_leaf) = (
            index(IDX_ROW_IN)?,
            index(IDX_ROW_FILE)?,
            index(IDX_ROW_LEAF)?,
        );
        let in_pool = |off, len| pool_text(&pool, off, len);
        let pooled = |cur: &mut Cursor<'_>| required(cur, &in_pool);
        let stamp = read_stamp(container.payload, &in_pool)?;
        let mut cur = Cursor::new(container.section(GROUPINGS)?);
        let mut clusters = || -> Result<Vec<Cluster>, String> {
            let mut out = Vec::new();
            for _ in 0..cur.u32()? {
                let id = pooled(&mut cur)?;
                let label = pooled(&mut cur)?;
                let resolution = f64::from_bits(cur.u64()?);
                let hash = pooled(&mut cur)?;
                let mut members = Vec::new();
                for _ in 0..cur.u32()? {
                    members.push(pooled(&mut cur)?);
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
            into_id,
            of_file,
            by_leaf,
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
        let strings: [(u16, &[usize]); 13] = [
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
            (ROW_NODES, &[0, STR]),
            (ROW_ATTRS, &[0, STR]),
            (ROW_EDGES, &[0, STR, 2 * STR, 3 * STR]),
        ];
        // The strings a row may leave absent: a proof's caller, when test code inside a product
        // entity makes the call.
        let absent_allowed = [(PROOFS, STR)];
        for (kind, offsets) in strings {
            let table = self.table(kind);
            for i in 0..table.rows {
                let row = self.row(table, i);
                for &at in offsets {
                    let absent = le32(row, at) == NONE && absent_allowed.contains(&(kind, at));
                    if !absent && pool_text(&self.pool, le32(row, at), le32(row, at + 4)).is_none()
                    {
                        return Err(format!("section {kind} row {i}: a string outside the pool"));
                    }
                }
            }
        }
        let files = self.table(FILES);
        let mut runs = [Runs::default(); LISTS.len()];
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
                if !runs[l].take(self.list(row, l)) {
                    return Err(format!("file {path}: section {kind} is not one run a file"));
                }
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
            if !runs[l].cover(self.table(kind).rows) {
                return Err(format!("section {kind} has rows no file owns"));
            }
        }
        let modules = self.table(MODULES);
        let mut candidates = Runs::default();
        for i in 0..modules.rows {
            // A module owns two runs: its `as_root` list, then its `as_child` list.
            let [start, mid, end] = candidate_bounds(self.row(modules, i));
            if !candidates.take(start..mid) || !candidates.take(mid..end) {
                return Err("module candidates are not one run a module".into());
            }
        }
        if !candidates.cover(self.table(CANDIDATES).rows) {
            return Err("candidates no module owns".into());
        }
        let (proofs, by_file) = (self.table(PROOFS), self.table(IDX_PROOF_FILE));
        if !ascending(proofs.rows, false, |i| self.text(self.row(proofs, i), 0)) {
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
        if by_file.rows != proofs.rows || !ascending(by_file.rows, false, |i| self.proof_file(i)) {
            return Err("the proof file index does not cover its proofs in file order".into());
        }
        let nodes = self.table(ROW_NODES);
        if !ascending(nodes.rows, true, |i| self.text(self.row(nodes, i), 0)) {
            return Err("settled nodes out of id order".into());
        }
        let mut attrs = Runs::default();
        for i in 0..nodes.rows {
            if !attrs.take(self.attrs_of(self.row(nodes, i))) {
                return Err("node attributes are not one run a node".into());
            }
        }
        if !attrs.cover(self.table(ROW_ATTRS).rows) {
            return Err("node attributes no node owns".into());
        }
        let edges = self.table(ROW_EDGES);
        let ends = |i| [0, STR, 2 * STR].map(|at| self.text(self.row(edges, i), at));
        if !ascending(edges.rows, true, ends) {
            return Err("settled edges out of order".into());
        }
        for (index, kind) in [
            (self.leaf, DEFS),
            (self.name, MENTIONS),
            (self.into_id, ROW_EDGES),
            (self.of_file, ROW_NODES),
            (self.by_leaf, ROW_NODES),
        ] {
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
        self.bytes.is_mapped()
    }

    /// The whole facts of the file `row`.
    fn facts(&self, row: &[u8]) -> FileFacts {
        let defs = self.table(DEFS);
        let mentions = self.table(MENTIONS);
        FileFacts {
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
                        doc: span(r, D_DOC),
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
            modules: self.modules_of(row),
            ..self.intent_of(row)
        }
    }

    /// The modules the file `row` declares ([`CodeView::modules`]).
    fn modules_of(&self, row: &[u8]) -> Vec<ModuleDecl> {
        let modules = self.table(MODULES);
        let candidates = self.table(CANDIDATES);
        let paths = |run: Range<usize>| -> Vec<String> {
            run.map(|c| self.text(self.row(candidates, c), 0).to_owned())
                .collect()
        };
        self.list(row, L_MODULES)
            .map(|m| {
                let r = self.row(modules, m);
                let [start, mid, end] = candidate_bounds(r);
                ModuleDecl {
                    name: self.text(r, 0).to_owned(),
                    def: le32(r, MOD_DEF),
                    test: byte(r, MOD_FLAGS) & FLAG_SET != 0,
                    as_root: paths(start..mid),
                    as_child: paths(mid..end),
                    path_attr: byte(r, MOD_FLAGS) & FLAG_PATH_ATTR != 0,
                }
            })
            .collect()
    }

    /// The facts of the file `row` without its definitions, mentions and modules: what a read of
    /// its design and workflow facts needs ([`CodeView::intent_facts`]).
    fn intent_of(&self, row: &[u8]) -> FileFacts {
        FileFacts {
            path: self.text(row, 0).to_owned(),
            blob: self.text(row, STR).to_owned(),
            lang: self.text(row, 2 * STR).to_owned(),
            half: half_of(byte(row, F_HALF)).unwrap_or_default(),
            scope: scope_of(byte(row, F_SCOPE)).unwrap_or_default(),
            partial: byte(row, F_FLAGS) & FLAG_SET != 0,
            doc: span(row, F_DOC),
            defs: Vec::new(),
            mentions: Vec::new(),
            modules: Vec::new(),
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

    /// The row number of the file at `path`.
    fn file_at(&self, path: &str) -> Option<usize> {
        let files = self.table(FILES);
        let (mut lo, mut hi) = (0, files.rows);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.text(self.row(files, mid), 0).cmp(path) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
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
            caller: (le32(row, STR) != NONE).then(|| self.text(row, STR).to_owned()),
            evidence: format!("{}:{}", self.text(row, P_FILE), le32(row, P_LINE)),
        }
    }

    /// The [start, end) a ROW_NODES `row` ranges into ROW_ATTRS.
    fn attrs_of(&self, row: &[u8]) -> Range<usize> {
        le32(row, N_ATTRS) as usize..le32(row, N_ATTRS + 4) as usize
    }

    /// The settled node at ROW_NODES row `i`.
    fn settled_node(&self, i: usize) -> Node {
        let (row, attrs) = (self.row(self.table(ROW_NODES), i), self.table(ROW_ATTRS));
        Node {
            id: self.text(row, 0).to_owned(),
            kind: self.text(row, STR).to_owned(),
            attrs: (self.attrs_of(row))
                .map(|a| self.row(attrs, a))
                .map(|r| (self.text(r, 0).to_owned(), self.text(r, STR).to_owned()))
                .collect(),
        }
    }

    /// The settled edge at ROW_EDGES row `i`: a live one, as the half holds no other.
    fn settled_edge(&self, i: usize) -> Edge {
        let row = self.row(self.table(ROW_EDGES), i);
        Edge {
            from: self.text(row, 0).to_owned(),
            to: self.text(row, STR).to_owned(),
            rel: self.text(row, 2 * STR).to_owned().into(),
            valid_from: le64(row, E_VALID).cast_signed(),
            valid_to: None,
            source: le64(row, E_SOURCE),
            tier: self.text(row, 3 * STR).to_owned().into(),
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

    fn half(&self) -> Option<&dyn SettledHalf> {
        Some(self)
    }
}

/// The code half read from the rows it was settled as, each read binary-searching its rows or
/// its index.
impl SettledHalf for Snapshot {
    fn node(&self, id: &str) -> Option<Node> {
        let nodes = self.table(ROW_NODES);
        let key = |i| self.text(self.row(nodes, i), 0);
        let at = Self::lower_bound(nodes.rows, id, key);
        (at < nodes.rows && key(at) == id).then(|| self.settled_node(at))
    }

    fn out(&self, id: &str) -> Vec<Edge> {
        let edges = self.table(ROW_EDGES);
        let from = |i| self.text(self.row(edges, i), 0);
        (Self::lower_bound(edges.rows, id, from)..edges.rows)
            .take_while(|&i| from(i) == id)
            .map(|i| self.settled_edge(i))
            .collect()
    }

    fn incoming(&self, id: &str) -> Vec<Edge> {
        let into = self.postings(self.into_id, id);
        into.map(|i| self.settled_edge(i)).collect()
    }

    fn members(&self, file: &str) -> Vec<Node> {
        let of = self.postings(self.of_file, file);
        of.map(|i| self.settled_node(i)).collect()
    }

    fn leaf_nodes(&self, leaf: &str) -> Vec<Node> {
        let by = self.postings(self.by_leaf, leaf);
        by.map(|i| self.settled_node(i)).collect()
    }
}

fn pool_text(pool: &str, off: u32, len: u32) -> Option<&str> {
    let start = off as usize;
    pool.get(start..start.checked_add(len as usize)?)
}

/// The string `cur` refers to next, as an offset and length read through `text`; none when it
/// refers to none.
fn optional<'p>(
    cur: &mut Cursor<'_>,
    text: &dyn Fn(u32, u32) -> Option<&'p str>,
) -> Result<Option<String>, String> {
    let (off, len) = (cur.u32()?, cur.u32()?);
    if off == NONE {
        return Ok(None);
    }
    text(off, len)
        .map(|s| Some(s.to_owned()))
        .ok_or_else(|| "a string lies outside the pool".to_string())
}

/// The string `cur` refers to next, which must be one ([`optional`]).
fn required<'p>(
    cur: &mut Cursor<'_>,
    text: &dyn Fn(u32, u32) -> Option<&'p str>,
) -> Result<String, String> {
    optional(cur, text)?.ok_or_else(|| "a required string is absent".to_string())
}

/// The stamp a snapshot's header payload records, its strings read through `text`.
fn read_stamp<'p>(
    payload: &[u8],
    text: &dyn Fn(u32, u32) -> Option<&'p str>,
) -> Result<Stamp, String> {
    let mut cur = Cursor::new(payload);
    let fingerprint = cur.u64()?;
    let tip = Tip {
        head: required(&mut cur, text)?,
        commit: optional(&mut cur, text)?,
        tree: required(&mut cur, text)?,
    };
    Ok(Stamp {
        tip,
        fingerprint,
        seq: cur.u64()?,
    })
}

/// The stamp the snapshot `bytes` hold records, read from their header without checking their
/// sections: a guess at what the snapshot describes, for work that overlaps the read of it, which
/// its caller confirms against the snapshot [`Snapshot::decode`] returns from the same file.
pub(crate) fn stamp_unchecked(bytes: &[u8]) -> Option<Stamp> {
    let container = Container::unchecked(bytes, MAGIC, VERSION).ok()?;
    let pool = container.section(STRINGS).ok()?;
    let in_pool = |off: u32, len: u32| {
        let start = off as usize;
        std::str::from_utf8(pool.get(start..start.checked_add(len as usize)?)?).ok()
    };
    read_stamp(container.payload, &in_pool).ok()
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
        let at = self.file_at(path)?;
        let facts = self.decoded[at].get_or_init(|| self.facts(self.row(self.table(FILES), at)));
        Some(Cow::Borrowed(facts))
    }

    /// The whole facts when this open has decoded them, else the intent lists alone.
    fn intent_facts(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
        let at = self.file_at(path)?;
        Some(match self.decoded[at].get() {
            Some(facts) => Cow::Borrowed(facts),
            None => Cow::Owned(self.intent_of(self.row(self.table(FILES), at))),
        })
    }

    /// The modules of the whole facts when this open has decoded them, else decoded alone.
    fn modules(&self, path: &str) -> Option<Cow<'_, [ModuleDecl]>> {
        let at = self.file_at(path)?;
        Some(match self.decoded[at].get() {
            Some(facts) => Cow::Borrowed(&facts.modules[..]),
            None => Cow::Owned(self.modules_of(self.row(self.table(FILES), at))),
        })
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
pub(crate) mod tests {
    use super::*;
    use rigger_domain::code_facts::{
        Def, DocConcept, DocLink, Half, Mention, MentionKind, ModuleDecl, Scope, Tip, WfNode,
        WfRelation,
    };
    use rigger_domain::code_graph::CodeGraph;
    use rigger_domain::code_view::{Cluster, FactsView, ViewFilter};
    use rigger_domain::composed::Indexed;
    use rigger_domain::contextgraph::query::node_name;
    use rigger_domain::contextgraph::{Neighbors, PLACEHOLDER_SIGIL};
    use rigger_domain::resolve::Resolver;
    use std::collections::BTreeSet;
    use std::fmt::Debug;

    /// The snapshot `bytes`, at [`VERSION`], written again at format `version` with `edit` applied
    /// to each section by its kind, every crc recomputed.
    pub(crate) fn rewritten(
        bytes: &[u8],
        version: u16,
        edit: impl Fn(u16, &mut Vec<u8>),
    ) -> Vec<u8> {
        let parsed = Container::parse(bytes, MAGIC, VERSION).expect("parses");
        let sections: Vec<(u16, Vec<u8>)> = (STRINGS..=LAST)
            .map(|kind| {
                let mut section = parsed.section(kind).expect("every section").to_vec();
                edit(kind, &mut section);
                (kind, section)
            })
            .collect();
        write_container(MAGIC, version, parsed.payload, &sections).expect("writes")
    }

    pub(crate) fn stamp(commit: Option<&str>, seq: u64) -> Stamp {
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

    /// A code file with a one-line file doc span, a definition with no doc span beside one whose
    /// span covers three lines, and two module declarations: `inner`, whose `as_root` and
    /// `as_child` lists differ, and `renamed`, with `path_attr` and lists of different lengths,
    /// one of them empty.
    pub(crate) fn code_file() -> FileFacts {
        FileFacts {
            path: "src/ünï.rs".into(),
            blob: "b1".into(),
            lang: "rust".into(),
            half: Half::Code,
            scope: Scope::Product,
            partial: true,
            doc: Some((1, 1)),
            defs: vec![
                Def {
                    local: "Outer".into(),
                    leaf: "Outer".into(),
                    kind: "struct".into(),
                    line: 2,
                    end_line: 11,
                    parent: None,
                    test: false,
                    doc: None,
                },
                Def {
                    local: "Outer::run".into(),
                    leaf: "run".into(),
                    kind: "fn".into(),
                    line: 6,
                    end_line: 8,
                    parent: Some(0),
                    test: true,
                    doc: Some((3, 5)),
                },
            ],
            mentions: vec![
                Mention {
                    enclosing: Some(1),
                    name: "crate::x::run".into(),
                    leaf: "run".into(),
                    kind: MentionKind::Call,
                    line: 7,
                    test: true,
                },
                Mention {
                    enclosing: None,
                    name: "Outer".into(),
                    leaf: "Outer".into(),
                    kind: MentionKind::Tagged("type".into()),
                    line: 13,
                    test: false,
                },
            ],
            modules: vec![
                ModuleDecl {
                    name: "inner".into(),
                    def: 1,
                    test: true,
                    as_root: vec!["src/inner.rs".into(), "src/inner/mod.rs".into()],
                    as_child: vec!["src/ünï/inner.rs".into(), "src/ünï/inner/mod.rs".into()],
                    path_attr: false,
                },
                ModuleDecl {
                    name: "renamed".into(),
                    def: 0,
                    test: false,
                    as_root: vec!["src/p/y.rs".into()],
                    as_child: Vec::new(),
                    path_attr: true,
                },
            ],
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

    /// The cluster `id` of `members`, at resolution 1.5.
    fn cluster(id: &str, members: &[&str]) -> Cluster {
        Cluster {
            id: id.into(),
            label: format!("label of {id}"),
            resolution: 1.5,
            hash: "h".into(),
            members: members.iter().map(|m| (*m).into()).collect(),
        }
    }

    fn groupings() -> Groupings {
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
    /// declaring def, test, and the one path its `as_root` and `as_child` lists both hold).
    pub(crate) fn proving_file(
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
                    doc: None,
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
                .map(|&(name, def, test, path)| ModuleDecl {
                    name: name.into(),
                    def,
                    test,
                    as_root: vec![path.into()],
                    as_child: vec![path.into()],
                    path_attr: false,
                })
                .collect(),
            ..empty_file(path)
        }
    }

    /// A test file by its scope, one by the test-module closure alone and calls from both (at
    /// file level and inside a definition, on lines whose text order is not their number order).
    fn proving_files() -> Vec<FileFacts> {
        vec![
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
        ]
    }

    /// WHAT A STORE SETTLES READS BACK AS THE RESOLVER COMPUTES IT: over a view holding
    /// [`proving_files`], the snapshot's test files, its proof calls (every one, those of each
    /// definition, those in each file) and its definition leaves are what a resolver computes
    /// over the facts.
    #[test]
    fn a_snapshot_settles_the_test_files_and_proofs_the_resolver_computes() {
        let view = FactsView::new(stamp(Some("c"), 1), proving_files());
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

    /// DOC SPANS AND MODULE LISTS SURVIVE A SNAPSHOT: a code file whose facts carry a file doc
    /// span, a definition's doc span beside an absent one, and module declarations whose
    /// `as_root` and `as_child` lists differ, one with `path_attr`, and a design file with no
    /// doc span, encoded at format version 6, read back with each span, list and flag as written.
    #[test]
    fn doc_spans_and_module_lists_survive_a_snapshot() {
        let view = FactsView::new(stamp(None, 1), vec![code_file(), design_file()]);
        let bytes = encode(&view, &Groupings::default()).expect("encodes");
        if let Err(e) = Container::unchecked(&bytes, MAGIC, 6) {
            panic!("the header's format version: {e}");
        }
        let snap = Snapshot::decode(bytes).expect("decodes");
        let code = snap.file("src/ünï.rs").expect("the code file");
        let design = snap.file("docs/a.md").expect("the design file");
        assert_eq!(
            (code.doc, design.doc),
            (Some((1, 1)), None),
            "file doc spans"
        );
        let def_docs: Vec<Option<(u32, u32)>> = code.defs.iter().map(|d| d.doc).collect();
        assert_eq!(def_docs, [None, Some((3, 5))], "definition doc spans");
        fn strs(list: &[String]) -> Vec<&str> {
            list.iter().map(String::as_str).collect()
        }
        let modules: Vec<(&str, bool, Vec<&str>, Vec<&str>)> = (code.modules.iter())
            .map(|m| {
                (
                    m.name.as_str(),
                    m.path_attr,
                    strs(&m.as_root),
                    strs(&m.as_child),
                )
            })
            .collect();
        assert_eq!(
            modules,
            [
                (
                    "inner",
                    false,
                    vec!["src/inner.rs", "src/inner/mod.rs"],
                    vec!["src/ünï/inner.rs", "src/ünï/inner/mod.rs"],
                ),
                ("renamed", true, vec!["src/p/y.rs"], vec![]),
            ],
            "module declarations: name, path_attr, as_root, as_child"
        );
    }

    /// Each row: the bounds into CANDIDATES of the second module of [`code_file`]'s snapshot,
    /// written over its MODULES row with every crc recomputed, and what decoding gives: that
    /// module's `as_root` [start, mid) and `as_child` [mid, end) lists, or the refusal of a run
    /// that does not start where the first module's [0, 4) ends or whose mid lies outside it.
    #[test]
    fn a_module_decodes_only_as_one_run_of_candidates() {
        let view = FactsView::new(stamp(None, 1), vec![code_file()]);
        let bytes = encode(&view, &Groupings::default()).expect("encodes");
        let rewrite = |bounds: [u32; 3]| {
            rewritten(&bytes, VERSION, |kind, section| {
                if kind == MODULES {
                    let at = MODULE_ROW + MOD_CANDS;
                    let row: Vec<u8> = bounds.iter().flat_map(|b| b.to_le_bytes()).collect();
                    section[at..at + 12].copy_from_slice(&row);
                }
            })
        };
        assert_eq!(rewrite([4, 5, 5]), bytes, "the bounds as encoded");
        const REFUSED: &str = "module candidates are not one run a module";
        /// The module's `as_root` and `as_child` lists, or the decode's refusal.
        type Decoded = Result<[&'static [&'static str]; 2], &'static str>;
        let cases: [(&str, [u32; 3], Decoded); 5] = [
            ("as encoded", [4, 5, 5], Ok([&["src/p/y.rs"], &[]])),
            ("an empty as_root", [4, 4, 5], Ok([&[], &["src/p/y.rs"]])),
            ("a gap before the run", [5, 5, 5], Err(REFUSED)),
            ("mid before start", [4, 3, 5], Err(REFUSED)),
            ("end before mid", [4, 5, 4], Err(REFUSED)),
        ];
        for (name, bounds, want) in cases {
            let got = Snapshot::decode(rewrite(bounds)).map(|snap| {
                let modules = snap.modules("src/ünï.rs").expect("the code file");
                [modules[1].as_root.clone(), modules[1].as_child.clone()]
            });
            let want = want
                .map(|lists| lists.map(|list| list.iter().map(|p| p.to_string()).collect()))
                .map_err(String::from);
            assert_eq!(got, want, "{name}");
        }
    }

    /// Each row: a stamp, the files and the groupings a view holds. Every section must read back
    /// as written: the stamp, the file list, each file's facts, both leaf indexes, the groupings,
    /// and the code half settled whole ([`assert_settles_the_half`]); and re-encoding the
    /// snapshot yields the same bytes.
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
            (
                "proving files and a community",
                stamp(Some("c"), 4),
                proving_files(),
                Groupings {
                    communities: vec![cluster(
                        "community/1.5/0",
                        &["src/lib.rs::run", "src/lib.rs::stop"],
                    )],
                    concepts: Vec::new(),
                },
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
                // Decoding a file's modules alone reads none of its other facts.
                assert_eq!(
                    snap.modules(&file.path).as_deref(),
                    Some(&file.modules[..]),
                    "{name}: modules of {}",
                    file.path
                );
                // Before the whole facts are read, the intent facts leave the code lists out.
                let intent = FileFacts {
                    defs: Vec::new(),
                    mentions: Vec::new(),
                    modules: Vec::new(),
                    ..file.clone()
                };
                assert_eq!(
                    snap.intent_facts(&file.path).as_deref(),
                    Some(&intent),
                    "{name}: intent facts of {}",
                    file.path
                );
                assert_eq!(
                    snap.file(&file.path).as_deref(),
                    Some(file),
                    "{name}: facts of {}",
                    file.path
                );
                assert_eq!(
                    snap.intent_facts(&file.path).as_deref(),
                    Some(file),
                    "{name}: intent facts of {} once its facts are read",
                    file.path
                );
                assert_eq!(
                    snap.modules(&file.path).as_deref(),
                    Some(&file.modules[..]),
                    "{name}: modules of {} once its facts are read",
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
            assert!(
                snap.intent_facts("absent").is_none(),
                "{name}: absent intent"
            );
            assert!(snap.modules("absent").is_none(), "{name}: absent modules");
            assert_eq!(snap.defs_named("absent").count(), 0, "{name}: absent leaf");
            assert_settles_the_half(name, &snap, &view, &groupings);
            assert_eq!(
                encode(&snap, snap.groupings()),
                Ok(bytes),
                "{name}: re-encode"
            );
        }
    }

    /// The code half `snap` settles reads as `view` resolved whole under the default filter with
    /// `groupings` joined on: each node and its edges both ways, asked of every id the whole graph
    /// holds or an edge of it names, each file's members and the nodes of every leaf a node can
    /// be bound by, and each of them asked of one the graph does not hold.
    fn assert_settles_the_half(
        name: &str,
        snap: &Snapshot,
        view: &FactsView,
        groupings: &Groupings,
    ) {
        let half = (snap.settled().and_then(|s| s.half()))
            .unwrap_or_else(|| panic!("{name}: a snapshot settles its code half"));
        let want = Indexed::resolved(&CodeGraph::new(view, ViewFilter::default()), groupings);
        let whole = want.whole().expect("an indexed graph reads whole");
        let same = |read: &str, asked: &str, got: &dyn Debug, want: &dyn Debug| {
            assert_eq!(
                format!("{got:?}"),
                format!("{want:?}"),
                "{name}: {read} of {asked}"
            );
        };
        let mut ids: BTreeSet<&str> = whole.nodes.iter().map(|n| n.id.as_str()).collect();
        ids.extend(
            whole
                .edges
                .iter()
                .flat_map(|e| [e.from.as_str(), e.to.as_str()]),
        );
        for id in ids.into_iter().chain(["absent"]) {
            same("node", id, &half.node(id), &want.node(id).unwrap());
            same("out", id, &half.out(id), &want.out(id).unwrap());
            same(
                "incoming",
                id,
                &half.incoming(id),
                &want.incoming(id).unwrap(),
            );
        }
        for file in view.files().map(|f| f.path).chain(["absent"]) {
            same(
                "members",
                file,
                &half.members(file),
                &want.members(file).unwrap(),
            );
        }
        // A node is bound by its name or by a tail of its id.
        let texts = (whole.nodes.iter()).flat_map(|n| node_name(n).into_iter().chain([&*n.id]));
        let leaves: BTreeSet<&str> = texts.flat_map(tails).collect();
        for leaf in leaves.into_iter().chain(["absent"]) {
            let got = half.leaf_nodes(leaf);
            same("leaf nodes", leaf, &got, &want.leaf_nodes(leaf).unwrap());
        }
    }

    /// `text`, and each of its tails after a `:`, a `.` or the placeholder sigil.
    fn tails(text: &str) -> impl Iterator<Item = &str> {
        let cuts =
            (text.char_indices()).filter(|&(_, c)| matches!(c, ':' | '.' | PLACEHOLDER_SIGIL));
        std::iter::once(text).chain(cuts.map(move |(at, c)| &text[at + c.len_utf8()..]))
    }

    /// Each row: a section of the code half a snapshot settles, the damage written over it with
    /// every crc recomputed, and what opening the snapshot gives: its refusal, or none for the
    /// section as encoded.
    #[test]
    fn a_snapshot_whose_settled_code_half_is_damaged_is_refused() {
        let view = FactsView::new(stamp(Some("c"), 1), proving_files());
        let bytes = encode(&view, &Groupings::default()).expect("encodes");
        type Damage = Box<dyn Fn(&mut Vec<u8>)>;
        // The u32 `value` written at `at`.
        let put = |at: usize, value: u32| -> Damage {
            Box::new(move |s| s[at..at + 4].copy_from_slice(&value.to_le_bytes()))
        };
        // The first `len` bytes of the first row, `width` wide, written over the second row's.
        let repeat =
            |width: usize, len: usize| -> Damage { Box::new(move |s| s.copy_within(..len, width)) };
        // The first `len` bytes of the first two rows, `width` wide, swapped.
        let swap = |width: usize, len: usize| -> Damage {
            Box::new(move |s| {
                let (first, rest) = s.split_at_mut(width);
                first[..len].swap_with_slice(&mut rest[..len]);
            })
        };
        // The second node's attribute run ending one before it starts, and the third's starting
        // there, so each run still starts where the one before it ends.
        let backwards = || -> Damage {
            Box::new(|s| {
                let at = NODE_ROW + N_ATTRS;
                let back = (le32(s, at) - 1).to_le_bytes();
                s[at + 4..at + 8].copy_from_slice(&back);
                s[NODE_ROW + at..NODE_ROW + at + 4].copy_from_slice(&back);
            })
        };
        let last_posting = || -> Damage {
            Box::new(|s| {
                let at = s.len() - 4;
                s[at..].copy_from_slice(&NONE.to_le_bytes())
            })
        };
        let refused = |why: &str| Err(why.to_string());
        let outside = |kind: u16| Err(format!("section {kind} row 0: a string outside the pool"));
        let cases: Vec<(&str, u16, Damage, Result<(), String>)> = vec![
            ("as encoded", ROW_NODES, Box::new(|_| ()), Ok(())),
            (
                "a node id repeated",
                ROW_NODES,
                repeat(NODE_ROW, STR),
                refused("settled nodes out of id order"),
            ),
            (
                "node ids descending",
                ROW_NODES,
                swap(NODE_ROW, STR),
                refused("settled nodes out of id order"),
            ),
            (
                "an attribute run starting late",
                ROW_NODES,
                put(N_ATTRS, 1),
                refused("node attributes are not one run a node"),
            ),
            (
                "an attribute run ending before it starts",
                ROW_NODES,
                backwards(),
                refused("node attributes are not one run a node"),
            ),
            (
                "an attribute no node owns",
                ROW_ATTRS,
                Box::new(|s| s.extend_from_within(..ATTR_ROW)),
                refused("node attributes no node owns"),
            ),
            (
                "an attribute value outside the pool",
                ROW_ATTRS,
                put(STR, NONE),
                outside(ROW_ATTRS),
            ),
            (
                "an edge repeated",
                ROW_EDGES,
                repeat(EDGE_ROW, 3 * STR),
                refused("settled edges out of order"),
            ),
            (
                "edges descending",
                ROW_EDGES,
                swap(EDGE_ROW, 3 * STR),
                refused("settled edges out of order"),
            ),
            (
                "an edge tier outside the pool",
                ROW_EDGES,
                put(3 * STR, NONE),
                outside(ROW_EDGES),
            ),
            (
                "an incoming posting past the edges",
                IDX_ROW_IN,
                last_posting(),
                refused("a posting outside its section"),
            ),
            (
                "a member posting past the nodes",
                IDX_ROW_FILE,
                last_posting(),
                refused("a posting outside its section"),
            ),
            (
                "a leaf posting past the nodes",
                IDX_ROW_LEAF,
                last_posting(),
                refused("a posting outside its section"),
            ),
        ];
        for (name, kind, damage, want) in cases {
            let damaged = rewritten(&bytes, VERSION, |k, s| {
                if k == kind {
                    damage(s)
                }
            });
            assert_eq!(Snapshot::decode(damaged).map(|_| ()), want, "{name}");
        }
    }
}
