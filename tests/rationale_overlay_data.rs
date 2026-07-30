//! Periphery (integration) test for the RATIONALE OVERLAY data path (spec 55, criterion 3): the
//! read-only `GET /api/graph?explain=<id>[,<id>...]` route returns, in ONE request, the decisions,
//! findings, and lessons attached to each requested node - CONTENT only, deterministically ordered,
//! and only for the visible nodes that carry any. This criterion OWNS the overlay data.
//!
//! This runs OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::serve`), and crosses
//! the REAL loopback HTTP socket the operator's browser actually hits. The implementer's inside-out
//! unit tests in `dash.rs` (`mod rationale_overlay_c3`) call the pure `node_rationale` /
//! `rationale_batch` / `route` IN-PROCESS: they are structurally blind to the serve path (the `route`
//! dispatch of `GET /api/graph?explain=` and the HTTP framing the socket delivers, and that the
//! overlay batch rides the SAME lazy whole-graph provider `/api/graph` already reads). This layer
//! proves the SERVED endpoint - the bytes a client receives from the public `serve` entrypoint -
//! carries the batch end-to-end.
//!
//! `dash`, `contextgraph` are compiled on BOTH the default and the `--no-default-features` lane (none
//! feature-gated), so this guards the served boundary in both lanes.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_CODE_ENTITY, KIND_DECISION, KIND_FILE, KIND_FINDING,
    KIND_HANDBOOK_RULE, KIND_LESSON, REL_ABOUT, REL_GOVERNS, REL_SUPERSEDES, TIER_INFERRED,
};
use rigger::dash::{self, DashInputs};

/// The fixture served graph. `shared.rs` carries four live rationale leaves - two decisions
/// (`da`, `dz`), a finding (`a-find`, laid out so a wrong id-only sort would float it first), and a
/// lesson (`l1`) - plus three NON-leaves (a `handbook-rule` that GOVERNS it, an INVALIDATED governing
/// decision, and a `SUPERSEDES` edge into `dz`). `shared.rs::foo` carries one leaf and `other.rs`
/// carries none, so the batch proves the visible-set coverage and the has-any filter over the wire.
fn rationale_graph() -> Graph {
    let node = |id: &str, kind: &str, summary: &str| Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: if summary.is_empty() {
            BTreeMap::new()
        } else {
            BTreeMap::from([("summary".to_string(), summary.to_string())])
        },
    };
    let finding = |id: &str, summary: &str, by: &str, unit: &str| Node {
        id: id.to_string(),
        kind: KIND_FINDING.to_string(),
        attrs: BTreeMap::from([
            ("summary".to_string(), summary.to_string()),
            ("by".to_string(), by.to_string()),
            ("unit".to_string(), unit.to_string()),
        ]),
    };
    let edge = |from: &str, to: &str, rel: &str, valid_to: Option<i64>| Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to,
        source: 0,
        tier: TIER_INFERRED.to_string(),
    };
    Graph {
        nodes: vec![
            node("shared.rs", KIND_FILE, ""),
            node("shared.rs::foo", KIND_CODE_ENTITY, ""),
            node("other.rs", KIND_FILE, ""),
            node("dz", KIND_DECISION, "decision zed"),
            node("da", KIND_DECISION, "decision ay"),
            node("dgone", KIND_DECISION, "the superseded governing decision"),
            node("dnew", KIND_DECISION, "the superseding decision"),
            finding(
                "a-find",
                "the finding content",
                "lens:architecture-reviewer",
                "u7",
            ),
            node("l1", KIND_LESSON, "the lesson content"),
            node("hb", KIND_HANDBOOK_RULE, "the handbook rule"),
        ],
        edges: vec![
            edge("dz", "shared.rs", REL_GOVERNS, None),
            edge("da", "shared.rs", REL_GOVERNS, None),
            edge("a-find", "shared.rs", REL_ABOUT, None),
            edge("l1", "shared.rs", REL_ABOUT, None),
            edge("hb", "shared.rs", REL_GOVERNS, None), // wrong kind
            edge("dgone", "shared.rs", REL_GOVERNS, Some(5)), // invalidated
            edge("dnew", "dz", REL_SUPERSEDES, None),   // supersedes, not rationale
            edge("da", "shared.rs::foo", REL_GOVERNS, None),
        ],
    }
}

