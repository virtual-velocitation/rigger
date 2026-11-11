//! The code graph adapter: applies the resolver to a code view and returns the graph's nodes and
//! edges in today's shapes. Code files become `file` nodes, definitions `code-entity` nodes with
//! their kind, language, line and name, unresolved names placeholder nodes, and proven product
//! definitions carry their proof count and evidence. Design and workflow facts become their
//! intent nodes; a link's end no fact names becomes an `artifact` node. A node a fact gives a
//! specific kind takes it over `artifact`, whatever order the files are read in.
//!
//! The view filter decides test code: excluded by default, shown beside product code, or shown
//! alone with the nodes its edges reach. A node only test code asserts carries `test=true`.
//! Product edges are the same under every filter.

use std::collections::{BTreeMap, BTreeSet};

use crate::code_facts::{FileFacts, Half};
use crate::code_view::{CodeView, Tests, ViewFilter};
use crate::contextgraph::query::leaf_of;
use crate::contextgraph::{Edge, Graph, Node, KIND_ARTIFACT, KIND_CODE_ENTITY, KIND_FILE};
use crate::resolve::{Link, Resolver};

/// A view resolved into a graph under one filter.
pub struct CodeGraph<'v> {
    view: &'v dyn CodeView,
    resolver: Resolver<'v>,
    filter: ViewFilter,
}

/// The graph being built: nodes by id, the ids a product fact asserts, and each edge's tier and
/// test-ness by its ends and relation.
#[derive(Default)]
struct Builder {
    nodes: BTreeMap<String, Node>,
    product: BTreeSet<String>,
    edges: BTreeMap<(String, &'static str, String), (&'static str, bool)>,
}

impl Builder {
    /// Asserts the node `id` of `kind` with `attrs`; a specific kind replaces `artifact`.
    fn node<const N: usize>(
        &mut self,
        id: &str,
        kind: &str,
        attrs: [(&str, String); N],
        test: bool,
    ) {
        let node = self.nodes.entry(id.to_string()).or_insert_with(|| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::new(),
        });
        if node.kind == KIND_ARTIFACT {
            node.kind = kind.to_string();
        }
        node.attrs
            .extend(attrs.into_iter().map(|(k, v)| (k.to_string(), v)));
        if !test {
            self.product.insert(id.to_string());
        }
    }

    /// Asserts `link` and its ends; a product assertion of an edge replaces a test one.
    fn link(&mut self, link: Link) {
        self.node(&link.from, KIND_ARTIFACT, [], link.test);
        self.node(&link.to, KIND_ARTIFACT, [], link.test);
        let held = self
            .edges
            .entry((link.from, link.rel, link.to))
            .or_insert((link.tier, link.test));
        if held.1 && !link.test {
            *held = (link.tier, false);
        }
    }

    /// The graph: every node and edge, a node no product fact asserts marked `test=true`; with
    /// `only`, the test edges and the nodes test code asserts or those edges reach.
    fn graph(self, only: bool) -> Graph {
        let Builder {
            nodes,
            product,
            edges,
        } = self;
        let edges: Vec<_> = edges
            .into_iter()
            .filter(|(_, (_, test))| !only || *test)
            .collect();
        let reached: BTreeSet<&str> = edges
            .iter()
            .flat_map(|((from, _, to), _)| [from.as_str(), to.as_str()])
            .collect();
        let nodes = nodes
            .into_values()
            .filter(|n| !only || !product.contains(&n.id) || reached.contains(n.id.as_str()))
            .map(|mut n| {
                if !product.contains(&n.id) {
                    n.attrs.insert("test".to_string(), "true".to_string());
                }
                n
            })
            .collect();
        let edges = edges
            .into_iter()
            .map(|((from, rel, to), (tier, _))| Edge {
                from,
                to,
                rel: rel.to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
                tier: tier.to_string(),
            })
            .collect();
        Graph { nodes, edges }
    }
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

    /// Every node and edge of the view under the filter, nodes in id order and edges in ends then
    /// relation order. Code edges carry no log position or valid time.
    pub fn whole(&self) -> Graph {
        let tests = self.filter.tests != Tests::Exclude;
        let mut b = Builder::default();
        for f in self.view.files() {
            let Some(facts) = self.view.file(f.path) else {
                continue;
            };
            if f.half == Half::Code {
                self.code(&facts, tests, &mut b);
            }
            let (nodes, links) = self.resolver.intent(&facts);
            for n in nodes {
                b.node(&n.id, n.kind, [("title", n.title), ("doc", n.doc)], false);
            }
            links.into_iter().for_each(|l| b.link(l));
        }
        for (id, evidence) in self.resolver.proofs() {
            if let Some(n) = b.nodes.get_mut(&id) {
                n.attrs
                    .insert("proven_by".to_string(), evidence.len().to_string());
                let evidence = serde_json::to_string(&evidence).expect("strings serialize");
                n.attrs.insert("proof_evidence".to_string(), evidence);
            }
        }
        b.graph(self.filter.tests == Tests::Only)
    }

    /// The file node, entities, placeholders and edges of the code file `facts`.
    fn code(&self, facts: &FileFacts, tests: bool, b: &mut Builder) {
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
        for e in self.resolver.entities(facts, tests) {
            let attrs = [
                ("kind", e.kind),
                ("lang", facts.lang.clone()),
                ("line", e.line.to_string()),
                ("name", e.name),
            ];
            b.node(&e.id, KIND_CODE_ENTITY, attrs, e.test);
        }
        for l in self.resolver.links(facts, tests) {
            if leaf_of(&l.to).is_some() {
                b.node(&l.to, KIND_CODE_ENTITY, [], l.test);
            }
            b.link(l);
        }
    }
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
                    "edge src/a.rs::tests::checks -CALLS/extracted-> src/a.rs::run",
                    "edge src/a.rs::run -CALLS/extracted-> src/a.rs::helper",
                ],
                &[],
            ),
            (
                Tests::Only,
                &[
                    "node tests/it.rs test",
                    "node src/a.rs::run",
                    "edge src/a.rs::tests::checks -CALLS/extracted-> src/a.rs::run",
                ],
                &["edge src/a.rs::run -", "node src/c.rs", "docs/d.md"],
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
