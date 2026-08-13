//! Periphery (contract / API / integration) test for spec 45, criterion 2 - the DIRECT-PROJECTION
//! REACH the dedicated `/api/graph` provider consults. The unit adds the public `Projector::whole`
//! read (the entire live projection, no seed and no reachability walk) and rewires the dash's lazy
//! graph provider to read through it, so the whole-graph overview and a seeded neighborhood reach ANY
//! node the projection holds - the fix for the never-built repo whose run-seed set is empty and whose
//! run-seeded read therefore collapses to `Graph::default()`.
//!
//! This layer runs OUTSIDE the crate, over the library's PUBLIC surface only (`Projector::open` ->
//! `Projection::apply` -> the new `whole` -> `dash::graph_seeds` / `Projection::subgraph` /
//! `dash::serve` + `/api/graph`), exactly as the real dash provider and any external caller reach it.
//! It cannot touch the private `conn`, the crate-internal fold helpers, or the binary-private
//! `dash_read_whole_graph` the implementer's in-binary test drives - so it guards what the two
//! inside-out tests are structurally blind to:
//!
//!   * the implementer's `sqlite` unit test uses a POPULATED projection and asserts only NODE sort;
//!     it never pins the EDGE ordering the `whole` contract promises, the read STABILITY across two
//!     reads, or the FRESH/empty projection degrading to an empty graph (never an error);
//!   * neither inside-out test proves, over the public API, that `whole` REACHES a node the
//!     run-seeded `subgraph(graph_seeds(events), depth)` structurally cannot; and
//!   * the implementer's `main.rs` test composes the BINARY-PRIVATE `dash_read_whole_graph` with the
//!     in-process `route`, never the real `serve` socket - so the whole-projection read crossing the
//!     wire on a never-built repo (real HTTP transport + framing) is unguarded.
//!
//! `contextgraph` and `dash` compile on BOTH the default and `--no-default-features` lanes, so these
//! tests are not feature-gated and run in both - the fold and the read they exercise are always
//! compiled.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Graph, Projection, REL_CONTAINS, REL_GOVERNS, TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE,
    TYPE_EDGE_INFERRED,
};
use rigger::dash::{self, DashInputs};
use rigger::eventstore::Event;

/// A `DecisionMade` event in its on-log JSON form (`id` decides, governing `governs`, superseding
/// `supersedes`) - built by hand so the test pins the wire contract the fold reads, not an in-crate
/// payload struct. Returned so the caller can BOTH apply it AND put it in a run log for
/// [`dash::graph_seeds`], the way a live run does.
fn decision_event(pos: u64, id: &str, summary: &str, governs: &[&str], supersedes: &str) -> Event {
    let payload = serde_json::json!({
        "id": id, "summary": summary, "governs": governs, "supersedes": supersedes,
    });
    let mut e = Event::new(TYPE_DECISION_MADE, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    e
}

/// A `CodeEntityExtracted` event (`file` defines `name`) in its on-log JSON form - one definition the
/// extraction pass emits, folded into a file container node, a `file::name` code-entity node, and the
/// structural CONTAINS edge between them. Code ingest emits these with NO decision/finding, so a
/// projection built from them alone has an EMPTY run-seed set.
fn code_entity_event(pos: u64, file: &str, name: &str, line: u32) -> Event {
    let payload = serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
    });
    let mut e = Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&payload).unwrap(),
    );
    e.position = pos;
    e
}

/// An `EdgeInferred` event (`file` references `name`) in its on-log JSON form - one reference the
/// extraction pass emits, folded into a REFERENCES edge to the referenced symbol's `file::name` node
/// (created bare when the name is not defined in the file).
fn edge_inferred_event(pos: u64, file: &str, name: &str) -> Event {
    let payload = serde_json::json!({ "file": file, "name": name, "lang": "rust" });
    let mut e = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&payload).unwrap());
    e.position = pos;
    e
}

/// The comparable projection of a graph's edges: `(from, to, rel, valid_from)` per edge, in the order
/// the read returned them. `Edge` derives no `PartialEq`, so the test compares this tuple projection,
/// which is also exactly the key the `whole` contract promises to sort by.
fn edge_keys(g: &Graph) -> Vec<(String, String, String, i64)> {
    g.edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone(), e.rel.clone(), e.valid_from))
        .collect()
}

