//! Periphery (API / contract) tests for spec 93 criterion 5, THE QUERY ENGINE MOVES WITH THE OPS:
//! the graph query engine (`neighborhood`, `card`, `path`, `clustered_overview`, `cluster_detail`)
//! relocated from `src/dash.rs` into `src/contextgraph/query.rs`, the new `search` query authored
//! beside it, and the two op-level entry points - `graph_load`/`graph_query` - the console ABI
//! (criterion 2) will wire to.
//!
//! The five relocated functions keep a LEGACY `rigger::dash::...` path (a `pub use` re-export, so
//! the pre-existing crate-external periphery suite that pins them there - `metadata_card_periphery.rs`,
//! `dash_graph_exploration_fold.rs`, `dash_cluster_detail_drill.rs`, `dash_graph_exploration_overview.rs`,
//! `subject_view_memory_rail_contract.rs` and others - keeps proving that boundary without a single
//! line changing here. This file guards what is GENUINELY NEW in this criterion, which has no prior
//! coverage anywhere:
//!
//!   - EXPORT REACHABILITY at the query engine's OWN canonical path: `search`, `SearchHit`,
//!     `SEARCH_RESULT_LIMIT`, `graph_load` and `graph_query` are `pub` and reachable as
//!     `rigger::contextgraph::query::...` from OUTSIDE the crate - unlike the five relocated
//!     functions, none of these five has a `rigger::dash::...` alias at all, so this is their ONLY
//!     external reachability proof.
//!   - THE CRITERION 5 CONTRACT, FROM OUTSIDE: "`graph_load` accepts the map payload and
//!     `graph_query` answers neighborhood, card, path, communities and search with the same results
//!     the library's query functions return for the same graph" (spec 93 criterion 5's own
//!     done-when), proven here using ONLY the crate's public surface. The implementer's in-module
//!     proof (`src/contextgraph/query.rs`'s `graph_query_answers_every_kind_identically_to_the_direct_library_call`)
//!     cannot tell a genuinely `pub` function from one accidentally narrowed to `pub(crate)` - both
//!     compile and pass from inside the defining module. This layer can, because it lives outside
//!     the crate boundary entirely.
//!   - THE WIRE-FORM BOUNDARY: `Node`/`Edge`/`Graph` gained `Serialize`/`Deserialize` in this
//!     criterion ("the wire form `graph_load` parses a `Graph` payload from and the future console
//!     page would send" - `src/contextgraph/mod.rs`), a genuinely new capability with no prior wire
//!     form to stay backward-compatible with. This layer proves a graph built purely from the public
//!     `Node`/`Edge`/`Graph` fields - including every optional/default field a real caller can send -
//!     round-trips through JSON losslessly, from outside the crate.
//!
//! `contextgraph::query` is a `core` module (no feature gate anywhere on its declaration or its
//! `contextgraph::{Node, Edge, Graph}` dependencies), so - like the rest of this crate's periphery
//! suite - these tests run on both the default and the `--no-default-features` lane. No reference to
//! any external tool or project; hyphens, never em dashes.

mod common;

use common::fixtures::node_with_attrs as node;
use rigger::contextgraph::query::{
    card, cluster_detail, clustered_overview, graph_load, graph_query, neighborhood, path, search,
    Lens, SearchHit, DEFAULT_COMMUNITY_RESOLUTION, SEARCH_RESULT_LIMIT,
};
use rigger::contextgraph::{Edge, Graph, KIND_CODE_ENTITY, KIND_DECISION, KIND_FILE, REL_CONTAINS};

fn edge(from: &str, to: &str, rel: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 1,
        valid_to: None,
        source: 7,
        tier: "extracted".to_string(),
    }
}

