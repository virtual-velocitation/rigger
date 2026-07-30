//! Periphery (contract / API / integration) tests for spec 52 criterion 4 - the DIRECTED-CALL
//! ROUTE `/api/graph?view=calls&dir=down|up|both`. The unit adds a second seeded branch to the dash
//! that dispatches to the store-side `Projection::calls` traversal through a lazy direct-projection
//! provider, and reuses the `Neighborhood` shape with the additive `dir` / `layer` / `frontier` /
//! `back` / `referenced_not_called` fields the layered left-to-right renderer draws.
//!
//! These run OUTSIDE the crate, over the library's PUBLIC surface only - they bind a loopback
//! listener and drive `rigger::dash::serve_on` over a REAL socket (transport + HTTP framing + JSON on
//! the wire), exactly as a browser (and the production `rigger dash`) reaches the route. That is a
//! genuinely different boundary than the implementer's inside-out unit tests: those call the private
//! `calls_view` / `calls_route` IN-PROCESS with a fake provider and never cross `handle_conn`, the
//! socket, or serde-on-the-wire. This layer guards what they are structurally blind to:
//!
//!  - the DISPATCH WIRING in `handle_conn`: a `view=calls` request is routed to the directed
//!    traversal BEFORE the polled read, so the whole-graph provider is NEVER consulted (the "opens
//!    only the calls provider" contract) - unreachable from an in-process `calls_route` call;
//!  - the DIRECTION contract end-to-end: `dir=down|up|both`, an absent dir, and an UNRECOGNIZED dir
//!    each pick the walk(s) the route invokes and the `dir` the body echoes, asserted on the CAPTURED
//!    provider invocations and the served JSON;
//!  - the SIGNED LAYER the route applies (callee `+hop`, caller `-hop`, seed `0`) crossing the wire,
//!    with `dir=both` centering one node array around the seed;
//!  - the `back` recursion marker and the multi-candidate `frontier` riding through to the JSON, and
//!    (the serde `skip_serializing_if` contract) OMITTED for a forward edge / resolved node;
//!  - the QUERY-PARAM plumbing from the request line through `handle_conn` -> `calls_route` -> the
//!    provider: the `depth` clamp, the `tier` floor default, the percent-decoded `seed`, and the
//!    spec-50 `instance` selector, asserted on the CAPTURED provider arguments;
//!  - the ADDITIVE-FIELD BYTE-IDENTITY: a plain `/api/graph?seed=` neighborhood NEVER gains `dir` /
//!    `layer` / `frontier` / `back` / `referenced_not_called`, and NEVER runs the directed traversal;
//!  - and one INTEGRATION over a real `Projector::calls` wired into `serve_on` exactly as the binary's
//!    calls-provider does, proving the cross-module seam (route -> provider -> store) over the socket.
//!
//! `dash` and `contextgraph` compile on BOTH the default and `--no-default-features` lanes (nothing
//! here is feature-gated), so these tests run in both. No reference to any external tool or project;
//! hyphens, never em dashes.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    CallEdge, CallGraph, CallNode, Direction, Edge, Graph, Node, Projection, KIND_CODE_ENTITY,
    KIND_FILE, REL_CALLS, TIER_AMBIGUOUS, TIER_INFERRED, TYPE_CODE_ENTITY_EXTRACTED,
    TYPE_EDGE_INFERRED,
};
use rigger::dash::{self, DashInputs, DEFAULT_GRAPH_DEPTH, MAX_GRAPH_DEPTH};
use rigger::eventstore::Event;

// ---------------------------------------------------------------------------
// Constructors for the hand-built CallGraph a controlled provider returns. Every field is public, so
// the test drives the ROUTE's translation with a deterministic input, independent of the traversal
// (which the c1/c3 periphery tests already prove). The traversal always returns a POSITIVE hop
// `layer`; the SIGN is the route's job (`call_node_view(cn, +/-1)`), which is exactly what we assert.
// ---------------------------------------------------------------------------

fn code_node(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: BTreeMap::new(),
    }
}

fn file_node(id: &str) -> Node {
    Node {
        id: id.to_string(),
        kind: KIND_FILE.to_string(),
        attrs: BTreeMap::new(),
    }
}

