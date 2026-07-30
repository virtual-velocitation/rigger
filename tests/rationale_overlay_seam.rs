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

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_DECISION, KIND_FILE, KIND_LESSON, REL_ABOUT, REL_GOVERNS, TIER_INFERRED,
};
use rigger::dash::{self, DashInputs};

fn node(id: &str, kind: &str, summary: &str) -> Node {
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

fn edge(from: &str, to: &str, rel: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None, // live
        source: 0,
        tier: TIER_INFERRED.to_string(),
    }
}

/// Start `serve` on a FRESH ephemeral loopback port with two DISTINCT graphs - `whole_graph` behind
/// the lazy whole-graph provider (`/api/graph` reads it) and `poll_graph` behind the state-poll
/// provider (every `/api/*` request rides it) - fetch `GET <path>` once, and return the raw HTTP
/// response. `None` means THIS attempt lost the free-port handoff race (the probe binds port 0, learns
/// the port, releases it, and `serve` re-binds it); a `None` is always a transient handoff loss, never
/// a content failure, so the caller retries. Production `rigger dash` binds ONE stable port once.
fn try_fetch_served(path: &str, whole_graph: Graph, poll_graph: Graph) -> Option<String> {
    let port = TcpListener::bind(("127.0.0.1", 0))
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // `/api/graph` reads through the SEPARATE lazy whole-graph provider (spec 45 c1); the state poll
    // reads its own, run-seeded graph. Give them DIFFERENT graphs so the served explain endpoint's
    // SOURCE is discriminated: whatever crosses the wire proves which provider it read.
    let graph_provider = {
        let g = whole_graph.clone();
        move |_instance: Option<&str>| -> Graph { g.clone() }
    };
    let provider = move |_instance: Option<&str>| -> Result<DashInputs, String> {
        Ok((Vec::new(), poll_graph.clone(), Vec::new(), HashMap::new()))
    };
    let calls_provider =
        |_: Option<&str>, _: &[String], _: rigger::contextgraph::Direction, _: i64, _: &str| {
            rigger::contextgraph::CallGraph::default()
        };
    let instances_provider = Vec::new;
    std::thread::spawn(move || {
        let _ = dash::serve(
            addr,
            provider,
            graph_provider,
            calls_provider,
            instances_provider,
            3,
            "rigger-run",
            "origin/main",
        );
    });

    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    };

    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    if client.write_all(req.as_bytes()).is_err() {
        return None;
    }
    let mut resp = String::new();
    match client.read_to_string(&mut resp) {
        Ok(_) => Some(resp),
        Err(_) => None,
    }
}

/// Drive the dash server over a REAL loopback socket, RETRYING the whole port handoff on a
/// connection-level transient (see [`try_fetch_served`]).
fn fetch_served(path: &str, whole_graph: &Graph, poll_graph: &Graph) -> String {
    for _ in 0..200 {
        if let Some(resp) = try_fetch_served(path, whole_graph.clone(), poll_graph.clone()) {
            return resp;
        }
    }
    panic!(
        "the dash server never served {path} over the real socket after many fresh-port attempts"
    );
}

/// Split a raw HTTP response into its body (everything past the header terminator). `Response::json`
/// frames the body as the exact JSON bytes with `Content-Length` and no trailing newline, so the body
/// this returns is byte-identical to the serialized batch.
fn body_of(resp: &str) -> &str {
    resp.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body")
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
            edge("d-real", "shared.rs", REL_GOVERNS),
            edge("les-real", "shared.rs", REL_ABOUT),
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
        edges: vec![edge("d-poll-decoy", "shared.rs", REL_GOVERNS)],
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
        edges: vec![edge("d1", "shared.rs", REL_GOVERNS)],
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
        edges: vec![edge("d1", "shared.rs", REL_GOVERNS)],
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
