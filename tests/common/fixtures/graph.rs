//! Context-graph fixtures: nodes, edges, whole graphs and projection doubles.

use std::collections::BTreeMap;

use rigger::contextgraph::{
    CallEdge, CallGraph, Edge, Graph, Node, KIND_CODE_ENTITY, KIND_CONCEPT, KIND_DECISION,
    KIND_FILE, KIND_FINDING, KIND_LESSON, KIND_UNIT, REL_ABOUT, REL_CALLS, REL_GOVERNS,
    REL_REALIZES, REL_REFERENCES, TIER_EXTRACTED, TIER_INFERRED,
};

/// A `kind` node with no attributes.
pub fn plain(id: &str, kind: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: BTreeMap::new(),
    }
}

/// A `kind` node carrying `attrs`.
pub fn node_with_attrs(id: &str, kind: &str, attrs: &[(&str, &str)]) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// A `kind` node carrying the optional `title` (an ingested document) and `name` (a code entity)
/// attributes - the two a concept label is picked from, in that preference order.
pub fn entity_node(id: &str, kind: &str, title: Option<&str>, name: Option<&str>) -> Node {
    let mut n = plain(id, kind);
    if let Some(t) = title {
        n.attrs.insert("title".to_string(), t.to_string());
    }
    if let Some(nm) = name {
        n.attrs.insert("name".to_string(), nm.to_string());
    }
    n
}

/// A `kind` node carrying a `summary` attribute, or no attribute at all when `summary` is empty.
pub fn summarized_node(id: &str, kind: &str, summary: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: if summary.is_empty() {
            BTreeMap::new()
        } else {
            BTreeMap::from([("summary".to_string(), summary.to_string())])
        },
    }
}

/// A live `rel` edge `from -> to` at confidence `tier`.
pub fn edge(from: &str, to: &str, rel: &str, tier: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: tier.to_string(),
    }
}

/// An [`edge`] whose validity ends at `valid_to` (`None`: still live).
pub fn edge_valid_to(from: &str, to: &str, rel: &str, tier: &str, valid_to: Option<i64>) -> Edge {
    Edge {
        valid_to,
        ..edge(from, to, rel, tier)
    }
}

/// A directed-walk `CALLS` edge `from -> to`, marked as a recursion arc when `back`.
pub fn calls_edge(from: &str, to: &str, back: bool) -> CallEdge {
    CallEdge {
        edge: edge(from, to, REL_CALLS, TIER_INFERRED),
        back,
    }
}

/// A unit `hub` referencing `spokes` code entities `<hub>-s<i>`.
pub fn star_graph(hub: &str, spokes: usize) -> Graph {
    let mut nodes = vec![plain(hub, KIND_UNIT)];
    let mut edges = Vec::new();
    for i in 0..spokes {
        let spoke = format!("{hub}-s{i}");
        nodes.push(plain(&spoke, "code-entity"));
        edges.push(edge(hub, &spoke, REL_REFERENCES, TIER_EXTRACTED));
    }
    Graph { nodes, edges }
}

/// `len` unit nodes `n0..` joined into one `REFERENCES` chain `n0 -> n1 -> ...`.
pub fn chain_graph(len: usize) -> Graph {
    let nodes = (0..len)
        .map(|i| plain(&format!("n{i}"), KIND_UNIT))
        .collect();
    let edges = (0..len.saturating_sub(1))
        .map(|i| {
            edge(
                &format!("n{i}"),
                &format!("n{}", i + 1),
                REL_REFERENCES,
                TIER_EXTRACTED,
            )
        })
        .collect();
    Graph { nodes, edges }
}

/// A `(key, value)` pair list (a derivation's membership or labels) as a lookup map.
pub fn pair_map(pairs: &[(String, String)]) -> BTreeMap<String, String> {
    pairs.iter().cloned().collect()
}

/// The two graph reads a `Projection` double answers with nothing - an empty subgraph and an
/// unresolved mention - expanded inside that double's `impl Projection` block.
#[macro_export]
macro_rules! projection_reads_nothing {
    () => {
        fn subgraph(
            &self,
            _seed: &[String],
            _depth: i64,
        ) -> Result<rigger::contextgraph::Graph, rigger::contextgraph::Error> {
            Ok(rigger::contextgraph::Graph::default())
        }
        fn resolve(&self, _mention: &str) -> Result<Option<String>, rigger::contextgraph::Error> {
            Ok(None)
        }
    };
}