fn call_node(id: &str, layer: i64, frontier: Option<Vec<String>>) -> CallNode {
    CallNode {
        node: code_node(id),
        layer,
        frontier,
    }
}

fn calls_edge(from: &str, to: &str, back: bool) -> CallEdge {
    CallEdge {
        edge: Edge {
            from: from.to_string(),
            to: to.to_string(),
            rel: REL_CALLS.to_string(),
            valid_from: 1,
            valid_to: None,
            source: 1,
            tier: TIER_INFERRED.to_string(),
        },
        back,
    }
}

/// A standard directed provider: a DOWN walk `seed -> callee` and an UP walk `user -> seed`, seeded on
/// the SAME id so a `dir=both` request merges them around one centered seed. The UP walk carries a
/// referenced-but-not-called file in its sidecar; the DOWN walk carries none.
const SEED_ID: &str = "src/c.rs::caller";
fn standard_calls(dir: Direction) -> CallGraph {
    match dir {
        Direction::Down => CallGraph {
            nodes: vec![
                call_node(SEED_ID, 0, None),
                call_node("src/a.rs::callee", 1, None),
            ],
            edges: vec![calls_edge(SEED_ID, "src/a.rs::callee", false)],
            referenced_not_called: Vec::new(),
        },
        Direction::Up => CallGraph {
            nodes: vec![
                call_node(SEED_ID, 0, None),
                call_node("src/z.rs::user", 1, None),
            ],
            edges: vec![calls_edge("src/z.rs::user", SEED_ID, false)],
            referenced_not_called: vec![file_node("src/import_only.rs")],
        },
    }
}

// ---------------------------------------------------------------------------
// The driver: bind a loopback listener, serve on it, drive ONE request, and return what the test
// observes - the raw response, the CAPTURED provider invocations, and whether the whole-graph
// provider was consulted. Binding the listener directly (then `serve_on`, which serves an
// ALREADY-BOUND listener) sidesteps any free-port re-bind race - the kernel queues the connect on the
// bound socket even before the serve thread reaches its accept loop.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct CallArgs {
    instance: Option<String>,
    seeds: Vec<String>,
    direction: Direction,
    depth: i64,
    floor: String,
}

struct Probe {
    resp: String,
    calls: Vec<CallArgs>,
    graph_consulted: bool,
}

/// Drive `GET <target>` against a freshly-served dash whose `calls_provider` is `provider` (wrapped to
/// record every invocation's arguments) and whose whole-graph `graph_provider` returns `graph` while
/// recording that it was consulted. Returns the raw response plus those observations.
fn drive<P>(target: &str, graph: Graph, provider: P) -> Probe
where
    P: Fn(Option<&str>, &[String], Direction, i64, &str) -> CallGraph + Send + 'static,
{
    let calls_log: Arc<Mutex<Vec<CallArgs>>> = Arc::new(Mutex::new(Vec::new()));
    let graph_consulted = Arc::new(AtomicBool::new(false));

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral loopback port");
    let addr = listener.local_addr().expect("learn the bound port");

    // The polled STATE provider: empty run inputs. A `/api/graph` request never reads its graph (the
    // whole-graph read rides the SEPARATE `graph_provider`), and a `view=calls` request never reads
    // it at all.
    let state_provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
        Ok((Vec::new(), Graph::default(), Vec::new(), HashMap::new()))
    };
    // The whole-graph provider: records that it was consulted, so a `view=calls` request can be proven
    // to bypass it (dispatch precedence) and a plain neighborhood request to use it.
    let graph_provider = {
        let consulted = graph_consulted.clone();
        move |_instance: Option<&str>| -> Graph {
            consulted.store(true, Ordering::SeqCst);
            graph.clone()
        }
    };
    // The directed-call provider under test's boundary: capture the arguments the route passes, then
    // delegate to the test's `provider`.
    let calls_provider = {
        let log = calls_log.clone();
        move |instance: Option<&str>,
              seeds: &[String],
              direction: Direction,
              depth: i64,
              floor: &str|
              -> CallGraph {
            log.lock().unwrap().push(CallArgs {
                instance: instance.map(|s| s.to_string()),
                seeds: seeds.to_vec(),
                direction,
                depth,
                floor: floor.to_string(),
            });
            provider(instance, seeds, direction, depth, floor)
        }
    };
    let instances_provider = Vec::new;

    std::thread::spawn(move || {
        let _ = dash::serve_on(
            listener,
            state_provider,
            graph_provider,
            calls_provider,
            instances_provider,
            3,
            "rigger-run",
            "origin/main",
        );
    });

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("never connected to the served dash on {addr}: {e}"),
        }
    };
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set a read timeout on the client");
    let req = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    client.write_all(req.as_bytes()).expect("write the request");
    let mut resp = String::new();
    client
        .read_to_string(&mut resp)
        .expect("read the served response to EOF");

    let calls = calls_log.lock().unwrap().clone();
    let graph_consulted = graph_consulted.load(Ordering::SeqCst);
    Probe {
        resp,
        calls,
        graph_consulted,
    }
}

