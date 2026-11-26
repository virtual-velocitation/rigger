//! The composed view: the code half and the record half of the graph read as one context graph.
//!
//! The code half is the graph a code view resolves to, with the view's groupings joined onto it;
//! the record half is the graph the event log folds. An id both hold is one node of the tree's
//! kind carrying both halves' attrs, an alias reads as its canonical id, a record edge whose end
//! neither half holds reaches an `artifact` node, and a decision governing a document realizes
//! that document's concepts. The view describes the caller's worktree (its differing files
//! overlaid on the stamp, section 13), and the honesty label says so. When the worktree cannot be
//! compared, every code-half node is withheld and counted. The searches every reader asks
//! ([`subgraph`], [`calls`], [`locate`]) are written once over [`Neighbors`], so they step through
//! both halves alike.

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

use crate::code_facts::{Recorded, Tip};
use crate::code_graph::{CodeGraph, DOC_ATTR};
use crate::code_view::{CodeView, Compared, Extract, Groupings, SourceLines, Tests, ViewFilter};
use crate::community::DEFAULT_RESOLUTION;
use crate::contextgraph::query::{
    attrs_name, entity_id, kept_of, leaf_of, member_file, node_file, node_leaf, node_name,
    path_leaf, placeholder_cut, split_id,
};
use crate::contextgraph::{
    entity_site, holds_attachment, is_call_target, CallEdge, CallGraph, CallNode, Candidate,
    ContextGraph, Direction, Edge, EntitySite, Error, Graph, Located, Neighbors, Node, Referrer,
    Rows, KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_COMMUNITY, KIND_CONCEPT, KIND_FILE, REL_CALLS,
    REL_GOVERNS, REL_IN_COMMUNITY, REL_REALIZES, REL_REFERENCES, TIER_AMBIGUOUS, TIER_EXTRACTED,
    TIER_INFERRED,
};
use crate::groupings::computed_at;
use crate::resolve::{ProofCall, INTENT_KINDS};
use crate::test_view::{join_tests, TestAttachment};

/// The honesty label (spec 6.1) every answer travels with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// The commit the answers describe the worktree over (its stamp), or the tree id when the
    /// tree was read from a directory.
    pub describes: String,
    /// The caller's worktree, as the label names it.
    pub worktree: String,
    pub compared: Compared,
    /// Why the view is behind the tree it was opened for, when it is.
    pub behind: Option<String>,
    /// The hits of the answer withheld because the worktree cannot be compared, counted as nodes:
    /// a grounding's are the entities its withheld hits name.
    pub withheld: usize,
}

/// A whole read: the graph it answers, the part of the graph it withheld, and its label.
#[derive(Debug)]
pub struct WholeRead {
    /// The graph the read answers.
    pub graph: Graph,
    /// The nodes the read withheld and the edges it dropped for an end it withheld: empty when
    /// nothing is withheld, so a view of the answer can count the withheld hits it would have held.
    pub withheld: Graph,
    /// The test code entities the filter hides, less any withheld: what a reader counts as test
    /// hits it does not show ([`ContextGraph::hidden_tests`]).
    pub tests_hidden: usize,
    /// What the read describes.
    pub label: Label,
}

impl Label {
    /// The label of answers drawn from the view at `tip`, read for `worktree` as `compared`
    /// says, behind its tree for the reason `behind` gives when it is: the ONE label builder.
    pub fn describing(
        tip: &Tip,
        worktree: String,
        compared: Compared,
        behind: Option<String>,
    ) -> Label {
        Label {
            describes: tip.commit.clone().unwrap_or_else(|| tip.tree.clone()),
            worktree,
            compared,
            behind,
            withheld: 0,
        }
    }

    /// The label of answers drawn from no view because the store `reason` names could not be
    /// opened: nothing was described, and nothing could be compared.
    pub fn unopened(worktree: String, reason: &str) -> Label {
        Label {
            describes: "nothing".to_string(),
            worktree,
            compared: Compared::Uncomparable,
            behind: Some(format!("the code store cannot be opened: {reason}")),
            withheld: 0,
        }
    }
}

/// `describes <commit>` for a worktree that matches it; `describes <worktree> (<commit> + N
/// files); groupings describe <commit>` for one whose differing files the view overlays; else
/// `describes <commit>; <worktree> cannot be compared`. Then how many nodes were withheld and why
/// the view is behind, each only when there is one.
impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (w, stamp) = (&self.worktree, &self.describes);
        match self.compared {
            Compared::Overlaid(0) => write!(f, "describes {stamp}")?,
            Compared::Overlaid(n) => {
                let files = if n == 1 { "file" } else { "files" };
                write!(
                    f,
                    "describes {w} ({stamp} + {n} {files}); groupings describe {stamp}"
                )?
            }
            Compared::Uncomparable => write!(f, "describes {stamp}; {w} cannot be compared")?,
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
    /// What the view describes of the worktree.
    pub compared: Compared,
    /// Why the view is behind the tree it was opened for, when it is.
    pub behind: Option<String>,
}

/// Whether `node` is bound by `leaf`: its [`node_name`] is `leaf`, or its id is
/// ([`id_binds_leaf`]).
pub(crate) fn binds_leaf(node: &Node, leaf: &str) -> bool {
    node_name(node) == Some(leaf) || id_binds_leaf(&node.id, leaf)
}

/// Whether the text of `id` after its first `::` is `leaf`, or a name behind the
/// [`crate::contextgraph::PLACEHOLDER_SIGIL`] that is `leaf` ([`leaf_of`]) or whose leaf, the
/// text after its last `::` or `.`, is ([`placeholder_cut`]): a placeholder keyed by a qualified
/// name binds its leaf.
pub(crate) fn id_binds_leaf(id: &str, leaf: &str) -> bool {
    split_id(id).1 == Some(leaf)
        || leaf_of(id) == Some(leaf)
        || placeholder_cut(id).is_some_and(|(_, at)| at == leaf)
}

/// The edges sorted by ends then relation, one per (from, to, relation), the first kept.
pub(crate) fn ordered(mut edges: Vec<Edge>) -> Vec<Edge> {
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

/// A graph held in memory and indexed for [`Neighbors`]: the code half a view resolves to. Its
/// nodes are held once in id order and its live edges once in [`ordered`] order; the indexes
/// name them by place and are built by the first read that needs them.
#[derive(Default)]
pub struct Indexed {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// The place of each node given, in the order given.
    given: Vec<usize>,
    aliases: Vec<(String, String)>,
    index: OnceLock<Index>,
}

/// The rows an [`Indexed`] is held and indexed as ([`Indexed::half_rows`]): each index names
/// nodes or edges by place.
#[derive(Debug)]
pub struct HalfRows<'a> {
    /// The nodes, in id order, one per id.
    pub nodes: &'a [Node],
    /// The live edges, in [`ordered`] order.
    pub edges: &'a [Edge],
    /// Each id's incoming edges, in edge order.
    pub incoming: BTreeMap<&'a str, Vec<usize>>,
    /// Each file's members ([`member_file`]), in the order the nodes were given.
    pub members: &'a BTreeMap<String, Vec<usize>>,
    /// Each leaf's nodes, in node order: those whose name, id rest, placeholder or its path leaf
    /// is the leaf, so each is one the leaf binds ([`binds_leaf`]).
    pub leaves: &'a BTreeMap<String, Vec<usize>>,
}

/// The places [`Indexed`] reads its neighbours by.
#[derive(Default)]
struct Index {
    /// The places of the edges into each id, by id then edge order.
    incoming: Vec<usize>,
    members: BTreeMap<String, Vec<usize>>,
    leaves: BTreeMap<String, Vec<usize>>,
    kinds: BTreeMap<String, Vec<usize>>,
}

impl Indexed {
    /// `graph` held, its edges live, with the recorded `aliases`. Of nodes sharing an id the
    /// last is held.
    pub fn new(graph: Graph, aliases: Vec<(String, String)>) -> Self {
        let edges: Vec<Edge> = ordered(graph.edges)
            .into_iter()
            .filter(|e| e.valid_to.is_none())
            .collect();
        let mut order: Vec<usize> = (0..graph.nodes.len()).collect();
        order.sort_by(|&a, &b| graph.nodes[a].id.cmp(&graph.nodes[b].id));
        let mut given = vec![0; graph.nodes.len()];
        let mut kept: Vec<usize> = Vec::new();
        for run in order.chunk_by(|&a, &b| graph.nodes[a].id == graph.nodes[b].id) {
            run.iter().for_each(|&at| given[at] = kept.len());
            kept.extend(run.last());
        }
        let mut slots: Vec<Option<Node>> = graph.nodes.into_iter().map(Some).collect();
        Indexed {
            nodes: kept.into_iter().filter_map(|at| slots[at].take()).collect(),
            edges,
            given,
            aliases,
            index: OnceLock::new(),
        }
    }

    /// The code half `graph` resolves to whole, with `groupings` joined on under the tests
    /// `graph`'s filter shows: what a store settles ([`Self::half_rows`]) and a neighbourhood
    /// reads.
    pub fn resolved(graph: &CodeGraph<'_>, groupings: &Groupings) -> Indexed {
        let resolved = graph.resolved();
        let attached = attachments(graph.resolver().proof_calls(), groupings);
        let joined = joined(resolved.graph, groupings, &attached, graph.tests());
        Indexed::new(joined, Vec::new())
    }

    /// The rows this graph is held and indexed as, which a store settles and answers
    /// [`crate::code_view::SettledHalf`] from.
    pub fn half_rows(&self) -> HalfRows<'_> {
        let ix = self.index();
        let mut incoming: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for &at in &ix.incoming {
            incoming.entry(&self.edges[at].to).or_default().push(at);
        }
        HalfRows {
            nodes: &self.nodes,
            edges: &self.edges,
            incoming,
            members: &ix.members,
            leaves: &ix.leaves,
        }
    }

    /// The indexes, built on first read.
    fn index(&self) -> &Index {
        self.index.get_or_init(|| {
            let mut ix = Index {
                incoming: (0..self.edges.len()).collect(),
                ..Index::default()
            };
            ix.incoming
                .sort_by(|&a, &b| self.edges[a].to.cmp(&self.edges[b].to));
            for &at in &self.given {
                let n = &self.nodes[at];
                if let Some(file) = member_file(n) {
                    ix.members.entry(file.to_string()).or_default().push(at);
                }
                ix.kinds.entry(n.kind.clone()).or_default().push(at);
            }
            for (at, n) in self.nodes.iter().enumerate() {
                let placeholder = placeholder_cut(&n.id).map(|(_, leaf)| leaf);
                let leaves = [node_name(n), split_id(&n.id).1, leaf_of(&n.id), placeholder];
                for leaf in leaves.into_iter().flatten() {
                    let held = ix.leaves.entry(leaf.to_string()).or_default();
                    if held.last() != Some(&at) {
                        held.push(at);
                    }
                }
            }
            ix
        })
    }

    /// The nodes at `places`, in the order given.
    fn held<'a>(&self, places: impl IntoIterator<Item = &'a usize>) -> Vec<Node> {
        places
            .into_iter()
            .map(|&at| self.nodes[at].clone())
            .collect()
    }

    /// The place of the node `id`.
    fn place(&self, id: &str) -> Option<usize> {
        self.nodes.binary_search_by(|n| n.id.as_str().cmp(id)).ok()
    }

    /// Whether this graph holds a node `id`.
    pub(crate) fn holds(&self, id: &str) -> bool {
        self.place(id).is_some()
    }
}

impl Neighbors for Indexed {
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        Ok(self.place(id).map(|at| self.nodes[at].clone()))
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        let start = self.edges.partition_point(|e| e.from.as_str() < id);
        let end = start + self.edges[start..].partition_point(|e| e.from == id);
        Ok(self.edges[start..end].to_vec())
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        let incoming = &self.index().incoming;
        let to = |&at: &usize| self.edges[at].to.as_str();
        let start = incoming.partition_point(|at| to(at) < id);
        let end = start + incoming[start..].partition_point(|at| to(at) == id);
        Ok(incoming[start..end]
            .iter()
            .map(|&at| self.edges[at].clone())
            .collect())
    }
    fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
        Ok(self.held(self.index().members.get(file).into_iter().flatten()))
    }
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
        let nodes = self.held(self.index().leaves.get(leaf).into_iter().flatten());
        Ok(nodes.into_iter().filter(|n| binds_leaf(n, leaf)).collect())
    }
    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        Ok(self.held(self.index().kinds.get(kind).into_iter().flatten()))
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
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
        })
    }
}

