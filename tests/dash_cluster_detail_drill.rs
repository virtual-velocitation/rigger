//! Periphery (API + serialized-form) tests for the cluster DRILL (spec 42, criterion c3):
//! [`rigger::dash::cluster_detail`] drills one exploration cluster to its members - the nodes whose
//! [`rigger::dash::cluster_key`] equals the drilled key, the currently-valid edges among them, and a
//! render budget ([`rigger::dash::CLUSTER_RENDER_BUDGET`]) that caps a big cluster to its highest
//! intra-cluster-degree members so the library-free SVG panel never draws a thousand nodes.
//!
//! This runs OUTSIDE the crate, over the library's PUBLIC surface. The implementer's inside-out unit
//! test in `dash.rs` calls `cluster_detail` IN-MODULE and compares the returned `Neighborhood` via
//! `PartialEq`, so it is structurally blind to four boundaries this layer owns:
//!   - EXPORT REACHABILITY: that `cluster_detail` and the `CLUSTER_RENDER_BUDGET` constant are
//!     genuinely `pub` and reachable across the crate boundary - their whole reason to exist, since
//!     the c4 route dispatch (a downstream unit) consumes them. A `pub` narrowed to `pub(crate)`
//!     keeps the unit test green but fails to compile here.
//!   - The CAP BOUNDARY off-by-one: the unit test drills a cluster two over budget; it never pins the
//!     exact `<=` edge - a cluster of EXACTLY `CLUSTER_RENDER_BUDGET` members renders whole while one
//!     member larger caps. A regression flipping `<=` to `<` stays green in the unit test but turns
//!     the exact-boundary assertion here RED.
//!   - The doc-comment's GRACEFUL-DEGRADATION claim the unit test never drives: an unknown / empty
//!     `key` (no node folds to it), and an empty graph, each yield an EMPTY drill, never an error.
//!   - The SERIALIZED-FORM (wire) contract of `Neighborhood::truncated`: its
//!     `#[serde(skip_serializing_if = "Option::is_none")]` means a plain neighborhood and an
//!     at/under-budget drill emit NO `truncated` key at all, so a present key unambiguously means
//!     "this view is capped". The unit test compares Rust structs, so it cannot see the wire form; a
//!     dropped `skip_serializing_if` (leaking `"truncated": null` onto every spec-30 neighborhood)
//!     stays green there and RED here.
//!
//! This layer does NOT re-assert the implementer's happy-path unit cases (the specific kept ids, the
//! specific degree ranking); it owns the crate-boundary reachability, the exact cap boundary, the
//! degradation edges, the drill INVARIANTS (purity, ascending-id order, no dangling edge), and the
//! wire contract. `dash` and `contextgraph` compile on BOTH the default and the
//! `--no-default-features` lane (neither is feature-gated), so this guards the drill in both lanes.

use std::collections::BTreeMap;

use rigger::contextgraph::{Edge, Graph, Node, KIND_CODE_ENTITY, REL_REFERENCES, TIER_EXTRACTED};
use rigger::dash::{cluster_detail, neighborhood, CLUSTER_RENDER_BUDGET};

/// A code entity `cl/f.rs::<name>` - every such id folds (via `cluster_key`) to the module bucket
/// `cl`, so a set of them forms one drillable cluster with the key `"cl"`.
fn member(name: &str) -> Node {
    Node {
        id: format!("cl/f.rs::{name}"),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: BTreeMap::new(),
    }
}

/// A currently-valid REFERENCES edge between two ids (the `extracted` tier, `valid_to = None`).
fn edge(from: &str, to: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: REL_REFERENCES.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: TIER_EXTRACTED.to_string(),
    }
}

/// A spoke id, zero-padded so its ASCII order matches its numeric order (the drill emits members in
/// ascending-id order, and the cap's id tie-break keeps the smallest ids).
fn spoke_id(i: usize) -> String {
    format!("cl/f.rs::s{i:05}")
}

/// EXPORT REACHABILITY: `cluster_detail` and `CLUSTER_RENDER_BUDGET` are genuinely `pub` and usable by
/// an external consumer (the downstream c4 route), which the in-module unit test cannot prove. The
/// mere compilation of this test IS the guarantee; the assertions pin the shape a caller relies on.
#[test]
fn cluster_detail_and_budget_are_reachable_over_the_public_crate_boundary() {
    let g = Graph {
        nodes: vec![member("only")],
        edges: Vec::new(),
    };
    let drill = cluster_detail(&g, "cl");
    assert_eq!(
        drill.seed, "cl",
        "the drill echoes the drilled cluster key as its seed, so the panel can label it"
    );
    assert_eq!(drill.depth, 0, "a cluster drill is not a hop-bounded walk");
    assert_eq!(drill.nodes.len(), 1, "the one-member cluster renders whole");
    // Consume the public `CLUSTER_RENDER_BUDGET` constant at runtime (proving it is reachable across
    // the crate boundary): a drill never renders more than the budget.
    assert!(
        drill.nodes.len() <= CLUSTER_RENDER_BUDGET,
        "a drill renders at most CLUSTER_RENDER_BUDGET members"
    );
    assert!(
        drill.truncated.is_none(),
        "a single-member cluster is complete, so nothing is truncated"
    );
}

