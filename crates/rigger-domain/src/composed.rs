//! The composed view: the code half and the record half of the graph read as one context graph.
//!
//! The code half is the graph a code view resolves to, with the view's groupings joined onto it;
//! the record half is the graph the event log folds. An id both hold is one node of the tree's
//! kind carrying both halves' attrs, an alias reads as its canonical id, a record edge whose end
//! neither half holds reaches an `artifact` node, and a decision governing a document realizes
//! that document's concepts. A node whose file differs in the caller's worktree is withheld and
//! counted on the honesty label. The searches every reader asks ([`subgraph`], [`calls`],
//! [`locate`]) are written once over [`Neighbors`], so they step through both halves alike.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::Mutex;

use crate::code_facts::Tip;
use crate::code_graph::CodeGraph;
use crate::code_view::{CodeView, Groupings, ViewFilter, WorktreeDiff};
use crate::contextgraph::query::{
    entity_id, leaf_of, member_file, node_file, node_leaf, node_name, path_leaf, split_id,
};
use crate::contextgraph::{
    entity_site, holds_attachment, is_call_target, CallEdge, CallGraph, CallNode, Candidate,
    ContextGraph, Direction, Edge, EntitySite, Error, Graph, Located, Neighbors, Node,
    KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, PLACEHOLDER_SIGIL, REL_CALLS,
    REL_GOVERNS, REL_IN_COMMUNITY, REL_REALIZES, REL_REFERENCES, TIER_AMBIGUOUS, TIER_EXTRACTED,
    TIER_INFERRED,
};

/// How the caller's worktree compares with the tree a view describes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compared {
    /// This many of the worktree's files differ from the tree; none means the answers describe
    /// the worktree exactly.
    Differ(usize),
    /// The worktree cannot be compared with the tree (its stamp commit is not in the worktree's
    /// object store, or its index cannot be read), so nothing is withheld.
    Uncomparable,
}

/// The honesty label (spec 6.1) every answer travels with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// The commit the answers describe, or the tree id when the tree was read from a directory.
    pub describes: String,
    /// The caller's worktree, as the label names it.
    pub worktree: String,
    pub compared: Compared,
    /// Why the view is behind the tree it was opened for, when it is.
    pub behind: Option<String>,
    /// The nodes withheld so far because their file differs.
    pub withheld: usize,
}

impl Label {
    /// The label of a view of `tip` read from the worktree `honesty` names, before anything is
    /// withheld, and the worktree's files that differ from the tip's tree.
    pub fn of(tip: &Tip, honesty: Honesty) -> (Label, BTreeSet<String>) {
        let (compared, differing) = match honesty.diff {
            Ok(diff) => {
                // A file out of the tree's scope (rigger's own state among them) holds no fact.
                let files: BTreeSet<String> = (diff.changed.into_iter())
                    .chain(diff.untracked)
                    .filter(|f| crate::code_view::in_scope(f))
                    .collect();
                (Compared::Differ(files.len()), files)
            }
            Err(_) => (Compared::Uncomparable, BTreeSet::new()),
        };
        let label = Label {
            describes: tip.commit.clone().unwrap_or_else(|| tip.tree.clone()),
            worktree: honesty.worktree,
            compared,
            behind: honesty.behind,
            withheld: 0,
        };
        (label, differing)
    }
}

/// `describes <commit>`, then how many files differ (or that the worktree cannot be compared),
/// how many nodes were withheld and why the view is behind, each only when there is one.
impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "describes {}", self.describes)?;
        let w = &self.worktree;
        match self.compared {
            Compared::Differ(0) => {}
            Compared::Differ(1) => write!(f, "; 1 file in {w} differs")?,
            Compared::Differ(n) => write!(f, "; {n} files in {w} differ")?,
            Compared::Uncomparable => write!(f, "; {w} cannot be compared")?,
        }
        if self.withheld > 0 {
            write!(f, "; {} withheld", self.withheld)?;
        }
        if let Some(behind) = &self.behind {
            write!(f, "; {behind}")?;
        }
        Ok(())
    }
}

/// What a reader knows of its own worktree when it opens a view.
#[derive(Clone, Debug)]
pub struct Honesty {
    /// The caller's worktree, as the label names it.
    pub worktree: String,
    /// How the worktree differs from the view's tree, or why it cannot be compared.
    pub diff: Result<WorktreeDiff, String>,
    /// Why the view is behind the tree it was opened for, when it is.
    pub behind: Option<String>,
}

/// Whether `node` is bound by `leaf`: its [`node_name`] is `leaf`, or its id's text after the
/// first `::` is `leaf` or `leaf` behind the [`PLACEHOLDER_SIGIL`].
fn binds_leaf(node: &Node, leaf: &str) -> bool {
    node_name(node) == Some(leaf) || id_binds_leaf(&node.id, leaf)
}

/// Whether the text of `id` after its first `::` is `leaf` or `leaf` behind the
/// [`PLACEHOLDER_SIGIL`].
fn id_binds_leaf(id: &str, leaf: &str) -> bool {
    split_id(id)
        .1
        .is_some_and(|rest| rest == leaf || rest.strip_prefix(PLACEHOLDER_SIGIL) == Some(leaf))
}

/// The edges sorted by ends then relation, one per (from, to, relation), the first kept.
fn ordered(mut edges: Vec<Edge>) -> Vec<Edge> {
    edges.sort_by(|a, b| (&a.from, &a.to, &a.rel).cmp(&(&b.from, &b.to, &b.rel)));
    edges.dedup_by(|b, a| (&a.from, &a.to, &a.rel) == (&b.from, &b.to, &b.rel));
    edges
}

/// The node an edge end no half holds reads as.
fn artifact(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_ARTIFACT.to_string(),
        attrs: BTreeMap::new(),
    }
}

/// `nodes` and `edges` as one graph: every edge end `nodes` does not hold an [`artifact`], nodes
/// in id order and edges in [`ordered`] order.
fn closed(mut nodes: BTreeMap<String, Node>, edges: Vec<Edge>) -> Graph {
    for e in &edges {
        for end in [&e.from, &e.to] {
            nodes.entry(end.clone()).or_insert_with(|| artifact(end));
        }
    }
    Graph {
        nodes: nodes.into_values().collect(),
        edges: ordered(edges),
    }
}

/// A graph held in memory and indexed for [`Neighbors`]: the code half a view resolves to.
#[derive(Default)]
pub struct Indexed {
    nodes: BTreeMap<String, Node>,
    out: BTreeMap<String, Vec<Edge>>,
    incoming: BTreeMap<String, Vec<Edge>>,
    members: BTreeMap<String, Vec<String>>,
    leaves: BTreeMap<String, BTreeSet<String>>,
    kinds: BTreeMap<String, Vec<String>>,
    aliases: Vec<(String, String)>,
}