/// `graph` with the view's `groupings` joined on: each community and concept a node carrying the
/// pass's resolution, hash and label, and an inferred `IN_COMMUNITY` or `REALIZES` edge from
/// each member `graph` holds as a kind that holds it ([`holds_attachment`]).
pub(crate) fn joined(
    mut graph: Graph,
    groupings: &Groupings,
    attachments: &[TestAttachment; 2],
    tests: Tests,
) -> Graph {
    let arms = [
        (KIND_COMMUNITY, REL_IN_COMMUNITY, &groupings.communities),
        (KIND_CONCEPT, REL_REALIZES, &groupings.concepts),
    ];
    // Each arm's membership edges, by the kinds `graph` holds before any grouping is joined on.
    let memberships = {
        let kinds: FxHashMap<&str, &str> = (graph.nodes.iter())
            .map(|n| (n.id.as_str(), n.kind.as_str()))
            .collect();
        arms.map(|(_, rel, clusters)| {
            let held = |m: &String| {
                kinds
                    .get(m.as_str())
                    .is_some_and(|k| holds_attachment(k, rel))
            };
            (clusters.iter())
                .flat_map(|c| c.members.iter().filter(|m| held(m)).map(|m| (m, &c.id)))
                .map(|(m, c)| Edge {
                    from: m.clone(),
                    to: c.clone(),
                    rel: rel.into(),
                    valid_from: 0,
                    valid_to: None,
                    source: 0,
                    tier: TIER_INFERRED.into(),
                })
                .collect::<Vec<Edge>>()
        })
    };
    for (((kind, _, clusters), attachment), edges) in
        arms.into_iter().zip(attachments).zip(memberships)
    {
        graph.nodes.extend(clusters.iter().map(|c| {
            let attrs = [
                ("hash", c.hash.clone()),
                ("label", c.label.clone()),
                ("resolution", crate::community::grain(c.resolution)),
            ];
            Node {
                id: c.id.clone(),
                kind: kind.to_string(),
                attrs: attrs.map(|(k, v)| (k.to_string(), v)).into(),
            }
        }));
        graph.edges.extend(edges);
        join_tests(&mut graph, attachment, tests);
    }
    graph
}

/// The tests `proofs` attach to `groupings`' communities, then to its concepts.
pub(crate) fn attachments(proofs: &[ProofCall], groupings: &Groupings) -> [TestAttachment; 2] {
    [&groupings.communities, &groupings.concepts].map(|c| TestAttachment::new(proofs, c))
}

/// The code half as [`Composed`] reads it: its neighbours, and what composing the halves reads
/// beside them. [`Whole`] resolves the view whole when it is opened; [`crate::near::Near`]
/// resolves only the ids it is asked about.
pub trait CodeHalf: Neighbors {
    /// Whether the half holds a node `id`.
    fn holds(&self, id: &str) -> bool;
    /// The concepts the view groups `member` into, in grouping order, when the half holds it.
    fn concepts_of(&self, member: &str) -> Vec<String> {
        self.concepts_of_within(member, None)
    }
    /// [`Self::concepts_of`], less any concept outside `within` when it is given: a concept a
    /// reader would drop is never read, nor is `member` asked whether the half holds it unless
    /// a concept is left.
    fn concepts_of_within(&self, member: &str, within: Option<&BTreeSet<String>>) -> Vec<String>;
    /// The members of `concept` the half holds, in grouping order.
    fn concept_members(&self, concept: &str) -> Vec<String>;
    /// The test entities the filter hides, in id order.
    fn hidden_tests(&self) -> Vec<Node>;
    /// How many of [`Self::hidden_tests`] `keep` keeps, counted without copying them.
    fn hidden_test_count(&self, keep: &dyn Fn(&Node) -> bool) -> usize {
        self.hidden_tests().iter().filter(|n| keep(n)).count()
    }
    /// Every distinct name a code entity of the half may carry, in order, perhaps with names
    /// none carries ([`ContextGraph::entity_names`]).
    fn entity_names(&self) -> Vec<String>;
    /// The test entities named `name` the filter hides, in id order.
    fn hidden_tests_named(&self, name: &str) -> Vec<Node>;
    /// The distinct test entities the filter hides that prove a node of `ids`, in id order,
    /// read from the proofs the half read for them ([`ContextGraph::hidden_provers`]).
    fn hidden_provers(&self, ids: &[String]) -> Vec<String>;
    /// The distinct test entities the filter hides whose calls to `leaf` land on a placeholder
    /// filed under `leaf` or under `id`, in order
    /// ([`crate::resolve::Resolver::placeholder_callers`]): the callers a callers walk from the
    /// definition `id` named `leaf` reaches through a placeholder bound to `leaf`.
    fn placeholder_callers(&self, id: &str, leaf: &str) -> Vec<String>;
    /// The candidate ids the down-hop of the placeholder `id` keeps, when it is keyed by a
    /// qualified name ([`CodeGraph::down_hop`]); none for any other id.
    fn down_hop(&self, id: &str) -> Option<Vec<String>>;
    /// Every referrer of the code entity `id`, test ones too, in path then mention order
    /// ([`crate::resolve::Resolver::referrers`]).
    fn referrers(&self, id: &str) -> Vec<Referrer>;
    /// The files the attributes of the grouping `id` summarise, by [`fact_file`]: its members'
    /// (its label and hash) and its attached tests' (its test count); none when `id` names no
    /// grouping.
    fn summarised_files(&self, id: &str) -> Vec<String>;
    /// The last line the facts of the code file `file` bound the `recorded` definition to
    /// ([`Recorded::widest_end`]); `None` when no such definition is there or the half keeps no
    /// facts of `file`, as a half resolved whole keeps none.
    fn definition_end(&self, _file: &str, _recorded: Recorded) -> Option<u32> {
        None
    }
    /// The language the facts of the code file `file` read it as, with the doc span they hold for
    /// the `recorded` definition ([`Recorded::widest`]), or for the file itself when none is given;
    /// `None` when no such definition is there, it or the file has no doc, or the half keeps no
    /// facts of `file`, as a half resolved whole keeps none.
    fn doc_span(&self, _file: &str, _recorded: Option<Recorded>) -> Option<(String, (u32, u32))> {
        None
    }
    /// The groupings of the view the half resolves.
    fn groupings(&self) -> &Groupings;
    /// The half's whole graph ([`Neighbors::whole`]), moved out of the half where it is held
    /// rather than copied: afterwards the half answers only [`Self::concepts_of`] and
    /// [`Self::groupings`].
    fn take_whole(&mut self) -> Result<Graph, Error> {
        self.whole()
    }
}

/// The file `site` names and the definition its node records there, when the node carries a
/// name: where [`ContextGraph::definition_end`] and [`ContextGraph::doc`] read a code file's facts.
fn recorded_at(site: &EntitySite) -> Option<(&str, Recorded<'_>)> {
    let name = attrs_name(&site.attrs)?;
    let (file, path) = split_id(&site.id);
    let kind = &site.kind;
    let recorded = Recorded {
        name,
        path,
        kind,
        line: site.line,
    };
    Some((file, recorded))
}

/// The distinct provers of the `(proved, prover)` pairs `proofs` that prove an id of `ids`, in
/// order: the one reading of [`CodeHalf::hidden_provers`].
pub(crate) fn provers_of<'p>(
    proofs: impl Iterator<Item = (&'p str, &'p str)>,
    ids: &[String],
) -> Vec<String> {
    let ids: FxHashSet<&str> = ids.iter().map(String::as_str).collect();
    let provers: BTreeSet<&str> = proofs
        .filter(|(proved, _)| ids.contains(proved))
        .map(|(_, prover)| prover)
        .collect();
    provers.into_iter().map(str::to_string).collect()
}

/// The file `node`'s facts came from, the one rule withholding reads: a design or workflow node's
/// document, else the file its id names ([`node_file`]).
pub(crate) fn fact_file(node: &Node) -> Option<&str> {
    let doc = INTENT_KINDS
        .contains(&node.kind.as_str())
        .then(|| node.attrs.get(DOC_ATTR))
        .flatten();
    doc.map(String::as_str)
        .or_else(|| node_file(&node.kind, &node.id))
}

/// The files the grouping `cluster` summarises: [`fact_file`] of each member `node` answers,
/// and the file of each of its `tests`, code entities all.
pub(crate) fn cluster_files<'m>(
    members: impl Iterator<Item = &'m String>,
    tests: impl Iterator<Item = &'m str>,
    node: impl Fn(&str) -> Option<Node>,
) -> Vec<String> {
    let mut files: Vec<String> = members
        .filter_map(|m| node(m).and_then(|n| fact_file(&n).map(str::to_string)))
        .chain(tests.filter_map(|t| node_file(KIND_CODE_ENTITY, t).map(str::to_string)))
        .collect();
    files.sort();
    files.dedup();
    files
}

/// The code half resolved whole: every node and edge the view resolves to, with its groupings
/// joined on.
pub struct Whole {
    code: Indexed,
    /// Each concept's members and each member's concepts, as the view grouped them.
    concept_members: BTreeMap<String, Vec<String>>,
    concepts_of: BTreeMap<String, Vec<String>>,
    hidden_tests: Vec<Node>,
    /// The view's stored groupings with those of the grain read beside them, and, per grain and
    /// arm, the tests attached to them.
    groupings: Groupings,
    attachments: Vec<TestAttachment>,
    /// The proof calls read to attach the tests, kept while the filter hides tests.
    proofs: Vec<ProofCall>,
    /// The callers of the test calls that land on a placeholder, in order, by the key each is
    /// filed under, a leaf or a candidate id, kept while the filter hides tests.
    placeholder_callers: BTreeMap<String, Vec<String>>,
    /// The candidates each placeholder keyed by a qualified name keeps ([`CodeGraph::down_hop`]),
    /// read when the half is walked.
    down_hops: BTreeMap<String, Vec<String>>,
    /// Every code entity's referrers, test ones too, by id ([`CodeGraph::referrers`]), read when
    /// the half is walked.
    referrers: BTreeMap<String, Vec<Referrer>>,
}

impl Whole {
    /// `view` resolved whole under `filter`, its stored groupings joined on.
    #[cfg(test)]
    pub(crate) fn new(view: &dyn CodeView, filter: ViewFilter) -> Self {
        Self::at(view, filter, DEFAULT_RESOLUTION)
    }

    /// `view` resolved whole under `filter`, its stored groupings joined on and, beside them, the
    /// groupings at `resolution` when that is not the stored grain: computed from this view as
    /// the grouping reports compute them ([`computed_at`]), and never stored.
    pub(crate) fn at(view: &dyn CodeView, filter: ViewFilter, resolution: f64) -> Self {
        Self::read(view, filter, resolution, true)
    }

    /// [`Self::at`], reading the test calls landing on a placeholder that a callers walk reaches
    /// ([`CodeGraph::placeholder_calls`]), the down-hops of its placeholders keyed by a qualified
    /// name ([`CodeGraph::down_hop`]) and its entities' referrers ([`CodeGraph::referrers`]) into
    /// the half only when `walked`: a read that is never walked holds none of them, though each
    /// placeholder node keyed by a qualified name carries its down-hop as its
    /// [`crate::contextgraph::query::CANDIDATES_ATTR`] either way, as [`CodeGraph`] writes it on
    /// every read.
    fn read(view: &dyn CodeView, filter: ViewFilter, resolution: f64, walked: bool) -> Self {
        let computed = computed_at(view, resolution);
        let grains: Vec<&Groupings> = std::iter::once(view.groupings())
            .chain(computed.as_ref())
            .collect();
        let code_graph = CodeGraph::new(view, filter);
        let resolved = code_graph.resolved();
        let mut graph = resolved.graph;
        let mut attached = Vec::new();
        for grain in &grains {
            let grain_attachments = attachments(code_graph.resolver().proof_calls(), grain);
            graph = joined(graph, grain, &grain_attachments, filter.tests);
            attached.extend(grain_attachments);
        }
        let code = Indexed::new(graph, Vec::new());
        let mut placeholder_callers: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let (placeholder_calls, down_hops, referrers) = if walked {
            let down_hops = (code.nodes.iter())
                .filter_map(|n| Some((n.id.clone(), code_graph.down_hop(&n.id)?)))
                .collect();
            let referrers = code_graph.referrers();
            (code_graph.placeholder_calls(), down_hops, referrers)
        } else {
            (Vec::new(), BTreeMap::new(), BTreeMap::new())
        };
        for (key, caller) in placeholder_calls {
            placeholder_callers.entry(key).or_default().push(caller);
        }
        for callers in placeholder_callers.values_mut() {
            callers.sort_unstable();
            callers.dedup();
        }
        let proofs = match filter.tests {
            Tests::Exclude => code_graph.into_proof_calls(),
            Tests::Include | Tests::Only => Vec::new(),
        };
        let mut groupings = Groupings::default();
        for grain in grains {
            groupings
                .communities
                .extend(grain.communities.iter().cloned());
            groupings.concepts.extend(grain.concepts.iter().cloned());
        }
        let mut concept_members: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut concepts_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in &groupings.concepts {
            for m in c.members.iter().filter(|m| code.holds(m)) {
                concept_members
                    .entry(c.id.clone())
                    .or_default()
                    .push(m.clone());
                concepts_of.entry(m.clone()).or_default().push(c.id.clone());
            }
        }
        Whole {
            code,
            concept_members,
            concepts_of,
            hidden_tests: resolved.hidden_tests,
            groupings,
            attachments: attached,
            proofs,
            placeholder_callers,
            down_hops,
            referrers,
        }
    }
}

impl Neighbors for Whole {
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        self.code.node(id)
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.code.out(id)
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.code.incoming(id)
    }
    fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
        self.code.members(file)
    }
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
        self.code.leaf_nodes(leaf)
    }
    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        self.code.of_kind(kind)
    }
    fn unresolved(&self, node: &Node) -> Result<Vec<String>, Error> {
        self.code.unresolved(node)
    }
    fn aliases(&self) -> Result<Vec<(String, String)>, Error> {
        self.code.aliases()
    }
    fn whole(&self) -> Result<Graph, Error> {
        self.code.whole()
    }
}

