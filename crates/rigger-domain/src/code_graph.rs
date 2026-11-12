//! The code graph adapter: applies the resolver to a code view and returns the graph's nodes and
//! edges in today's shapes. Code files become `file` nodes, definitions `code-entity` nodes with
//! their kind, language, line and name, unresolved names placeholder nodes, and proven product
//! definitions carry their proof count and evidence. Design and workflow facts become their
//! intent nodes; a link's end no fact names becomes an `artifact` node. A node a fact gives a
//! specific kind takes it over `artifact`, whatever order the files are read in.
//!
//! The view filter decides test code: excluded by default, shown beside product code, or shown
//! alone with the nodes its edges reach. A node only test code asserts carries `test=true`, and a
//! node in a test file is test code whatever fact names it.
//! Product edges are the same under every filter.

use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};

use crate::code_facts::{FileFacts, Half, Mention};
use crate::code_view::{CodeView, Tests, ViewFilter};
use crate::contextgraph::query::leaf_of;
use crate::contextgraph::{Edge, Graph, Node, KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_FILE};
use crate::resolve::{Entity, ProofCall, Resolver};

/// The attribute naming the file a design or workflow node's facts came from.
pub(crate) const DOC_ATTR: &str = "doc";

/// A view resolved into a graph under one filter.
pub struct CodeGraph<'v> {
    view: &'v dyn CodeView,
    resolver: Resolver<'v>,
    filter: ViewFilter,
}

/// The graph being built: each node once, by its place in the order its id was first asserted,
/// with whether a product fact asserts it, and each edge's tier and test-ness by its ends' places
/// and relation. The index holds the one copy of each id, which becomes the node's and its edges'
/// ids when the graph is put in id order, once.
#[derive(Default)]
pub(crate) struct Builder {
    index: FxHashMap<Box<str>, usize>,
    nodes: Vec<Placed>,
    edges: FxHashMap<(usize, &'static str, usize), (&'static str, bool)>,
}

/// A node being built: its kind, its attributes and whether a product fact asserts it.
struct Placed {
    kind: &'static str,
    attrs: BTreeMap<String, String>,
    product: bool,
}