impl Indexed {
    /// `graph` indexed, its edges live, with the recorded `aliases`.
    pub fn new(graph: Graph, aliases: Vec<(String, String)>) -> Self {
        let mut ix = Indexed {
            aliases,
            ..Indexed::default()
        };
        for e in ordered(graph.edges) {
            if e.valid_to.is_none() {
                ix.out.entry(e.from.clone()).or_default().push(e.clone());
                ix.incoming.entry(e.to.clone()).or_default().push(e);
            }
        }
        for n in graph.nodes {
            if let Some(file) = member_file(&n) {
                ix.members
                    .entry(file.to_string())
                    .or_default()
                    .push(n.id.clone());
            }
            let rest = split_id(&n.id).1;
            let leaves = [
                node_name(&n),
                rest,
                rest.and_then(|r| r.strip_prefix(PLACEHOLDER_SIGIL)),
            ];
            for leaf in leaves.into_iter().flatten() {
                ix.leaves
                    .entry(leaf.to_string())
                    .or_default()
                    .insert(n.id.clone());
            }
            ix.kinds
                .entry(n.kind.clone())
                .or_default()
                .push(n.id.clone());
            ix.nodes.insert(n.id.clone(), n);
        }
        ix
    }

    /// The nodes of `ids` this graph holds, in the order given.
    fn held<'a>(&self, ids: impl IntoIterator<Item = &'a String>) -> Vec<Node> {
        ids.into_iter()
            .filter_map(|id| self.nodes.get(id).cloned())
            .collect()
    }
}

impl Neighbors for Indexed {
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        Ok(self.nodes.get(id).cloned())
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        Ok(self.out.get(id).cloned().unwrap_or_default())
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        Ok(self.incoming.get(id).cloned().unwrap_or_default())
    }
    fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
        Ok(self.held(self.members.get(file).into_iter().flatten()))
    }
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
        let nodes = self.held(self.leaves.get(leaf).into_iter().flatten());
        Ok(nodes.into_iter().filter(|n| binds_leaf(n, leaf)).collect())
    }
    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        Ok(self.held(self.kinds.get(kind).into_iter().flatten()))
    }
    /// An in-memory graph records no reference rows, so it holds no unresolved leaf.
    fn unresolved(&self, _node: &Node) -> Result<Vec<String>, Error> {
        Ok(Vec::new())
    }
    fn aliases(&self) -> Result<Vec<(String, String)>, Error> {
        Ok(self.aliases.clone())
    }
    fn whole(&self) -> Result<Graph, Error> {
        Ok(Graph {
            nodes: self.nodes.values().cloned().collect(),
            edges: self.out.values().flatten().cloned().collect(),
        })
    }
}

/// `graph` with the view's `groupings` joined on: each community and concept a node carrying the
/// pass's resolution, hash and label, and an inferred `IN_COMMUNITY` or `REALIZES` edge from
/// each member `graph` holds as a kind that holds it ([`holds_attachment`]).
fn joined(mut graph: Graph, groupings: &Groupings) -> Graph {
    let kinds: BTreeMap<String, String> = graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.kind.clone()))
        .collect();
    let arms = [
        (KIND_COMMUNITY, REL_IN_COMMUNITY, &groupings.communities),
        (KIND_CONCEPT, REL_REALIZES, &groupings.concepts),
    ];
    for (kind, rel, clusters) in arms {
        for c in clusters {
            let attrs = [
                ("hash", c.hash.clone()),
                ("label", c.label.clone()),
                ("resolution", format!("{}", c.resolution)),
            ];
            graph.nodes.push(Node {
                id: c.id.clone(),
                kind: kind.to_string(),
                attrs: attrs.map(|(k, v)| (k.to_string(), v)).into(),
            });
            for m in &c.members {
                if kinds.get(m).is_some_and(|k| holds_attachment(k, rel)) {
                    graph.edges.push(Edge {
                        from: m.clone(),
                        to: c.id.clone(),
                        rel: rel.to_string(),
                        valid_from: 0,
                        valid_to: None,
                        source: 0,
                        tier: TIER_INFERRED.to_string(),
                    });
                }
            }
        }
    }
    graph
}

/// One id's node from both halves: the code half's kind with the record half's attrs merged
/// under its own, or whichever half holds it.
fn merge(code: Option<Node>, record: Option<Node>) -> Option<Node> {
    match (code, record) {
        (Some(mut code), Some(record)) => {
            for (k, v) in record.attrs {
                code.attrs.entry(k).or_insert(v);
            }
            Some(code)
        }
        (code, record) => code.or(record),
    }
}

/// The `REALIZES` edge the decision governing a document holds to `concept`, one of that
/// document's concepts: placed and sourced as the `GOVERNS` edge `governs`, inferred.
fn attachment(governs: &Edge, concept: &str) -> Edge {
    Edge {
        from: governs.from.clone(),
        to: concept.to_string(),
        rel: REL_REALIZES.to_string(),
        valid_from: governs.valid_from,
        valid_to: None,
        source: governs.source,
        tier: TIER_INFERRED.to_string(),
    }
}

/// Which way an edge read runs from the node it is read at.
#[derive(Clone, Copy)]
enum Way {
    Out,
    In,
}

/// The record half and a view's code half read as one context graph.
pub struct Composed<'r> {
    records: &'r dyn Neighbors,
    code: Indexed,
    /// Each recorded alias's canonical id.
    canonical: BTreeMap<String, String>,
    /// Each canonical id's recorded aliases.
    aliased: BTreeMap<String, Vec<String>>,
    /// Each concept's members and each member's concepts, as the view grouped them.
    concept_members: BTreeMap<String, Vec<String>>,
    concepts_of: BTreeMap<String, Vec<String>>,
    /// The files of the caller's worktree that differ from the view's tree.
    differing: BTreeSet<String>,
    label: Label,
    /// The ids withheld so far because their file differs.
    withheld: Mutex<BTreeSet<String>>,
}