/// The CAP BOUNDARY off-by-one the unit test skips: a cluster of EXACTLY `CLUSTER_RENDER_BUDGET`
/// members renders WHOLE (`truncated` stays `None`), while a cluster ONE member larger caps to
/// exactly `CLUSTER_RENDER_BUDGET` and reports its full count. This pins the `total <= budget`
/// comparison precisely - the edge a `<` / `<=` regression would cross.
#[test]
fn cluster_detail_renders_whole_at_budget_and_caps_one_over() {
    // EXACTLY at budget: every member renders, nothing truncated.
    let at: Vec<Node> = (0..CLUSTER_RENDER_BUDGET)
        .map(|i| member(&format!("m{i:05}")))
        .collect();
    let g_at = Graph {
        nodes: at,
        edges: Vec::new(),
    };
    let drill_at = cluster_detail(&g_at, "cl");
    assert_eq!(
        drill_at.nodes.len(),
        CLUSTER_RENDER_BUDGET,
        "a cluster of exactly CLUSTER_RENDER_BUDGET members renders whole"
    );
    assert!(
        drill_at.truncated.is_none(),
        "at the budget the view is complete, so truncated stays None"
    );

    // ONE over budget: caps to exactly the budget and reports the full member count.
    let over: Vec<Node> = (0..=CLUSTER_RENDER_BUDGET)
        .map(|i| member(&format!("m{i:05}")))
        .collect();
    let total = over.len();
    assert_eq!(
        total,
        CLUSTER_RENDER_BUDGET + 1,
        "fixture sanity: one over budget"
    );
    let g_over = Graph {
        nodes: over,
        edges: Vec::new(),
    };
    let drill_over = cluster_detail(&g_over, "cl");
    assert_eq!(
        drill_over.nodes.len(),
        CLUSTER_RENDER_BUDGET,
        "one member over budget caps the rendered set to exactly CLUSTER_RENDER_BUDGET"
    );
    assert_eq!(
        drill_over.truncated,
        Some(total),
        "an over-budget drill reports its FULL member count so the panel can caption 'N of M'"
    );
}

/// The DRILL INVARIANTS that must hold for ANY over-budget drill, which the once-run unit test does
/// not state as invariants: the drill is PURE (repeated calls agree, so the panel is poll-stable),
/// emits its nodes in ASCENDING-ID order (a stable layout), and NEVER dangles an edge - every returned
/// edge has BOTH endpoints in the returned node set, so a budget-dropped member takes its edges with
/// it. Built as a hub wired to every spoke: the hub survives on degree while the highest-id spokes are
/// dropped, and their hub-edges must vanish with them.
#[test]
fn cluster_detail_is_a_pure_stable_drill_that_never_dangles_an_edge() {
    let mut nodes: Vec<Node> = vec![member("hub")];
    let mut edges: Vec<Edge> = Vec::new();
    // Two more spokes than the budget, so the cap must drop the three highest-id spokes.
    let spokes = CLUSTER_RENDER_BUDGET + 2;
    for i in 0..spokes {
        nodes.push(member(&format!("s{i:05}")));
        edges.push(edge("cl/f.rs::hub", &spoke_id(i)));
    }
    let total = nodes.len(); // hub + (budget + 2) spokes = budget + 3 members
    let g = Graph { nodes, edges };

    let first = cluster_detail(&g, "cl");
    let second = cluster_detail(&g, "cl");
    assert_eq!(
        first, second,
        "cluster_detail is a pure function of the graph: repeated drills agree (poll-stable)"
    );

    assert_eq!(
        first.nodes.len(),
        CLUSTER_RENDER_BUDGET,
        "the over-budget drill renders exactly CLUSTER_RENDER_BUDGET members"
    );
    assert_eq!(
        first.truncated,
        Some(total),
        "the drill reports the full pre-cap member count"
    );

    // Nodes emitted in strictly ascending id order (a poll-stable, spiral-seeded layout).
    let ids: Vec<&str> = first.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(
        ids, sorted,
        "the drill emits its nodes in ascending-id order for a stable layout"
    );

    // No dangling edge: every returned edge has BOTH endpoints in the returned node set.
    let rendered: std::collections::BTreeSet<&str> =
        first.nodes.iter().map(|n| n.id.as_str()).collect();
    for e in &first.edges {
        assert!(
            rendered.contains(e.from.as_str()) && rendered.contains(e.to.as_str()),
            "a returned edge {e:?} must have both endpoints in the rendered node set"
        );
    }
    // The cap genuinely fired here, and the no-dangle loop above is NON-VACUOUSLY exercised: pin the
    // in-view edge count EXACTLY, not just "fewer than the spokes". The hub (highest intra-cluster
    // degree) always survives, so the rendered set is the hub plus the CLUSTER_RENDER_BUDGET - 1
    // smallest-id spokes; each surviving spoke keeps its single hub-edge, the dropped spokes take
    // theirs away, so EXACTLY CLUSTER_RENDER_BUDGET - 1 edges remain in view. A positive exact count
    // cannot be satisfied by an empty edge set, so were a regression to drop the hub (dangling every
    // edge to zero) this assertion turns RED instead of passing vacuously through a 0-iteration loop.
    assert_eq!(
        first.edges.len(),
        CLUSTER_RENDER_BUDGET - 1,
        "the surviving hub keeps exactly its CLUSTER_RENDER_BUDGET - 1 kept-spoke edges, so the \
         no-dangle check ran over a non-empty edge set (a dropped hub would zero this and go RED)"
    );
}

