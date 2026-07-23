//! Periphery (API) test for the whole-graph CLUSTERED OVERVIEW (spec 42, criterion c2):
//! [`rigger::dash::clustered_overview`] folds the entire knowledge graph into a few dozen
//! [`rigger::dash::Cluster`] super-nodes and the [`rigger::dash::ClusterEdge`]s among them, so the KG
//! panel can render a ~7k-node graph as its default whole-graph view. Each cluster carries its member
//! count and its DOMINANT member kind (for colour); every currently-valid edge that CROSSES two
//! clusters weights a symmetric cluster edge; and `total` reports the full node count.
//!
//! This runs OUTSIDE the crate, over the library's PUBLIC surface
//! (`rigger::dash::{clustered_overview, Cluster, ClusterEdge, ClusterOverview}`). The implementer's
//! inside-out unit test in `dash.rs` calls the fold IN-MODULE, so it is structurally blind to two
//! things this layer guards:
//!   - EXPORT REACHABILITY: that `clustered_overview` and its three carrier DTOs are genuinely `pub`
//!     and reachable across the crate boundary - their whole reason to exist, since the c4 route
//!     serializes them onto `/api/graph` and the c5 page draws them. A `pub` narrowed to `pub(crate)`
//!     keeps the unit test green but breaks the boundary; here it fails to compile.
//!   - The AGGREGATION CONTRACT the panel depends on but the unit test states only once: the
//!     dominant-kind tie rule (pc-adv-3), the symmetric merge of `a -> b` and `b -> a` graph edges,
//!     the exclusion of intra-cluster / self-loop / invalidated / dangling edges, determinism across
//!     repeated folds, the JSON wire shape, and the (non-gating) kind/directory namespace overlap
//!     carried forward from the c1 review (adv-u42c1-kind-dir-namespace-collision).
//!
//! `dash` and `contextgraph` compile on BOTH the default and the `--no-default-features` lane
//! (neither is feature-gated), so this guards the overview boundary in both lanes.

use std::collections::BTreeMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_DECISION, KIND_DESIGN_DOC, KIND_FILE, REL_REFERENCES,
    TIER_EXTRACTED,
};
use rigger::dash::{clustered_overview, Cluster, ClusterEdge, ClusterOverview};

/// A graph node with no attributes (the overview reads only its id and kind, never its label).
fn node(id: &str, kind: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: BTreeMap::new(),
    }
}

/// A currently-valid (`valid_to = None`) or invalidated edge between two node ids.
fn edge(from: &str, to: &str, valid_to: Option<i64>) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: REL_REFERENCES.to_string(),
        valid_from: 0,
        valid_to,
        source: 0,
        tier: TIER_EXTRACTED.to_string(),
    }
}

/// The public fold is reachable across the crate boundary and folds a whole graph into counted,
/// dominant-kind clusters with the full node `total`. This test's VALUE is structural: it proves
/// `clustered_overview` and its carrier DTOs are genuinely `pub` and usable by an external consumer
/// (the c4 route + the c5 page), which the in-module unit test cannot prove.
#[test]
fn clustered_overview_is_reachable_and_folds_nodes_into_counted_dominant_kind_clusters() {
    let graph = Graph {
        nodes: vec![
            node("src/a.rs::foo", KIND_CODE_ENTITY),
            node("src/b.rs::bar", KIND_CODE_ENTITY),
            node("docs/x.md", KIND_DESIGN_DOC),
            node("d1", KIND_DECISION),
        ],
        edges: vec![],
    };

    let overview: ClusterOverview = clustered_overview(&graph);

    assert_eq!(overview.total, 4, "total carries every node in the graph");
    // Clusters are deterministically ordered by key; each carries its member count and dominant kind.
    assert_eq!(
        overview.clusters,
        vec![
            Cluster {
                key: "decision".to_string(),
                count: 1,
                kind: KIND_DECISION.to_string(),
            },
            Cluster {
                key: "docs".to_string(),
                count: 1,
                kind: KIND_DESIGN_DOC.to_string(),
            },
            Cluster {
                key: "src".to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
            },
        ],
        "every cluster_key bucket becomes one counted, dominant-kind Cluster"
    );
    assert!(
        overview.edges.is_empty(),
        "a graph with no edges yields no cluster edges"
    );
}

