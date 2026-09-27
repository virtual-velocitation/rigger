//! Periphery (integration) tests for the RATIONALE OVERLAY seam (spec 55, criterion 3), the layer
//! the inside-out unit tests and the happy-path served test are structurally blind to.
//!
//! The implementer's `tests/rationale_overlay_data.rs` proves the served `GET /api/graph?explain=`
//! endpoint carries the batch end-to-end - but it feeds the SAME fixture graph to BOTH the lazy
//! whole-graph provider AND the state-poll provider, so it cannot discriminate WHICH provider the
//! overlay reads. The `route` unit tests drive `route` directly with an already-chosen graph, so they
//! are blind to the provider split entirely. This file closes that gap over the REAL loopback socket:
//!
//!   1. the served overlay reads the LAZY whole-graph provider, NEVER the state poll (the two providers
//!      carry DIFFERENT graphs here, so a regression that read the poll graph reddens);
//!   2. an EMPTY `explain=` value takes the overlay branch and answers a graceful empty batch, NOT the
//!      seeded neighborhood (an `explain=` present with an empty value returns `Some("")`, so the branch
//!      fires - a regression that filtered the empty value would fall through to the neighborhood);
//!   3. the served batch wire shape is BYTE-STABLE (a back-compat literal for the response contract).
//!
//! `dash` / `contextgraph` compile on BOTH the default and the `--no-default-features` lane (none
//! feature-gated), so this guards the served boundary in both lanes.

mod common;

use common::fixtures::edge;
use common::fixtures::summarized_node as node;
use common::served::body_of;
use common::served::{fetch_with_retry, graph_provider_of, try_fetch_over};
use rigger::contextgraph::{
    Graph, KIND_DECISION, KIND_FILE, KIND_LESSON, REL_ABOUT, REL_GOVERNS, TIER_INFERRED,
};

/// Drive the dash server over a REAL loopback socket with two DISTINCT graphs - `whole_graph`
/// behind the lazy whole-graph provider (`/api/graph` reads it) and `poll_graph` behind the
/// state-poll provider (every `/api/*` request rides it), so whatever crosses the wire proves
/// which provider it read - RETRYING the whole port handoff on a connection-level transient
/// (see [`try_fetch_over`]).
fn fetch_served(path: &str, whole_graph: &Graph, poll_graph: &Graph) -> String {
    fetch_with_retry(path, || {
        try_fetch_over(
            path,
            graph_provider_of(whole_graph.clone()),
            poll_graph.clone(),
        )
    })
}

/// The served `/api/graph?explain=` overlay reads the LAZY WHOLE-GRAPH provider, NEVER the state poll.
///
/// The whole-graph provider carries the real rationale (`d-real` GOVERNS + `les-real` ABOUT
/// `shared.rs`); the state-poll provider carries a DECOY (`d-poll-decoy` GOVERNS `shared.rs`) that must
/// NOT surface. `handle_conn` calls the state-poll `provider` on every `/api/*` request but REPLACES
/// its graph with `graph_provider`'s for a `/api/graph` path, so the batch must be built from the
/// whole graph. If a regression read the polled graph instead, the wire would carry `d-poll-decoy` and
/// this reddens.
#[test]
fn the_served_explain_overlay_reads_the_lazy_whole_graph_not_the_state_poll() {
    let whole_graph = Graph {
        nodes: vec![
            node("shared.rs", KIND_FILE, ""),
            node("d-real", KIND_DECISION, "the whole-graph rationale"),
            node("les-real", KIND_LESSON, "the whole-graph lesson"),
        ],
        edges: vec![
            edge("d-real", "shared.rs", REL_GOVERNS, TIER_INFERRED),
            edge("les-real", "shared.rs", REL_ABOUT, TIER_INFERRED),
        ],
    };
    let poll_graph = Graph {
        nodes: vec![
            node("shared.rs", KIND_FILE, ""),
            node(
                "d-poll-decoy",
                KIND_DECISION,
                "the state-poll decoy must not surface",
            ),
        ],
        edges: vec![edge(
            "d-poll-decoy",
            "shared.rs",
            REL_GOVERNS,
            TIER_INFERRED,
        )],
    };

    let resp = fetch_served("/api/graph?explain=shared.rs", &whole_graph, &poll_graph);
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the explain overlay answers 200 over the real socket:\n{resp}"
    );
    let body = body_of(&resp);
    let json: serde_json::Value =
        serde_json::from_str(body).expect("the served explain body is valid JSON");
    let nodes = json["nodes"].as_array().expect("a nodes array");
    assert_eq!(
        nodes.len(),
        1,
        "one requested node carries rationale: {json}"
    );
    assert_eq!(nodes[0]["node"].as_str().unwrap(), "shared.rs");

    let leaf_ids: Vec<&str> = nodes[0]["leaves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        leaf_ids,
        vec!["d-real", "les-real"],
        "the overlay serves the WHOLE-GRAPH rationale (decision before lesson), not the poll: {json}"
    );
    assert!(
        !body.contains("d-poll-decoy") && !body.contains("must not surface"),
        "the state-poll decoy rationale must NOT reach the overlay - explain reads the whole-graph \
         provider, never the state poll: {body}"
    );
}