impl<'r> Composed<'r> {
    /// The record half `records` composed with `view` under `filter`, labelled by `honesty`.
    pub fn new(
        records: &'r dyn Neighbors,
        view: &dyn CodeView,
        filter: ViewFilter,
        honesty: Honesty,
    ) -> Result<Self, Error> {
        let groupings = view.groupings();
        let code = Indexed::new(
            joined(CodeGraph::new(view, filter).whole(), groupings),
            Vec::new(),
        );
        let mut canonical = BTreeMap::new();
        let mut aliased: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (alias, id) in records.aliases()? {
            aliased.entry(id.clone()).or_default().push(alias.clone());
            canonical.insert(alias, id);
        }
        let mut concept_members: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut concepts_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in &groupings.concepts {
            for m in c.members.iter().filter(|m| code.nodes.contains_key(*m)) {
                concept_members
                    .entry(c.id.clone())
                    .or_default()
                    .push(m.clone());
                concepts_of.entry(m.clone()).or_default().push(c.id.clone());
            }
        }
        let (label, differing) = Label::of(&view.stamp().tip, honesty);
        Ok(Composed {
            records,
            code,
            canonical,
            aliased,
            concept_members,
            concepts_of,
            differing,
            label,
            withheld: Mutex::new(BTreeSet::new()),
        })
    }

    /// `id` read through the recorded aliases: its canonical id, or itself.
    fn canon<'a>(&'a self, id: &'a str) -> &'a str {
        self.canonical.get(id).map_or(id, String::as_str)
    }

    /// `e` with both ends read through the recorded aliases.
    fn canon_edge(&self, mut e: Edge) -> Edge {
        e.from = self.canon(&e.from).to_string();
        e.to = self.canon(&e.to).to_string();
        e
    }

    /// `id` and every alias recorded for it.
    fn spellings<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a str> {
        std::iter::once(id).chain(
            self.aliased
                .get(id)
                .into_iter()
                .flatten()
                .map(String::as_str),
        )
    }

    /// Whether `node` is withheld because its file differs in the caller's worktree, counting it
    /// when it is.
    fn withholds(&self, node: &Node) -> bool {
        let differs = node_file(&node.kind, &node.id).is_some_and(|f| self.differing.contains(f));
        if differs {
            self.withheld.lock().unwrap().insert(node.id.clone());
        }
        differs
    }

    /// The node `id` names in both halves, merged, before any is withheld; `None` for an alias.
    fn merged(&self, id: &str) -> Result<Option<Node>, Error> {
        if self.canonical.contains_key(id) {
            return Ok(None);
        }
        Ok(merge(self.code.node(id)?, self.records.node(id)?))
    }

    /// Whether an edge end `id` is answered: no file differs, or the node it reads as (an
    /// [`artifact`] when no half holds it) is not withheld.
    fn answers_end(&self, id: &str) -> Result<bool, Error> {
        if self.differing.is_empty() {
            return Ok(true);
        }
        let node = self.merged(id)?.unwrap_or_else(|| artifact(id));
        Ok(!self.withholds(&node))
    }

    /// The `REALIZES` attachments read at `id` running `way`: out of a decision, one per concept
    /// of each document it governs; into a concept, one per decision governing a member.
    fn attachments(&self, id: &str, way: Way) -> Result<Vec<Edge>, Error> {
        let mut found = Vec::new();
        match way {
            Way::Out => {
                for e in self.records.out(id)? {
                    if e.rel != REL_GOVERNS {
                        continue;
                    }
                    let e = self.canon_edge(e);
                    for c in self.concepts_of.get(&e.to).into_iter().flatten() {
                        found.push(attachment(&e, c));
                    }
                }
            }
            Way::In => {
                for m in self.concept_members.get(id).into_iter().flatten() {
                    for spelling in self.spellings(m) {
                        for e in self.records.incoming(spelling)? {
                            if e.rel == REL_GOVERNS {
                                found.push(attachment(&self.canon_edge(e), id));
                            }
                        }
                    }
                }
            }
        }
        Ok(found)
    }

    /// The edges read at `id` running `way` from both halves and its aliases, ends canonical,
    /// with its decision attachments, each (from, to, relation) once, the code half's first;
    /// none at an alias or a withheld node, and none whose other end is withheld.
    fn edges(&self, id: &str, way: Way) -> Result<Vec<Edge>, Error> {
        if self.canonical.contains_key(id) || !self.answers_end(id)? {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for spelling in self.spellings(id) {
            for half in [&self.code as &dyn Neighbors, self.records] {
                let edges = match way {
                    Way::Out => half.out(spelling)?,
                    Way::In => half.incoming(spelling)?,
                };
                found.extend(edges.into_iter().map(|e| self.canon_edge(e)));
            }
        }
        found.extend(self.attachments(id, way)?);
        let mut answered = Vec::new();
        for e in found {
            let other = match way {
                Way::Out => &e.to,
                Way::In => &e.from,
            };
            if self.answers_end(other)? {
                answered.push(e);
            }
        }
        Ok(ordered(answered))
    }

    /// The answered nodes of `ids` from both halves, in id order.
    fn answered(&self, ids: impl IntoIterator<Item = String>) -> Result<Vec<Node>, Error> {
        let ids: BTreeSet<String> = ids.into_iter().collect();
        let mut nodes = Vec::new();
        for id in ids {
            nodes.extend(self.node(&id)?);
        }
        Ok(nodes)
    }

    /// The ids both halves answer `read` with.
    fn both(
        &self,
        read: impl Fn(&dyn Neighbors) -> Result<Vec<Node>, Error>,
    ) -> Result<Vec<String>, Error> {
        let mut ids = Vec::new();
        for half in [&self.code as &dyn Neighbors, self.records] {
            ids.extend(read(half)?.into_iter().map(|n| n.id));
        }
        Ok(ids)
    }
}

impl Neighbors for Composed<'_> {
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        Ok(self.merged(id)?.filter(|n| !self.withholds(n)))
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.edges(id, Way::Out)
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.edges(id, Way::In)
    }
    fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
        let nodes = self.answered(self.both(|h| h.members(file))?)?;
        Ok(nodes
            .into_iter()
            .filter(|n| member_file(n) == Some(file))
            .collect())
    }
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
        let nodes = self.answered(self.both(|h| h.leaf_nodes(leaf))?)?;
        Ok(nodes.into_iter().filter(|n| binds_leaf(n, leaf)).collect())
    }
    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        let nodes = self.answered(self.both(|h| h.of_kind(kind))?)?;
        Ok(nodes.into_iter().filter(|n| n.kind == kind).collect())
    }
    fn unresolved(&self, node: &Node) -> Result<Vec<String>, Error> {
        let mut leaves: BTreeSet<String> = self.code.unresolved(node)?.into_iter().collect();
        leaves.extend(self.records.unresolved(node)?);
        Ok(leaves.into_iter().collect())
    }
    fn aliases(&self) -> Result<Vec<(String, String)>, Error> {
        self.records.aliases()
    }
    fn whole(&self) -> Result<Graph, Error> {
        let code = self.code.whole()?;
        let records = self.records.whole()?;
        let is_alias = |n: &Node| self.canonical.contains_key(&n.id);
        let mut nodes: BTreeMap<String, Node> = code
            .nodes
            .into_iter()
            .filter(|n| !is_alias(n))
            .map(|n| (n.id.clone(), n))
            .collect();
        for r in records.nodes.into_iter().filter(|n| !is_alias(n)) {
            let code = nodes.remove(&r.id);
            if let Some(n) = merge(code, Some(r)) {
                nodes.insert(n.id.clone(), n);
            }
        }
        let held: BTreeSet<String> = nodes.keys().cloned().collect();
        nodes.retain(|_, n| !self.withholds(n));
        let mut edges = Vec::new();
        for e in code.edges.into_iter().chain(records.edges) {
            let e = self.canon_edge(e);
            if e.rel == REL_GOVERNS {
                for c in self.concepts_of.get(&e.to).into_iter().flatten() {
                    edges.push(attachment(&e, c));
                }
            }
            edges.push(e);
        }
        let answered = |id: &str| {
            nodes.contains_key(id) || (!held.contains(id) && !self.withholds(&artifact(id)))
        };
        edges.retain(|e| answered(&e.from) && answered(&e.to));
        Ok(closed(nodes, edges))
    }
}

