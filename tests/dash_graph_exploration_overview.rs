//! Periphery (API) test for the whole-graph CLUSTERED OVERVIEW (spec 42, criterion c2, purified for
//! `Lens::Files` by spec 63 criterion 3): [`rigger::dash::clustered_overview`] folds the graph into
//! [`rigger::dash::Cluster`] super-nodes and the [`rigger::dash::ClusterEdge`]s among them, so the KG
//! panel can render a large graph as its default whole-graph view. Under [`rigger::dash::Lens::Files`]
//! (the DEFAULT lens), ONLY code entities fold - each by its OWN FILE (not its directory), so a
//! cluster IS a file sized by its CONTAINED-ENTITY count; every currently-valid edge that CROSSES two
//! files weights a symmetric cluster edge; and `total` reports the full node count regardless of what
//! folds. A dev-loop node (a decision, a finding), a design-doc, and a file's own node all carry NO
//! cluster of any kind here - this lens's whole point (spec 63's Goal) is that no storage-schema
//! bucket and no entity-as-a-peer-of-its-own-file ever renders.
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
//!     symmetric merge of `a -> b` and `b -> a` graph edges, the exclusion of intra-cluster /
//!     self-loop / invalidated / dangling edges, determinism across repeated folds, and the JSON wire
//!     shape. (The pre-spec-63 dominant-kind TIE rule and the kind/directory namespace-collision edge
//!     case, adv-u42c1-kind-dir-namespace-collision, are both now STRUCTURALLY UNREACHABLE for this
//!     lens: only one kind - code-entity - ever folds here, and a dev-loop kind never becomes a
//!     bucket at all, so there is no longer a second kind or a kind/directory namespace to collide
//!     with. That coverage lives on for a lens where multiple kinds can still occupy one bucket -
//!     e.g. `Lens::Concepts` - not here.)
//!
//! `dash` and `contextgraph` compile on BOTH the default and the `--no-default-features` lane
//! (neither is feature-gated), so this guards the overview boundary in both lanes.

use std::collections::BTreeMap;

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_DECISION, KIND_DESIGN_DOC, REL_REFERENCES,
    TIER_EXTRACTED,
};
use rigger::dash::{clustered_overview, Cluster, ClusterEdge, ClusterOverview, Lens};

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