/// The comparable projection of a graph's nodes: `(id, kind)` per node, in read order.
fn node_keys(g: &Graph) -> Vec<(String, String)> {
    g.nodes
        .iter()
        .map(|n| (n.id.clone(), n.kind.clone()))
        .collect()
}

/// API contract (spec 45, criterion 2): `whole` is "deterministic by construction" - the read itself
/// sorts, independent of any downstream fold. This pins the two determinism facts the implementer's
/// inside-out unit test omits (it asserts NODE sort only): (a) EDGES come back ordered by
/// `(from, to, rel, valid_from)`, regardless of the order the events were applied; and (b) two
/// successive reads of the same projection are byte-for-byte identical in BOTH their node and edge
/// sequences.
#[test]
fn whole_reads_the_projection_with_deterministic_node_and_edge_ordering() {
    let p = Projector::open(":memory:", "det").unwrap();

    // Apply in an order that is NOT the sorted `from_id` order (d2 before d1, files interleaved), so
    // a returned edge sequence sorted by `from_id` can only come from the read's own ORDER BY.
    p.apply(&decision_event(1, "d2", "second", &["z.rs"], ""))
        .unwrap();
    p.apply(&decision_event(2, "d1", "first", &["y.rs"], ""))
        .unwrap();
    p.apply(&code_entity_event(3, "a.rs", "run", 5)).unwrap();

    let g = p.whole().unwrap();

    // (a) edges come back ordered by the promised key.
    let keys = edge_keys(&g);
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(
        keys, sorted,
        "whole() returns edges ordered by (from, to, rel, valid_from), got {keys:?}"
    );
    // The GOVERNS edges are present and, being from `d1`/`d2`, sort AFTER the `a.rs` CONTAINS edge:
    // proof the ordering is the read's, not the apply order (which was d2, d1, a.rs).
    assert!(
        keys.iter()
            .any(|(f, t, r, _)| f == "d1" && t == "y.rs" && r == REL_GOVERNS)
            && keys
                .iter()
                .any(|(f, t, r, _)| f == "d2" && t == "z.rs" && r == REL_GOVERNS)
            && keys
                .iter()
                .any(|(f, _, r, _)| f == "a.rs" && r == REL_CONTAINS),
        "the two GOVERNS edges and the CONTAINS edge are all present, got {keys:?}"
    );

    // (b) a second read is identical in nodes AND edges - deterministic by construction.
    let g2 = p.whole().unwrap();
    assert_eq!(
        node_keys(&g),
        node_keys(&g2),
        "two whole() reads return an identical node sequence"
    );
    assert_eq!(
        edge_keys(&g),
        edge_keys(&g2),
        "two whole() reads return an identical edge sequence"
    );
}

/// API contract boundary edge: a FRESH, never-folded projection yields an EMPTY graph, never an error
/// (the `whole` docstring's "a fresh/empty graph yields an empty [`Graph`], never an error"). The
/// implementer's inside-out unit test only ever reads a populated projection, so the empty case - the
/// exact never-built-repo precondition this whole unit exists to survive - is unguarded there.
#[test]
fn whole_on_a_fresh_projection_is_empty_and_never_errors() {
    let p = Projector::open(":memory:", "fresh").unwrap();
    let g = p
        .whole()
        .expect("whole() on a fresh projection is Ok, never an error");
    assert!(
        g.nodes.is_empty() && g.edges.is_empty(),
        "a fresh projection reads back as an empty graph, got {g:?}"
    );
}