/// The DOMINANT member kind is the highest-count kind, ties broken by the lexicographically-SMALLEST
/// kind (pc-adv-3): a majority kind wins outright, and a genuine tie resolves to the smaller kind, so
/// the same graph always colours a cluster identically. Proven over one graph carrying BOTH shapes.
#[test]
fn clustered_overview_breaks_a_dominant_kind_tie_by_smallest_kind_and_is_deterministic() {
    let graph = Graph {
        nodes: vec![
            // Cluster "lib": 1 code-entity + 2 files -> "file" wins by MAJORITY (2 > 1), proving the
            // dominant kind is genuinely count-driven, not merely "always the smallest kind".
            node("lib/a.rs::x", KIND_CODE_ENTITY),
            node("lib/b.rs", KIND_FILE),
            node("lib/c.rs", KIND_FILE),
            // Cluster "src": 1 code-entity + 1 file -> a 1-1 TIE, resolved to the smallest kind
            // ("code-entity" < "file").
            node("src/a.rs::x", KIND_CODE_ENTITY),
            node("src/b.rs", KIND_FILE),
        ],
        edges: vec![],
    };

    let overview = clustered_overview(&graph);
    assert_eq!(
        overview.clusters,
        vec![
            Cluster {
                key: "lib".to_string(),
                count: 3,
                kind: KIND_FILE.to_string(),
            },
            Cluster {
                key: "src".to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
            },
        ],
        "the majority kind wins (lib -> file); a tie resolves to the smallest kind (src -> code-entity)"
    );

    // Determinism by construction: a second fold of the same graph yields an identical overview.
    assert_eq!(
        overview,
        clustered_overview(&graph),
        "clustered_overview is a pure function of the graph: repeated folds agree exactly"
    );
}

/// Only currently-valid edges that CROSS two clusters carry weight. An `a -> b` and a `b -> a` graph
/// edge fold into ONE symmetric edge whose weight sums both; an intra-cluster edge, a self-loop, an
/// invalidated edge, and an edge to a node absent from the graph all add NOTHING.
#[test]
fn clustered_overview_weights_only_cross_cluster_currently_valid_edges() {
    let graph = Graph {
        nodes: vec![
            node("a/x.rs", KIND_FILE),
            node("b/y.rs", KIND_FILE),
            node("c/z.rs", KIND_FILE),
        ],
        edges: vec![
            // a <-> b in BOTH directions -> one symmetric edge of weight 2.
            edge("a/x.rs", "b/y.rs", None),
            edge("b/y.rs", "a/x.rs", None),
            // a <-> c once -> weight 1.
            edge("a/x.rs", "c/z.rs", None),
            // A self-loop is intra-cluster -> adds nothing.
            edge("a/x.rs", "a/x.rs", None),
            // An invalidated b <-> c edge -> counts for nothing.
            edge("b/y.rs", "c/z.rs", Some(9)),
            // A dangling edge to a node ABSENT from the graph -> skipped (no cluster to weight).
            edge("a/x.rs", "ghost/none.rs", None),
        ],
    };

    let overview = clustered_overview(&graph);
    assert_eq!(overview.total, 3, "total counts the three graph nodes");
    assert_eq!(
        overview.edges,
        vec![
            ClusterEdge {
                from: "a".to_string(),
                to: "b".to_string(),
                weight: 2,
            },
            ClusterEdge {
                from: "a".to_string(),
                to: "c".to_string(),
                weight: 1,
            },
        ],
        "a<->b sums both directions to weight 2; a<->c is weight 1; self-loop, invalidated, and dangling edges add none"
    );
}