impl ContextGraph for Composed<'_> {
    fn subgraph(&self, seed: &[String], depth: i64) -> Result<Graph, Error> {
        subgraph(self, seed, depth)
    }
    fn calls(
        &self,
        seed: &[String],
        direction: Direction,
        depth: i64,
        tier_floor: &str,
    ) -> Result<CallGraph, Error> {
        calls(self, seed, direction, depth, tier_floor)
    }
    fn resolve(&self, mention: &str) -> Result<Option<String>, Error> {
        match self.canonical.get(mention) {
            Some(id) => Ok(Some(id.clone())),
            None => Ok(self.node(mention)?.map(|n| n.id)),
        }
    }
    fn locate(&self, entity: &str) -> Result<Located, Error> {
        locate(self, entity)
    }
    fn nodes_of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        self.of_kind(kind)
    }
    fn whole(&self) -> Result<Graph, Error> {
        Neighbors::whole(self)
    }
    fn stamp_label(&self) -> Label {
        Label {
            withheld: self.withheld.lock().unwrap().len(),
            ..self.label.clone()
        }
    }
}

/// The connected subgraph of `graph` within `depth` hops of any seed (a seed no half holds is
/// still stepped from): each hop a live edge either way, a reached file's members or a reached
/// member's file (THE MEMBER RULE). It holds the reached nodes and the live edges among the
/// reached ids, an edge end no half holds reading as an `artifact`.
pub fn subgraph(graph: &dyn Neighbors, seed: &[String], depth: i64) -> Result<Graph, Error> {
    let mut reached: BTreeSet<String> = seed.iter().cloned().collect();
    let mut queue: VecDeque<(String, i64)> = reached.iter().map(|s| (s.clone(), 0)).collect();
    let mut nodes = BTreeMap::new();
    let mut edges = Vec::new();
    while let Some((id, hops)) = queue.pop_front() {
        let node = graph.node(&id)?;
        let out = graph.out(&id)?;
        if hops < depth {
            let mut next: Vec<String> = out.iter().map(|e| e.to.clone()).collect();
            next.extend(graph.incoming(&id)?.into_iter().map(|e| e.from));
            next.extend(graph.members(&id)?.into_iter().map(|m| m.id));
            next.extend(node.as_ref().and_then(member_file).map(str::to_string));
            for n in next {
                if reached.insert(n.clone()) {
                    queue.push_back((n, hops + 1));
                }
            }
        }
        edges.extend(out);
        if let Some(node) = node {
            nodes.insert(id, node);
        }
    }
    edges.retain(|e| reached.contains(&e.from) && reached.contains(&e.to));
    Ok(closed(nodes, edges))
}

/// The confidence rank of a tier (spec 52): `extracted` 2, `inferred` 1, `ambiguous` 0, any other
/// value `unrecognized`. A directed walk follows an edge only when its rank is at or above the
/// floor's.
fn tier_rank(tier: &str, unrecognized: u8) -> u8 {
    match tier {
        TIER_EXTRACTED => 2,
        TIER_INFERRED => 1,
        TIER_AMBIGUOUS => 0,
        _ => unrecognized,
    }
}

/// [`tier_rank`]'s fallback for an edge's tier: the lowest.
const UNRECOGNIZED_EDGE_RANK: u8 = 0;

/// [`tier_rank`]'s fallback for a requested floor: the resolvable floor, `inferred`, so a walk
/// excludes `ambiguous` unless asked for it.
const UNRECOGNIZED_FLOOR_RANK: u8 = 1;

/// Which named nodes of a leaf [`definitions`] admits.
#[derive(Clone, Copy)]
enum Admitted {
    /// Every named node, a data member and a declaration included: what a lookup by name shows.
    Named,
    /// The call targets alone, by the node's `kind` attr: what a walk resolves a call onto.
    CallTargets,
}