/// An EMPTY `explain=` value takes the overlay branch and answers a graceful EMPTY batch, NOT the
/// seeded neighborhood. `explain=` present with an empty value parses to `Some("")`, so the overlay
/// branch fires and returns `{"nodes":[]}` (no ids requested -> no nodes). A regression that filtered
/// the empty value (like the `instance=` param does) would fall through to the neighborhood and this
/// reddens: the neighborhood echoes a `seed` and never a `nodes` batch.
#[test]
fn an_empty_explain_value_is_a_graceful_empty_batch_not_the_neighborhood() {
    // A graph that HAS rationale, so the empty result is due to the empty request, not an empty graph.
    let whole_graph = Graph {
        nodes: vec![
            node("shared.rs", KIND_FILE, ""),
            node("d1", KIND_DECISION, "why shared"),
        ],
        edges: vec![edge("d1", "shared.rs", REL_GOVERNS, TIER_INFERRED)],
    };
    let resp = fetch_served("/api/graph?explain=", &whole_graph, &Graph::default());
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "an empty explain still answers 200:\n{resp}"
    );
    assert!(
        resp.contains("application/json"),
        "the empty batch is JSON, i.e. the overlay branch was taken:\n{resp}"
    );
    let body = body_of(&resp);
    assert_eq!(
        body, r#"{"nodes":[]}"#,
        "an empty explain value is a graceful empty overlay batch, not the neighborhood: {body}"
    );
    assert!(
        !body.contains("\"seed\""),
        "the empty-explain request did NOT fall through to the seeded neighborhood: {body}"
    );
}

/// The served rationale batch WIRE SHAPE is byte-stable (a back-compat literal for the response
/// contract): the nested `{nodes:[{node,leaves:[{id,kind,summary}]}]}` shape, field names and order,
/// pinned exactly so a shape drift on the wire reddens.
#[test]
fn the_served_rationale_batch_wire_shape_is_byte_stable() {
    let whole_graph = Graph {
        nodes: vec![
            node("shared.rs", KIND_FILE, ""),
            node("d1", KIND_DECISION, "why shared"),
        ],
        edges: vec![edge("d1", "shared.rs", REL_GOVERNS, TIER_INFERRED)],
    };
    let resp = fetch_served(
        "/api/graph?explain=shared.rs",
        &whole_graph,
        &Graph::default(),
    );
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "200 over the socket:\n{resp}"
    );
    assert_eq!(
        body_of(&resp),
        r#"{"nodes":[{"node":"shared.rs","leaves":[{"id":"d1","kind":"decision","summary":"why shared"}]}]}"#,
        "the served batch wire shape is byte-stable"
    );
}