/// The GRACEFUL-DEGRADATION boundary the doc-comment promises but the unit test never drives: an
/// unknown key, an empty key, and an empty graph each yield an EMPTY drill - never a panic, never an
/// error - which is what the panel relies on when an operator drills a stale or vanished cluster.
#[test]
fn cluster_detail_degrades_gracefully_on_unknown_empty_key_and_empty_graph() {
    let populated = Graph {
        nodes: vec![member("a"), member("b")],
        edges: vec![edge("cl/f.rs::a", "cl/f.rs::b")],
    };

    // UNKNOWN key: no node folds to it, so the drill is empty but well-formed (seed echoed, depth 0).
    let unknown = cluster_detail(&populated, "no/such/cluster");
    assert!(
        unknown.nodes.is_empty() && unknown.edges.is_empty(),
        "an unknown cluster key yields an empty drill"
    );
    assert_eq!(
        unknown.seed, "no/such/cluster",
        "even an empty drill echoes the key"
    );
    assert_eq!(unknown.depth, 0);
    assert!(
        unknown.truncated.is_none(),
        "an empty drill is complete, not truncated"
    );

    // EMPTY key: totality - no panic, an empty drill.
    let empty_key = cluster_detail(&populated, "");
    assert!(
        empty_key.nodes.is_empty() && empty_key.edges.is_empty(),
        "an empty key folds to nothing, yielding an empty drill without panicking"
    );

    // EMPTY graph: any key drills to nothing.
    let empty_graph = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let none = cluster_detail(&empty_graph, "cl");
    assert!(
        none.nodes.is_empty() && none.edges.is_empty() && none.truncated.is_none(),
        "an empty graph yields an empty, untruncated drill for any key"
    );
}

/// The SERIALIZED-FORM (wire) contract of `Neighborhood::truncated` the struct-`PartialEq` unit test
/// is blind to. `skip_serializing_if = "Option::is_none"` means a present `truncated` key
/// UNAMBIGUOUSLY signals a capped view: a plain spec-30 neighborhood and an at/under-budget drill both
/// emit NO `truncated` key (byte-unchanged for existing consumers), while an over-budget drill emits
/// `"truncated": <total>`. A dropped `skip_serializing_if` would leak `"truncated": null` onto every
/// neighborhood - green in the unit test, RED here.
#[test]
fn truncated_serializes_only_when_the_drill_capped_preserving_neighborhood_backcompat() {
    // A plain spec-30 neighborhood carries NO truncated key (back-compat: the /api/graph JSON is
    // byte-unchanged for the existing panel).
    let g = Graph {
        nodes: vec![member("a"), member("b")],
        edges: vec![edge("cl/f.rs::a", "cl/f.rs::b")],
    };
    let nb = serde_json::to_value(neighborhood(&g, "cl/f.rs::a", 1))
        .expect("a Neighborhood serializes to JSON");
    assert!(
        nb.get("truncated").is_none(),
        "a plain neighborhood must OMIT the truncated key (spec-30 wire back-compat): {nb}"
    );

    // An UNDER-budget drill also omits the key, so its JSON shape matches a neighborhood - the SAME
    // renderer draws both, which is the whole point of reusing the Neighborhood shape.
    let under = serde_json::to_value(cluster_detail(&g, "cl"))
        .expect("a drill Neighborhood serializes to JSON");
    assert!(
        under.get("truncated").is_none(),
        "an at/under-budget drill must OMIT the truncated key: {under}"
    );

    // An OVER-budget drill emits truncated as the full member count.
    let big: Vec<Node> = (0..=CLUSTER_RENDER_BUDGET)
        .map(|i| member(&format!("m{i:05}")))
        .collect();
    let total = big.len();
    let g_big = Graph {
        nodes: big,
        edges: Vec::new(),
    };
    let over = serde_json::to_value(cluster_detail(&g_big, "cl"))
        .expect("an over-budget drill serializes to JSON");
    assert_eq!(
        over.get("truncated").and_then(|v| v.as_u64()),
        Some(total as u64),
        "an over-budget drill must serialize truncated as its full member count: {over}"
    );
}