/// The code-entity definitions of `leaf` (named `leaf`) that `admitted` admits, in `file`'s
/// `<file>::` id range when one is given, sorted by id.
fn definitions(
    graph: &dyn Neighbors,
    leaf: &str,
    admitted: Admitted,
    file: Option<&str>,
) -> Result<Vec<String>, Error> {
    let mut ids: Vec<String> = graph
        .leaf_nodes(leaf)?
        .into_iter()
        .filter(|n| n.kind == KIND_CODE_ENTITY && node_name(n) == Some(leaf))
        .filter(|n| match admitted {
            Admitted::Named => true,
            Admitted::CallTargets => is_call_target(n.attrs.get("kind").map(String::as_str)),
        })
        .filter(|n| file.is_none_or(|f| n.id.starts_with(&entity_id(f, ""))))
        .map(|n| n.id)
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Whether the named node `id` is a call target: `Some(true)` for a definition a call resolves
/// onto, `Some(false)` for a named data member or declaration, `None` for a node with no name or
/// no node.
fn named_call_target(graph: &dyn Neighbors, id: &str) -> Result<Option<bool>, Error> {
    Ok(graph.node(id)?.and_then(|n| {
        node_name(&n).map(|_| is_call_target(n.attrs.get("kind").map(String::as_str)))
    }))
}

/// One resolved `CALLS` hop of a layered walk: the neighbor reached, the edge it was reached by
/// (its tier, valid time and source), and the sorted candidate definitions when the hop is a
/// frontier the walk does not expand.
struct Hop {
    next: String,
    via: Edge,
    frontier: Option<Vec<String>>,
}

/// One DOWN hop's callee: a named call target lands on itself; any other target resolves by the
/// leaf its id names over the call-target definitions, one auto-continuing, more a frontier,
/// none leaving it where it is.
fn resolve_down_hop(
    graph: &dyn Neighbors,
    raw_to: &str,
) -> Result<(String, Option<Vec<String>>), Error> {
    if named_call_target(graph, raw_to)? == Some(true) {
        return Ok((raw_to.to_string(), None));
    }
    let cands = match leaf_of(raw_to) {
        Some(leaf) => definitions(graph, leaf, Admitted::CallTargets, None)?,
        None => Vec::new(),
    };
    Ok(match cands.len() {
        1 => (cands.into_iter().next().unwrap(), None),
        0 => (raw_to.to_string(), None),
        _ => (raw_to.to_string(), Some(cands)),
    })
}

/// The `CALLS` edges of `edges`, sorted by the end `end` names.
fn calls_at(edges: Vec<Edge>, end: impl Fn(&Edge) -> &String) -> Vec<Edge> {
    let mut calls: Vec<Edge> = edges.into_iter().filter(|e| e.rel == REL_CALLS).collect();
    calls.sort_by(|a, b| end(a).cmp(end(b)));
    calls
}

/// The callers of `cur` for one UP hop: none for a named non-target; else the direct callers
/// whose edge targets `cur`, and, for a definition, the callers through a placeholder bound to
/// its leaf, a frontier when the leaf has more than one call-target definition.
fn callers_of(graph: &dyn Neighbors, cur: &str) -> Result<Vec<Hop>, Error> {
    let definition = match named_call_target(graph, cur)? {
        Some(false) => return Ok(Vec::new()),
        target => target.is_some(),
    };
    let mut hops: Vec<Hop> = calls_at(graph.incoming(cur)?, |e| &e.from)
        .into_iter()
        .map(|via| Hop {
            next: via.from.clone(),
            via,
            frontier: None,
        })
        .collect();
    let node = if definition { graph.node(cur)? } else { None };
    if let Some(leaf) = node.as_ref().and_then(node_leaf) {
        let cands = definitions(graph, leaf, Admitted::CallTargets, None)?;
        let frontier = (cands.len() > 1).then_some(cands);
        let mut via_bare = Vec::new();
        for t in graph.leaf_nodes(leaf)? {
            let target = is_call_target(t.attrs.get("kind").map(String::as_str));
            if id_binds_leaf(&t.id, leaf) && (node_name(&t).is_none() || !target) {
                via_bare.extend(graph.incoming(&t.id)?);
            }
        }
        for via in calls_at(via_bare, |e| &e.from) {
            hops.push(Hop {
                next: via.from.clone(),
                via,
                frontier: frontier.clone(),
            });
        }
    }
    Ok(hops)
}

/// The file of an edge's source: the text before its first `::`, or the source itself when that
/// is empty or absent.
fn source_file(from: &str) -> &str {
    match split_id(from) {
        (file, Some(_)) if !file.is_empty() => file,
        _ => from,
    }
}

/// The file nodes that reference a seed's leaf (a live `REFERENCES` edge from the file or a
/// definition in it, to a node bound to the leaf) but call it from nowhere in them, sorted.
fn referenced_not_called(graph: &dyn Neighbors, seed: &[String]) -> Result<Vec<Node>, Error> {
    let mut leaves = BTreeSet::new();
    for s in seed {
        if let Some(n) = graph.node(s)? {
            leaves.extend(node_leaf(&n).map(str::to_string));
        }
    }
    let mut referencing = BTreeSet::new();
    let mut calling = BTreeSet::new();
    for leaf in &leaves {
        for t in graph.leaf_nodes(leaf)? {
            for e in graph.incoming(&t.id)? {
                let files = match e.rel.as_str() {
                    REL_REFERENCES => &mut referencing,
                    REL_CALLS => &mut calling,
                    _ => continue,
                };
                files.insert(source_file(&e.from).to_string());
            }
        }
    }
    let mut nodes = Vec::new();
    for f in referencing.difference(&calling) {
        nodes.extend(graph.node(f)?);
    }
    Ok(nodes)
}

/// The layered `CALLS` walk both directions share: breadth first by layer from the seeds `graph`
/// holds, each expanded node's [`Hop`]s from `hops`, bounded by `depth`. `orient` turns
/// (expanded node, neighbor) into the edge's real ends. A frontier is marked and not expanded; an
/// edge to a neighbor no deeper than its source is a back edge. Nodes come back by (layer, id),
/// edges by ends.
fn layered_call_walk(
    graph: &dyn Neighbors,
    seed: &[String],
    depth: i64,
    hops: impl Fn(&str) -> Result<Vec<Hop>, Error>,
    orient: impl Fn(&str, &str) -> (String, String),
) -> Result<(Vec<CallNode>, Vec<CallEdge>), Error> {
    let mut layer_of: BTreeMap<String, i64> = BTreeMap::new();
    let mut frontier_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut call_edges = Vec::new();
    let mut queue = VecDeque::new();
    for s in seed.iter().collect::<BTreeSet<_>>() {
        if graph.node(s)?.is_some() {
            layer_of.insert(s.clone(), 0);
            queue.push_back((s.clone(), 0));
        }
    }
    while let Some((cur, cur_layer)) = queue.pop_front() {
        if cur_layer >= depth {
            continue;
        }
        for Hop {
            next,
            via,
            frontier,
        } in hops(&cur)?
        {
            let next_layer = match layer_of.get(&next) {
                Some(layer) => *layer,
                None => {
                    layer_of.insert(next.clone(), cur_layer + 1);
                    match frontier {
                        Some(cands) => {
                            frontier_of.insert(next.clone(), cands);
                        }
                        None => queue.push_back((next.clone(), cur_layer + 1)),
                    }
                    cur_layer + 1
                }
            };
            let (from, to) = orient(&cur, &next);
            call_edges.push(CallEdge {
                edge: Edge {
                    from,
                    to,
                    rel: REL_CALLS.to_string(),
                    valid_from: via.valid_from,
                    valid_to: None,
                    source: via.source,
                    tier: via.tier,
                },
                back: next_layer <= cur_layer,
            });
        }
    }
    let mut nodes = Vec::with_capacity(layer_of.len());
    for (id, layer) in &layer_of {
        if let Some(node) = graph.node(id)? {
            nodes.push(CallNode {
                node,
                layer: *layer,
                frontier: frontier_of.get(id).cloned(),
            });
        }
    }
    nodes.sort_by(|a, b| (a.layer, &a.node.id).cmp(&(b.layer, &b.node.id)));
    call_edges.sort_by(|a, b| {
        (&a.edge.from, &a.edge.to, &a.edge.rel).cmp(&(&b.edge.from, &b.edge.to, &b.edge.rel))
    });
    Ok((nodes, call_edges))
}

/// The directed `CALLS` walk of [`ContextGraph::calls`] over `graph`: callees (`Down`, a hop to a
/// placeholder resolved by its leaf) or callers (`Up`, through the placeholders bound to each
/// definition's leaf, with the referenced-but-not-called files beside them), each edge followed
/// only at or above `tier_floor`.
pub fn calls(
    graph: &dyn Neighbors,
    seed: &[String],
    direction: Direction,
    depth: i64,
    tier_floor: &str,
) -> Result<CallGraph, Error> {
    let floor = tier_rank(tier_floor, UNRECOGNIZED_FLOOR_RANK);
    let followed = |e: &Edge| tier_rank(&e.tier, UNRECOGNIZED_EDGE_RANK) >= floor;
    match direction {
        Direction::Down => {
            let hops = |cur: &str| {
                let mut hops = Vec::new();
                for via in calls_at(graph.out(cur)?, |e| &e.to) {
                    if followed(&via) {
                        let (next, frontier) = resolve_down_hop(graph, &via.to)?;
                        hops.push(Hop {
                            next,
                            via,
                            frontier,
                        });
                    }
                }
                Ok(hops)
            };
            let orient = |cur: &str, next: &str| (cur.to_string(), next.to_string());
            let (nodes, edges) = layered_call_walk(graph, seed, depth, hops, orient)?;
            Ok(CallGraph {
                nodes,
                edges,
                referenced_not_called: Vec::new(),
            })
        }
        Direction::Up => {
            let hops = |cur: &str| {
                let mut hops = callers_of(graph, cur)?;
                hops.retain(|h| followed(&h.via));
                Ok(hops)
            };
            let orient = |cur: &str, next: &str| (next.to_string(), cur.to_string());
            let (nodes, edges) = layered_call_walk(graph, seed, depth, hops, orient)?;
            Ok(CallGraph {
                nodes,
                edges,
                referenced_not_called: referenced_not_called(graph, seed)?,
            })
        }
    }
}

/// The site of `node`: [`entity_site`] at its live one-hop degree, with its unresolved leaves.
fn site_of(graph: &dyn Neighbors, node: Node) -> Result<EntitySite, Error> {
    let degree = graph.out(&node.id)?.len()
        + graph
            .incoming(&node.id)?
            .iter()
            .filter(|e| e.from != node.id)
            .count();
    let mut site = entity_site(&node, degree);
    site.unresolved = graph.unresolved(&node)?;
    Ok(site)
}

/// Whether `node` is a knowledge-held flat id's bare node: a nameless `code-entity` or `artifact`
/// whose id holds `::`, answered by its exact id only when no definition answers.
fn is_bare_flat_node(node: &Node) -> bool {
    node_name(node).is_none()
        && [KIND_CODE_ENTITY, KIND_ARTIFACT].contains(&node.kind.as_str())
        && split_id(&node.id).1.is_some()
}

/// The definitions a show query names when no exact id answers it, sorted: the first non-empty of
/// the query as a leaf, its path leaf filtered on the id suffix `::<query>`, and its rest half as
/// a leaf in its file half.
fn shown_candidates(graph: &dyn Neighbors, query: &str) -> Result<Vec<String>, Error> {
    let by_leaf = definitions(graph, query, Admitted::Named, None)?;
    if !by_leaf.is_empty() {
        return Ok(by_leaf);
    }
    let suffix = entity_id("", query);
    let by_path: Vec<String> = definitions(graph, path_leaf(query), Admitted::Named, None)?
        .into_iter()
        .filter(|id| id.ends_with(&suffix))
        .collect();
    if !by_path.is_empty() {
        return Ok(by_path);
    }
    let (file, rest) = split_id(query);
    definitions(graph, rest.unwrap_or_default(), Admitted::Named, Some(file))
}

/// [`ContextGraph::locate`] over `graph`: an exact id (a bare flat node's only once no definition
/// answers), else the definitions [`shown_candidates`] names, one a site and more the candidates.
pub fn locate(graph: &dyn Neighbors, entity: &str) -> Result<Located, Error> {
    let (exact, deferred) = match graph.node(entity)? {
        Some(node) if is_bare_flat_node(&node) => (None, Some(node)),
        found => (found, None),
    };
    if let Some(node) = exact {
        return Ok(Located::One(site_of(graph, node)?));
    }
    match shown_candidates(graph, entity)?.as_slice() {
        [] => match deferred {
            Some(node) => Ok(Located::One(site_of(graph, node)?)),
            None => Ok(Located::None),
        },
        [only] => {
            let node = graph.node(only)?.ok_or_else(|| {
                Error(format!("locate: candidate {only:?} vanished between reads"))
            })?;
            Ok(Located::One(site_of(graph, node)?))
        }
        many => Ok(Located::Many(
            many.iter()
                .map(|id| Candidate {
                    file: node_file(KIND_CODE_ENTITY, id)
                        .unwrap_or_default()
                        .to_string(),
                    id: id.clone(),
                })
                .collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_facts::{
        Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, Stamp, Tip,
    };
    use crate::code_view::{Cluster, DefRef, FactsView, FileRef, Groupings, MentionRef};
    use crate::contextgraph::{
        KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_CONCEPT, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE,
        KIND_FINDING, REL_ABOUT, REL_GOVERNS, REL_REALIZES, REL_SPECIFIES, TIER_EXTRACTED,
    };
    use std::borrow::Cow;

    /// A view with the groupings a store computes beside its facts.
    struct Grouped {
        facts: FactsView,
        groupings: Groupings,
    }

    impl CodeView for Grouped {
        fn stamp(&self) -> &Stamp {
            self.facts.stamp()
        }
        fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
            self.facts.files()
        }
        fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
            self.facts.file(path)
        }
        fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
            self.facts.defs_named(leaf)
        }
        fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
            self.facts.mentions_named(leaf)
        }
        fn groupings(&self) -> &Groupings {
            &self.groupings
        }
    }

    const CONCEPT: &str = "concept/1/0";

    fn def(local: &str) -> Def {
        Def {
            local: local.into(),
            leaf: local.into(),
            kind: "function".into(),
            line: 1,
            end_line: 2,
            parent: None,
            test: false,
        }
    }

    fn rust(path: &str, defs: Vec<Def>, mentions: Vec<Mention>) -> FileFacts {
        FileFacts {
            path: path.into(),
            lang: "rust".into(),
            half: Half::Code,
            defs,
            mentions,
            ..Default::default()
        }
    }

    /// `src/a.rs::run` calls `compute`, which `src/b.rs` defines; `docs/d.md` specifies the alias
    /// `the-compute`; one concept groups `docs/d.md`.
    fn view(commit: Option<&str>) -> Grouped {
        let a = rust(
            "src/a.rs",
            vec![def("run")],
            vec![Mention {
                enclosing: Some(0),
                name: "compute".into(),
                leaf: "compute".into(),
                kind: MentionKind::Call,
                line: 1,
                test: false,
            }],
        );
        let b = rust("src/b.rs", vec![def("compute")], Vec::new());
        let doc = FileFacts {
            path: "docs/d.md".into(),
            lang: "markdown".into(),
            half: Half::Design,
            doc_concepts: vec![DocConcept {
                kind: KIND_DESIGN_DOC.into(),
                id: "docs/d.md".into(),
                title: "D".into(),
            }],
            doc_links: vec![DocLink {
                from: "docs/d.md".into(),
                rel: REL_SPECIFIES.into(),
                to: "the-compute".into(),
            }],
            ..Default::default()
        };
        let stamp = Stamp {
            tip: Tip {
                head: "main".into(),
                commit: commit.map(str::to_string),
                tree: "tree0".into(),
            },
            fingerprint: 0,
            seq: 1,
        };
        Grouped {
            facts: FactsView::new(stamp, vec![a, b, doc]),
            groupings: Groupings {
                communities: Vec::new(),
                concepts: vec![Cluster {
                    id: CONCEPT.into(),
                    label: "design".into(),
                    resolution: 1.0,
                    hash: "h".into(),
                    members: vec!["docs/d.md".into()],
                }],
            },
        }
    }

    fn node(id: &str, kind: &str, attrs: &[(&str, &str)]) -> Node {
        Node {
            id: id.into(),
            kind: kind.into(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn edge(from: &str, to: &str, rel: &str) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            rel: rel.into(),
            valid_from: 7,
            valid_to: None,
            source: 3,
            tier: TIER_EXTRACTED.into(),
        }
    }

    /// The record half: a finding about `src/b.rs::compute` and one about a file the tree no
    /// longer holds, a decision governing `docs/d.md`, the record rows of a tree file and a tree
    /// entity, and the alias `the-compute`.
    fn records() -> Indexed {
        let graph = Graph {
            nodes: vec![
                node("f1", KIND_FINDING, &[("summary", "slow")]),
                node("f2", KIND_FINDING, &[]),
                node("dec-1", KIND_DECISION, &[("title", "keep it")]),
                node("src/b.rs", KIND_ARTIFACT, &[("noted", "yes")]),
                node("src/b.rs::compute", KIND_ARTIFACT, &[("by", "lens")]),
            ],
            edges: vec![
                edge("f1", "src/b.rs::compute", REL_ABOUT),
                edge("f2", "src/gone.rs", REL_ABOUT),
                edge("dec-1", "docs/d.md", REL_GOVERNS),
            ],
        };
        Indexed::new(
            graph,
            vec![("the-compute".into(), "src/b.rs::compute".into())],
        )
    }

    fn honest(diff: Result<WorktreeDiff, String>) -> Honesty {
        Honesty {
            worktree: "W".into(),
            diff,
            behind: None,
        }
    }

    fn composed<'r>(records: &'r Indexed, view: &Grouped, honesty: Honesty) -> Composed<'r> {
        Composed::new(records, view, ViewFilter::default(), honesty).unwrap()
    }

    fn exact() -> Honesty {
        honest(Ok(WorktreeDiff::default()))
    }

    fn ids(nodes: &[Node]) -> Vec<&str> {
        nodes.iter().map(|n| n.id.as_str()).collect()
    }

    fn has_edge(edges: &[Edge], from: &str, to: &str, rel: &str) -> bool {
        edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.rel == rel)
    }

    #[test]
    fn composed_merges_one_id_into_one_node_with_the_tree_kind() {
        let (records, view) = (records(), view(Some("c1")));
        let g = composed(&records, &view, exact());
        let whole = ContextGraph::whole(&g).unwrap();
        // (id, the tree's kind, a tree attr, a record attr)
        let cases = [
            ("src/b.rs", KIND_FILE, "lang", "noted"),
            ("src/b.rs::compute", KIND_CODE_ENTITY, "name", "by"),
        ];
        for (id, kind, tree_attr, record_attr) in cases {
            let merged = g.node(id).unwrap().expect(id);
            assert_eq!(merged.kind, kind, "{id}");
            assert!(merged.attrs.contains_key(tree_attr), "{id}: {merged:?}");
            assert!(merged.attrs.contains_key(record_attr), "{id}: {merged:?}");
            let held: Vec<&Node> = whole.nodes.iter().filter(|n| n.id == id).collect();
            assert_eq!(held.len(), 1, "{id} is one node in the whole graph");
            assert_eq!(
                (&held[0].kind, &held[0].attrs),
                (&merged.kind, &merged.attrs),
                "{id}"
            );
        }
    }

    #[test]
    fn composed_synthesizes_a_missing_record_endpoint_as_an_artifact() {
        let (records, view) = (records(), view(Some("c1")));
        let g = composed(&records, &view, exact());
        let reads: [(&str, Graph); 2] = [
            ("subgraph", g.subgraph(&["f2".to_string()], 1).unwrap()),
            ("whole", ContextGraph::whole(&g).unwrap()),
        ];
        for (read, graph) in reads {
            let gone: Vec<&Node> = graph
                .nodes
                .iter()
                .filter(|n| n.id == "src/gone.rs")
                .collect();
            assert_eq!(gone.len(), 1, "{read}: {:?}", ids(&graph.nodes));
            assert_eq!(gone[0].kind, KIND_ARTIFACT, "{read}");
            assert!(
                has_edge(&graph.edges, "f2", "src/gone.rs", REL_ABOUT),
                "{read}"
            );
        }
    }

    #[test]
    fn composed_applies_aliases_at_read() {
        let (records, view) = (records(), view(Some("c1")));
        let g = composed(&records, &view, exact());
        let whole = ContextGraph::whole(&g).unwrap();
        let out = g.out("docs/d.md").unwrap();
        let into = g.incoming("src/b.rs::compute").unwrap();
        let link =
            |edges: &[Edge]| has_edge(edges, "docs/d.md", "src/b.rs::compute", REL_SPECIFIES);
        // (read, whether it holds the link resolved to the canonical id)
        let cases = [
            ("out", link(&out)),
            ("incoming", link(&into)),
            ("whole", link(&whole.edges)),
            ("no alias node", !ids(&whole.nodes).contains(&"the-compute")),
            (
                "no alias edge",
                !whole.edges.iter().any(|e| e.to == "the-compute"),
            ),
            (
                "resolve",
                g.resolve("the-compute").unwrap().as_deref() == Some("src/b.rs::compute"),
            ),
        ];
        for (read, holds) in cases {
            assert!(holds, "{read}");
        }
    }

    #[test]
    fn a_decision_attaches_to_the_concept_of_the_doc_it_governs() {
        let (records, view) = (records(), view(Some("c1")));
        let g = composed(&records, &view, exact());
        let concept = g.node(CONCEPT).unwrap().expect("the concept node");
        assert_eq!(concept.kind, KIND_CONCEPT);
        let into_concept = g.incoming(CONCEPT).unwrap();
        assert!(has_edge(&into_concept, "docs/d.md", CONCEPT, REL_REALIZES));
        let reads: [(&str, Vec<Edge>); 4] = [
            ("out", g.out("dec-1").unwrap()),
            ("incoming", into_concept),
            ("whole", ContextGraph::whole(&g).unwrap().edges),
            (
                "subgraph",
                g.subgraph(&["dec-1".to_string()], 1).unwrap().edges,
            ),
        ];
        for (read, edges) in reads {
            assert!(
                has_edge(&edges, "dec-1", CONCEPT, REL_REALIZES),
                "{read}: {edges:?}"
            );
        }
    }

    #[test]
    fn subgraph_is_one_bfs_over_both_halves() {
        let (records, view) = (records(), view(Some("c1")));
        let g = composed(&records, &view, exact());
        // (seed, depth, ids reached, ids not reached)
        let cases: [(&str, i64, &[&str], &[&str]); 3] = [
            // A record edge, then the code half's file of the entity it reached.
            ("f1", 2, &["f1", "src/b.rs::compute", "src/b.rs"], &[]),
            // A code edge, then a record edge into the entity it reached.
            ("src/b.rs", 2, &["src/b.rs", "src/b.rs::compute", "f1"], &[]),
            // The depth bounds the walk across the halves alike.
            ("f1", 1, &["f1", "src/b.rs::compute"], &["src/b.rs"]),
        ];
        for (seed, depth, reached, unreached) in cases {
            let sub = g.subgraph(&[seed.to_string()], depth).unwrap();
            let got = ids(&sub.nodes);
            for id in reached {
                assert!(got.contains(id), "{seed}@{depth} reaches {id}: {got:?}");
            }
            for id in unreached {
                assert!(
                    !got.contains(id),
                    "{seed}@{depth} stops before {id}: {got:?}"
                );
            }
            for e in &sub.edges {
                assert!(got.contains(&e.from.as_str()) && got.contains(&e.to.as_str()));
            }
        }
    }

    #[test]
    fn the_honesty_label_names_the_stamp_and_counts_differing_files() {
        let differing = WorktreeDiff {
            changed: vec!["src/a.rs".into()],
            untracked: vec!["src/new.rs".into()],
        };
        // (stamp commit, diff, behind, label)
        let cases = [
            (
                Some("c1"),
                Ok(WorktreeDiff::default()),
                None,
                "describes c1",
            ),
            (None, Ok(WorktreeDiff::default()), None, "describes tree0"),
            (
                Some("c1"),
                Ok(differing),
                None,
                "describes c1; 2 files in W differ",
            ),
            (
                Some("c1"),
                Ok(WorktreeDiff {
                    changed: vec![".rigger/graph.db".into()],
                    untracked: vec![".rigger/workflow.yml".into()],
                }),
                None,
                "describes c1; 1 file in W differs",
            ),
            (
                Some("c1"),
                Err("no such commit".to_string()),
                None,
                "describes c1; W cannot be compared",
            ),
            (
                Some("c1"),
                Ok(WorktreeDiff::default()),
                Some("advance in flight"),
                "describes c1; advance in flight",
            ),
        ];
        let records = records();
        for (commit, diff, behind, want) in cases {
            let view = view(commit);
            let honesty = Honesty {
                worktree: "W".into(),
                diff,
                behind: behind.map(str::to_string),
            };
            let g = composed(&records, &view, honesty);
            assert_eq!(g.stamp_label().to_string(), want);
        }
    }

    #[test]
    fn hits_in_differing_files_are_withheld_and_counted() {
        let (records, view) = (records(), view(Some("c1")));
        let diff = WorktreeDiff {
            changed: vec!["src/b.rs".into()],
            untracked: Vec::new(),
        };
        let g = composed(&records, &view, honest(Ok(diff)));
        let whole = ContextGraph::whole(&g).unwrap();
        let sub = g.subgraph(&["f1".to_string()], 2).unwrap();
        let answered =
            |nodes: Vec<Node>| -> Vec<String> { nodes.into_iter().map(|n| n.id).collect() };
        // (read, the ids it answers)
        let reads: [(&str, Vec<String>); 4] = [
            (
                "node",
                answered(g.node("src/b.rs::compute").unwrap().into_iter().collect()),
            ),
            ("whole", answered(whole.nodes.clone())),
            ("subgraph", answered(sub.nodes.clone())),
            (
                "of_kind",
                answered(g.nodes_of_kind(KIND_CODE_ENTITY).unwrap()),
            ),
        ];
        for (read, ids) in reads {
            assert!(
                !ids.iter().any(|id| id.starts_with("src/b.rs")),
                "{read} withholds src/b.rs: {ids:?}"
            );
        }
        for edges in [&whole.edges, &sub.edges] {
            assert!(!edges
                .iter()
                .any(|e| e.from.starts_with("src/b.rs") || e.to.starts_with("src/b.rs")));
        }
        assert!(
            ids(&whole.nodes).contains(&"src/a.rs::run"),
            "an unchanged file still answers"
        );
        let label = g.stamp_label();
        assert_eq!(label.withheld, 2, "the file and its entity: {label:?}");
        assert_eq!(
            label.to_string(),
            "describes c1; 1 file in W differs; 2 withheld"
        );
    }
}