/// A small graph built purely from the PUBLIC `Node`/`Edge`/`Graph` fields - spec 93 criterion 5's
/// wire-form claim only means something if a caller outside the crate can build one at all.
fn fixture_graph() -> Graph {
    Graph {
        nodes: vec![
            node("src/a.rs", KIND_FILE, &[]),
            node("src/a.rs::widget", KIND_CODE_ENTITY, &[("name", "widget")]),
            node("src/b.rs::gadget", KIND_CODE_ENTITY, &[("name", "gadget")]),
            node(
                "adj-relocate-query-engine",
                KIND_DECISION,
                &[("summary", "the query engine moves with the ops")],
            ),
        ],
        edges: vec![edge("src/a.rs", "src/a.rs::widget", REL_CONTAINS)],
    }
}

// ---------------------------------------------------------------------------
// EXPORT REACHABILITY: the criterion's genuinely new items compile and behave sanely at their new
// canonical path. Each call below only compiles if the item is truly `pub` at `contextgraph::query`.
// ---------------------------------------------------------------------------

#[test]
fn search_and_search_hit_and_its_limit_const_are_reachable_over_the_public_crate_boundary() {
    let g = fixture_graph();
    let hits = search(&g, "widget", SEARCH_RESULT_LIMIT);
    assert_eq!(
        hits,
        vec![SearchHit {
            id: "src/a.rs::widget".to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            label: "widget".to_string(),
        }],
        "search and SearchHit's fields must be directly constructible/comparable from outside the crate"
    );
}

#[test]
fn graph_load_and_graph_query_are_reachable_over_the_public_crate_boundary() {
    let g = fixture_graph();
    let payload = serde_json::to_vec(&g).expect("a public Graph must serialize");
    let loaded = graph_load(&payload).expect("a graph's own wire form must load");
    assert_eq!(loaded.nodes.len(), g.nodes.len());
    assert_eq!(loaded.edges.len(), g.edges.len());

    let answer =
        graph_query(&loaded, "card", br#"{"id":"src/a.rs::widget"}"#).expect("card op must answer");
    assert_eq!(
        answer,
        serde_json::to_value(card(&g, "src/a.rs::widget")).unwrap()
    );
}

// ---------------------------------------------------------------------------
// THE CRITERION 5 CONTRACT, FROM OUTSIDE: graph_load + graph_query answers every one of the five
// documented kinds identically to the direct library call, proven entirely through the public
// surface - `rigger::contextgraph::query::*` only, no `super::`, no crate-internal visibility.
// ---------------------------------------------------------------------------

/// Shared assertion for the kind-parity proofs below: `graph_query`'s JSON answer for
/// `kind`/`params` over the fixture graph's own wire-loaded round trip must equal `direct` - the
/// direct library call's own serialized result over the same graph.
fn assert_op_matches_direct_call(
    kind: &str,
    params: &[u8],
    direct: impl Fn(&Graph) -> serde_json::Value,
) {
    let graph = fixture_graph();
    let loaded = graph_load(&serde_json::to_vec(&graph).unwrap()).unwrap();
    let got = graph_query(&loaded, kind, params)
        .unwrap_or_else(|e| panic!("graph_query({kind:?}, ...) must answer, got {e:?}"));
    assert_eq!(
        got,
        direct(&graph),
        "graph_query({kind:?}, ...) must match the direct library call"
    );
}

rigger::test_cases! {
    graph_query_neighborhood_matches_the_direct_library_call:
        assert_op_matches_direct_call("neighborhood", br#"{"seed":"src/a.rs","depth":1}"#, |g| {
            serde_json::to_value(neighborhood(g, "src/a.rs", 1)).unwrap()
        });
    /// No `depth` key at all: graph_query must apply the SAME DEFAULT_GRAPH_DEPTH a direct caller
    /// would have to name explicitly - proving the op layer's own default, not just its dispatch.
    graph_query_neighborhood_default_depth_matches_the_direct_library_default:
        assert_op_matches_direct_call("neighborhood", br#"{"seed":"src/a.rs"}"#, |g| {
            serde_json::to_value(neighborhood(
                g,
                "src/a.rs",
                rigger::contextgraph::query::DEFAULT_GRAPH_DEPTH,
            ))
            .unwrap()
        });
    graph_query_card_matches_the_direct_library_call:
        assert_op_matches_direct_call("card", br#"{"id":"src/a.rs::widget"}"#, |g| {
            serde_json::to_value(card(g, "src/a.rs::widget")).unwrap()
        });
    graph_query_path_matches_the_direct_library_call:
        assert_op_matches_direct_call("path", br#"{"from":"src/a.rs","to":"src/a.rs::widget"}"#, |g| {
            serde_json::to_value(path(g, "src/a.rs", "src/a.rs::widget")).unwrap()
        });
    /// No `key`: the whole-graph overview shape (clustered_overview), the default Lens::Files.
    graph_query_communities_overview_matches_the_direct_library_call:
        assert_op_matches_direct_call("communities", b"{}", |g| {
            serde_json::to_value(clustered_overview(g, &Lens::Files)).unwrap()
        });
    /// A `key`: the bucket-drill shape (cluster_detail) - the SAME two-shape dispatch dash.rs's
    /// `/api/graph` route makes on `cluster=`, now reachable through the op layer too.
    graph_query_communities_drill_matches_the_direct_library_call:
        assert_op_matches_direct_call("communities", br#"{"key":"src"}"#, |g| {
            serde_json::to_value(cluster_detail(g, "src", &Lens::Files)).unwrap()
        });
    graph_query_search_matches_the_direct_library_call:
        assert_op_matches_direct_call("search", br#"{"query":"widget"}"#, |g| {
            serde_json::to_value(search(g, "widget", SEARCH_RESULT_LIMIT)).unwrap()
        });
}

