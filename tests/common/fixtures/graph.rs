//! Context-graph fixtures: nodes, edges, whole graphs and projection doubles.

use std::collections::BTreeMap;

use rigger::contextgraph::{
    CallEdge, Edge, Graph, Node, KIND_UNIT, REL_CALLS, REL_REFERENCES, TIER_EXTRACTED,
    TIER_INFERRED,
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