/// The criterion-2 REACH property at the public API: `whole` returns a node that the run-seeded
/// `subgraph(graph_seeds(events), depth)` - the read the OLD provider used - structurally cannot
/// reach, and it is a SUPERSET of that seeded read. A projection built from code ingest carries a
/// component (`src/unrelated.rs`) disconnected from any decision seed; the run-seeded walk from the
/// one decision never reaches it, but `whole` does. This pins the fix over the public surface, with
/// no dependence on the binary-private read the implementer's `main.rs` test uses.
#[test]
fn whole_reaches_nodes_the_run_seeded_subgraph_cannot_and_is_a_superset_of_it() {
    let p = Projector::open(":memory:", "reach").unwrap();

    // A decision seeds one component (d1 GOVERNS src/combat.rs, which CONTAINS apply_damage).
    let d1 = decision_event(1, "d1", "wire combat", &["src/combat.rs"], "");
    p.apply(&d1).unwrap();
    p.apply(&code_entity_event(2, "src/combat.rs", "apply_damage", 7))
        .unwrap();
    // A SEPARATE component with no path to the seed: code ingest of an unrelated file. No decision
    // or finding concerns it, so no run seed can ever reach it.
    p.apply(&code_entity_event(3, "src/unrelated.rs", "helper", 3))
        .unwrap();
    p.apply(&edge_inferred_event(4, "src/unrelated.rs", "detail"))
        .unwrap();

    // The run-seeded read the OLD provider used: seeds derived from the run log (the decision only),
    // walked depth-2. It reaches the decision's own component but NOT the unrelated one.
    let events = vec![d1];
    let seeds = dash::graph_seeds(&events);
    assert!(
        !seeds.is_empty(),
        "the decision contributes run seeds (its id + governed file)"
    );
    let seeded = p.subgraph(&seeds, 2).unwrap();
    let seeded_ids: Vec<&str> = seeded.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(
        !seeded_ids.contains(&"src/unrelated.rs::helper"),
        "the run-seeded walk cannot reach the disconnected code node, got {seeded_ids:?}"
    );

    let whole = p.whole().unwrap();
    let whole_ids: Vec<&str> = whole.nodes.iter().map(|n| n.id.as_str()).collect();
    // REACH: whole returns the node no run seed could reach.
    assert!(
        whole_ids.contains(&"src/unrelated.rs::helper"),
        "whole() reaches the disconnected code node with no seed, got {whole_ids:?}"
    );
    // SUPERSET: every node the seeded walk returned is also in the whole read.
    for id in &seeded_ids {
        assert!(
            whole_ids.contains(id),
            "whole() is a superset of the run-seeded read; missing {id}"
        );
    }
}

/// Populate a file-backed graph.db from CODE INGEST ALONE (no decision / finding) and return its
/// path. This is exactly what a cold-checkout `graph build` folds, and the never-built-repo
/// precondition: the run log carries no content event, so `graph_seeds` is empty and the run-seeded
/// read collapses to `Graph::default`.
fn code_ingest_db(dir: &std::path::Path, identity: &str) -> String {
    let path = dir.join("graph.db");
    let path = path.to_str().unwrap().to_string();
    let p = Projector::open(&path, identity).unwrap();
    p.apply(&code_entity_event(1, "src/combat.rs", "apply_damage", 7))
        .unwrap();
    p.apply(&code_entity_event(2, "src/combat.rs", "take_hit", 20))
        .unwrap();
    p.apply(&edge_inferred_event(3, "src/combat.rs", "clamp"))
        .unwrap();
    p.apply(&code_entity_event(4, "src/loot.rs", "roll_drop", 11))
        .unwrap();
    path
}

