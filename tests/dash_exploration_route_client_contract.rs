//! Periphery (contract) test for the dash's WHOLE-GRAPH EXPLORATION VIZ (spec 42, criterion c5):
//! the CLIENT <-> SERVER JSON CONTRACT at the `/api/graph` boundary.
//!
//! The c5 served page draws the exploration viz by reading specific fields off the `/api/graph`
//! body: `renderKgOverview` sizes each cluster super-node by `clusters[].count`, colours it by
//! `clusters[].kind`, labels it by `clusters[].key`, reports the whole size from `total`, and scales
//! each cross-cluster line by `edges[].weight`; `renderKgDrill` sizes/emphasizes each member by
//! `nodes[].degree` + `nodes[].god`, colours by `nodes[].kind`, labels by `nodes[].label`, keys off
//! `nodes[].id`, echoes the drilled cluster as `seed`, and captions "showing the N most-connected of
//! M" from `truncated`. If a serde rename / field drop on the projection DTOs silently removed one of
//! those keys, the viz would draw wrong (uniform circles, no god emphasis, no cap caption) while the
//! page still loads.
//!
//! The EXISTING layers do not jointly pin this. The c2/c3 tests assert the same fields but by calling
//! `clustered_overview` / `cluster_detail` IN-PROCESS (never through the route). The c4 served-route
//! tests drive the real socket but pin only the DISPATCH-discriminating fields (`total`,
//! `clusters[].key`; `seed`, `nodes[].id`) - not the SIZING / COLOURING / GOD / CAP fields the viz
//! actually draws. The c5 node-vm harness drives the page but feeds HAND-BUILT fixtures through a
//! mocked `fetch`, so a real-route field divergence never reddens it. This layer closes that gap: it
//! drives the public `route` (the exact body `serve` ships - `serve` delegates to `route`) and asserts
//! the overview and drill bodies carry EVERY field the c5 viz reads, each bound to the mechanism (a
//! weighted cross edge, a god-node hub, an over-budget cap) so an unrelated token cannot satisfy it.
//!
//! `dash` + `contextgraph` compile on BOTH the default and the `--no-default-features` lane (neither
//! the route nor these DTOs is feature-gated), so this guards the served contract in both lanes.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_DECISION, REL_REFERENCES, TIER_EXTRACTED,
};
use rigger::dash::{route, CLUSTER_RENDER_BUDGET, GOD_NODE_DEGREE_THRESHOLD};

/// A code-entity node whose id names a file under a module directory, so `cluster_key` folds it into
/// that directory's cluster (e.g. `src/big/mod.rs::hub` -> cluster `src/big`).
fn ce(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: BTreeMap::new(),
    }
}

/// A dev-loop decision node (no path id), so `cluster_key` folds it by its KIND into the `decision`
/// cluster - a second cluster of a DIFFERENT dominant kind than the code-entity clusters, so the
/// overview's per-cluster `kind` field is proven to discriminate (it drives the super-node colour).
fn dec(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_DECISION.to_string(),
        attrs: BTreeMap::new(),
    }
}