impl CodeHalf for Whole {
    fn holds(&self, id: &str) -> bool {
        self.code.holds(id)
    }
    fn concepts_of_within(&self, member: &str, within: Option<&BTreeSet<String>>) -> Vec<String> {
        let concepts = self.concepts_of.get(member).into_iter().flatten();
        (concepts.filter(|c| within.is_none_or(|ids| ids.contains(*c))))
            .cloned()
            .collect()
    }
    fn concept_members(&self, concept: &str) -> Vec<String> {
        self.concept_members
            .get(concept)
            .cloned()
            .unwrap_or_default()
    }
    fn hidden_tests(&self) -> Vec<Node> {
        self.hidden_tests.clone()
    }
    fn hidden_test_count(&self, keep: &dyn Fn(&Node) -> bool) -> usize {
        self.hidden_tests.iter().filter(|n| keep(n)).count()
    }
    fn entity_names(&self) -> Vec<String> {
        let shown = (self.code).held(
            self.code
                .index()
                .kinds
                .get(KIND_CODE_ENTITY)
                .into_iter()
                .flatten(),
        );
        let mut names: Vec<String> = (shown.iter().chain(&self.hidden_tests))
            .filter_map(node_name)
            .map(str::to_string)
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }
    fn hidden_tests_named(&self, name: &str) -> Vec<Node> {
        (self.hidden_tests.iter())
            .filter(|n| node_name(n) == Some(name))
            .cloned()
            .collect()
    }
    fn hidden_provers(&self, ids: &[String]) -> Vec<String> {
        let proofs = self.proofs.iter();
        provers_of(
            proofs.filter_map(|c| Some((c.target.as_str(), c.caller.as_deref()?))),
            ids,
        )
    }
    fn placeholder_callers(&self, id: &str, leaf: &str) -> Vec<String> {
        let filed = [leaf, id].map(|key| self.placeholder_callers.get(key));
        let mut callers: Vec<String> = filed.into_iter().flatten().flatten().cloned().collect();
        callers.sort_unstable();
        callers.dedup();
        callers
    }
    fn down_hop(&self, id: &str) -> Option<Vec<String>> {
        self.down_hops.get(id).cloned()
    }
    fn referrers(&self, id: &str) -> Vec<Referrer> {
        self.referrers.get(id).cloned().unwrap_or_default()
    }
    fn summarised_files(&self, id: &str) -> Vec<String> {
        let arms = [&self.groupings.communities, &self.groupings.concepts];
        let members = arms
            .into_iter()
            .flatten()
            .filter(|c| c.id == id)
            .flat_map(|c| &c.members);
        let tests = self.attachments.iter().flat_map(|a| a.tests_of(id));
        cluster_files(members, tests, |m| self.code.node(m).ok().flatten())
    }
    fn groupings(&self) -> &Groupings {
        &self.groupings
    }
    fn take_whole(&mut self) -> Result<Graph, Error> {
        let Indexed { nodes, edges, .. } = std::mem::take(&mut self.code);
        Ok(Graph { nodes, edges })
    }
}

/// A code half resolved whole that a [`WholeMemo`] may share with later reads: it answers what
/// the [`Whole`] it holds answers.
impl Neighbors for Arc<Whole> {
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        (**self).node(id)
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        (**self).out(id)
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        (**self).incoming(id)
    }
    fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
        (**self).members(file)
    }
    fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
        (**self).leaf_nodes(leaf)
    }
    fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        (**self).of_kind(kind)
    }
    fn unresolved(&self, node: &Node) -> Result<Vec<String>, Error> {
        (**self).unresolved(node)
    }
    fn aliases(&self) -> Result<Vec<(String, String)>, Error> {
        (**self).aliases()
    }
    fn whole(&self) -> Result<Graph, Error> {
        (**self).whole()
    }
}

impl CodeHalf for Arc<Whole> {
    fn holds(&self, id: &str) -> bool {
        (**self).holds(id)
    }
    fn concepts_of_within(&self, member: &str, within: Option<&BTreeSet<String>>) -> Vec<String> {
        (**self).concepts_of_within(member, within)
    }
    fn concept_members(&self, concept: &str) -> Vec<String> {
        (**self).concept_members(concept)
    }
    fn hidden_tests(&self) -> Vec<Node> {
        (**self).hidden_tests()
    }
    fn hidden_test_count(&self, keep: &dyn Fn(&Node) -> bool) -> usize {
        (**self).hidden_test_count(keep)
    }
    fn entity_names(&self) -> Vec<String> {
        (**self).entity_names()
    }
    fn hidden_tests_named(&self, name: &str) -> Vec<Node> {
        (**self).hidden_tests_named(name)
    }
    fn hidden_provers(&self, ids: &[String]) -> Vec<String> {
        (**self).hidden_provers(ids)
    }
    fn placeholder_callers(&self, id: &str, leaf: &str) -> Vec<String> {
        (**self).placeholder_callers(id, leaf)
    }
    fn down_hop(&self, id: &str) -> Option<Vec<String>> {
        (**self).down_hop(id)
    }
    fn referrers(&self, id: &str) -> Vec<Referrer> {
        (**self).referrers(id)
    }
    fn summarised_files(&self, id: &str) -> Vec<String> {
        (**self).summarised_files(id)
    }
    fn groupings(&self) -> &Groupings {
        (**self).groupings()
    }
    /// Moved out of the half when no memo shares it, else copied, so a held half keeps its graph.
    fn take_whole(&mut self) -> Result<Graph, Error> {
        match Arc::get_mut(self) {
            Some(whole) => whole.take_whole(),
            None => self.whole(),
        }
    }
}

/// The code halves a process resolved whole for its whole reads ([`Composed::whole_held_at`]),
/// each held by what names it: the view's stamp and the files it takes from the worktree
/// ([`CodeView::overlaid`]), with the read's filter and resolution. A read whose key one holds
/// takes it rather than resolving the view again; the record half is never held, so every read
/// merges the log as it stands. Every entry describes one tree: a read of another drops them all.
/// Of one tree it holds at most its bound, dropping the least recently read beyond that.
pub struct WholeMemo {
    bound: usize,
    held: Mutex<HeldWholes>,
}

/// What names a held whole: the tree, the worktree files over it, the filter and the grain.
#[derive(Clone, PartialEq)]
struct WholeKey {
    stamp: crate::code_facts::Stamp,
    overlaid: Vec<(String, Option<String>)>,
    tests: Tests,
    /// The resolution's bits, so a grain is held by exactly the value read.
    resolution: u64,
}

impl WholeKey {
    /// Whether `self` and `other` describe one tree: the same stamp and worktree files.
    fn of_tree(&self, other: &WholeKey) -> bool {
        self.stamp == other.stamp && self.overlaid == other.overlaid
    }
}

/// A memo's wholes, each with the tick it was last read or held at.
#[derive(Default)]
struct HeldWholes {
    tick: u64,
    wholes: Vec<(WholeKey, u64, Arc<Whole>)>,
}

impl WholeMemo {
    /// The bound a dash holds: four reads of its tree, so the default view, the view with tests
    /// shown or shown alone and one other grain each answer from it while the tree stands.
    pub const BOUND: usize = 4;

    /// An empty memo holding at most `bound` reads of one tree.
    pub fn new(bound: usize) -> Self {
        Self {
            bound,
            held: Mutex::new(HeldWholes::default()),
        }
    }

    /// `view` resolved whole under `filter` at `resolution` for a whole read: the held whole of
    /// the read when the memo holds it, else resolved now and held. It resolves outside the
    /// memo's lock, so a read of the memo never waits on another's resolve.
    fn whole(&self, view: &dyn CodeView, filter: ViewFilter, resolution: f64) -> Arc<Whole> {
        let key = WholeKey {
            stamp: view.stamp().clone(),
            overlaid: view.overlaid(),
            tests: filter.tests,
            resolution: resolution.to_bits(),
        };
        if let Some(whole) = self.get(&key) {
            return whole;
        }
        let whole = Arc::new(Whole::read(view, filter, resolution, false));
        self.put(key, Arc::clone(&whole));
        whole
    }

    fn get(&self, key: &WholeKey) -> Option<Arc<Whole>> {
        let mut held = self.held.lock().ok()?;
        held.tick += 1;
        let tick = held.tick;
        let (_, read, whole) = held.wholes.iter_mut().find(|(k, ..)| k == key)?;
        *read = tick;
        Some(Arc::clone(whole))
    }

    fn put(&self, key: WholeKey, whole: Arc<Whole>) {
        let Ok(mut held) = self.held.lock() else {
            return;
        };
        held.tick += 1;
        let tick = held.tick;
        held.wholes.retain(|(k, ..)| k.of_tree(&key) && *k != key);
        held.wholes.push((key, tick, whole));
        while held.wholes.len() > self.bound {
            let oldest = (held.wholes.iter().enumerate())
                .min_by_key(|(_, (_, read, _))| *read)
                .map(|(at, _)| at);
            match oldest {
                Some(at) => held.wholes.remove(at),
                None => break,
            };
        }
    }
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
        rel: REL_REALIZES.into(),
        valid_from: governs.valid_from,
        valid_to: None,
        source: governs.source,
        tier: TIER_INFERRED.into(),
    }
}

/// Which way an edge read runs from the node it is read at.
#[derive(Clone, Copy)]
enum Way {
    Out,
    In,
}

/// What a read takes a node's doc from ([`ContextGraph::doc`]): the lines of the caller's
/// worktree, and the extractor the store was opened with, which gives a span's text.
#[derive(Clone, Copy)]
pub struct DocReader<'r> {
    pub lines: &'r dyn SourceLines,
    pub extract: &'r dyn Extract,
}

/// The record half and a view's code half `C` read as one context graph.
pub struct Composed<'r, C = Whole> {
    records: &'r dyn Neighbors,
    code: C,
    /// What the read takes a node's doc from; none for a read that gives no doc.
    docs: Option<DocReader<'r>>,
    /// The tests mode of the filter the graph was opened under, which
    /// [`ContextGraph::locate`] reads a site's referrers under.
    tests: Tests,
    /// Each recorded alias's canonical id.
    canonical: BTreeMap<String, String>,
    /// Each canonical id's recorded aliases.
    aliased: BTreeMap<String, Vec<String>>,
    /// Whether every code-half node is withheld: the caller's worktree cannot be compared.
    stale: bool,
    label: Label,
    /// The ids withheld so far because the worktree cannot be compared.
    withheld: Mutex<BTreeSet<String>>,
    /// The record half's rows read ahead ([`Neighbors::read_ahead`]), by the spelling read.
    ahead: Mutex<FxHashMap<String, Rows>>,
}

impl<'r> Composed<'r> {
    /// The record half `records` composed with `view` resolved whole under `filter`, labelled by
    /// `honesty`: the graph a reader of the whole graph reads.
    pub fn new(
        records: &'r dyn Neighbors,
        view: &dyn CodeView,
        filter: ViewFilter,
        honesty: Honesty,
    ) -> Result<Self, Error> {
        Self::at(records, view, filter, honesty, DEFAULT_RESOLUTION)
    }

    /// [`Self::new`] with the groupings at `resolution` joined on beside the stored ones
    /// ([`Whole::at`]): the graph a reader of one grain reads.
    pub fn at(
        records: &'r dyn Neighbors,
        view: &dyn CodeView,
        filter: ViewFilter,
        honesty: Honesty,
        resolution: f64,
    ) -> Result<Self, Error> {
        Self::over(
            records,
            Whole::at(view, filter, resolution),
            &view.stamp().tip,
            honesty,
            filter.tests,
        )
    }

    /// The record half `records` composed with `view` under `filter`, labelled by `honesty`, its
    /// code half resolved only near the ids it is asked about (section 6.3): the graph a reader
    /// of a neighbourhood reads. Its answers are [`Self::new`]'s, and it alone gives a node's doc,
    /// read through `docs` when given ([`ContextGraph::doc`]).
    pub fn near(
        records: &'r dyn Neighbors,
        view: &'r dyn CodeView,
        filter: ViewFilter,
        honesty: Honesty,
        docs: Option<DocReader<'r>>,
    ) -> Result<impl ContextGraph + 'r, Error> {
        let code = crate::near::Near::new(view, filter);
        let composed = Composed::over(records, code, &view.stamp().tip, honesty, filter.tests)?;
        Ok(Composed { docs, ..composed })
    }
}

/// An open of the whole graph that answers only the whole graph ([`Composed::whole_at`]): it
/// cannot be walked, so it reads nothing only a walk reads.
pub struct WholeOpen<'r>(Composed<'r, Arc<Whole>>);

impl WholeOpen<'_> {
    /// The whole graph, its withheld part and its label ([`ContextGraph::into_whole`]).
    pub fn into_whole(self) -> Result<WholeRead, Error> {
        Box::new(self.0).into_whole()
    }
}

impl<'r> Composed<'r> {
    /// [`Self::at`] opened for its whole graph alone ([`WholeOpen`]): the read the dash's graph
    /// view makes.
    pub fn whole_at(
        records: &'r dyn Neighbors,
        view: &dyn CodeView,
        filter: ViewFilter,
        honesty: Honesty,
        resolution: f64,
    ) -> Result<WholeOpen<'r>, Error> {
        let code = Arc::new(Whole::read(view, filter, resolution, false));
        Composed::over(records, code, &view.stamp().tip, honesty, filter.tests).map(WholeOpen)
    }

    /// [`Self::whole_at`] with the code half taken from `memo` when it holds it for this read,
    /// else resolved and held there ([`WholeMemo`]): the read of a process that reads the whole
    /// graph again and again, as the dash does.
    pub fn whole_held_at(
        records: &'r dyn Neighbors,
        view: &dyn CodeView,
        filter: ViewFilter,
        honesty: Honesty,
        resolution: f64,
        memo: &WholeMemo,
    ) -> Result<WholeOpen<'r>, Error> {
        let code = memo.whole(view, filter, resolution);
        Composed::over(records, code, &view.stamp().tip, honesty, filter.tests).map(WholeOpen)
    }
}