/// The public fold is reachable across the crate boundary and folds a whole graph into counted
/// clusters with the full node `total`. This test's VALUE is structural: it proves
/// `clustered_overview` and its carrier DTOs are genuinely `pub` and usable by an external consumer
/// (the c4 route + the c5 page), which the in-module unit test cannot prove. It also proves spec 63
/// c3's FILES-LENS PURITY over the crate boundary: only code entities fold, each by its OWN FILE, and
/// a dev-loop node / design-doc carries no cluster at all.
#[test]
fn clustered_overview_is_reachable_and_folds_code_entities_by_their_own_file() {
    let graph = Graph {
        nodes: vec![
            node("src/a.rs::foo", KIND_CODE_ENTITY),
            node("src/b.rs::bar", KIND_CODE_ENTITY),
            node("docs/x.md", KIND_DESIGN_DOC),
            node("d1", KIND_DECISION),
        ],
        edges: vec![],
    };

    let overview: ClusterOverview = clustered_overview(&graph, &Lens::Files);

    assert_eq!(overview.total, 4, "total carries every node in the graph");
    // Clusters are deterministically ordered by key; each code entity folds to its OWN FILE. The
    // design-doc and the decision carry no cluster at all (spec 63 c3 purity).
    assert_eq!(
        overview.clusters,
        vec![
            Cluster {
                key: "src/a.rs".to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            Cluster {
                key: "src/b.rs".to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
        ],
        "each code entity becomes one counted Cluster keyed by its own file; the design-doc and the \
         decision carry no cluster: {overview:?}"
    );
    assert!(
        overview.edges.is_empty(),
        "a graph with no edges yields no cluster edges"
    );
}

/// The fold is DETERMINISTIC: two entities sharing the same file merge into ONE cluster (sized by
/// their count), two entities in different files never merge, and repeated folds of the same graph
/// agree exactly - so the panel colours/sizes a cluster identically on every poll. (The pre-spec-63
/// dominant-kind TIE rule is dropped here: post-purity only one kind - code-entity - ever folds under
/// this lens, so a kind tie is structurally unreachable for it; see the module doc.)
#[test]
fn clustered_overview_merges_same_file_entities_and_is_deterministic() {
    let graph = Graph {
        nodes: vec![
            // Same file "lib/a.rs" - TWO entities merge into one cluster, count 2.
            node("lib/a.rs::x", KIND_CODE_ENTITY),
            node("lib/a.rs::y", KIND_CODE_ENTITY),
            // A DIFFERENT file, even in the same directory - its own cluster, count 1.
            node("lib/b.rs::z", KIND_CODE_ENTITY),
        ],
        edges: vec![],
    };

    let overview = clustered_overview(&graph, &Lens::Files);
    assert_eq!(
        overview.clusters,
        vec![
            Cluster {
                key: "lib/a.rs".to_string(),
                count: 2,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
            Cluster {
                key: "lib/b.rs".to_string(),
                count: 1,
                kind: KIND_CODE_ENTITY.to_string(),
                label: None,
            },
        ],
        "same-file entities merge (count 2); a different file never merges, even in the same directory"
    );

    // Determinism by construction: a second fold of the same graph yields an identical overview.
    assert_eq!(
        overview,
        clustered_overview(&graph, &Lens::Files),
        "clustered_overview is a pure function of the graph: repeated folds agree exactly"
    );
}

/// Only currently-valid edges that CROSS two FILE clusters carry weight. An `a -> b` and a `b -> a`
/// graph edge fold into ONE symmetric edge whose weight sums both; an intra-file edge, a self-loop, an
/// invalidated edge, an edge touching a purity-excluded node, and an edge to a node absent from the
/// graph all add NOTHING.
#[test]
fn clustered_overview_weights_only_cross_file_currently_valid_edges() {
    let graph = Graph {
        nodes: vec![
            node("a/x.rs::p", KIND_CODE_ENTITY),
            node("b/y.rs::q", KIND_CODE_ENTITY),
            node("c/z.rs::r", KIND_CODE_ENTITY),
            // A dev-loop node: purity-excluded, so an edge touching it adds nothing (spec 63 c3).
            node("d1", KIND_DECISION),
        ],
        edges: vec![
            // a <-> b in BOTH directions -> one symmetric edge of weight 2.
            edge("a/x.rs::p", "b/y.rs::q", None),
            edge("b/y.rs::q", "a/x.rs::p", None),
            // a <-> c once -> weight 1.
            edge("a/x.rs::p", "c/z.rs::r", None),
            // A self-loop is intra-file -> adds nothing.
            edge("a/x.rs::p", "a/x.rs::p", None),
            // An invalidated b <-> c edge -> counts for nothing.
            edge("b/y.rs::q", "c/z.rs::r", Some(9)),
            // A dangling edge to a node ABSENT from the graph -> skipped (no cluster to weight).
            edge("a/x.rs::p", "ghost/none.rs::x", None),
            // An edge to the purity-excluded decision -> adds nothing (no cluster on that endpoint).
            edge("a/x.rs::p", "d1", None),
        ],
    };

    let overview = clustered_overview(&graph, &Lens::Files);
    assert_eq!(overview.total, 4, "total counts every graph node");
    assert_eq!(
        overview.edges,
        vec![
            ClusterEdge {
                from: "a/x.rs".to_string(),
                to: "b/y.rs".to_string(),
                weight: 2,
            },
            ClusterEdge {
                from: "a/x.rs".to_string(),
                to: "c/z.rs".to_string(),
                weight: 1,
            },
        ],
        "a<->b sums both directions to weight 2; a<->c is weight 1; self-loop, invalidated, dangling, \
         and excluded-endpoint edges add none"
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
            node("docs/x.rs::bar", KIND_CODE_ENTITY),
        ],
        edges: vec![edge("src/a.rs::foo", "docs/x.rs::bar", None)],
    };
    let value = serde_json::to_value(clustered_overview(&graph, &Lens::Files))
        .expect("overview serializes");

    assert_eq!(value["total"], 2, "total is a plain node count on the wire");
    let clusters = value["clusters"].as_array().expect("clusters is an array");
    assert_eq!(clusters.len(), 2, "two clusters on the wire");
    assert_eq!(clusters[0]["key"], "docs/x.rs");
    assert_eq!(clusters[0]["count"], 1);
    assert_eq!(clusters[0]["kind"], KIND_CODE_ENTITY);
    let edges = value["edges"].as_array().expect("edges is an array");
    assert_eq!(edges.len(), 1, "one cross-cluster edge on the wire");
    assert_eq!(edges[0]["from"], "docs/x.rs");
    assert_eq!(edges[0]["to"], "src/a.rs");
    assert_eq!(edges[0]["weight"], 1);
}

/// RESOLVED by spec 63 c3 (was NON-GATING carry-forward adv-u42c1-kind-dir-namespace-collision under
/// the pre-purity fold): a directory bucket and a dev-loop KIND bucket used to share one string
/// namespace, so a repo whose top-level directory was named exactly like a node kind (`decision`)
/// co-folded that directory's file nodes with dev-loop nodes of that kind into one cluster. Under the
/// purified `Lens::Files`, this can no longer happen at all: a dev-loop node never gets a kind bucket
/// (spec 63 c3 purity), and a code entity folds by its own FILE, not its directory - so a directory
/// literally named `decision` is just a normal path segment inside a file cluster's key, never a
/// namespace a dev-loop kind can collide with. This pins that resolution directly.
#[test]
fn a_directory_named_like_a_kind_no_longer_collides_with_anything() {
    let graph = Graph {
        nodes: vec![
            // A file under a directory literally named "decision" - folds to ITS OWN FILE, a plain
            // path with no kind-bucket namespace to collide with.
            node("decision/notes.rs::helper", KIND_CODE_ENTITY),
            // Two dev-loop decision nodes - carry NO cluster at all under files-lens purity.
            node("d1", KIND_DECISION),
            node("d2", KIND_DECISION),
        ],
        edges: vec![],
    };

    let overview = clustered_overview(&graph, &Lens::Files);
    assert_eq!(
        overview.clusters,
        vec![Cluster {
            key: "decision/notes.rs".to_string(),
            count: 1,
            kind: KIND_CODE_ENTITY.to_string(),
            label: None,
        }],
        "the code entity folds to its own file; the two decisions carry no cluster - nothing collides: {overview:?}"
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

    let overview = clustered_overview(&graph, &Lens::Files);

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