/// A currently-valid REFERENCES edge (`extracted` tier, `valid_to = None`).
fn refs(from: &str, to: &str) -> Edge {
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

/// A zero-padded spoke id under `src/big` (padding makes ASCII order match numeric order, which the
/// drill cap's smallest-id tie-break relies on).
fn spoke(i: usize) -> String {
    format!("src/big/mod.rs::s{i:05}")
}

/// The exploration fixture. It folds into THREE clusters that exercise every overview + drill field.
/// `src/big` is a hub wired to `CLUSTER_RENDER_BUDGET + 1` spokes (`CLUSTER_RENDER_BUDGET + 2`
/// members, ONE over the render budget), so a drill on it CAPS and reports `truncated`, and the
/// surviving hub is a god-node (its in-view degree `CLUSTER_RENDER_BUDGET - 1` is well above the god
/// threshold). `src/small` is three members, a small code-entity cluster. `decision` is one dev-loop
/// node, a cluster of a DIFFERENT dominant kind. TWO graph edges cross `src/big` -> `src/small`, so
/// the overview carries exactly one cross-cluster edge of `weight` 2. ONE graph drives both route
/// calls, exactly as the browser hits the same live graph for the overview and then the drill.
fn exploration_graph() -> Graph {
    let hub = "src/big/mod.rs::hub";
    let mut nodes: Vec<Node> = vec![ce(hub)];
    let mut edges: Vec<Edge> = Vec::new();

    // src/big: hub -> every spoke (all intra-cluster). One over budget so the drill caps.
    let spokes = CLUSTER_RENDER_BUDGET + 1;
    for i in 0..spokes {
        nodes.push(ce(&spoke(i)));
        edges.push(refs(hub, &spoke(i)));
    }

    // src/small: three members.
    for m in ["a", "b", "c"] {
        nodes.push(ce(&format!("src/small/mod.rs::{m}")));
    }

    // decision: one dev-loop node (a distinct dominant kind for the overview colour).
    nodes.push(dec("d0"));

    // Two cross edges src/big -> src/small, so the ONE overview cluster edge has weight 2. They dangle
    // out of the src/big drill (their src/small endpoint is not a src/big member) and so must be
    // dropped from the drill body, never dangled.
    edges.push(refs(hub, "src/small/mod.rs::a"));
    edges.push(refs(&spoke(0), "src/small/mod.rs::b"));

    Graph { nodes, edges }
}

/// Drive the public `route` for a `GET <path>` over the fixture graph and parse the JSON body. `route`
/// is the exact body-builder `serve` ships (serve delegates to it), so the field contract this pins is
/// byte-identical to what the browser receives; the c4 socket test already covers the framing seam.
fn served_body(path: &str) -> serde_json::Value {
    let graph = exploration_graph();
    let liveness: HashMap<String, u64> = HashMap::new();
    let resp = route(
        "GET",
        path,
        &[],
        &graph,
        &[],
        &liveness,
        0,
        "rigger-run",
        "origin/main",
    );
    assert_eq!(
        resp.status, 200,
        "GET {path} must be served 200 (the exploration route never errors on a live graph)"
    );
    serde_json::from_slice(&resp.body)
        .unwrap_or_else(|e| panic!("the served {path} body must be valid JSON: {e}"))
}

/// The served OVERVIEW body (`GET /api/graph`, no argument = the default KG view) carries EVERY field
/// `renderKgOverview` reads: `clusters[].{key,count,kind}` (label / size / colour), `total` (the
/// headline node count), and `edges[].{from,to,weight}` (the cross-cluster lines, thickness by weight).
/// Each is asserted to a concrete value bound to the fixture, so a renamed / dropped key reddens here.
#[test]
fn the_served_overview_route_carries_every_field_the_c5_overview_viz_reads() {
    let ov = served_body("/api/graph");

    // It is the OVERVIEW shape, not a neighborhood: no `nodes` key (the drill / seed views carry that).
    assert!(
        ov.get("nodes").is_none() || ov["nodes"].is_null(),
        "the no-argument route is the clustered overview, not a neighborhood: {ov}"
    );

    // `total`: the whole-graph node count the panel headlines. hub + (budget+1) spokes + 3 small + 1
    // decision = budget + 6.
    assert_eq!(
        ov["total"].as_u64(),
        Some((CLUSTER_RENDER_BUDGET + 6) as u64),
        "the overview reports the full node total (drives 'N nodes in M clusters'): {ov}"
    );

    // `clusters[]`: the super-nodes, each carrying key + count + kind. Index them by key so the
    // count/kind assertions bind per cluster.
    let clusters = ov["clusters"]
        .as_array()
        .expect("overview carries a clusters array");
    let by_key: BTreeMap<&str, (&serde_json::Value, &serde_json::Value)> = clusters
        .iter()
        .map(|c| {
            (
                c["key"]
                    .as_str()
                    .expect("each cluster carries a string key"),
                (&c["count"], &c["kind"]),
            )
        })
        .collect();

    let keys: BTreeSet<&str> = by_key.keys().copied().collect();
    assert_eq!(
        keys,
        ["decision", "src/big", "src/small"].into_iter().collect(),
        "the graph folds into exactly its three clusters (label = key): {ov}"
    );

    // `count` drives the super-node RADIUS - it must be the real member count, and must discriminate
    // (the big cluster is far larger than the small ones), not a constant.
    assert_eq!(
        by_key["src/big"].0.as_u64(),
        Some((CLUSTER_RENDER_BUDGET + 2) as u64),
        "src/big's count is hub + all its spokes (drives the super-node size): {ov}"
    );
    assert_eq!(
        by_key["src/small"].0.as_u64(),
        Some(3),
        "src/small's count is its three members: {ov}"
    );
    assert_eq!(
        by_key["decision"].0.as_u64(),
        Some(1),
        "the decision cluster's count is its single member: {ov}"
    );

    // `kind` drives the super-node COLOUR - it must be the dominant member kind and must discriminate
    // (a code-entity cluster vs the decision cluster), so a uniform-colour regression reddens here.
    assert_eq!(
        by_key["src/big"].1.as_str(),
        Some(KIND_CODE_ENTITY),
        "src/big's dominant kind is code-entity (drives its colour): {ov}"
    );
    assert_eq!(
        by_key["decision"].1.as_str(),
        Some(KIND_DECISION),
        "the decision cluster's dominant kind differs, so the colour field discriminates: {ov}"
    );

    // `edges[].{from,to,weight}`: the cross-cluster lines. Exactly one here (src/big <-> src/small),
    // its weight the count of crossing graph edges (2), which scales the line thickness.
    let cedges = ov["edges"]
        .as_array()
        .expect("overview carries an edges array");
    assert_eq!(
        cedges.len(),
        1,
        "the two src/big -> src/small graph edges fold into ONE weighted cluster edge: {ov}"
    );
    assert_eq!(
        (cedges[0]["from"].as_str(), cedges[0]["to"].as_str()),
        (Some("src/big"), Some("src/small")),
        "the cluster edge is canonicalized from <= to by key: {ov}"
    );
    assert_eq!(
        cedges[0]["weight"].as_u64(),
        Some(2),
        "the cluster edge weight sums both crossing edges (drives the line thickness): {ov}"
    );
}

/// The served DRILL body (`GET /api/graph?cluster=<key>`, the key `encodeURIComponent`d so its `/`
/// arrives percent-encoded) carries EVERY field `renderKgDrill` reads: `seed` (the echoed cluster
/// key), `nodes[].{id,kind,label,degree,god}` (key / colour / label / size / hub emphasis), and
/// `truncated` (the cap caption). Bound to the mechanism: an over-budget cluster with a god-node hub,
/// so `truncated` and `god:true` both appear and discriminate from a plain spoke.
#[test]
fn the_served_drill_route_carries_every_field_the_c5_drill_viz_reads() {
    // The `/` in the module key arrives percent-encoded, exactly as the page's encodeURIComponent
    // emits it; the route decodes it back to the fold key.
    let nb = served_body("/api/graph?cluster=src%2Fbig");

    // `seed`: the drill echoes the decoded cluster key (the panel titles "cluster <key>").
    assert_eq!(
        nb["seed"].as_str(),
        Some("src/big"),
        "the drill echoes the decoded cluster key as its seed: {nb}"
    );

    // `truncated`: the cap fired (the cluster is one over budget), so it reports the FULL member count
    // for the "showing the N most-connected of M" caption. A complete drill omits this key.
    assert_eq!(
        nb["truncated"].as_u64(),
        Some((CLUSTER_RENDER_BUDGET + 2) as u64),
        "an over-budget drill reports its full member count for the cap caption: {nb}"
    );

    // The rendered set is capped to exactly the budget (hub + budget-1 highest-degree spokes).
    let nodes = nb["nodes"]
        .as_array()
        .expect("the drill carries a nodes array");
    assert_eq!(
        nodes.len(),
        CLUSTER_RENDER_BUDGET,
        "the over-budget drill caps the drawn set to exactly the render budget: {nb}"
    );

    // Locate the hub (a god-node) and a plain spoke, and prove every per-node field the viz reads.
    let hub_id = "src/big/mod.rs::hub";
    let hub = nodes
        .iter()
        .find(|n| n["id"].as_str() == Some(hub_id))
        .unwrap_or_else(|| panic!("the surviving hub must be in the drilled set: {nb}"));

    // `id` / `kind` / `label`: the key, colour, and label the viz draws. label falls back to the id
    // (the fixture nodes carry no summary/title/name), so it must be the non-empty id here.
    assert_eq!(
        hub["id"].as_str(),
        Some(hub_id),
        "each drill node carries its id: {nb}"
    );
    assert_eq!(
        hub["kind"].as_str(),
        Some(KIND_CODE_ENTITY),
        "each drill node carries its kind (drives the member colour): {nb}"
    );
    assert_eq!(
        hub["label"].as_str(),
        Some(hub_id),
        "each drill node carries a label (id fallback here) the viz draws without re-deriving: {nb}"
    );

    // `degree` + `god`: the c5 god-node emphasis. The hub's IN-VIEW degree is its edges to the
    // surviving spokes (budget-1), well above the god threshold, so `god` is true and `degree`
    // discriminates it from a spoke.
    // Fixture sanity (compile-time): the hub's in-view degree must clear the god threshold, else the
    // cluster would produce no god-node and the `god == true` assertion below would be vacuous.
    const _: () = assert!(
        CLUSTER_RENDER_BUDGET - 1 > GOD_NODE_DEGREE_THRESHOLD,
        "the fixture's hub degree must exceed the god threshold"
    );
    assert_eq!(
        hub["degree"].as_u64(),
        Some((CLUSTER_RENDER_BUDGET - 1) as u64),
        "the hub's in-view degree is its edges to the surviving spokes (drives its radius): {nb}"
    );
    assert_eq!(
        hub["god"].as_bool(),
        Some(true),
        "the high-degree hub is flagged god so the viz emphasizes it: {nb}"
    );

    let spoke0 = nodes
        .iter()
        .find(|n| n["id"].as_str() == Some(spoke(0).as_str()))
        .unwrap_or_else(|| panic!("the smallest-id spoke survives the cap: {nb}"));
    assert_eq!(
        spoke0["degree"].as_u64(),
        Some(1),
        "a plain spoke's in-view degree is 1 (its single edge to the hub): {nb}"
    );
    assert_eq!(
        spoke0["god"].as_bool(),
        Some(false),
        "a plain spoke is NOT a god-node, so `god` discriminates the hub from a leaf: {nb}"
    );

    // `edges[].{from,to}`: every drawn edge, both endpoints in the returned set (no dangle) and never
    // a cross-cluster (src/small) endpoint - the drill draws only the cluster's own edges.
    let ids: BTreeSet<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    let dedges = nb["edges"]
        .as_array()
        .expect("the drill carries an edges array");
    assert!(
        !dedges.is_empty(),
        "the drilled cluster's hub-to-spoke edges are drawn: {nb}"
    );
    for e in dedges {
        let from = e["from"]
            .as_str()
            .expect("each drill edge carries a string from");
        let to = e["to"]
            .as_str()
            .expect("each drill edge carries a string to");
        assert!(
            ids.contains(from) && ids.contains(to),
            "no drill edge dangles - both endpoints are in the drawn set: {from} -> {to} in {nb}"
        );
        assert!(
            !from.starts_with("src/small/") && !to.starts_with("src/small/"),
            "the cross-cluster edges are dropped from the drill (no src/small endpoint): {from} -> {to}"
        );
    }
}