impl<'r, C: CodeHalf> Composed<'r, C> {
    /// `records` composed with `code`, a view of the tree `tip` under the tests mode `tests`,
    /// labelled by `honesty`.
    fn over(
        records: &'r dyn Neighbors,
        code: C,
        tip: &Tip,
        honesty: Honesty,
        tests: Tests,
    ) -> Result<Self, Error> {
        let mut canonical = BTreeMap::new();
        let mut aliased: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (alias, id) in records.aliases()? {
            aliased.entry(id.clone()).or_default().push(alias.clone());
            canonical.insert(alias, id);
        }
        // A worktree that cannot be compared fails closed: no code hit is known fresh.
        let stale = honesty.compared.withholds_code();
        let label = Label::describing(tip, honesty.worktree, honesty.compared, honesty.behind);
        Ok(Composed {
            records,
            code,
            docs: None,
            tests,
            canonical,
            aliased,
            stale,
            label,
            withheld: Mutex::new(BTreeSet::new()),
            ahead: Mutex::new(FxHashMap::default()),
        })
    }

    /// The record half's node `spelling`, read ahead or read now.
    fn record_node(&self, spelling: &str) -> Result<Option<Node>, Error> {
        match self.ahead.lock().unwrap().get(spelling) {
            Some(rows) => Ok(rows.node.clone()),
            None => self.records.node(spelling),
        }
    }

    /// The record half's edges at `spelling` running `way`, read ahead or read now.
    fn record_edges(&self, spelling: &str, way: Way) -> Result<Vec<Edge>, Error> {
        if let Some(rows) = self.ahead.lock().unwrap().get(spelling) {
            return Ok(match way {
                Way::Out => rows.out.clone(),
                Way::In => rows.incoming.clone(),
            });
        }
        match way {
            Way::Out => self.records.out(spelling),
            Way::In => self.records.incoming(spelling),
        }
    }