#[test]
fn graph_query_communities_threads_the_lens_param_to_the_direct_library_call() {
    let g = fixture_graph();
    let loaded = graph_load(&serde_json::to_vec(&g).unwrap()).unwrap();
    // `lens=code` is a wire-only concern: CommunitiesParams's field names are private, so only an
    // actual JSON payload proves graph_query threads `lens`/`resolution` to Lens::from_query rather
    // than silently ignoring them and always answering the Lens::Files default.
    let got = graph_query(&loaded, "communities", br#"{"lens":"code"}"#).unwrap();
    let want = clustered_overview(
        &g,
        &Lens::Code {
            resolution: DEFAULT_COMMUNITY_RESOLUTION.to_string(),
        },
    );
    assert_eq!(got, serde_json::to_value(&want).unwrap());
    // And it must genuinely differ from the files-lens default this fixture graph carries a live
    // CONTAINS-only membership for - a query that silently dropped `lens` would answer identically.
    let files_default = graph_query(&loaded, "communities", b"{}").unwrap();
    assert_ne!(
        got, files_default,
        "lens=code must answer differently from the files-lens default on a graph with a real \
         code entity, or the lens param is being silently dropped"
    );
}

#[test]
fn graph_query_search_threads_the_limit_param() {
    let g = fixture_graph();
    let loaded = graph_load(&serde_json::to_vec(&g).unwrap()).unwrap();
    // Both `src/a.rs::widget` and `src/b.rs::gadget` match "rs" (both ids contain it); `limit: 1`
    // must cap the op's answer exactly like a direct `search(&g, "rs", 1)` call would.
    let got = graph_query(&loaded, "search", br#"{"query":"rs","limit":1}"#).unwrap();
    assert_eq!(got, serde_json::to_value(search(&g, "rs", 1)).unwrap());
    let hits = got.as_array().expect("search answers a JSON array");
    assert_eq!(
        hits.len(),
        1,
        "limit=1 must cap the op-routed answer to exactly one hit"
    );
}

// ---------------------------------------------------------------------------
// THE `{"error": ...}` REPLY CONTRACT, FROM OUTSIDE: an unknown op kind and malformed params never
// panic across the public boundary - the documented degrade the future console op layer relies on.
// ---------------------------------------------------------------------------

#[test]
fn graph_query_rejects_an_unknown_kind_without_panicking() {
    let g = fixture_graph();
    let err = graph_query(&g, "not-a-real-kind", b"{}").unwrap_err();
    assert!(
        !err.0.is_empty(),
        "an unknown kind must answer a non-empty Error message, never panic"
    );
}

#[test]
fn graph_query_rejects_malformed_params_without_panicking() {
    let g = fixture_graph();
    assert!(graph_query(&g, "card", b"not json").is_err());
    assert!(
        graph_query(&g, "neighborhood", b"{}").is_err(),
        "seed is a required field"
    );
    assert!(
        graph_query(&g, "path", br#"{"from":"a"}"#).is_err(),
        "to is a required field"
    );
    assert!(
        graph_query(&g, "search", b"{}").is_err(),
        "query is a required field"
    );
}

#[test]
fn graph_load_rejects_malformed_json_without_panicking() {
    assert!(graph_load(b"not json").is_err());
    assert!(
        graph_load(b"{}").is_err(),
        "a Graph payload requires nodes and edges"
    );
}

#[test]
fn an_empty_search_query_yields_no_hits_over_the_op_layer_too() {
    let g = fixture_graph();
    let loaded = graph_load(&serde_json::to_vec(&g).unwrap()).unwrap();
    let got = graph_query(&loaded, "search", br#"{"query":""}"#).unwrap();
    assert_eq!(got, serde_json::Value::Array(Vec::new()));
}

// ---------------------------------------------------------------------------
// THE WIRE-FORM BOUNDARY: Node/Edge/Graph's newly-added Serialize/Deserialize round-trips losslessly
// from outside the crate, across every optional/default-valued field a real caller can send.
// ---------------------------------------------------------------------------

#[test]
fn a_graph_built_from_public_fields_round_trips_through_json_losslessly() {
    let original = fixture_graph();
    let payload = serde_json::to_vec(&original).expect("a public Graph must serialize");
    let loaded = graph_load(&payload).expect("its own wire form must load back");
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&original).unwrap(),
        "a graph must survive a JSON round trip through its own public wire form byte-for-byte"
    );
}

#[test]
fn an_edge_with_valid_to_set_round_trips_through_json() {
    // The common construction (`valid_to: None`) is covered above; a SUPERSEDED edge (`valid_to:
    // Some(_)`) is the other half of the type's state space a real caller can send, and the only
    // field on Edge with a non-trivial Option to lose in a lossy round trip.
    let superseded = Graph {
        nodes: vec![
            node("src/a.rs", KIND_FILE, &[]),
            node("src/a.rs::widget", KIND_CODE_ENTITY, &[]),
        ],
        edges: vec![Edge {
            from: "src/a.rs".to_string(),
            to: "src/a.rs::widget".to_string(),
            rel: REL_CONTAINS.to_string(),
            valid_from: 1,
            valid_to: Some(9),
            source: 3,
            tier: "inferred".to_string(),
        }],
    };
    let loaded = graph_load(&serde_json::to_vec(&superseded).unwrap()).unwrap();
    assert_eq!(loaded.edges[0].valid_to, Some(9));
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&superseded).unwrap()
    );
}

#[test]
fn a_node_with_no_attrs_round_trips_through_json() {
    // An empty `attrs` map (the common case for a KIND_FILE node, per this file's own fixture) must
    // not collapse to a missing key or fail to parse back - the empty-map edge of the wire contract.
    let g = Graph {
        nodes: vec![node("src/a.rs", KIND_FILE, &[])],
        edges: Vec::new(),
    };
    let loaded = graph_load(&serde_json::to_vec(&g).unwrap()).unwrap();
    assert!(loaded.nodes[0].attrs.is_empty());
}