/// Start `serve` on a FRESH ephemeral loopback port, fetch `GET <path>` once against a fixture-graph
/// provider, and return the raw HTTP response - or `None` when THIS attempt lost the free-port handoff
/// race (the same TOCTOU window `dash_kg_graph_route` documents: the probe binds port 0, learns the
/// port, releases it, and `serve` re-binds it). A `None` is always a transient handoff loss, never a
/// content failure. Production `rigger dash` binds ONE stable port once and never drop-rebinds.
fn try_fetch_served(path: &str, graph: Graph) -> Option<String> {
    let port = TcpListener::bind(("127.0.0.1", 0))
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // `/api/graph` reads through the SEPARATE lazy graph provider (spec 45 c1), so the fixture graph
    // is what `graph_provider` yields; the polled provider carries a run-seeded slice (unused here).
    let graph_provider = {
        let graph = graph.clone();
        move |_instance: Option<&str>| -> Graph { graph.clone() }
    };
    let provider = move |_instance: Option<&str>| -> Result<DashInputs, String> {
        Ok((Vec::new(), graph.clone(), Vec::new(), HashMap::new()))
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

/// Drive the hand-rolled dash server over a REAL loopback socket and fetch `GET <path>`, RETRYING the
/// whole port handoff on a connection-level transient (see [`try_fetch_served`]).
fn fetch_served(path: &str, graph: &Graph) -> String {
    for _ in 0..200 {
        if let Some(resp) = try_fetch_served(path, graph.clone()) {
            return resp;
        }
    }
    panic!(
        "the dash server never served {path} over the real socket after many fresh-port attempts"
    );
}

/// Split a raw HTTP response into its body (everything past the header terminator).
fn body_of(resp: &str) -> &str {
    resp.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body")
}

/// The SERVED `/api/graph?explain=` endpoint returns the rationale batch over the real `serve` socket:
/// a well-formed `200 application/json` whose body covers the requested visible set in ONE request -
/// only the nodes with rationale, each with its ordered CONTENT-only leaves, and no builder-agent
/// machinery. A percent-encoded id (`::` -> `%3A%3A`) proves the split-then-decode of the list.
#[test]
fn the_served_explain_route_returns_the_rationale_batch_in_one_request() {
    let graph = rationale_graph();
    // shared.rs, shared.rs::foo (encoded), and other.rs (no rationale) - all in one GET.
    let resp = fetch_served(
        "/api/graph?explain=shared.rs,shared.rs%3A%3Afoo,other.rs",
        &graph,
    );

    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET /api/graph?explain= returns 200 over the real serve socket:\n{resp}"
    );
    assert!(
        resp.contains("application/json"),
        "the rationale batch is self-contained JSON:\n{resp}"
    );

    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served explain body is valid JSON");
    let nodes = json["nodes"].as_array().expect("a nodes array");

    // The batch covers the visible set, keeping ONLY the nodes with rationale, ordered by node id.
    let node_ids: Vec<&str> = nodes.iter().map(|n| n["node"].as_str().unwrap()).collect();
    assert_eq!(
        node_ids,
        vec!["shared.rs", "shared.rs::foo"],
        "the encoded id decodes to shared.rs::foo and other.rs (no rationale) is dropped: {json}"
    );

    // shared.rs carries its four leaves in (kind, id) order: decisions (da, dz), then finding, lesson.
    let shared = &nodes[0]["leaves"];
    let leaf_ids: Vec<&str> = shared
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        leaf_ids,
        vec!["da", "dz", "a-find", "l1"],
        "the served leaves are deterministically ordered by kind then id: {json}"
    );
    let leaf_kinds: Vec<&str> = shared
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        leaf_kinds,
        vec!["decision", "decision", "finding", "lesson"],
        "each served leaf carries its node kind: {json}"
    );
    // shared.rs::foo carries its one leaf.
    assert_eq!(
        nodes[1]["leaves"].as_array().unwrap().len(),
        1,
        "shared.rs::foo carries exactly one leaf: {json}"
    );

    // CONTENT crosses the wire; run-machinery attribution does NOT (spec 55 content-not-machinery).
    let body = body_of(&resp);
    assert!(
        body.contains("the finding content") && body.contains("the lesson content"),
        "leaf content is served: {body}"
    );
    assert!(
        !body.contains("architecture-reviewer")
            && !body.contains("\"by\"")
            && !body.contains("\"unit\""),
        "no builder-agent attribution is served: {body}"
    );
}

/// Additive guarantee at the served boundary: with `explain=` ABSENT, `/api/graph` is the existing
/// seeded neighborhood (it echoes `seed` and carries no rationale `leaves`), so the overlay endpoint
/// never regresses the views that predate it.
#[test]
fn the_served_graph_route_is_unchanged_without_explain() {
    let resp = fetch_served("/api/graph?seed=shared.rs&depth=1", &rationale_graph());
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the explain-less graph route still serves 200:\n{resp}"
    );
    let body = body_of(&resp);
    assert!(
        body.contains("\"seed\""),
        "an explain-less request is the seeded neighborhood: {body}"
    );
    assert!(
        !body.contains("\"leaves\""),
        "the neighborhood carries no rationale batch: {body}"
    );
}