/// A `view=calls` provider whose walk is `standard_calls` regardless of the request path, so a single
/// `drive` returns the layered view for whichever direction(s) the route asks.
fn standard_provider(
) -> impl Fn(Option<&str>, &[String], Direction, i64, &str) -> CallGraph + Send + 'static {
    |_instance, _seeds, direction, _depth, _floor| standard_calls(direction)
}

/// The parsed JSON body of a raw HTTP response (everything past the header terminator).
fn body_json(resp: &str) -> serde_json::Value {
    let body = resp
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .expect("a served response has a body");
    serde_json::from_str(body)
        .unwrap_or_else(|e| panic!("the served body was not valid JSON: {e}\n---\n{resp}"))
}

/// Assert a 200 JSON status line and return the parsed body.
fn ok_json(probe: &Probe) -> serde_json::Value {
    assert!(
        probe.resp.starts_with("HTTP/1.1 200 OK") && probe.resp.contains("application/json"),
        "the calls route serves 200 application/json over the real socket:\n{}",
        probe.resp
    );
    body_json(&probe.resp)
}

/// The `layer` a node id carries in a served body's `nodes` array, if present (absent field -> None).
fn served_layer(body: &serde_json::Value, id: &str) -> Option<i64> {
    body["nodes"]
        .as_array()
        .and_then(|ns| ns.iter().find(|n| n["id"] == id))
        .and_then(|n| n.get("layer").and_then(|l| l.as_i64()))
}

// ===========================================================================
// The DOWN execution path.
// ===========================================================================

#[test]
fn the_served_calls_route_signs_callees_positive_echoes_dir_down_and_walks_only_down() {
    let probe = drive(
        "/api/graph?view=calls&dir=down&seed=src%2Fc.rs%3A%3Acaller&depth=5",
        Graph::default(),
        standard_provider(),
    );
    let body = ok_json(&probe);

    // The body echoes the DOWN direction and the percent-decoded seed.
    assert_eq!(
        body["dir"], "down",
        "a dir=down request echoes dir=down: {body}"
    );
    assert_eq!(
        body["seed"], SEED_ID,
        "the body echoes the decoded seed: {body}"
    );

    // The route applies the +1 sign: the seed at layer 0, the callee at +1.
    assert_eq!(
        served_layer(&body, SEED_ID),
        Some(0),
        "the seed sits at layer 0: {body}"
    );
    assert_eq!(
        served_layer(&body, "src/a.rs::callee"),
        Some(1),
        "the DOWN callee sits at layer +1 (the seed drawn at the LEFT): {body}"
    );

    // A DOWN walk carries no referenced-but-not-called sidecar (the field is UP-only and omitted).
    assert!(
        body.get("referenced_not_called").is_none(),
        "a DOWN view omits the referenced_not_called sidecar entirely: {body}"
    );

    // Dispatch: exactly ONE Down walk, and the whole-graph provider is never consulted.
    assert_eq!(
        probe.calls.len(),
        1,
        "dir=down runs the provider exactly once, got {:?}",
        probe.calls
    );
    assert_eq!(probe.calls[0].direction, Direction::Down);
    assert!(
        !probe.graph_consulted,
        "a view=calls request opens only the calls provider, never the whole-graph read"
    );
}

// ===========================================================================
// The UP call sites (with the referenced-but-not-called sidecar).
// ===========================================================================