/// The overview serializes to the exact JSON wire shape the KG panel reads: `clusters` (each with
/// `key` / `count` / `kind`), `edges` (each with `from` / `to` / `weight`), and `total`. The c5 page
/// binds to these field names, so a rename would silently break the viz; this pins the contract.
#[test]
fn clustered_overview_serializes_to_the_wire_shape_the_kg_panel_reads() {
    let graph = Graph {
        nodes: vec![
            node("src/a.rs::foo", KIND_CODE_ENTITY),
            node("docs/x.md", KIND_DESIGN_DOC),
        ],
        edges: vec![edge("src/a.rs::foo", "docs/x.md", None)],
    };
    let value = serde_json::to_value(clustered_overview(&graph)).expect("overview serializes");

    assert_eq!(value["total"], 2, "total is a plain node count on the wire");
    let clusters = value["clusters"].as_array().expect("clusters is an array");
    assert_eq!(clusters.len(), 2, "two clusters on the wire");
    assert_eq!(clusters[0]["key"], "docs");
    assert_eq!(clusters[0]["count"], 1);
    assert_eq!(clusters[0]["kind"], KIND_DESIGN_DOC);
    let edges = value["edges"].as_array().expect("edges is an array");
    assert_eq!(edges.len(), 1, "one cross-cluster edge on the wire");
    assert_eq!(edges[0]["from"], "docs");
    assert_eq!(edges[0]["to"], "src");
    assert_eq!(edges[0]["weight"], 1);
}

/// NON-GATING carry-forward from the c1 review (adv-u42c1-kind-dir-namespace-collision): a directory
/// bucket and a kind bucket share ONE string namespace, so a repo whose top-level directory is named
/// exactly like a node kind (here `decision`) co-folds that directory's file nodes with the dev-loop
/// nodes of that kind into a SINGLE cluster. c2 does not own kind/directory disjointness (that is the
/// c1 fold key's shape), and the spec never requires it; this pins that `clustered_overview` folds the
/// overlap TOTALLY and deterministically - one merged cluster whose dominant kind is taken over the
/// union - rather than panicking or double-counting. It documents the behaviour, it does not "fix" it.
#[test]
fn clustered_overview_folds_a_kind_named_directory_into_one_deterministic_cluster() {
    let graph = Graph {
        nodes: vec![
            // A file under a directory literally named "decision" -> folds to bucket "decision".
            node("decision/notes.rs::helper", KIND_CODE_ENTITY),
            // Two dev-loop decision nodes -> also fold to bucket "decision".
            node("d1", KIND_DECISION),
            node("d2", KIND_DECISION),
        ],
        edges: vec![],
    };

    let overview = clustered_overview(&graph);
    assert_eq!(
        overview.clusters,
        vec![Cluster {
            key: "decision".to_string(),
            // All three co-fold into the one bucket - counted, never dropped or double-counted.
            count: 3,
            // Dominant kind over the union: 2 decisions outweigh 1 code-entity.
            kind: KIND_DECISION.to_string(),
        }],
        "a kind-named directory and dev-loop nodes of that kind fold into one deterministic cluster"
    );
    assert_eq!(overview.total, 3, "every node is still counted in total");
}

/// The overview folds an EMPTY graph into a well-formed EMPTY overview, never an error, and that empty
/// shape is exactly `ClusterOverview::default()`. This pins two boundary facts the in-module unit test
/// (populated graph only) leaves unproven: that `clustered_overview`'s documented empty-graph edge
/// ("zero clusters, zero total, never an error") actually holds, and that the DERIVED public
/// `ClusterOverview::default()` is reachable across the crate boundary and yields that same empty shape,
/// the empty value the c4 route dispatch and the c6 empty/degraded path lean on. c2 owns the pure fold
/// over an empty graph; it does NOT own the c6 route-level empty handling, which this never drives.
#[test]
fn clustered_overview_over_an_empty_graph_is_the_default_empty_overview() {
    let graph = Graph {
        nodes: vec![],
        edges: vec![],
    };

    let overview = clustered_overview(&graph);

    assert_eq!(overview.total, 0, "an empty graph has zero nodes in total");
    assert!(
        overview.clusters.is_empty(),
        "an empty graph yields no cluster super-nodes"
    );
    assert!(
        overview.edges.is_empty(),
        "an empty graph yields no cross-cluster edges"
    );
    // The derived, publicly-reachable Default is the same empty overview the empty-graph fold produces
    // - so a caller (the c4 route, the c6 empty path) can use either and get an identical wire shape.
    assert_eq!(
        overview,
        ClusterOverview::default(),
        "the empty-graph fold equals ClusterOverview::default(): the empty value callers rely on"
    );
}