/// What reading a file asserts, in the order read: each node with its kind, attributes and
/// test-ness, and each edge with its tier and test-ness. The whole graph's [`Builder`] folds every
/// assertion; a neighbourhood folds the ones that touch the ids it is asked for.
pub(crate) trait Asserts {
    /// Asserts the node `id` of `kind` with `attrs`.
    fn node(
        &mut self,
        id: &str,
        kind: &'static str,
        attrs: impl IntoIterator<Item = (&'static str, String)>,
        test: bool,
    );
    /// Asserts the edge `from` -`rel`-> `to` of `tier`.
    fn link(&mut self, from: &str, rel: &'static str, to: &str, tier: &'static str, test: bool);
}

impl Asserts for Builder {
    fn node(
        &mut self,
        id: &str,
        kind: &'static str,
        attrs: impl IntoIterator<Item = (&'static str, String)>,
        test: bool,
    ) {
        self.place(id, kind, attrs, test);
    }
    fn link(&mut self, from: &str, rel: &'static str, to: &str, tier: &'static str, test: bool) {
        Builder::link(self, from, rel, to, tier, test);
    }
}

/// Whether a node the filter `tests` is applied to is shown: a product node unless tests are
/// shown alone, a test node unless tests are excluded, and, shown alone, a product node a test
/// edge reaches.
pub(crate) fn shown(tests: Tests, product: bool, reached: bool) -> bool {
    match tests {
        Tests::Exclude => product,
        Tests::Include => true,
        Tests::Only => !product || reached,
    }
}

impl Builder {
    /// Asserts the node `id` of `kind` with `attrs`; a specific kind replaces `artifact`. Returns
    /// the node's place.
    fn place(
        &mut self,
        id: &str,
        kind: &'static str,
        attrs: impl IntoIterator<Item = (&'static str, String)>,
        test: bool,
    ) -> usize {
        let at = match self.index.get(id) {
            Some(&at) => at,
            None => {
                self.index.insert(id.into(), self.nodes.len());
                self.nodes.push(Placed {
                    kind,
                    attrs: BTreeMap::new(),
                    product: false,
                });
                self.nodes.len() - 1
            }
        };
        let node = &mut self.nodes[at];
        if node.kind == KIND_ARTIFACT && kind != KIND_ARTIFACT {
            node.kind = kind;
        }
        node.attrs
            .extend(attrs.into_iter().map(|(k, v)| (k.to_string(), v)));
        node.product |= !test;
        at
    }

    /// Asserts the edge `from` -`rel`-> `to` of `tier` and its ends; a product assertion of an
    /// edge replaces a test one.
    fn link(&mut self, from: &str, rel: &'static str, to: &str, tier: &'static str, test: bool) {
        let from = self.place(from, KIND_ARTIFACT, [], test);
        let to = self.place(to, KIND_ARTIFACT, [], test);
        let held = self.edges.entry((from, rel, to)).or_insert((tier, test));
        if held.1 && !test {
            *held = (tier, false);
        }
    }

    /// The node `id`, when one is asserted, carrying the proofs whose `evidence` lands on it.
    pub(crate) fn prove(&mut self, id: &str, evidence: &[String]) {
        if let Some(&at) = self.index.get(id) {
            prove(&mut self.nodes[at].attrs, evidence);
        }
    }

    /// The graph under `tests`: a node no product fact asserts is marked `test=true`. Excluded,
    /// test nodes and edges are dropped; included, all are kept; only, the test edges and the
    /// nodes test code asserts or those edges reach. Nodes come in id order, edges in ends then
    /// relation order.
    pub(crate) fn into_graph(self, tests: Tests) -> Graph {
        let Builder {
            index,
            nodes,
            edges,
        } = self;
        let mut ids = vec![String::new(); nodes.len()];
        for (id, at) in index {
            ids[at] = id.into();
        }
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        order.sort_unstable_by(|&a, &b| ids[a].cmp(&ids[b]));
        let mut rank = vec![0; nodes.len()];
        for (r, &at) in order.iter().enumerate() {
            rank[at] = r;
        }
        let mut edges: Vec<_> = edges
            .into_iter()
            .filter(|(_, (_, test))| tests.admits(*test))
            .collect();
        edges.sort_unstable_by_key(|&((from, rel, to), _)| (rank[from], rank[to], rel));
        let mut reached = vec![false; nodes.len()];
        for &((from, _, to), _) in &edges {
            reached[from] = true;
            reached[to] = true;
        }
        let edges = edges
            .into_iter()
            .map(|((from, rel, to), (tier, _))| Edge {
                from: ids[from].clone(),
                to: ids[to].clone(),
                rel: rel.into(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: tier.into(),
            })
            .collect();
        let mut slots: Vec<Option<Placed>> = nodes.into_iter().map(Some).collect();
        let nodes = order
            .into_iter()
            .filter_map(|at| {
                let n = slots[at].take()?;
                if !shown(tests, n.product, reached[at]) {
                    return None;
                }
                let mut attrs = n.attrs;
                if !n.product {
                    attrs.insert("test".to_string(), "true".to_string());
                }
                Some(Node {
                    id: std::mem::take(&mut ids[at]),
                    kind: n.kind.to_string(),
                    attrs,
                })
            })
            .collect();
        Graph { nodes, edges }
    }
}

/// The edge `from` -`rel`-> `to` asserted to `b`, its end first asserted as a code entity when
/// it is a placeholder.
fn linked(
    b: &mut impl Asserts,
    from: &str,
    rel: &'static str,
    to: &str,
    tier: &'static str,
    test: bool,
) {
    if leaf_of(to).is_some() {
        b.node(to, KIND_CODE_ENTITY, [], test);
    }
    b.link(from, rel, to, tier, test);
}

/// A view resolved whole under one filter.
pub struct Resolved {
    /// Every node and edge the filter shows.
    pub graph: Graph,
    /// The test code entities a filter that excludes tests leaves out, in id order; none when
    /// tests are shown.
    pub hidden_tests: Vec<Node>,
    /// The test calls landing on a placeholder ([`Resolver::file_placeholder_calls`]) as
    /// (leaf, caller) pairs, unordered, while a filter that excludes tests leaves them out; none
    /// when tests are shown.
    pub placeholder_calls: Vec<(String, String)>,
}

/// The attributes a code entity node carries.
fn entity_attrs(facts: &FileFacts, e: Entity) -> [(&'static str, String); 4] {
    [
        ("kind", e.kind),
        ("lang", facts.lang.clone()),
        ("line", e.line.to_string()),
        ("name", e.name),
    ]
}

impl<'v> CodeGraph<'v> {
    pub fn new(view: &'v dyn CodeView, filter: ViewFilter) -> Self {
        Self {
            view,
            resolver: Resolver::new(view),
            filter,
        }
    }

    /// The resolver the graph is built by.
    pub fn resolver(&self) -> &Resolver<'v> {
        &self.resolver
    }

    /// The resolver's proof calls ([`Resolver::into_proof_calls`]), moved out with it.
    pub fn into_proof_calls(self) -> Vec<ProofCall> {
        self.resolver.into_proof_calls()
    }

    /// Every node and edge of the view under the filter, nodes in id order and edges in ends then
    /// relation order. Code edges carry no log position or valid time.
    pub fn whole(&self) -> Graph {
        self.resolved().graph
    }

    /// [`Self::whole`] with the test entities it hides, read in one pass over the files, which
    /// also settles the resolver's proofs.
    pub fn resolved(&self) -> Resolved {
        let tests = self.filter.tests != Tests::Exclude;
        let mut b = Builder::default();
        let mut hidden_tests = Vec::new();
        let mut placeholder_calls = Vec::new();
        // A view holding its proof calls settled is read, not walked.
        let walk = !self.resolver.proofs_settled();
        let mut proofs = Vec::new();
        for f in self.view.files() {
            let Some(facts) = self.view.file(f.path) else {
                continue;
            };
            if f.half == Half::Code {
                self.code(&facts, tests, &mut b, Some(&mut hidden_tests));
                if walk {
                    proofs.extend(self.resolver.file_proofs(&facts));
                }
                if !tests {
                    placeholder_calls.extend(self.resolver.file_placeholder_calls(&facts));
                }
            }
            self.intent(&facts, &mut b);
        }
        if walk {
            self.resolver.settle_proofs(proofs);
        }
        for (id, evidence) in self.resolver.proofs() {
            b.prove(&id, &evidence);
        }
        hidden_tests.sort_by(|a, b| a.id.cmp(&b.id));
        Resolved {
            graph: b.into_graph(self.filter.tests),
            hidden_tests,
            placeholder_calls,
        }
    }

    /// The file node, entities, placeholders and edges of the code file `facts`, asserted to `b`.
    /// The test entities it hides go to `hidden`, when one is given, when tests are not shown.
    pub(crate) fn code(
        &self,
        facts: &FileFacts,
        tests: bool,
        b: &mut impl Asserts,
        hidden: Option<&mut Vec<Node>>,
    ) {
        self.code_nodes(facts, tests, b, hidden);
        self.code_mentions(facts, facts.mentions.iter(), tests, b);
        self.code_declared(facts, tests, b);
    }

    /// The file node and entities of the code file `facts`, asserted to `b`: what [`Self::code`]
    /// asserts first.
    pub(crate) fn code_nodes(
        &self,
        facts: &FileFacts,
        tests: bool,
        b: &mut impl Asserts,
        mut hidden: Option<&mut Vec<Node>>,
    ) {
        let file_test = self.resolver.is_test_file(&facts.path);
        if tests || !file_test {
            b.node(
                &facts.path,
                KIND_FILE,
                [("lang", facts.lang.clone())],
                file_test,
            );
            if facts.partial {
                b.node(
                    &facts.path,
                    KIND_FILE,
                    [("partial", "true".to_string())],
                    file_test,
                );
            }
        }
        for e in self.resolver.entities(facts, true) {
            if e.test && !tests {
                if let Some(hidden) = hidden.as_deref_mut() {
                    hidden.push(hidden_node(facts, e));
                }
                continue;
            }
            let test = e.test;
            b.node(
                &e.id.clone(),
                KIND_CODE_ENTITY,
                entity_attrs(facts, e),
                test,
            );
        }
    }

    /// The edges and placeholders of `mentions`, mentions of the code file `facts` in file order,
    /// asserted to `b` ([`Resolver::each_mention_link`]).
    pub(crate) fn code_mentions<'f>(
        &self,
        facts: &'f FileFacts,
        mentions: impl Iterator<Item = &'f Mention> + Clone,
        tests: bool,
        b: &mut impl Asserts,
    ) {
        (self.resolver).each_mention_link(facts, mentions, tests, |from, rel, to, tier, test| {
            linked(b, from, rel, to, tier, test)
        });
    }

    /// The module declarations' and definitions' `CONTAINS` edges of the code file `facts`,
    /// asserted to `b` ([`Resolver::each_declared_link`]).
    pub(crate) fn code_declared(&self, facts: &FileFacts, tests: bool, b: &mut impl Asserts) {
        (self.resolver).each_declared_link(facts, tests, |from, rel, to, tier, test| {
            linked(b, from, rel, to, tier, test)
        });
    }

    /// The design and workflow nodes and edges of `facts`, asserted to `b`.
    pub(crate) fn intent(&self, facts: &FileFacts, b: &mut impl Asserts) {
        let (nodes, links) = self.resolver.intent(facts);
        for n in nodes {
            b.node(
                &n.id,
                n.kind,
                [("title", n.title), (DOC_ATTR, n.doc)],
                n.test,
            );
        }
        for l in links {
            b.link(&l.from, l.rel, &l.to, l.tier, l.test);
        }
    }

    /// The test entities of every code file, which a filter that excludes tests hides, in id
    /// order; none when tests are shown.
    pub(crate) fn hidden_tests(&self) -> Vec<Node> {
        if self.filter.tests != Tests::Exclude {
            return Vec::new();
        }
        let mut hidden = Vec::new();
        for f in self.view.files().filter(|f| f.half == Half::Code) {
            if let Some(facts) = self.view.file(f.path) {
                for e in self.resolver.entities(&facts, true) {
                    if e.test {
                        hidden.push(hidden_node(&facts, e));
                    }
                }
            }
        }
        hidden.sort_by(|a, b| a.id.cmp(&b.id));
        hidden
    }

    /// The test entities named `name` a filter that excludes tests hides, in id order: those of
    /// [`Self::hidden_tests`] named `name`, read from the code files defining a leaf `name` alone.
    pub(crate) fn hidden_tests_named(&self, name: &str) -> Vec<Node> {
        if self.filter.tests != Tests::Exclude {
            return Vec::new();
        }
        let files: BTreeSet<&str> = (self.view.defs_named(name))
            .filter(|d| d.file.half == Half::Code)
            .map(|d| d.file.path)
            .collect();
        let mut hidden = Vec::new();
        for path in files {
            if let Some(facts) = self.view.file(path) {
                for e in self.resolver.entities(&facts, true) {
                    if e.test && e.name == name {
                        hidden.push(hidden_node(&facts, e));
                    }
                }
            }
        }
        hidden.sort_by(|a, b| a.id.cmp(&b.id));
        hidden
    }
}

/// The test entity `e` of `facts` as the node a filter that excludes tests hides.
fn hidden_node(facts: &FileFacts, e: Entity) -> Node {
    Node {
        id: e.id.clone(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: entity_attrs(facts, e)
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .chain([("test".to_string(), "true".to_string())])
            .collect(),
    }
}

/// A node's `attrs` carrying the proofs whose `evidence` lands on it: their count and their
/// evidence.
pub(crate) fn prove(attrs: &mut BTreeMap<String, String>, evidence: &[String]) {
    attrs.insert("proven_by".to_string(), evidence.len().to_string());
    let evidence = serde_json::to_string(evidence).expect("strings serialize");
    attrs.insert("proof_evidence".to_string(), evidence);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::tests::fixture;

    /// The graph of the resolver's fixture under `tests`, as `node <id>[ test]` and
    /// `edge <from> -<rel>/<tier>-> <to>` lines.
    fn lines(tests: Tests) -> Vec<String> {
        let view = fixture();
        let g = CodeGraph::new(&view, ViewFilter { tests }).whole();
        let nodes = g.nodes.iter().map(|n| {
            let mark = if n.attrs.contains_key("test") {
                " test"
            } else {
                ""
            };
            format!("node {}{mark}", n.id)
        });
        let edges = g
            .edges
            .iter()
            .map(|e| format!("edge {} -{}/{}-> {}", e.from, e.rel, e.tier, e.to));
        nodes.chain(edges).collect()
    }

    /// Each row: a filter, lines its graph holds, and text no line of it holds.
    #[test]
    fn the_filter_shows_product_code_tests_beside_it_or_tests_alone() {
        let cases: &[(Tests, &[&str], &[&str])] = &[
            (
                Tests::Exclude,
                &[
                    "node src/a.rs::run",
                    "node src/sub.rs::sub_fn",
                    "edge src/a.rs::run -CALLS/extracted-> src/a.rs::helper",
                ],
                &[" test", "tests/it.rs", "src/a.rs::tests", "src/a_tests"],
            ),
            (
                Tests::Include,
                &[
                    "node src/a.rs::run",
                    "node tests/it.rs test",
                    "node src/a.rs::tests::checks test",
                    "node tests/it.rs#why test",
                    "node src/a_tests.rs test",
                    "edge src/a.rs::tests::checks -CALLS/extracted-> src/a.rs::run",
                    "edge src/a.rs::run -CALLS/extracted-> src/a.rs::helper",
                    "edge docs/d.md -SPECIFIES/extracted-> tests/it.rs",
                ],
                &[],
            ),
            (
                Tests::Only,
                &[
                    "node tests/it.rs test",
                    "node src/a.rs::run",
                    "edge src/a.rs::tests::checks -CALLS/extracted-> src/a.rs::run",
                    "edge docs/d.md -SPECIFIES/extracted-> tests/it.rs",
                ],
                &["edge src/a.rs::run -", "node src/c.rs", "-> src/b.rs"],
            ),
        ];
        for (tests, present, absent) in cases {
            let got = lines(*tests);
            for want in *present {
                assert!(
                    got.iter().any(|l| l == want),
                    "{tests:?}: {want} not in {got:#?}"
                );
            }
            for unwanted in *absent {
                assert!(
                    !got.iter().any(|l| l.contains(unwanted)),
                    "{tests:?}: {unwanted} in {got:#?}"
                );
            }
        }
    }
}