/// Start the dash server on a FRESH ephemeral loopback port whose graph provider is the REAL
/// whole-projection read - `Projector::open(db).whole()`, byte-for-byte what the production
/// `dash_read_whole_graph` does - over a file-backed graph.db, while the polled state provider
/// carries an EMPTY run-seeded graph (the never-built repo). Fetch `GET <path>` once and return the
/// raw response, or `None` on a genuine socket-level failure.
///
/// The listener this attempt binds is HANDED to `serve_on`, never dropped and re-bound. Releasing it
/// first would leave the port free for the whole handoff window, so a sibling test's `bind(0)` in
/// this same binary could be handed it; one `serve` then wins the re-bind and the loser's client
/// CONNECTS SUCCESSFULLY to it and reads the OTHER test's fixture - a content failure no
/// connect-error retry can see, reddening only on a loaded machine. Owning the port from `bind`
/// through `serve_on` closes that window by construction.
fn try_fetch_whole_served(graph_db: &str, identity: &str, path: &str) -> Option<String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let addr = listener.local_addr().ok()?;

    // The SEPARATE lazy graph provider (spec 45, criteria 1+2): opens the projection and reads the
    // WHOLE graph on a graph request - the exact read production wires into `/api/graph`.
    let graph_provider = {
        let db = graph_db.to_string();
        let id = identity.to_string();
        move |_instance: Option<&str>| -> Graph {
            match Projector::open(&db, &id) {
                Ok(p) => p.whole().unwrap_or_default(),
                Err(_) => Graph::default(),
            }
        }
    };
    // The polled STATE provider: a never-built repo has no run content, so its run-seeded graph is
    // empty. This is the other half of the provider split - `/api/graph` must still reach the whole
    // projection even though the state poll's graph is `Graph::default`.
    let provider = move |_instance: Option<&str>| -> Result<DashInputs, String> {
        Ok((Vec::new(), Graph::default(), Vec::new(), HashMap::new()))
    };
    // The lazy directed-call provider (spec 52, criterion 4): this test drives the overview /
    // neighborhood reach, not a call view, so an empty walk satisfies `serve`'s calls-provider bound.
    let calls_provider =
        |_: Option<&str>, _: &[String], _: rigger::contextgraph::Direction, _: i64, _: &str| {
            rigger::contextgraph::CallGraph::default()
        };
    let instances_provider = Vec::new;

    std::thread::spawn(move || {
        let _ = dash::serve_on(
            listener,
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
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
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

/// Drive the real `serve` socket, retrying the whole port handoff on a connection-level transient.
fn fetch_whole_served(graph_db: &str, identity: &str, path: &str) -> String {
    for _ in 0..200 {
        if let Some(resp) = try_fetch_whole_served(graph_db, identity, path) {
            return resp;
        }
    }
    panic!(
        "the dash server never served {path} over the real socket after many fresh-port attempts"
    );
}

/// The body (everything past the header terminator) of a raw HTTP response.
fn body_of(resp: &str) -> &str {
    resp.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body")
}

/// Integration (spec 45, criterion 2) over the REAL serve socket: on a never-built repo (a graph.db
/// populated by code ingest alone, empty run seeds), the dedicated `/api/graph` provider reads the
/// WHOLE projection, so BOTH the whole-graph overview and a seeded neighborhood reach real nodes -
/// instead of the `Graph::default` dead-end the run-seeded read produced. This crosses the wire
/// (transport + framing + JSON) through the real whole-projection read, which the implementer's
/// in-process `main.rs` test (binary-private read + in-process `route`) does not exercise.
#[test]
fn the_served_api_graph_reaches_the_whole_projection_when_run_seeds_are_empty() {
    let dir = tempfile::tempdir().unwrap();
    let identity = "wholeserve";
    let graph_db = code_ingest_db(dir.path(), identity);

    // Whole-graph overview: `/api/graph` with no seed clusters the WHOLE projection - non-empty
    // clusters and a positive total, never the empty default.
    let resp = fetch_whole_served(&graph_db, identity, "/api/graph");
    assert!(
        resp.starts_with("HTTP/1.1 200 OK") && resp.contains("application/json"),
        "GET /api/graph returns 200 JSON over the real serve socket:\n{resp}"
    );
    let overview: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served /api/graph overview is valid JSON");
    assert!(
        overview["total"].as_u64().unwrap_or(0) > 0
            && !overview["clusters"]
                .as_array()
                .map(|a| a.is_empty())
                .unwrap_or(true),
        "the whole-graph overview clusters real nodes, not an empty default, got {overview}"
    );

    // Seeded neighborhood: `/api/graph?seed=<real code node>` returns that node's neighborhood over
    // the whole projection - reachable with NO run seed at all.
    let seed = "src/combat.rs::apply_damage";
    let resp2 = fetch_whole_served(&graph_db, identity, &format!("/api/graph?seed={seed}"));
    assert!(
        resp2.starts_with("HTTP/1.1 200 OK"),
        "GET /api/graph?seed=... returns 200 over the real serve socket:\n{resp2}"
    );
    let nb: serde_json::Value = serde_json::from_str(body_of(&resp2))
        .expect("the served /api/graph?seed body is valid JSON");
    assert_eq!(nb["seed"], seed, "the neighborhood echoes the seed: {nb}");
    assert!(
        nb["nodes"]
            .as_array()
            .map(|a| a.iter().any(|n| n["id"] == seed))
            .unwrap_or(false),
        "the seeded neighborhood over the whole projection contains the seed node, got {nb}"
    );
}