    /// `e` with both ends read through the recorded aliases.
    fn canon_edge(&self, mut e: Edge) -> Edge {
        for end in [&mut e.from, &mut e.to] {
            if let Some(id) = self.canonical.get(end.as_str()) {
                end.clone_from(id);
            }
        }
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

    /// Whether `node` is withheld because the worktree cannot be compared, counting it when it
    /// is. A node of the code half is withheld when its facts came from a file ([`fact_file`]; a
    /// grouping's are the files it summarises).
    fn withholds(&self, node: &Node) -> bool {
        self.stale && self.code.holds(&node.id) && self.withholds_code(node)
    }

    /// How many test entities [`ContextGraph::hidden_tests`] answers, counted without copying
    /// them.
    fn hidden_test_count(&self) -> usize {
        if self.label.compared == Compared::Uncomparable {
            return 0;
        }
        self.code.hidden_test_count(&|n| !self.withholds_code(n))
    }

    /// Whether `node`, a node of the code half shown or hidden, is withheld because the worktree
    /// cannot be compared, counting it when it is.
    fn withholds_code(&self, node: &Node) -> bool {
        self.withholds_code_by(node, |id| self.summarises_withheld(id))
    }

    /// Whether the grouping `id` summarises a file, all of which a worktree that cannot be
    /// compared withholds.
    fn summarises_withheld(&self, id: &str) -> bool {
        self.stale && !self.code.summarised_files(id).is_empty()
    }

    /// [`Self::withholds_code`], reading whether a grouping summarises a withheld file by
    /// `summarises`.
    fn withholds_code_by(&self, node: &Node, summarises: impl Fn(&str) -> bool) -> bool {
        let stale = self.stale
            && if [KIND_COMMUNITY, KIND_CONCEPT].contains(&node.kind.as_str()) {
                summarises(&node.id)
            } else {
                fact_file(node).is_some()
            };
        if stale {
            self.withheld.lock().unwrap().insert(node.id.clone());
        }
        stale
    }

    /// The groupings of the code half that summarise a file the caller's worktree withholds:
    /// what withholding the half's whole reads of it beside its graph.
    fn withheld_groupings(&self) -> FxHashSet<String> {
        if !self.stale {
            return FxHashSet::default();
        }
        let groupings = self.code.groupings();
        (groupings.communities.iter().chain(&groupings.concepts))
            .map(|c| &c.id)
            .filter(|id| self.code.holds(id) && self.summarises_withheld(id))
            .cloned()
            .collect()
    }

    /// Both halves whole over `code`, the code half's whole graph, `withheld` the groupings
    /// that summarise a withheld file ([`Self::withheld_groupings`]): of the code half it reads
    /// only [`CodeHalf::concepts_of`], so `code` may have been moved out of it. Beside it, the
    /// part withheld: the held nodes not answered and the edges dropped for an end not answered.
    fn whole_over(
        &self,
        code: Graph,
        withheld: &FxHashSet<String>,
    ) -> Result<(Graph, Graph), Error> {
        let records = self.records.whole()?;
        let is_alias = |n: &Node| self.canonical.contains_key(&n.id);
        // Each id's record nodes, in the order recorded, folded onto its code node in turn.
        let mut recorded: BTreeMap<String, Vec<Node>> = BTreeMap::new();
        for r in records.nodes.into_iter().filter(|n| !is_alias(n)) {
            recorded.entry(r.id.clone()).or_default().push(r);
        }
        let fold = |code: Option<Node>, records: Vec<Node>| {
            records.into_iter().fold(code, |acc, r| merge(acc, Some(r)))
        };
        let mut recorded = recorded.into_iter().peekable();
        // A node of the code half is withheld as `withholds` withholds it.
        let withholds = |n: &Node| self.withholds_code_by(n, |id| withheld.contains(id));
        let record_only = |rs| fold(None, rs).map(|n| (n, true));
        // Each held node with whether it is answered, and the code half's aliased ids.
        let mut held: Vec<(Node, bool)> = Vec::new();
        let mut aliased: FxHashSet<String> = FxHashSet::default();
        for n in code.nodes {
            if is_alias(&n) {
                aliased.insert(n.id);
                continue;
            }
            while let Some((_, rs)) = recorded.next_if(|(id, _)| *id < n.id) {
                held.extend(record_only(rs));
            }
            let rs = recorded.next_if(|(id, _)| *id == n.id).map(|(_, rs)| rs);
            let n = fold(Some(n), rs.unwrap_or_default());
            held.extend(n.map(|n| {
                let answered = !withholds(&n);
                (n, answered)
            }));
        }
        held.extend(recorded.flat_map(|(_, rs)| record_only(rs)));
        let answers: FxHashMap<&str, bool> =
            held.iter().map(|(n, a)| (n.id.as_str(), *a)).collect();
        let mut edges = Vec::new();
        for e in code.edges.into_iter().chain(records.edges) {
            let e = self.canon_edge(e);
            if e.rel == REL_GOVERNS {
                for c in self.code.concepts_of(&e.to) {
                    edges.push(attachment(&e, &c));
                }
            }
            edges.push(e);
        }
        let answered = |id: &str| match answers.get(id) {
            Some(&answered) => answered,
            None => !(aliased.contains(id) && withholds(&artifact(id))),
        };
        // Nothing withheld answers every edge.
        let mut dropped = Vec::new();
        if self.stale {
            (edges, dropped) =
                (edges.into_iter()).partition(|e| answered(&e.from) && answered(&e.to));
        }
        let mut ends: BTreeMap<&str, Node> = BTreeMap::new();
        for e in &edges {
            for end in [&e.from, &e.to] {
                if !answers.contains_key(end.as_str()) {
                    ends.entry(end).or_insert_with(|| artifact(end));
                }
            }
        }
        drop(answers);
        let mut nodes: Vec<Node> = Vec::with_capacity(held.len() + ends.len());
        let mut unanswered = Vec::new();
        let mut ends = ends.into_values().peekable();
        for (n, answered) in held {
            nodes.extend(std::iter::from_fn(|| ends.next_if(|a| a.id < n.id)));
            match answered {
                true => nodes.push(n),
                false => unanswered.push(n),
            }
        }
        nodes.extend(ends);
        let whole = Graph {
            nodes,
            edges: ordered(edges),
        };
        let withheld = Graph {
            nodes: unanswered,
            edges: ordered(dropped),
        };
        Ok((whole, withheld))
    }

    /// The node `id` names in both halves, merged, before any is withheld; `None` for an alias.
    fn merged(&self, id: &str) -> Result<Option<Node>, Error> {
        if self.canonical.contains_key(id) {
            return Ok(None);
        }
        Ok(merge(self.code.node(id)?, self.record_node(id)?))
    }

    /// Whether an edge end `id` is answered: the worktree can be compared, or the node it reads
    /// as (an [`artifact`] when no half holds it) is not withheld.
    fn answers_end(&self, id: &str) -> Result<bool, Error> {
        if !self.stale {
            return Ok(true);
        }
        let node = self.merged(id)?.unwrap_or_else(|| artifact(id));
        Ok(!self.withholds(&node))
    }

    /// The edges both halves hold at `spelling` running `way`, ends canonical.
    fn halves_at(&self, spelling: &str, way: Way) -> Result<Vec<Edge>, Error> {
        let code = match way {
            Way::Out => self.code.out(spelling)?,
            Way::In => self.code.incoming(spelling)?,
        };
        let records = self.record_edges(spelling, way)?;
        Ok((code.into_iter().chain(records))
            .map(|e| self.canon_edge(e))
            .collect())
    }

    /// The edges read at `id` running `way` from both halves and its aliases, ends canonical,
    /// with its `REALIZES` attachments, each (from, to, relation) once, the code half's first;
    /// none at an alias or a withheld node, none whose other end is withheld, and, given
    /// `within`, none whose other end is outside it, which is never asked whether it is withheld. An attachment
    /// comes of a GOVERNS edge in either half, as the whole graph's do: out of its source, one
    /// per concept of the node it governs; into a concept, one per GOVERNS into a member.
    fn edges(
        &self,
        id: &str,
        way: Way,
        within: Option<&BTreeSet<String>>,
    ) -> Result<Vec<Edge>, Error> {
        if self.canonical.contains_key(id) || !self.answers_end(id)? {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for spelling in self.spellings(id) {
            found.extend(self.halves_at(spelling, way)?);
        }
        let mut attached = Vec::new();
        match way {
            Way::Out => {
                for e in found.iter().filter(|e| e.rel == REL_GOVERNS) {
                    for c in self.code.concepts_of_within(&e.to, within) {
                        attached.push(attachment(e, &c));
                    }
                }
            }
            Way::In => {
                for m in self.code.concept_members(id) {
                    for spelling in self.spellings(&m) {
                        for e in self.halves_at(spelling, Way::In)? {
                            if e.rel == REL_GOVERNS {
                                attached.push(attachment(&e, id));
                            }
                        }
                    }
                }
            }
        }
        found.extend(attached);
        let mut answered = Vec::new();
        for e in found {
            let other = match way {
                Way::Out => &e.to,
                Way::In => &e.from,
            };
            if within.is_none_or(|ids| ids.contains(other)) && self.answers_end(other)? {
                answered.push(e);
            }
        }
        Ok(ordered(answered))
    }

    /// The answered nodes of `ids` from both halves, in id order, the record half's read together.
    fn answered(&self, ids: impl IntoIterator<Item = String>) -> Result<Vec<Node>, Error> {
        let ids: BTreeSet<String> = ids.into_iter().collect();
        self.read_ahead(&ids.iter().cloned().collect::<Vec<_>>())?;
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

impl<C: CodeHalf> Neighbors for Composed<'_, C> {
    fn node(&self, id: &str) -> Result<Option<Node>, Error> {
        Ok(self.merged(id)?.filter(|n| !self.withholds(n)))
    }
    fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.edges(id, Way::Out, None)
    }
    fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
        self.edges(id, Way::In, None)
    }
    fn out_within(&self, id: &str, within: &BTreeSet<String>) -> Result<Vec<Edge>, Error> {
        self.edges(id, Way::Out, Some(within))
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
        Ok(self
            .whole_over(self.code.whole()?, &self.withheld_groupings())?
            .0)
    }
    /// The record half's rows at every spelling of `ids` not read ahead yet, read together.
    fn read_ahead(&self, ids: &[String]) -> Result<(), Error> {
        let spellings: BTreeSet<&str> = {
            let ahead = self.ahead.lock().unwrap();
            (ids.iter())
                .flat_map(|id| self.spellings(id))
                .filter(|s| !ahead.contains_key(*s))
                .collect()
        };
        if spellings.is_empty() {
            return Ok(());
        }
        let spellings: Vec<&str> = spellings.into_iter().collect();
        let rows = self.records.rows(&spellings)?;
        let mut ahead = self.ahead.lock().unwrap();
        for (spelling, rows) in spellings.into_iter().zip(rows) {
            ahead.insert(spelling.to_string(), rows);
        }
        Ok(())
    }
}

impl<C: CodeHalf> ContextGraph for Composed<'_, C> {
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
        let down_hop = |id: &str| self.code.down_hop(id);
        calls(self, seed, direction, depth, tier_floor, &down_hop)
    }
    fn resolve(&self, mention: &str) -> Result<Option<String>, Error> {
        match self.canonical.get(mention) {
            Some(id) => Ok(Some(id.clone())),
            None => Ok(self.node(mention)?.map(|n| n.id)),
        }
    }
    fn locate(&self, entity: &str) -> Result<Located, Error> {
        Ok(match locate(self, entity)? {
            Located::One(mut site) => {
                site.doc_comment = self.node(&site.id)?.and_then(|node| self.doc(&node));
                site.referrers = self.referrers(&site.id, self.tests);
                Located::One(site)
            }
            other => other,
        })
    }
    fn definition_end(&self, site: &EntitySite) -> Result<Option<u32>, Error> {
        let at = recorded_at(site).filter(|_| !self.label.compared.withholds_code());
        Ok(at.and_then(|(file, recorded)| self.code.definition_end(file, recorded)))
    }
    fn doc(&self, node: &Node) -> Option<String> {
        let docs = self
            .docs
            .filter(|_| !self.label.compared.withholds_code())?;
        let site = entity_site(node, 0);
        let (file, recorded) = match node.kind == KIND_FILE {
            true => (site.file.as_deref()?, None),
            false => recorded_at(&site).map(|(file, recorded)| (file, Some(recorded)))?,
        };
        let (lang, span) = self.code.doc_span(file, recorded)?;
        Some(docs.extract.doc_text(&lang, &docs.lines.lines(file, span)?))
    }
    fn referrers(&self, id: &str, tests: Tests) -> Vec<Referrer> {
        if self.stale {
            return Vec::new();
        }
        let mut referrers = self.code.referrers(id);
        referrers.retain(|r| !r.test || tests.admits(true));
        referrers
    }
    fn nodes_of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
        self.of_kind(kind)
    }

    fn hidden_tests(&self) -> Result<Vec<Node>, Error> {
        if self.label.compared == Compared::Uncomparable {
            return Ok(Vec::new());
        }
        Ok(self
            .code
            .hidden_tests()
            .into_iter()
            .filter(|n| !self.withholds_code(n))
            .collect())
    }
    fn entity_names(&self) -> Result<Vec<String>, Error> {
        let mut names = self.code.entity_names();
        let recorded = self.records.of_kind(KIND_CODE_ENTITY)?;
        names.extend(recorded.iter().filter_map(node_name).map(str::to_string));
        names.sort_unstable();
        names.dedup();
        Ok(names)
    }
    /// The nodes binding `name` in either half, kept when they read as a code entity named
    /// `name` before any is withheld, so the label counts only the entities so named.
    fn entities_named(&self, name: &str) -> Result<Vec<Node>, Error> {
        let ids: BTreeSet<String> = self.both(|h| h.leaf_nodes(name))?.into_iter().collect();
        let mut nodes = Vec::new();
        for id in ids {
            let named = (self.merged(&id)?)
                .filter(|n| n.kind == KIND_CODE_ENTITY && node_name(n) == Some(name));
            nodes.extend(named.filter(|n| !self.withholds(n)));
        }
        Ok(nodes)
    }
    fn hidden_tests_named(&self, name: &str) -> Result<Vec<Node>, Error> {
        if self.label.compared == Compared::Uncomparable {
            return Ok(Vec::new());
        }
        Ok(self
            .code
            .hidden_tests_named(name)
            .into_iter()
            .filter(|n| !self.withholds_code(n))
            .collect())
    }
    fn hidden_provers(&self, ids: &[String]) -> Result<Vec<String>, Error> {
        if self.stale {
            return Ok(Vec::new());
        }
        Ok(self.code.hidden_provers(ids))
    }
    fn hidden_placeholder_callers(&self, ids: &[String]) -> Result<Vec<String>, Error> {
        if self.stale {
            return Ok(Vec::new());
        }
        let mut hidden = Vec::new();
        for id in ids {
            if let Some(leaf) = bound_leaf(self, id)? {
                hidden.extend(self.code.placeholder_callers(id, &leaf));
            }
        }
        hidden.sort_unstable();
        hidden.dedup();
        Ok(hidden)
    }
    fn whole(&self) -> Result<Graph, Error> {
        Neighbors::whole(self)
    }
    /// The whole read in two phases: what withholding reads of the code half beside its graph
    /// first, then the graph moved out of the half and merged with the record half's.
    fn into_whole(mut self: Box<Self>) -> Result<WholeRead, Error> {
        let tests_hidden = self.hidden_test_count();
        let withheld = self.withheld_groupings();
        let code = self.code.take_whole()?;
        let (graph, withheld) = self.whole_over(code, &withheld)?;
        Ok(WholeRead {
            graph,
            withheld,
            tests_hidden,
            label: self.stamp_label(),
        })
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
    let mut level: Vec<String> = reached.iter().cloned().collect();
    let mut nodes = BTreeMap::new();
    let mut edges = Vec::new();
    // Level by level, each level's ids told ahead of their reads.
    for hops in 0.. {
        if level.is_empty() {
            break;
        }
        graph.read_ahead(&level)?;
        let mut deeper = Vec::new();
        for id in std::mem::take(&mut level) {
            let node = graph.node(&id)?;
            // The last level reads its edges among the ids reached alone: an end past it is
            // never reached, so it is never counted as withheld either.
            let out = match hops < depth {
                true => graph.out(&id)?,
                false => graph.out_within(&id, &reached)?,
            };
            if hops < depth {
                let mut next: Vec<String> = out.iter().map(|e| e.to.clone()).collect();
                next.extend(graph.incoming(&id)?.into_iter().map(|e| e.from));
                next.extend(graph.members(&id)?.into_iter().map(|m| m.id));
                next.extend(node.as_ref().and_then(member_file).map(str::to_string));
                for n in next {
                    if reached.insert(n.clone()) {
                        deeper.push(n);
                    }
                }
            }
            edges.extend(out);
            if let Some(node) = node {
                nodes.insert(id, node);
            }
        }
        level = deeper;
    }
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
        .map(|n| n.id)
        .filter(|id| in_file(id, file))
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Whether `id` is in `file`'s `<file>::` id range, any id when no file is given.
fn in_file(id: &str, file: Option<&str>) -> bool {
    file.is_none_or(|f| id.starts_with(&entity_id(f, "")))
}

/// Whether the named node `id` is a call target: `Some(true)` for a definition a call resolves
/// onto, `Some(false)` for a named data member or declaration, `None` for a node with no name or
/// no node.
fn named_call_target(graph: &dyn Neighbors, id: &str) -> Result<Option<bool>, Error> {
    Ok(graph.node(id)?.as_ref().and_then(call_target))
}

/// [`named_call_target`] of the node `n`.
fn call_target(n: &Node) -> Option<bool> {
    node_name(n).map(|_| is_call_target(n.attrs.get("kind").map(String::as_str)))
}

/// One resolved `CALLS` hop of a layered walk: the neighbor reached, the edge it was reached by
/// (its tier, valid time and source), and the sorted candidate definitions when the hop is a
/// frontier the walk does not expand.
struct Hop {
    next: String,
    via: Edge,
    frontier: Option<Vec<String>>,
}

/// The candidates a placeholder keyed by a qualified name keeps ([`CodeHalf::down_hop`]): none
/// for any other id.
type DownHop<'a> = &'a dyn Fn(&str) -> Option<Vec<String>>;

/// One DOWN hop's callee: a named call target lands on itself; any other target resolves by the
/// leaf its id names over the call-target definitions, those a placeholder keyed by a qualified
/// name reaches by its leaf and keeps by its down-hop (`down_hop`), one auto-continuing, more a
/// frontier, none leaving it where it is.
fn resolve_down_hop(
    graph: &dyn Neighbors,
    raw_to: &str,
    down_hop: DownHop,
) -> Result<(String, Option<Vec<String>>), Error> {
    if named_call_target(graph, raw_to)? == Some(true) {
        return Ok((raw_to.to_string(), None));
    }
    let cands = match (leaf_of(raw_to), down_hop(raw_to)) {
        (Some(name), Some(kept)) => {
            let cands = definitions(graph, path_leaf(name), Admitted::CallTargets, None)?;
            kept_of(&cands, Some(&kept))
        }
        (Some(leaf), None) => definitions(graph, leaf, Admitted::CallTargets, None)?,
        (None, _) => Vec::new(),
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

/// The leaf whose placeholders an UP hop reaches `cur` through ([`callers_of`]): a call-target
/// definition's leaf; none for any other node.
fn bound_leaf(graph: &dyn Neighbors, cur: &str) -> Result<Option<String>, Error> {
    let node = graph.node(cur)?.filter(|n| call_target(n) == Some(true));
    Ok(node.as_ref().and_then(node_leaf).map(str::to_string))
}

/// The callers of `cur` for one UP hop: none for a named non-target; else the direct callers
/// whose edge targets `cur`, and, for a definition, the callers through a placeholder bound to
/// its leaf ([`bound_leaf`]) that reaches it: one keyed by a qualified name only when its
/// down-hop keeps `cur` (`down_hop`). Each is a frontier when the call-target definitions of the
/// leaf its placeholder reaches, those its down-hop keeps, are more than one.
fn callers_of(graph: &dyn Neighbors, cur: &str, down_hop: DownHop) -> Result<Vec<Hop>, Error> {
    if named_call_target(graph, cur)? == Some(false) {
        return Ok(Vec::new());
    }
    let mut hops: Vec<Hop> = calls_at(graph.incoming(cur)?, |e| &e.from)
        .into_iter()
        .map(|via| Hop {
            next: via.from.clone(),
            via,
            frontier: None,
        })
        .collect();
    if let Some(leaf) = bound_leaf(graph, cur)?.as_deref() {
        let cands = definitions(graph, leaf, Admitted::CallTargets, None)?;
        let mut via_bare = Vec::new();
        for t in graph.leaf_nodes(leaf)? {
            let target = is_call_target(t.attrs.get("kind").map(String::as_str));
            if !id_binds_leaf(&t.id, leaf) || (node_name(&t).is_some() && target) {
                continue;
            }
            let kept = down_hop(&t.id);
            if kept.as_ref().is_some_and(|k| !k.iter().any(|c| c == cur)) {
                continue;
            }
            let reached = kept_of(&cands, kept.as_deref());
            let frontier = (reached.len() > 1).then_some(reached);
            let calls = calls_at(graph.incoming(&t.id)?, |e| &e.from);
            via_bare.extend(calls.into_iter().map(|via| (via, frontier.clone())));
        }
        via_bare.sort_by(|(a, _), (b, _)| a.from.cmp(&b.from));
        for (via, frontier) in via_bare {
            hops.push(Hop {
                next: via.from.clone(),
                via,
                frontier,
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
                    rel: REL_CALLS.into(),
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
/// only at or above `tier_floor`. A placeholder keyed by a qualified name reaches only the
/// candidates its down-hop keeps (`down_hop`, [`CodeHalf::down_hop`]).
pub fn calls(
    graph: &dyn Neighbors,
    seed: &[String],
    direction: Direction,
    depth: i64,
    tier_floor: &str,
    down_hop: DownHop,
) -> Result<CallGraph, Error> {
    let floor = tier_rank(tier_floor, UNRECOGNIZED_FLOOR_RANK);
    let followed = |e: &Edge| tier_rank(&e.tier, UNRECOGNIZED_EDGE_RANK) >= floor;
    match direction {
        Direction::Down => {
            let hops = |cur: &str| {
                let mut hops = Vec::new();
                for via in calls_at(graph.out(cur)?, |e| &e.to) {
                    if followed(&via) {
                        let (next, frontier) = resolve_down_hop(graph, &via.to, down_hop)?;
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
                let mut hops = callers_of(graph, cur, down_hop)?;
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

/// The definitions a show query names when no exact id answers it, sorted.
fn shown_candidates(graph: &dyn Neighbors, query: &str) -> Result<Vec<String>, Error> {
    named_by(query, |leaf, file| {
        definitions(graph, leaf, Admitted::Named, file)
    })
}

/// The ids a show query names among those `named` reads by leaf (in a file's id range when one
/// is given), sorted: the first non-empty of the query as a leaf, its path leaf filtered on the
/// id suffix `::<query>`, and its rest half as a leaf in its file half. The one rule of what a
/// show names, read over the shown definitions and over the hidden tests alike.
fn named_by(
    query: &str,
    named: impl Fn(&str, Option<&str>) -> Result<Vec<String>, Error>,
) -> Result<Vec<String>, Error> {
    let by_leaf = named(query, None)?;
    if !by_leaf.is_empty() {
        return Ok(by_leaf);
    }
    let suffix = entity_id("", query);
    let by_path: Vec<String> = named(path_leaf(query), None)?
        .into_iter()
        .filter(|id| id.ends_with(&suffix))
        .collect();
    if !by_path.is_empty() {
        return Ok(by_path);
    }
    let (file, rest) = split_id(query);
    named(rest.unwrap_or_default(), Some(file))
}

/// The test entities the graph's `tests` filter hides that a show of `query` names, sorted: the
/// one whose id is the query, else those [`named_by`] names among the hidden tests, as
/// [`locate`] names a shown definition. None when tests are shown.
pub(crate) fn hidden_named(graph: &dyn ContextGraph, query: &str) -> Result<Vec<String>, Error> {
    let hidden = |leaf: &str, file: Option<&str>| -> Result<Vec<String>, Error> {
        Ok((graph.hidden_tests_named(leaf)?.into_iter())
            .map(|n| n.id)
            .filter(|id| in_file(id, file))
            .collect())
    };
    let exact: Vec<String> = (hidden(path_leaf(query), None)?.into_iter())
        .filter(|id| id == query)
        .collect();
    if !exact.is_empty() {
        return Ok(exact);
    }
    named_by(query, hidden)
}

/// [`ContextGraph::locate`] over `graph`: an exact id (a bare flat node's only once no definition
/// answers), else the definitions [`shown_candidates`] names, one a site and more the candidates.
/// The site carries no doc and no referrer, which the composed graph's `locate` adds to it.
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
        Def, DocConcept, DocLink, FileFacts, Half, Mention, MentionKind, Stamp, Tip, WfNode,
    };
    use crate::code_view::{
        Cluster, DefRef, Entry, Extract, FactsView, FileRef, Groupings, MentionRef, SourceLines,
    };
    use crate::contextgraph::{
        KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_CONCEPT, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE,
        KIND_FINDING, KIND_STAGE, REL_ABOUT, REL_GOVERNS, REL_REALIZES, REL_SPECIFIES,
        TIER_EXTRACTED,
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

    /// [`view`] whose file reads and definitions read by leaf are recorded, to count the files
    /// and leaves an open resolves, holding what a store settles as a snapshot does.
    struct Counted<'v> {
        view: &'v Grouped,
        asked: Mutex<Vec<String>>,
        /// The files the view takes from a worktree ([`CodeView::overlaid`]).
        overlaid: Vec<(String, Option<String>)>,
    }

    impl<'v> Counted<'v> {
        /// `view`, its stamped tree alone, with nothing read yet.
        fn new(view: &'v Grouped) -> Self {
            Self {
                view,
                asked: Mutex::new(Vec::new()),
                overlaid: Vec::new(),
            }
        }
    }

    impl CodeView for Counted<'_> {
        fn stamp(&self) -> &Stamp {
            self.view.stamp()
        }
        fn files(&self) -> Box<dyn Iterator<Item = FileRef<'_>> + '_> {
            self.view.files()
        }
        fn file(&self, path: &str) -> Option<Cow<'_, FileFacts>> {
            self.asked.lock().unwrap().push(format!("file {path}"));
            self.view.file(path)
        }
        fn defs_named(&self, leaf: &str) -> Box<dyn Iterator<Item = DefRef<'_>> + '_> {
            self.asked.lock().unwrap().push(format!("defs {leaf}"));
            self.view.defs_named(leaf)
        }
        fn mentions_named(&self, leaf: &str) -> Box<dyn Iterator<Item = MentionRef<'_>> + '_> {
            self.view.mentions_named(leaf)
        }
        fn groupings(&self) -> &Groupings {
            self.view.groupings()
        }
        fn settled(&self) -> Option<&dyn crate::code_view::Settled> {
            Some(self)
        }
        fn overlaid(&self) -> Vec<(String, Option<String>)> {
            self.overlaid.clone()
        }
    }

    /// What a store settles for the view at advance: no test file, and no proof, its one test
    /// call landing on a placeholder; a read takes them rather than walking the test calls.
    impl crate::code_view::Settled for Counted<'_> {
        fn test_files(&self) -> Vec<&str> {
            Vec::new()
        }
        fn proofs(&self, _of: crate::code_view::Proofs<'_>) -> Vec<ProofCall> {
            Vec::new()
        }
    }

    /// A whole read resolves no leaf only a walk reads: GIVEN a test in a code file calling
    /// `helper`, which nothing defines, so the call lands on a placeholder, WHEN the graph is
    /// opened for walks and for its whole graph alone under each filter, THEN only the open for
    /// walks hiding tests reads `helper`'s definitions (the callers a walk reaches through its
    /// placeholder), the open showing tests reads them once for the call's edge, and both opens
    /// answer the same whole graph and count the hidden tests [`ContextGraph::hidden_tests`]
    /// lists.
    #[test]
    fn a_whole_read_resolves_no_leaf_only_a_walk_reads() {
        let mut grouped = view(Some("c1"));
        let mut files: Vec<FileFacts> = (grouped.facts.files())
            .filter_map(|f| grouped.facts.file(f.path).map(Cow::into_owned))
            .collect();
        let mut check = def("check");
        check.test = true;
        files.push(rust(
            "src/c.rs",
            vec![def("serve"), check],
            vec![Mention {
                enclosing: Some(1),
                name: "helper".into(),
                leaf: "helper".into(),
                kind: MentionKind::Call,
                line: 1,
                test: true,
            }],
        ));
        grouped.facts = FactsView::new(grouped.facts.stamp().clone(), files);
        let records = records();
        // (open, filter, the reads of `helper`'s definitions)
        let cases = [
            ("walks", Tests::Exclude, 1),
            ("whole", Tests::Exclude, 0),
            ("walks", Tests::Include, 1),
            ("whole", Tests::Include, 1),
        ];
        let mut wholes = BTreeMap::new();
        for (open, tests, want) in cases {
            let counted = Counted::new(&grouped);
            let filter = ViewFilter { tests };
            let read = match open {
                "walks" => Box::new(
                    Composed::at(&records, &counted, filter, exact(), DEFAULT_RESOLUTION).unwrap(),
                )
                .into_whole(),
                _ => Composed::whole_at(&records, &counted, filter, exact(), DEFAULT_RESOLUTION)
                    .unwrap()
                    .into_whole(),
            }
            .unwrap();
            let asked = counted.asked.lock().unwrap();
            let helper = asked.iter().filter(|read| *read == "defs helper").count();
            assert_eq!(helper, want, "{open} under {tests:?}: reads of helper");
            let hidden = Composed::at(&records, &grouped, filter, exact(), DEFAULT_RESOLUTION)
                .unwrap()
                .hidden_tests()
                .unwrap()
                .len();
            assert_eq!(
                read.tests_hidden, hidden,
                "{open} under {tests:?}: the hidden tests counted"
            );
            let graph = format!("{:?}", read.graph);
            let first = wholes
                .entry(format!("{tests:?}"))
                .or_insert_with(|| graph.clone());
            assert_eq!(*first, graph, "{open} under {tests:?}: the whole graph");
        }
    }

    /// A neighbourhood's last level resolves no file past it: GIVEN `dec-9`, about
    /// `src/b.rs::compute` and governing `src/z.rs`, a file nothing else reaches, in a concept or
    /// not, WHEN the neighbourhood of `compute` one hop deep is read near it, THEN `dec-9` at the
    /// last level reads none of `src/z.rs` (its edge there, and an attachment to a concept the
    /// walk never reached, are dropped unread), and the neighbourhood is the whole open's.
    #[test]
    fn a_neighbourhood_s_last_level_resolves_no_file_past_it() {
        // (case, whether a concept groups `src/z.rs`)
        let cases = [("in a concept", true), ("in no concept", false)];
        for (case, grouped) in cases {
            let mut view = view(Some("c1"));
            let mut files: Vec<FileFacts> = (view.facts.files())
                .filter_map(|f| view.facts.file(f.path).map(Cow::into_owned))
                .collect();
            files.push(rust("src/z.rs", vec![def("idle")], Vec::new()));
            view.facts = FactsView::new(view.facts.stamp().clone(), files);
            if grouped {
                view.groupings.concepts.push(Cluster {
                    id: "concept/1/1".into(),
                    label: "idle".into(),
                    resolution: 1.0,
                    hash: "z".into(),
                    members: vec!["src/z.rs".into()],
                });
            }
            let (mut rows, aliases) = record_rows();
            rows.nodes.push(node("dec-9", KIND_DECISION, &[]));
            rows.edges
                .push(edge("dec-9", "src/b.rs::compute", REL_ABOUT));
            rows.edges.push(edge("dec-9", "src/z.rs", REL_GOVERNS));
            let records = Indexed::new(rows, aliases);
            let counted = Counted::new(&view);
            let seed = ["src/b.rs::compute".to_string()];
            let near =
                Composed::near(&records, &counted, ViewFilter::default(), exact(), None).unwrap();
            let got = near.subgraph(&seed, 1).unwrap();
            let asked = counted.asked.lock().unwrap();
            let past: Vec<&String> = asked.iter().filter(|r| r.contains("src/z.rs")).collect();
            assert!(
                past.is_empty(),
                "{case}: read past the last level: {past:?}"
            );
            let whole = composed(&records, &view, exact())
                .subgraph(&seed, 1)
                .unwrap();
            assert_eq!(
                format!("{got:?}"),
                format!("{whole:?}"),
                "{case}: the whole open's neighbourhood"
            );
        }
    }

    /// What a whole read of `view` under `tests` at `resolution` answers over `records`, through
    /// `memo` when one is given, with whether it read any of the view (it resolved the view).
    fn whole_read(
        records: &Indexed,
        view: &Counted,
        tests: Tests,
        resolution: f64,
        memo: Option<&WholeMemo>,
    ) -> (String, bool) {
        view.asked.lock().unwrap().clear();
        let filter = ViewFilter { tests };
        let open = match memo {
            Some(memo) => Composed::whole_held_at(records, view, filter, exact(), resolution, memo),
            None => Composed::whole_at(records, view, filter, exact(), resolution),
        };
        let read = open.unwrap().into_whole().unwrap();
        let answer = format!(
            "{:?} {:?} {} {}",
            read.graph, read.withheld, read.tests_hidden, read.label
        );
        (answer, !view.asked.lock().unwrap().is_empty())
    }

    /// A held whole answers only the read it was resolved for: GIVEN a memo holding the whole of
    /// the view at `c1` with tests hidden at the default resolution, WHEN the graph is read whole
    /// through it again for each row's tree, worktree files, filter and resolution, THEN only the
    /// read of the same tree, files, filter and resolution is answered from the memo, reading
    /// nothing of the view, and every read answers what a read through no memo answers.
    #[test]
    fn a_held_whole_answers_only_the_read_it_was_resolved_for() {
        let (records, c1, c2) = (records(), view(Some("c1")), view(Some("c2")));
        let other = vec![("src/a.rs".to_string(), Some("a2".to_string()))];
        let deleted = vec![("src/a.rs".to_string(), None)];
        // (read, its tree, the files it takes from the worktree, filter, resolution, whether it
        // resolves the view again)
        let rows = [
            (
                "the same read",
                &c1,
                vec![],
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                false,
            ),
            (
                "tests shown",
                &c1,
                vec![],
                Tests::Include,
                DEFAULT_RESOLUTION,
                true,
            ),
            (
                "tests alone",
                &c1,
                vec![],
                Tests::Only,
                DEFAULT_RESOLUTION,
                true,
            ),
            ("another grain", &c1, vec![], Tests::Exclude, 2.0, true),
            (
                "another tree",
                &c2,
                vec![],
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                true,
            ),
            (
                "a worktree file",
                &c1,
                other,
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                true,
            ),
            (
                "a deleted file",
                &c1,
                deleted,
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                true,
            ),
        ];
        for (read, tree, overlaid, tests, resolution, resolves) in rows {
            let memo = WholeMemo::new(WholeMemo::BOUND);
            let first = Counted::new(&c1);
            whole_read(
                &records,
                &first,
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                Some(&memo),
            );
            let view = Counted {
                overlaid,
                ..Counted::new(tree)
            };
            let (held, resolved) = whole_read(&records, &view, tests, resolution, Some(&memo));
            let (unheld, _) = whole_read(&records, &view, tests, resolution, None);
            assert_eq!(
                (resolved, held == unheld),
                (resolves, true),
                "{read}: (resolved, answers the unheld read)"
            );
        }
    }

    /// The record half is never held: GIVEN a memo holding the whole of the view, WHEN a decision
    /// about `src/b.rs::compute` is recorded and the graph is read whole through it again, THEN
    /// the read reads nothing of the view and its answer holds the new decision, as a read
    /// through no memo does.
    #[test]
    fn a_held_whole_merges_the_records_as_they_stand() {
        let (c1, memo) = (view(Some("c1")), WholeMemo::new(WholeMemo::BOUND));
        let view = Counted::new(&c1);
        let (before, _) = whole_read(&records(), &view, Tests::Exclude, 1.0, Some(&memo));
        let (mut rows, aliases) = record_rows();
        rows.nodes.push(node("dec-2", KIND_DECISION, &[]));
        rows.edges
            .push(edge("dec-2", "src/b.rs::compute", REL_ABOUT));
        let recorded = Indexed::new(rows, aliases);
        let (held, resolved) = whole_read(&recorded, &view, Tests::Exclude, 1.0, Some(&memo));
        let (unheld, _) = whole_read(&recorded, &view, Tests::Exclude, 1.0, None);
        assert_eq!(
            (
                resolved,
                held.contains("dec-2"),
                before.contains("dec-2"),
                held == unheld
            ),
            (false, true, false, true)
        );
    }

    /// A memo holds its bound of one tree's reads: GIVEN a memo bound to two reads, WHEN the
    /// rows' reads are made through it in order, THEN a read resolves the view again only when
    /// the memo does not hold it: past the bound the least recently read is dropped, and a read
    /// of another tree drops every read of the last.
    #[test]
    fn a_whole_memo_holds_its_bound_of_one_tree_s_reads() {
        let (records, c1, c2) = (records(), view(Some("c1")), view(Some("c2")));
        let memo = WholeMemo::new(2);
        // (read, tree, filter, resolution, whether it resolves the view)
        let rows = [
            ("hidden", &c1, Tests::Exclude, DEFAULT_RESOLUTION, true),
            ("shown", &c1, Tests::Include, DEFAULT_RESOLUTION, true),
            (
                "hidden again",
                &c1,
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                false,
            ),
            (
                "another grain, dropping shown",
                &c1,
                Tests::Exclude,
                2.0,
                true,
            ),
            (
                "hidden held",
                &c1,
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                false,
            ),
            (
                "shown, dropping the grain",
                &c1,
                Tests::Include,
                DEFAULT_RESOLUTION,
                true,
            ),
            ("the grain again", &c1, Tests::Exclude, 2.0, true),
            (
                "another tree",
                &c2,
                Tests::Exclude,
                DEFAULT_RESOLUTION,
                true,
            ),
            ("the last tree's grain", &c1, Tests::Exclude, 2.0, true),
        ];
        for (read, tree, tests, resolution, resolves) in rows {
            let view = Counted::new(tree);
            let (_, resolved) = whole_read(&records, &view, tests, resolution, Some(&memo));
            assert_eq!(resolved, resolves, "{read}: resolves the view");
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
            doc: None,
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
            rel: rel.to_string().into(),
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
        let (graph, aliases) = record_rows();
        Indexed::new(graph, aliases)
    }

    /// [`records`]'s rows and aliases.
    fn record_rows() -> (Graph, Vec<(String, String)>) {
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
        let aliases = vec![("the-compute".into(), "src/b.rs::compute".into())];
        (graph, aliases)
    }

    fn honest(compared: Compared) -> Honesty {
        Honesty {
            worktree: "W".into(),
            compared,
            behind: None,
        }
    }

    fn composed<'r>(records: &'r Indexed, view: &Grouped, honesty: Honesty) -> Composed<'r> {
        Composed::new(records, view, ViewFilter::default(), honesty).unwrap()
    }

    fn exact() -> Honesty {
        honest(Compared::Overlaid(0))
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

    /// A record half that counts its reads by name.
    struct CountingRecords {
        records: Indexed,
        reads: Mutex<BTreeMap<&'static str, usize>>,
    }

    impl CountingRecords {
        fn read(&self, name: &'static str) {
            *self.reads.lock().unwrap().entry(name).or_default() += 1;
        }
    }

    impl Neighbors for CountingRecords {
        fn node(&self, id: &str) -> Result<Option<Node>, Error> {
            self.read("node");
            self.records.node(id)
        }
        fn out(&self, id: &str) -> Result<Vec<Edge>, Error> {
            self.read("out");
            self.records.out(id)
        }
        fn incoming(&self, id: &str) -> Result<Vec<Edge>, Error> {
            self.read("incoming");
            self.records.incoming(id)
        }
        fn members(&self, file: &str) -> Result<Vec<Node>, Error> {
            self.read("members");
            self.records.members(file)
        }
        fn leaf_nodes(&self, leaf: &str) -> Result<Vec<Node>, Error> {
            self.read("leaf_nodes");
            self.records.leaf_nodes(leaf)
        }
        fn of_kind(&self, kind: &str) -> Result<Vec<Node>, Error> {
            self.read("of_kind");
            self.records.of_kind(kind)
        }
        fn unresolved(&self, node: &Node) -> Result<Vec<String>, Error> {
            self.records.unresolved(node)
        }
        fn aliases(&self) -> Result<Vec<(String, String)>, Error> {
            self.records.aliases()
        }
        fn whole(&self) -> Result<Graph, Error> {
            self.read("whole");
            self.records.whole()
        }
        fn rows(&self, ids: &[&str]) -> Result<Vec<Rows>, Error> {
            self.read("rows");
            self.records.rows(ids)
        }
    }

    /// A neighbourhood reads the record half's rows a level at a time: GIVEN a record half that
    /// counts its reads, WHEN the subgraph two hops around each seed is read, over the code half
    /// resolved whole and near, THEN the record half is read once per level for every node and
    /// edge the walk reads, and per id only for a file's members.
    #[test]
    fn a_neighbourhood_reads_the_record_half_in_batches_never_per_id() {
        let view = view_with_workflow();
        // (seed, batched reads, ids stepped from): one batch a level, plus one for a stepped
        // file's members, whose answered nodes are read together before the level they join.
        let cases = [("f1", 3, 2), ("src/a.rs", 3 + 1, 3), ("dec-1", 3, 3)];
        for (seed, batches, stepped) in cases {
            for near in [false, true] {
                let records = CountingRecords {
                    records: records(),
                    reads: Mutex::new(BTreeMap::new()),
                };
                let filter = ViewFilter::default();
                let g: Box<dyn ContextGraph + '_> = match near {
                    true => {
                        Box::new(Composed::near(&records, &view, filter, exact(), None).unwrap())
                    }
                    false => Box::new(Composed::new(&records, &view, filter, exact()).unwrap()),
                };
                g.subgraph(&[seed.to_string()], 2).unwrap();
                let reads = records.reads.lock().unwrap().clone();
                let want = BTreeMap::from([("members", stepped), ("rows", batches)]);
                assert_eq!(reads, want, "{seed} (near {near})");
            }
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
    fn the_honesty_label_says_the_answer_describes_the_worktree() {
        // (stamp commit, what the view describes of the worktree, behind, label)
        let cases = [
            (Some("c1"), Compared::Overlaid(0), None, "describes c1"),
            (None, Compared::Overlaid(0), None, "describes tree0"),
            (
                Some("c1"),
                Compared::Overlaid(2),
                None,
                "describes W (c1 + 2 files); groupings describe c1",
            ),
            (
                Some("c1"),
                Compared::Overlaid(1),
                None,
                "describes W (c1 + 1 file); groupings describe c1",
            ),
            (
                Some("c1"),
                Compared::Uncomparable,
                None,
                "describes c1; W cannot be compared",
            ),
            (
                Some("c1"),
                Compared::Overlaid(0),
                Some("advance in flight"),
                "describes c1; advance in flight",
            ),
            (
                Some("c1"),
                Compared::Overlaid(1),
                Some("advance in flight"),
                "describes W (c1 + 1 file); groupings describe c1; advance in flight",
            ),
        ];
        let records = records();
        for (commit, compared, behind, want) in cases {
            let view = view(commit);
            let honesty = Honesty {
                worktree: "W".into(),
                compared,
                behind: behind.map(str::to_string),
            };
            let g = composed(&records, &view, honesty);
            assert_eq!(g.stamp_label().to_string(), want);
        }
    }

    /// An overlaid view describes the worktree, so every hit answers: GIVEN a view that overlays
    /// one of the worktree's files, WHEN it is read by node, whole, walk and kind, THEN each read
    /// answers the overlaid file's nodes, nothing is withheld and the label says it describes the
    /// worktree.
    #[test]
    fn an_overlaid_view_answers_every_hit_and_withholds_none() {
        let (records, view) = (records(), view(Some("c1")));
        let g = composed(&records, &view, honest(Compared::Overlaid(1)));
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
                ids.iter().any(|id| id == "src/b.rs::compute"),
                "{read} answers src/b.rs::compute: {ids:?}"
            );
        }
        let label = g.stamp_label();
        assert_eq!(label.withheld, 0, "{label:?}");
        assert_eq!(
            label.to_string(),
            "describes W (c1 + 1 file); groupings describe c1"
        );
    }

    /// [`view`] with the workflow definition's stage and a community grouping `src/a.rs::run`.
    fn view_with_workflow() -> Grouped {
        let mut grouped = view(Some("c1"));
        let workflow = FileFacts {
            path: ".rigger/workflow.yml".into(),
            lang: "yaml".into(),
            half: Half::Workflow,
            wf_nodes: vec![WfNode {
                kind: KIND_STAGE.into(),
                id: "stage:implement".into(),
                title: "implement".into(),
            }],
            ..Default::default()
        };
        let mut files: Vec<FileFacts> = grouped
            .facts
            .files()
            .filter_map(|f| grouped.facts.file(f.path).map(Cow::into_owned))
            .collect();
        files.push(workflow);
        grouped.facts = FactsView::new(grouped.facts.stamp().clone(), files);
        grouped.groupings.communities = vec![Cluster {
            id: COMMUNITY.into(),
            label: "run".into(),
            resolution: 1.0,
            hash: "h".into(),
            members: vec!["src/a.rs::run".into()],
        }];
        grouped
    }

    const COMMUNITY: &str = "community/1/0";

    /// A whole read that consumes the graph moves its code half into the answer instead of
    /// copying it: GIVEN each worktree comparison (exact, overlaid, none possible), WHEN the
    /// graph is read whole by consuming it and, on a second open, by borrowing it, THEN both
    /// answer the same graph and label.
    #[test]
    fn a_consuming_whole_read_answers_the_borrowing_one() {
        // (case, the comparison)
        let cases = [
            ("exact", Compared::Overlaid(0)),
            ("overlaid", Compared::Overlaid(1)),
            ("no comparison", Compared::Uncomparable),
        ];
        let (records, view) = (records(), view_with_workflow());
        for (case, compared) in cases {
            for near in [false, true] {
                let honesty = || honest(compared);
                let open = || -> Box<dyn ContextGraph + '_> {
                    let filter = ViewFilter::default();
                    match near {
                        true => Box::new(
                            Composed::near(&records, &view, filter, honesty(), None).unwrap(),
                        ),
                        false => Box::new(composed(&records, &view, honesty())),
                    }
                };
                // Neither a node nor an edge compares, so the two read as their debug forms.
                let borrowed = open();
                let want = format!("{:#?}", (borrowed.whole().unwrap(), borrowed.stamp_label()));
                let read = open().into_whole().unwrap();
                let got = format!("{:#?}", (read.graph, read.label));
                assert_eq!(got, want, "{case} (near {near})");
            }
        }
    }

    /// A located definition's end is read from the facts the graph was read from: GIVEN the facts
    /// of `src/a.rs` bounding `run` from line 1 to line 2, WHEN the near read is asked the end of
    /// a site at each worktree comparison, THEN a read describing the worktree answers the facts'
    /// bound for the definition at its name, kind and line, and none for a site no definition
    /// sits at, while a read withholding its code facts answers none.
    #[test]
    fn a_located_definition_s_end_is_read_from_the_facts() {
        // (case, the comparison, the site's id, kind and line, the end)
        let cases = [
            (
                "the facts' bound",
                Compared::Overlaid(0),
                "src/a.rs::run",
                "function",
                1,
                Some(2),
            ),
            (
                "another line",
                Compared::Overlaid(0),
                "src/a.rs::run",
                "function",
                2,
                None,
            ),
            (
                "another kind",
                Compared::Overlaid(0),
                "src/a.rs::run",
                "struct",
                1,
                None,
            ),
            (
                "another path",
                Compared::Overlaid(0),
                "src/a.rs::x::run",
                "function",
                1,
                None,
            ),
            (
                "no comparison",
                Compared::Uncomparable,
                "src/a.rs::run",
                "function",
                1,
                None,
            ),
        ];
        let (records, view) = (records(), view(Some("c1")));
        for (case, compared, id, kind, line, want) in cases {
            let near = Composed::near(
                &records,
                &view,
                ViewFilter::default(),
                honest(compared),
                None,
            );
            let site = EntitySite {
                id: id.into(),
                kind: kind.into(),
                file: Some("src/a.rs".into()),
                line,
                degree: 0,
                attrs: BTreeMap::from([("name".to_string(), "run".to_string())]),
                unresolved: Vec::new(),
                doc_comment: None,
                referrers: Vec::new(),
            };
            let got = near.unwrap().definition_end(&site).unwrap();
            assert_eq!(got, want, "{case}");
        }
    }

    /// A worktree's lines as a table: each file's span with the raw lines it holds there.
    struct Lines(Vec<(&'static str, (u32, u32), &'static str)>);

    impl SourceLines for Lines {
        fn lines(&self, path: &str, span: (u32, u32)) -> Option<String> {
            (self.0.iter())
                .find(|(file, at, _)| *file == path && *at == span)
                .map(|(_, _, raw)| raw.to_string())
        }
    }

    /// An extractor whose doc text names the language it read the span's lines as; it extracts as
    /// [`crate::test_support::NoDocRule`] does.
    struct LangMarked;

    impl Extract for LangMarked {
        fn fingerprint(&self) -> u64 {
            crate::test_support::NoDocRule.fingerprint()
        }

        fn extract(&self, entry: &Entry, bytes: &[u8]) -> FileFacts {
            crate::test_support::NoDocRule.extract(entry, bytes)
        }

        fn doc_text(&self, lang: &str, raw: &str) -> String {
            format!("{lang}: {raw}")
        }
    }

    /// A node's doc is its facts' doc span read from the worktree: GIVEN the facts of `src/a.rs`
    /// documenting the file on lines 1-2, `run` at line 4 on line 3, two `helper`s and a test twin
    /// at line 8 and `gone` at line 14 on a span the worktree does not hold, and `src/b.rs` with no
    /// doc, WHEN the near read is asked each node's doc, THEN a read describing the worktree gives
    /// the extractor's text of the span's lines - the file's, or the widest production definition's
    /// at the node's name, kind and line - and none for a site no definition sits at, a nameless
    /// node, a definition or file with no doc, a span the worktree does not hold, a record node or
    /// a file the view holds no facts of though the worktree holds its lines at (1, 2),
    /// while a read withholding its code facts, or opened with no doc reader, gives none.
    #[test]
    fn a_node_s_doc_is_its_facts_span_read_from_the_worktree() {
        let documented = |local: &str, line, end_line, doc, test| Def {
            line,
            end_line,
            doc: Some(doc),
            test,
            ..def(local)
        };
        let a = FileFacts {
            doc: Some((1, 2)),
            ..rust(
                "src/a.rs",
                vec![
                    documented("run", 4, 5, (3, 3), false),
                    documented("helper", 8, 9, (7, 7), false),
                    documented("helper", 8, 12, (6, 7), false),
                    documented("helper", 8, 20, (5, 5), true),
                    documented("gone", 14, 15, (13, 13), false),
                ],
                Vec::new(),
            )
        };
        let b = rust("src/b.rs", vec![def("compute")], Vec::new());
        let mut view = view(Some("c1"));
        view.facts = FactsView::new(view.facts.stamp().clone(), vec![a, b]);
        let lines = Lines(vec![
            ("src/a.rs", (1, 2), "//! A.\n//! More."),
            ("src/a.rs", (3, 3), "/// Runs."),
            ("src/a.rs", (5, 5), "/// Tests."),
            ("src/a.rs", (6, 7), "/// Helps\n/// widely."),
            ("src/a.rs", (7, 7), "/// Helps."),
            ("src/c.rs", (1, 2), "//! C."),
        ]);
        let reader = DocReader {
            lines: &lines,
            extract: &LangMarked,
        };
        let records = records();
        let read = |compared, docs| {
            Composed::near(
                &records,
                &view,
                ViewFilter::default(),
                honest(compared),
                docs,
            )
            .unwrap()
        };
        let held = |id: &str| -> Node {
            let graph = read(Compared::Overlaid(0), None).subgraph(&[id.into()], 0);
            graph
                .unwrap()
                .nodes
                .into_iter()
                .find(|n| n.id == id)
                .expect(id)
        };
        let entity = |id: &str, name: &str, kind: &str, line: &str| {
            let attrs = [("name", name), ("kind", kind), ("line", line)];
            node(id, KIND_CODE_ENTITY, &attrs)
        };
        let run = held("src/a.rs::run");
        // (case, the comparison, whether the read has a doc reader, the node, its doc)
        let cases = [
            (
                "a definition's doc",
                Compared::Overlaid(0),
                true,
                run.clone(),
                Some("rust: /// Runs."),
            ),
            (
                "the file's doc",
                Compared::Overlaid(0),
                true,
                held("src/a.rs"),
                Some("rust: //! A.\n//! More."),
            ),
            (
                "an overlaid read",
                Compared::Overlaid(1),
                true,
                run.clone(),
                Some("rust: /// Runs."),
            ),
            (
                "the widest production definition at its line",
                Compared::Overlaid(0),
                true,
                entity("src/a.rs::helper", "helper", "function", "8"),
                Some("rust: /// Helps\n/// widely."),
            ),
            (
                "another line",
                Compared::Overlaid(0),
                true,
                entity("src/a.rs::run", "run", "function", "5"),
                None,
            ),
            (
                "another kind",
                Compared::Overlaid(0),
                true,
                entity("src/a.rs::run", "run", "struct", "4"),
                None,
            ),
            (
                "a nameless node",
                Compared::Overlaid(0),
                true,
                node("src/a.rs::run", KIND_CODE_ENTITY, &[("line", "4")]),
                None,
            ),
            (
                "a definition with no doc",
                Compared::Overlaid(0),
                true,
                held("src/b.rs::compute"),
                None,
            ),
            (
                "a file with no doc",
                Compared::Overlaid(0),
                true,
                held("src/b.rs"),
                None,
            ),
            (
                "a span the worktree does not hold",
                Compared::Overlaid(0),
                true,
                entity("src/a.rs::gone", "gone", "function", "14"),
                None,
            ),
            (
                "a record node",
                Compared::Overlaid(0),
                true,
                node("dec-1", KIND_DECISION, &[("title", "keep it")]),
                None,
            ),
            (
                "a file the view holds no facts of",
                Compared::Overlaid(0),
                true,
                node("src/c.rs", KIND_FILE, &[]),
                None,
            ),
            (
                "no comparison",
                Compared::Uncomparable,
                true,
                run.clone(),
                None,
            ),
            (
                "no doc reader",
                Compared::Overlaid(0),
                false,
                run.clone(),
                None,
            ),
        ];
        for (case, compared, docs, node, want) in cases {
            let got = read(compared, docs.then_some(reader)).doc(&node);
            assert_eq!(got.as_deref(), want, "{case}");
        }
    }

    /// The edges read at an id are the whole graph's edges at it: GIVEN a decision's GOVERNS
    /// recorded under an alias and a design doc's GOVERNS the code half holds, each to a doc a
    /// concept groups, and each worktree comparison, WHEN each id the whole graph holds is read
    /// one hop out (its edges out and in), over the code half resolved whole and near, THEN the
    /// edges at it are the whole graph's edges at it, the REALIZES attachments among them.
    #[test]
    fn the_edges_read_at_an_id_are_the_whole_graphs_edges_at_it() {
        let mut view = view_with_workflow();
        let mut files: Vec<FileFacts> = (view.facts.files())
            .filter_map(|f| view.facts.file(f.path).map(Cow::into_owned))
            .collect();
        files.push(FileFacts {
            path: "docs/e.md".into(),
            lang: "markdown".into(),
            half: Half::Design,
            doc_concepts: vec![DocConcept {
                kind: KIND_DESIGN_DOC.into(),
                id: "docs/e.md".into(),
                title: "E".into(),
            }],
            doc_links: vec![DocLink {
                from: "docs/e.md".into(),
                rel: REL_GOVERNS.into(),
                to: "docs/d.md".into(),
            }],
            ..Default::default()
        });
        view.facts = FactsView::new(view.facts.stamp().clone(), files);
        let (mut rows, mut aliases) = record_rows();
        rows.nodes.push(node("dec-2", KIND_DECISION, &[]));
        rows.edges.push(edge("dec-2", "the-doc", REL_GOVERNS));
        aliases.push(("the-doc".into(), "docs/d.md".into()));
        let records = Indexed::new(rows, aliases);
        // (case, the comparison)
        let cases = [
            ("exact", Compared::Overlaid(0)),
            ("overlaid", Compared::Overlaid(1)),
            ("no comparison", Compared::Uncomparable),
        ];
        for (case, compared) in cases {
            for near in [false, true] {
                let filter = ViewFilter::default();
                let honesty = || honest(compared);
                let g: Box<dyn ContextGraph + '_> = match near {
                    true => {
                        Box::new(Composed::near(&records, &view, filter, honesty(), None).unwrap())
                    }
                    false => Box::new(composed(&records, &view, honesty())),
                };
                let whole = ContextGraph::whole(&composed(&records, &view, honesty())).unwrap();
                for id in ids(&whole.nodes) {
                    let at =
                        |edges: &[Edge]| -> Vec<(String, String, crate::contextgraph::EdgeTerm)> {
                            (edges.iter())
                                .filter(|e| e.from == id || e.to == id)
                                .map(|e| (e.from.clone(), e.to.clone(), e.rel.clone()))
                                .collect()
                        };
                    let got = at(&g.subgraph(&[id.to_string()], 1).unwrap().edges);
                    assert_eq!(got, at(&whole.edges), "{case} (near {near}) at {id}");
                }
            }
        }
        // Both attachments are the whole graph's, so the reads above are asked for them.
        let whole = ContextGraph::whole(&composed(&records, &view, exact())).unwrap();
        for from in ["dec-2", "docs/e.md"] {
            assert!(
                has_edge(&whole.edges, from, CONCEPT, REL_REALIZES),
                "{from}"
            );
        }
    }

    /// Every code-half node names the file its facts came from by one rule, which the
    /// cannot-compare arm reads: GIVEN the worktree overlaid or not comparable, WHEN each kind of
    /// code-half node is resolved and read in the whole graph, borrowed and consumed, over the
    /// code half resolved whole and near, THEN an overlaid view withholds nothing, and one not
    /// comparable withholds and counts exactly the nodes whose facts came from a file, a grouping
    /// by its members' files, and nothing else: neither a record naming a file (a record is never
    /// stale) nor a link end naming no file (no file withholds it).
    #[test]
    fn an_uncomparable_worktree_withholds_exactly_the_nodes_whose_facts_came_from_a_file() {
        let code = [
            "src/a.rs",
            "src/a.rs::run",
            "docs/d.md",
            "stage:implement",
            COMMUNITY,
            CONCEPT,
        ];
        // A record naming a file, and a link end the design doc names no file by.
        let probes = [code.as_slice(), &["src/gone.rs", "glossary"]].concat();
        // (case, the comparison, the probes withheld)
        let cases: [(&str, Compared, &[&str]); 2] = [
            ("overlaid", Compared::Overlaid(1), &[]),
            ("no comparison", Compared::Uncomparable, &code),
        ];
        let (mut rows, aliases) = record_rows();
        rows.nodes
            .push(node("src/gone.rs", KIND_ARTIFACT, &[("noted", "gone")]));
        let records = Indexed::new(rows, aliases);
        let mut view = view_with_workflow();
        let mut files: Vec<FileFacts> = (view.facts.files())
            .filter_map(|f| view.facts.file(f.path).map(Cow::into_owned))
            .collect();
        let doc = files.iter_mut().find(|f| f.path == "docs/d.md").unwrap();
        doc.doc_links.push(DocLink {
            from: "docs/d.md".into(),
            rel: REL_SPECIFIES.into(),
            to: "glossary".into(),
        });
        view.facts = FactsView::new(view.facts.stamp().clone(), files);
        for (case, compared, withheld) in cases {
            for near in [false, true] {
                let honesty = honest(compared);
                let g: Box<dyn ContextGraph + '_> = if near {
                    Box::new(
                        Composed::near(&records, &view, ViewFilter::default(), honesty, None)
                            .unwrap(),
                    )
                } else {
                    Box::new(composed(&records, &view, honesty))
                };
                let answered: Vec<&str> = probes
                    .iter()
                    .copied()
                    .filter(|id| g.resolve(id).unwrap().is_some())
                    .collect();
                let want: Vec<&str> = probes
                    .iter()
                    .copied()
                    .filter(|id| !withheld.contains(id))
                    .collect();
                assert_eq!(answered, want, "{case} (near {near}): the probes answered");
                assert_eq!(
                    g.stamp_label().withheld,
                    withheld.len(),
                    "{case} (near {near}): the probes withheld are counted"
                );
                let held = |g: &Graph| -> Vec<&str> {
                    let held = |id: &&str| g.nodes.iter().any(|n| n.id == *id);
                    probes.iter().copied().filter(held).collect()
                };
                let whole = g.whole().unwrap();
                assert_eq!(held(&whole), want, "{case} (near {near}): the whole holds");
                let read = g.into_whole().unwrap();
                assert_eq!(
                    held(&read.graph),
                    want,
                    "{case} (near {near}): the consumed whole holds"
                );
                assert_eq!(
                    held(&read.withheld),
                    withheld.to_vec(),
                    "{case} (near {near}): the withheld part holds the rest"
                );
                let edges = |graphs: &[&Graph]| {
                    let mut edges: Vec<String> = (graphs.iter())
                        .flat_map(|g| g.edges.iter().map(|e| format!("{e:?}")))
                        .collect();
                    edges.sort();
                    edges
                };
                let agreeing = composed(&records, &view, exact());
                assert_eq!(
                    edges(&[&read.graph, &read.withheld]),
                    edges(&[&Neighbors::whole(&agreeing).unwrap()]),
                    "{case} (near {near}): the edges answered and dropped are the whole's"
                );
            }
        }
    }

    /// A walk counts the withheld nodes it reaches and no other: GIVEN a worktree that cannot be
    /// compared, WHEN the walk starts at each seed to each depth, THEN its label counts what the
    /// dash counts over the whole read's withheld part, so a withheld node one hop past the last
    /// level (`compute`, which `f1` is about, at depth 0) is never counted.
    #[test]
    fn a_walk_counts_the_withheld_nodes_it_reaches_and_no_other() {
        let (records, view) = (records(), view(Some("c1")));
        let differing = || honest(Compared::Uncomparable);
        let read = Box::new(composed(&records, &view, differing()))
            .into_whole()
            .unwrap();
        // (seed, depth, the withheld nodes the walk reaches)
        let cases: [(&str, i64, usize); 7] = [
            ("f1", 0, 0),
            ("f1", 1, 1),
            ("docs/d.md", 0, 1),
            ("docs/d.md", 1, 1),
            ("dec-1", 1, 2),
            ("dec-1", 2, 2),
            ("src/a.rs", 2, 2),
        ];
        for (seed, depth, want) in cases {
            let g = composed(&records, &view, differing());
            let seeds = [seed.to_string()];
            g.subgraph(&seeds, depth).unwrap();
            let counted = (
                g.stamp_label().withheld,
                crate::contextgraph::query::withheld_reached(
                    &read.graph,
                    &read.withheld,
                    &seeds,
                    depth,
                ),
            );
            assert_eq!(
                counted,
                (want, want),
                "{seed} at depth {depth}: (walk, dash)"
            );
        }
    }
}