#[test]
fn the_served_calls_route_negates_callers_carries_the_referenced_sidecar_and_walks_only_up() {
    let probe = drive(
        "/api/graph?view=calls&dir=up&seed=src%2Fc.rs%3A%3Acaller",
        Graph::default(),
        standard_provider(),
    );
    let body = ok_json(&probe);

    assert_eq!(body["dir"], "up", "a dir=up request echoes dir=up: {body}");
    assert_eq!(
        served_layer(&body, SEED_ID),
        Some(0),
        "the seed sits at layer 0: {body}"
    );
    assert_eq!(
        served_layer(&body, "src/z.rs::user"),
        Some(-1),
        "the UP caller sits at layer -1 (the seed drawn at the RIGHT): {body}"
    );

    // The UP sidecar is carried verbatim as flat FILE nodes.
    let refd: Vec<&str> = body["referenced_not_called"]
        .as_array()
        .map(|a| a.iter().filter_map(|n| n["id"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        refd,
        vec!["src/import_only.rs"],
        "the UP view carries the referenced-but-not-called file over the wire: {body}"
    );
    assert!(
        body["referenced_not_called"]
            .as_array()
            .map(|a| a.iter().all(|n| n["kind"] == KIND_FILE))
            .unwrap_or(false),
        "every referenced-but-not-called entry is a FILE node: {body}"
    );

    assert_eq!(
        probe.calls.len(),
        1,
        "dir=up runs the provider exactly once, got {:?}",
        probe.calls
    );
    assert_eq!(probe.calls[0].direction, Direction::Up);
    assert!(!probe.graph_consulted);
}

// ===========================================================================
// dir=both: both walks, one centered node array.
// ===========================================================================

#[test]
fn the_served_calls_route_centers_the_seed_and_runs_both_walks_for_dir_both() {
    let probe = drive(
        "/api/graph?view=calls&dir=both&seed=src%2Fc.rs%3A%3Acaller",
        Graph::default(),
        standard_provider(),
    );
    let body = ok_json(&probe);

    assert_eq!(
        body["dir"], "both",
        "a dir=both request echoes dir=both: {body}"
    );

    // ONE node array centers the seed at 0, callees on the +side, callers on the -side.
    assert_eq!(
        served_layer(&body, SEED_ID),
        Some(0),
        "seed centered at 0: {body}"
    );
    assert_eq!(
        served_layer(&body, "src/a.rs::callee"),
        Some(1),
        "the callee is on the +side: {body}"
    );
    assert_eq!(
        served_layer(&body, "src/z.rs::user"),
        Some(-1),
        "the caller is on the -side: {body}"
    );

    // The UP sidecar still rides through on a both walk.
    assert!(
        body["referenced_not_called"]
            .as_array()
            .map(|a| a.iter().any(|n| n["id"] == "src/import_only.rs"))
            .unwrap_or(false),
        "dir=both still carries the UP referenced-but-not-called sidecar: {body}"
    );

    // Dispatch: BOTH directions are walked (one Down, one Up), and the whole-graph read is bypassed.
    let dirs: Vec<Direction> = probe.calls.iter().map(|c| c.direction).collect();
    assert!(
        dirs.contains(&Direction::Down) && dirs.contains(&Direction::Up) && dirs.len() == 2,
        "dir=both runs exactly the Down and Up walks, got {dirs:?}"
    );
    assert!(!probe.graph_consulted);
}

// ===========================================================================
// The direction default: an absent or unrecognized dir is the DOWN execution path.
// ===========================================================================

#[test]
fn an_absent_or_unrecognized_dir_defaults_to_the_down_execution_path() {
    for target in [
        "/api/graph?view=calls&seed=src%2Fc.rs%3A%3Acaller",
        "/api/graph?view=calls&dir=sideways&seed=src%2Fc.rs%3A%3Acaller",
    ] {
        let probe = drive(target, Graph::default(), standard_provider());
        let body = ok_json(&probe);
        assert_eq!(
            body["dir"], "down",
            "{target} defaults to the DOWN execution path: {body}"
        );
        assert_eq!(
            probe.calls.len(),
            1,
            "{target} runs exactly one Down walk, got {:?}",
            probe.calls
        );
        assert_eq!(
            probe.calls[0].direction,
            Direction::Down,
            "{target} walks Down"
        );
    }
}

// ===========================================================================
// The query-param plumbing: depth clamp, tier floor default, seed decode, instance selector - all
// asserted on the CAPTURED provider arguments (the route -> provider contract crossing the wire).
// ===========================================================================

#[test]
fn the_route_clamps_depth_and_defaults_the_tier_floor_in_the_provider_arguments() {
    // depth over the max is clamped to MAX_GRAPH_DEPTH.
    let over = drive(
        "/api/graph?view=calls&seed=x&depth=9999",
        Graph::default(),
        standard_provider(),
    );
    assert_eq!(
        over.calls[0].depth, MAX_GRAPH_DEPTH,
        "an over-large depth is clamped to MAX_GRAPH_DEPTH in the provider argument, got {}",
        over.calls[0].depth
    );

    // an absent depth defaults to DEFAULT_GRAPH_DEPTH.
    let absent = drive(
        "/api/graph?view=calls&seed=x",
        Graph::default(),
        standard_provider(),
    );
    assert_eq!(
        absent.calls[0].depth, DEFAULT_GRAPH_DEPTH,
        "an absent depth defaults to DEFAULT_GRAPH_DEPTH in the provider argument, got {}",
        absent.calls[0].depth
    );

    // an absent tier defaults to the inferred FLOOR (the ambiguous tier excluded until opted in).
    assert_eq!(
        absent.calls[0].floor, TIER_INFERRED,
        "an absent tier passes the inferred floor to the provider, got {}",
        absent.calls[0].floor
    );

    // an explicit tier=ambiguous lowers the floor.
    let amb = drive(
        "/api/graph?view=calls&seed=x&tier=ambiguous",
        Graph::default(),
        standard_provider(),
    );
    assert_eq!(
        amb.calls[0].floor, TIER_AMBIGUOUS,
        "tier=ambiguous lowers the provider's floor, got {}",
        amb.calls[0].floor
    );
}

#[test]
fn the_route_percent_decodes_the_seed_and_threads_the_instance_selector_to_the_provider() {
    let probe = drive(
        "/api/graph?view=calls&dir=down&instance=inst-a&seed=src%2Fc.rs%3A%3Acaller",
        Graph::default(),
        standard_provider(),
    );
    let body = ok_json(&probe);

    // The seed reaches the provider percent-DECODED (a code-entity id carries `::` and `/`).
    assert_eq!(
        probe.calls[0].seeds,
        vec!["src/c.rs::caller".to_string()],
        "the seed reaches the provider percent-decoded, got {:?}",
        probe.calls[0].seeds
    );
    assert_eq!(
        body["seed"], SEED_ID,
        "the body echoes the decoded seed: {body}"
    );
    // The spec-50 attach selector is threaded to the provider so the walk opens that instance's store.
    assert_eq!(
        probe.calls[0].instance.as_deref(),
        Some("inst-a"),
        "the instance selector is threaded to the calls provider, got {:?}",
        probe.calls[0].instance
    );
}

// ===========================================================================
// The recursion `back` marker and the multi-candidate `frontier` ride through to the JSON, and the
// serde skip_serializing_if contract OMITS them for a forward edge / resolved node.
// ===========================================================================

#[test]
fn the_back_recursion_marker_and_the_multi_candidate_frontier_ride_through_to_the_json() {
    // A DOWN walk carrying a recursion (b calls a again) and a multi-candidate cross-file hop.
    let provider = |_i: Option<&str>, _s: &[String], _d: Direction, _depth: i64, _f: &str| {
        CallGraph {
            nodes: vec![
                call_node("src/c.rs::a", 0, None),
                call_node("src/c.rs::b", 1, None),
                // an unresolved multi-candidate hop: a marked frontier carrying its SORTED candidates.
                call_node(
                    "src/x.rs::amb",
                    2,
                    Some(vec![
                        "src/m.rs::amb".to_string(),
                        "src/n.rs::amb".to_string(),
                    ]),
                ),
            ],
            edges: vec![
                calls_edge("src/c.rs::a", "src/c.rs::b", false),
                calls_edge("src/c.rs::b", "src/x.rs::amb", false),
                // the recursion: b -> a points back at a shallower layer, marked not followed.
                calls_edge("src/c.rs::b", "src/c.rs::a", true),
            ],
            referenced_not_called: Vec::new(),
        }
    };
    let probe = drive(
        "/api/graph?view=calls&dir=down&seed=src%2Fc.rs%3A%3Aa",
        Graph::default(),
        provider,
    );
    let body = ok_json(&probe);

    let edges = body["edges"].as_array().expect("edges array");
    // The recursion edge is marked back:true on the wire.
    let back_edge = edges
        .iter()
        .find(|e| e["from"] == "src/c.rs::b" && e["to"] == "src/c.rs::a")
        .expect("the recursion edge is present");
    assert_eq!(
        back_edge["back"], true,
        "the recursion edge carries back:true over the wire: {back_edge}"
    );
    // A forward edge OMITS the back key entirely (skip_serializing_if), so a plain edge is unchanged.
    let fwd_edge = edges
        .iter()
        .find(|e| e["from"] == "src/c.rs::a" && e["to"] == "src/c.rs::b")
        .expect("the forward edge is present");
    assert!(
        fwd_edge.get("back").is_none(),
        "a forward edge omits the back marker entirely: {fwd_edge}"
    );

    // The multi-candidate hop carries its SORTED candidate ids as `frontier`.
    let amb = body["nodes"]
        .as_array()
        .and_then(|ns| ns.iter().find(|n| n["id"] == "src/x.rs::amb"))
        .expect("the frontier node is present");
    let candidates: Vec<&str> = amb["frontier"]
        .as_array()
        .map(|a| a.iter().filter_map(|c| c.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        candidates,
        vec!["src/m.rs::amb", "src/n.rs::amb"],
        "the frontier node carries its candidate ids over the wire: {amb}"
    );
    // A fully-resolved node OMITS the frontier key.
    let resolved = body["nodes"]
        .as_array()
        .and_then(|ns| ns.iter().find(|n| n["id"] == "src/c.rs::b"))
        .expect("the resolved node is present");
    assert!(
        resolved.get("frontier").is_none(),
        "a fully-resolved node omits the frontier marker: {resolved}"
    );
}

// ===========================================================================
// Additive-field byte-identity: a plain neighborhood NEVER gains the call fields, and NEVER runs the
// directed traversal - the back-compat contract the additive `#[serde(skip_serializing_if)]` promises.
// ===========================================================================

#[test]
fn a_plain_neighborhood_request_gains_no_call_fields_and_never_runs_the_directed_traversal() {
    // A whole-graph the neighborhood route walks: n1 -- CALLS --> n2, both reachable at depth 2.
    let graph = Graph {
        nodes: vec![code_node("src/p.rs::n1"), code_node("src/p.rs::n2")],
        edges: vec![Edge {
            from: "src/p.rs::n1".to_string(),
            to: "src/p.rs::n2".to_string(),
            rel: REL_CALLS.to_string(),
            valid_from: 1,
            valid_to: None,
            source: 1,
            tier: TIER_INFERRED.to_string(),
        }],
    };
    // The calls provider must exist for `serve_on` but must never be called by a plain request.
    let never = |_i: Option<&str>, _s: &[String], _d: Direction, _depth: i64, _f: &str| {
        CallGraph::default()
    };
    let probe = drive("/api/graph?seed=src%2Fp.rs%3A%3An1&depth=2", graph, never);
    let body = ok_json(&probe);

    // The neighborhood really returned nodes and an edge (so the omission below is meaningful).
    assert_eq!(
        body["seed"], "src/p.rs::n1",
        "the neighborhood echoes the seed: {body}"
    );
    assert!(
        body["nodes"]
            .as_array()
            .map(|a| a.iter().any(|n| n["id"] == "src/p.rs::n1")
                && a.iter().any(|n| n["id"] == "src/p.rs::n2"))
            .unwrap_or(false),
        "the plain neighborhood carries its nodes: {body}"
    );

    // NONE of the additive call fields appear on a plain neighborhood.
    assert!(
        body.get("dir").is_none(),
        "a plain neighborhood omits the dir marker: {body}"
    );
    assert!(
        body.get("referenced_not_called").is_none(),
        "a plain neighborhood omits the referenced_not_called sidecar: {body}"
    );
    for n in body["nodes"].as_array().unwrap() {
        assert!(
            n.get("layer").is_none() && n.get("frontier").is_none(),
            "a plain neighborhood node omits layer and frontier: {n}"
        );
    }
    for e in body["edges"].as_array().unwrap() {
        assert!(
            e.get("back").is_none(),
            "a plain neighborhood edge omits the back marker: {e}"
        );
    }

    // The directed traversal was never run; the neighborhood read the whole-graph provider instead.
    assert!(
        probe.calls.is_empty(),
        "a plain neighborhood request never invokes the calls provider, got {:?}",
        probe.calls
    );
    assert!(
        probe.graph_consulted,
        "a plain neighborhood request reads the whole-graph provider"
    );
}

// ===========================================================================
// Integration over a REAL Projector::calls wired into serve_on exactly as the binary's calls-provider
// is - proving the cross-module seam (route -> provider -> store) end-to-end over the socket, which
// the controlled-provider tests (and the implementer's in-process unit tests) stub out.
// ===========================================================================

/// Fold a code DEFINITION (`file` defines `name`) from its raw on-log JSON - the wire form the always
/// compiled fold reads, deliberately not an in-crate payload struct.
fn apply_def(p: &Projector, pos: u64, file: &str, name: &str, line: u32) {
    let payload = serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust", "fresh": true,
    });
    let mut e = Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&payload).unwrap(),
    );
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Fold a CALLER-ATTRIBUTED reference (spec 37): `file`'s `caller` calls `name`, folded into
/// `<file>::<caller> --CALLS--> <target>`.
fn apply_call(p: &Projector, pos: u64, file: &str, name: &str, caller: &str) {
    let payload =
        serde_json::json!({ "file": file, "name": name, "lang": "rust", "caller": caller });
    let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    p.apply(&e).unwrap();
}