/// A `Projection` implementing only the trait's REQUIRED methods, each trivially, so every
/// provided method (`locate`, `calls`, ...) is the trait DEFAULT under test.
pub struct MinimalProjection;

impl rigger::contextgraph::Projection for MinimalProjection {
    fn apply(&self, _e: &rigger::eventstore::Event) -> Result<(), rigger::contextgraph::Error> {
        Ok(())
    }
    projection_reads_nothing!();
}

/// A `kind` node carrying its display `label`, when it has one.
pub fn labelled_node(id: &str, kind: &str, label: Option<&str>) -> Node {
    let mut n = plain(id, kind);
    if let Some(l) = label {
        n.attrs.insert("label".to_string(), l.to_string());
    }
    n
}

/// The id of spoke `i` of a many-spoke graph fixture: a code entity in `file`, zero-padded so the
/// ids sort in spoke order.
pub fn spoke_id(file: &str, i: usize) -> String {
    format!("{file}::s{i:05}")
}

/// Every `GOVERNS` edge in `graph_edges` as a sorted `(from, to, source, valid_from)` list.
pub fn governs(graph_edges: &[Edge]) -> Vec<(String, String, u64, i64)> {
    let mut out: Vec<_> = graph_edges
        .iter()
        .filter(|e| e.rel == rigger::contextgraph::REL_GOVERNS)
        .map(|e| (e.from.clone(), e.to.clone(), e.source, e.valid_from))
        .collect();
    out.sort();
    out
}

/// The reached node ids of a `CallGraph`, sorted, for a stable membership assertion.
pub fn call_node_ids(cg: &CallGraph) -> Vec<String> {
    let mut v: Vec<String> = cg.nodes.iter().map(|n| n.node.id.clone()).collect();
    v.sort();
    v
}

/// The `(from, to)` endpoints of a `CallGraph`'s edges, sorted.
pub fn call_edge_pairs(cg: &CallGraph) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = cg
        .edges
        .iter()
        .map(|e| (e.edge.from.clone(), e.edge.to.clone()))
        .collect();
    v.sort();
    v
}

/// The layer a `CallGraph` places node `id` at, if it holds it.
pub fn call_layer(cg: &CallGraph, id: &str) -> Option<i64> {
    cg.nodes.iter().find(|n| n.node.id == id).map(|n| n.layer)
}

/// Whether a `CallGraph`'s `from -> to` edge is a back edge, if it holds one.
pub fn call_back_edge(cg: &CallGraph, from: &str, to: &str) -> Option<bool> {
    cg.edges
        .iter()
        .find(|e| e.edge.from == from && e.edge.to == to)
        .map(|e| e.back)
}

/// A code entity `combat.rs::fire` carrying a governing decision `d1`, an ABOUT finding `f1`, an
/// ABOUT lesson `l1` (build-process memory, which the memory rail excludes) and its own live
/// `REALIZES` edge to the concept `concept/combat` (the direction a MEMBER carries toward the
/// concept it realizes). A second, unrelated file `other.rs` carries none of it, for the empty
/// case.
pub fn subject_graph() -> Graph {
    let rel = |from: &str, to: &str, rel: &str| edge(from, to, rel, TIER_INFERRED);
    Graph {
        nodes: vec![
            plain("combat.rs::fire", KIND_CODE_ENTITY),
            plain("other.rs", KIND_FILE),
            summarized_node("d1", KIND_DECISION, "use the shared authority"),
            summarized_node("f1", KIND_FINDING, "the finding content"),
            summarized_node("l1", KIND_LESSON, "the lesson content"),
            node_with_attrs(
                "concept/combat",
                KIND_CONCEPT,
                &[("label", "combat resolution")],
            ),
        ],
        edges: vec![
            rel("d1", "combat.rs::fire", REL_GOVERNS),
            rel("f1", "combat.rs::fire", REL_ABOUT),
            rel("l1", "combat.rs::fire", REL_ABOUT),
            rel("combat.rs::fire", "concept/combat", REL_REALIZES),
        ],
    }
}

/// A code-entity DEFINITION node: its `name` attr marks it a real definition (not a bare
/// cross-file placeholder), exactly as the extraction fold records.
pub fn def_node(id: &str, name: &str) -> Node {
    entity_node(id, KIND_CODE_ENTITY, None, Some(name))
}