#[test]
fn the_served_calls_route_walks_the_real_projection_over_the_wire() {
    // A real graph.db from code ingest alone: b.rs::caller calls callee, DEFINED cross-file in a.rs.
    let dir = tempfile::tempdir().unwrap();
    let graph_db = dir.path().join("graph.db").to_string_lossy().into_owned();
    let identity = "callsroute";
    {
        let p = Projector::open(&graph_db, identity).unwrap();
        apply_def(&p, 1, "src/a.rs", "callee", 1);
        apply_def(&p, 2, "src/b.rs", "caller", 1);
        apply_call(&p, 3, "src/b.rs", "callee", "caller");
    }

    // The calls provider the binary wires (dash_read_calls): open the projection per request and run
    // the store-side directed walk, best-effort to an empty CallGraph.
    let provider = {
        let db = graph_db.clone();
        let id = identity.to_string();
        move |_instance: Option<&str>,
              seeds: &[String],
              direction: Direction,
              depth: i64,
              floor: &str|
              -> CallGraph {
            match Projector::open(&db, &id) {
                Ok(p) => p.calls(seeds, direction, depth, floor).unwrap_or_default(),
                Err(_) => CallGraph::default(),
            }
        }
    };

    // A DOWN walk from the caller resolves the cross-file callee onto its real definition, over the
    // wire, and the seed sits at layer 0.
    let probe = drive(
        "/api/graph?view=calls&dir=down&seed=src%2Fb.rs%3A%3Acaller&depth=5",
        Graph::default(),
        provider,
    );
    let body = ok_json(&probe);

    assert_eq!(
        body["dir"], "down",
        "the served real walk echoes dir=down: {body}"
    );
    assert_eq!(
        served_layer(&body, "src/b.rs::caller"),
        Some(0),
        "the seed sits at layer 0 over the real projection: {body}"
    );
    assert!(
        body["nodes"]
            .as_array()
            .map(|a| a.iter().any(|n| n["id"] == "src/a.rs::callee"))
            .unwrap_or(false),
        "the real DOWN walk resolves the cross-file callee onto its definition, served over the wire: {body}"
    );
    // The whole-graph provider is bypassed even for a real-store call view (dispatch precedence).
    assert!(
        !probe.graph_consulted,
        "the real calls route still opens only the calls provider, never the whole-graph read"
    );
}
