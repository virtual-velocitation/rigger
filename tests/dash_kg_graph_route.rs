//! Periphery (integration) test for the dash's UNIFIED-KG detail panel (spec 30, criterion c5): the
//! read-only `GET /api/graph?seed=&depth=&tier=` route returns the SEEDED NEIGHBORHOOD of a selected
//! node (nodes + TIER-TAGGED edges) as self-contained JSON, and the served page is wired so selecting
//! a tree node (or a graph node) SETS that seed - there is no hand-seeding. This criterion OWNS the
//! graph route and select-to-seed.
//!
//! This runs OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::serve`), and crosses
//! the REAL loopback HTTP socket the operator's browser actually hits. The implementer's inside-out
//! unit tests in `dash.rs` call the pure `route`/`neighborhood` IN-PROCESS: they are structurally
//! blind to the serve path (the `route` dispatch of `GET /api/graph` and the HTTP framing the socket
//! delivers). This layer proves the SERVED route - the bytes a client receives from the public
//! `serve` entrypoint - carries the c5 seeded neighborhood end-to-end, and that the served root page
//! ships the KG panel + the select-to-seed wiring the browser runs.
//!
//! Criterion 6 (query-path + god-node) and criterion 7 (explain provenance + client tier-filter)
//! EXTEND the same served route + panel, so their periphery guards live here too, each bound to its
//! own mechanism; the c5 tests stay scoped to the neighborhood shape (nodes + tier-tagged edges) and
//! the select-to-seed mechanism c5 owns.
//!
//! `dash`, `contextgraph` are compiled on BOTH the default and the `--no-default-features` lane (none
//! feature-gated), so this guards the served boundary in both lanes.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::Command;
use std::time::{Duration, Instant};

use rigger::contextgraph::{
    Edge, Graph, Node, KIND_DECISION, KIND_UNIT, REL_DECIDED, REL_REFERENCES, TIER_EXTRACTED,
    TIER_INFERRED,
};
use rigger::dash::{self, DashInputs};

/// A tier-tagged fixture neighborhood: a unit `u1`, the decision `d1` that DECIDED it (extracted),
/// and a code entity `c1` the decision REFERENCES (inferred). A depth-2 walk from `u1` reaches all
/// three; the two edges among them carry two distinct confidence tiers, so the served JSON proves
/// tier-tagged edges cross the wire.
fn fixture_graph() -> Graph {
    let node = |id: &str, kind: &str, summary: &str| Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: if summary.is_empty() {
            BTreeMap::new()
        } else {
            BTreeMap::from([("summary".to_string(), summary.to_string())])
        },
    };
    let edge = |from: &str, to: &str, rel: &str, tier: &str| Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: tier.to_string(),
    };
    Graph {
        nodes: vec![
            node("u1", KIND_UNIT, ""),
            node("d1", KIND_DECISION, "the d1 decision"),
            node("c1", "code-entity", ""),
        ],
        edges: vec![
            edge("d1", "u1", REL_DECIDED, TIER_EXTRACTED),
            edge("d1", "c1", REL_REFERENCES, TIER_INFERRED),
        ],
    }
}

/// A linear chain `n0 -> n1 -> ... -> n{len-1}` of BARE nodes (no summary / title / name), each edge
/// `extracted`. A depth-`d` walk from `n0` reaches exactly {n0..nd}, so the served neighborhood's node
/// count reads the EFFECTIVE (defaulted / clamped) depth straight off the wire; and a bare node's
/// served `label` is its own id (`node_label`'s final fallback), pinned here at the boundary.
fn chain_graph(len: usize) -> Graph {
    let nodes = (0..len)
        .map(|i| Node {
            id: format!("n{i}"),
            kind: KIND_UNIT.to_string(),
            attrs: BTreeMap::new(),
        })
        .collect();
    let edges = (0..len.saturating_sub(1))
        .map(|i| Edge {
            from: format!("n{i}"),
            to: format!("n{}", i + 1),
            rel: REL_REFERENCES.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        })
        .collect();
    Graph { nodes, edges }
}

/// Start `serve` on a FRESH ephemeral loopback port, fetch `GET <path>` once against a fixture-graph
/// provider, and return the raw HTTP response - or `None` when THIS attempt lost the free-port
/// handoff race (the same TOCTOU window `dash_decisions_progressive_disclosure` documents: the probe
/// binds port 0, learns the port, releases it, and `serve` re-binds it, so under parallel load the
/// released port can be re-taken before `serve` re-binds). A `None` is always a transient handoff
/// loss, never a content failure: a cleanly-served response is returned whole so the caller's
/// assertions run on it. Production `rigger dash` binds ONE stable port once and never drop-rebinds,
/// so it is never exposed to this test-harness race.
fn try_fetch_served(path: &str, graph: Graph) -> Option<String> {
    let port = TcpListener::bind(("127.0.0.1", 0))
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let provider = move || -> Result<DashInputs, String> {
        Ok((Vec::new(), graph.clone(), Vec::new(), HashMap::new()))
    };
    std::thread::spawn(move || {
        let _ = dash::serve(addr, provider, 3);
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
/// whole port handoff on a connection-level transient (see [`try_fetch_served`]). Each attempt is
/// independent, so the guard is deterministic without weakening what it proves.
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

/// The SERVED `/api/graph` route returns the seeded neighborhood as tier-tagged JSON over the real
/// `serve` socket: a well-formed `200 application/json` whose body is the neighborhood of the seed
/// (nodes + tier-tagged edges). Guards the serve/route/framing seam the pure in-process `route` test
/// is blind to.
#[test]
fn the_served_graph_route_returns_a_tier_tagged_seeded_neighborhood() {
    let graph = fixture_graph();
    let resp = fetch_served("/api/graph?seed=u1&depth=2", &graph);

    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET /api/graph returns 200 over the real serve socket:\n{resp}"
    );
    assert!(
        resp.contains("application/json"),
        "the served graph route is self-contained JSON:\n{resp}"
    );

    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served /api/graph body is valid JSON");
    assert_eq!(
        json["seed"], "u1",
        "the neighborhood echoes the seed: {json}"
    );

    // The depth-2 neighborhood of `u1` reaches all three fixture nodes.
    let ids: BTreeSet<&str> = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        ["u1", "d1", "c1"].into_iter().collect(),
        "the served neighborhood carries the reachable nodes: {json}"
    );
    // Every node carries its own label + kind (the decision's label is its summary).
    let d1 = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "d1")
        .unwrap();
    assert_eq!(
        d1["label"], "the d1 decision",
        "a node carries its label: {json}"
    );

    // The edges cross the wire TIER-TAGGED (extracted + inferred), the c5 charter.
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(
        edges.len(),
        2,
        "both in-neighborhood edges are returned: {json}"
    );
    let tiers: BTreeSet<&str> = edges.iter().map(|e| e["tier"].as_str().unwrap()).collect();
    assert_eq!(
        tiers,
        [TIER_EXTRACTED, TIER_INFERRED].into_iter().collect(),
        "each served edge is tagged with its confidence tier: {json}"
    );
    assert!(
        edges
            .iter()
            .all(|e| e["from"].is_string() && e["to"].is_string() && e["rel"].is_string()),
        "each served edge carries from/to/rel: {json}"
    );
}

/// The SERVED root page ships the unified-KG detail PANEL and the SELECT-TO-SEED wiring c5 owns: the
/// `kgpanel` render region, the read-only `GET /api/graph?seed=` fetch keyed on the selected node,
/// the `data-seed` handle the tree nodes carry, and the single delegated listener that maps a click
/// on a `data-seed` node to `seedGraph`. Structural, but bound to the c5 mechanism so a `<details>`
/// or `fetch` some OTHER panel emits cannot satisfy it.
#[test]
fn the_served_root_page_ships_the_kg_panel_and_select_to_seed_wiring() {
    let resp = fetch_served("/", &fixture_graph());
    assert!(
        resp.starts_with("HTTP/1.1 200 OK") && resp.contains("text/html"),
        "GET / returns a 200 HTML page over the real serve socket:\n{resp}"
    );
    let page = body_of(&resp);

    // The KG detail panel ships as its own render region.
    assert!(
        page.contains("id=\"kgpanel\""),
        "the served page must carry the KG detail panel region (id=kgpanel)"
    );
    // Select-to-seed fetches the read-only route, keyed on the selected node (encodeURIComponent'd).
    assert!(
        page.contains("/api/graph?seed=") && page.contains("encodeURIComponent(seed)"),
        "the served page must fetch /api/graph for the selected, url-encoded seed"
    );
    // The tree nodes carry the data-seed select handle, and the delegated listener drives seedGraph.
    assert!(
        page.contains("data-seed=") && page.contains("seedGraph("),
        "the served page must wire select-to-seed via a data-seed handle -> seedGraph"
    );
    assert!(
        page.contains("closest(\"[data-seed]\")"),
        "a single delegated listener must map a click on a data-seed node to the seed"
    );

    // Bind the tree render to the select handle: the tree node summary/leaf must carry data-seed, so
    // selecting a TREE node (not merely a graph node) sets the seed - the design's drill-in path.
    let t = page
        .find("function treeNode(")
        .expect("the served page carries the tree render");
    let tree_region = &page[t..(t + 900).min(page.len())];
    assert!(
        tree_region.contains("data-seed=\"' + esc(n.label)"),
        "each tree node must carry its label as the select-to-seed handle: {tree_region}"
    );

    // The neighborhood render must show the edge's confidence TIER (tier-tagged edges, c5 charter).
    assert!(
        page.contains("tierClass(") && page.contains("kgedge"),
        "the KG panel must render each edge with its confidence-tier badge"
    );
    // The panel is NOT written by render(): render() must never touch el("kgpanel"), so an operator
    // selection survives the live poll. (The runtime guard below proves the survival behaviorally.)
    let r = page
        .find("function render(state)")
        .expect("the served page carries render()");
    let render_end = page[r..]
        .find("\n// The run-tree spine")
        .map(|i| r + i)
        .expect("render() ends before the tree helpers");
    assert!(
        !page[r..render_end].contains("kgpanel"),
        "render() must NOT touch the KG panel, so an operator's selection survives the live poll"
    );
}

/// True when a `node` runtime can be spawned (present on dev machines and on GitHub `ubuntu-latest`,
/// which ships Node.js on PATH, so this runtime guard runs in CI).
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Extract the single inline `<script>` body from the served page.
fn page_script(page: &str) -> &str {
    let open = page
        .find("<script>")
        .expect("the served page carries a <script>")
        + "<script>".len();
    let close = page
        .find("</script>")
        .expect("the served page closes its <script>");
    &page[open..close]
}

/// A DOM shim + test driver (JavaScript) that RUNS the served page's OWN select-to-seed path: it
/// dispatches a click carrying `data-seed` through the tree's delegated listener, lets `seedGraph`
/// fetch a fixture neighborhood, and asserts (a) the click SET the seed and fetched `/api/graph` for
/// it, (b) the panel rendered the tier-tagged neighborhood, and (c) a subsequent live-poll `render()`
/// leaves the KG panel UNTOUCHED (the operator's selection survives the poll).
///
/// The template-grep test above proves the wiring TOKENS ship; it is blind to the RUNTIME behavior -
/// that a click actually reaches `seedGraph`, that `seedGraph` fetches the right seed, and that
/// `render()` does not clobber the panel. This harness executes the real page script under node's
/// built-in `vm` (no npm, hermetic - `fetch` is a fixture and `setTimeout` is inert so the live tail
/// never touches the network). Mutation-proven: dropping the delegated listener (or letting
/// `render()` write `el("kgpanel")`) makes the driver throw.
const SELECT_TO_SEED_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script). A fixture neighborhood stands in for the
// /api/graph body; fetch resolves it for the graph route and rejects everything else (the load-time
// poll's /api/state fetch), recording the graph URL so the driver can assert the seed was carried.
const SHIM = String.raw`
const __els = {};
let __fetchedGraphUrl = "";
const __NB = { seed: "u1", depth: 2,
  nodes: [ { id: "u1", kind: "unit", label: "u1" }, { id: "d1", kind: "decision", label: "the d1 decision" } ],
  edges: [ { from: "d1", to: "u1", rel: "DECIDED", tier: "extracted" } ] };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={}; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    __fetchedGraphUrl = String(url);
    return Promise.resolve({ json: function(){ return Promise.resolve(__NB); } });
  }
  return Promise.reject(new Error("no network for " + url));
};
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope, so it calls el()/render()
// and reads kgSeed/__fetchedGraphUrl directly).
const DRIVER = String.raw`
;(async function(){
  // A click on a run-tree node carrying data-seed="u1" (what treeNode() emits), dispatched through
  // the tree container's delegated listener that wireSelectToSeed() attached at load.
  const tree = el("tree");
  const handlers = (tree._listeners && tree._listeners.click) || [];
  if (!handlers.length) throw new Error("no delegated click listener on the tree (select-to-seed unwired)");
  const target = { dataset: { seed: "u1" }, closest: function(sel){ return sel === "[data-seed]" ? this : null; } };
  handlers.forEach(function(fn){ fn({ target: target }); });

  // seedGraph() is async (fetch -> json -> renderGraph); flush the microtask queue so it completes.
  for (let k = 0; k < 12; k++) { await Promise.resolve(); }

  if (kgSeed !== "u1") throw new Error("clicking a data-seed node did not set the seed: " + kgSeed);
  if (__fetchedGraphUrl.indexOf("seed=u1") === -1)
    throw new Error("select-to-seed did not fetch /api/graph for the selected seed: " + __fetchedGraphUrl);
  const panel = el("kgpanel")._html;
  if (panel.indexOf("DECIDED") === -1) throw new Error("the KG panel did not render the neighborhood edge: " + panel);
  if (panel.indexOf("extracted") === -1) throw new Error("the KG panel did not render the edge confidence tier: " + panel);
  if (panel.indexOf("d1") === -1) throw new Error("the KG panel did not render the neighborhood node: " + panel);

  // A live-poll re-render must NOT wipe the operator's KG selection: render() never touches kgpanel.
  const before = el("kgpanel")._html;
  render({ run: { units: [] }, metrics: {}, step: { wave: [] }, graph: { decisions: [], findings: [] },
           tree: [], blockers: [], events: [], generated_at: 0, position: 1 });
  if (el("kgpanel")._html !== before)
    throw new Error("REGRESSION: the live poll re-render wiped the operator's KG panel selection");

  console.log("OK select-to-seed-and-survives-poll");
})().catch(function(e){ console.error(String((e && e.stack) || e)); throw e; });
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-kg-harness.js" });
"##;

/// RUNTIME guard for spec 30 c5's select-to-seed charter: selecting a node (a click on a `data-seed`
/// handle) SETS the seed, fetches its `/api/graph` neighborhood, renders it tier-tagged, and the
/// selection SURVIVES the 1.5s live poll. This drives the SERVED page's real listener + `seedGraph` +
/// `render()` under a DOM shim (via node's `vm`); it is the runtime check the grep test cannot make -
/// dropping the delegated listener, or letting `render()` clobber the panel, makes it go red.
#[test]
fn selecting_a_node_seeds_the_kg_panel_and_it_survives_the_live_poll() {
    if !node_available() {
        eprintln!(
            "SKIP selecting_a_node_seeds_the_kg_panel_and_it_survives_the_live_poll: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the KG harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, SELECT_TO_SEED_HARNESS).expect("write the KG harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served select-to-seed path");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "selecting a node must seed the KG panel and survive the live poll, but the runtime harness \
         failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK select-to-seed-and-survives-poll"),
        "the KG harness must confirm select-to-seed + poll-survival:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// The served `/api/graph` route's DEPTH query-param edges (spec 30 c5): the panel's `depth=` is
/// defaulted, clamped, and ECHOED on the wire. Over the real serve socket against a linear chain:
/// an omitted `depth` applies `DEFAULT_GRAPH_DEPTH` (2 hops); an over-large / hostile `depth` is
/// clamped to `MAX_GRAPH_DEPTH` (6 hops) so it can NEVER make the in-memory walk churn the whole
/// graph; a non-numeric `depth` falls back to the default (never a 500); and the response echoes the
/// EFFECTIVE depth (the field `renderGraph()` shows as "depth N"). These are the two public depth
/// consts + the route's parse/clamp/default that the in-process route tests never exercise at the
/// wire. Expectations are hardcoded (2, 6), not read from the consts, so a change to either const
/// reddens this guard.
#[test]
fn the_served_graph_route_defaults_and_clamps_the_depth_and_echoes_it() {
    // A chain long enough to distinguish default(2), clamp(6), and beyond: n0..n8 (9 nodes), so a
    // clamp to 6 provably excludes n7/n8.
    let graph = chain_graph(9);

    let ids = |resp: &str| -> BTreeSet<String> {
        let json: serde_json::Value =
            serde_json::from_str(body_of(resp)).expect("the served body is valid JSON");
        json["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_string())
            .collect()
    };
    let depth_of = |resp: &str| -> i64 {
        let json: serde_json::Value = serde_json::from_str(body_of(resp)).unwrap();
        json["depth"]
            .as_i64()
            .expect("the neighborhood echoes its depth")
    };

    // An omitted depth -> DEFAULT_GRAPH_DEPTH = 2: reaches exactly {n0,n1,n2}, echoes depth 2.
    let def = fetch_served("/api/graph?seed=n0", &graph);
    assert!(
        def.starts_with("HTTP/1.1 200 OK"),
        "the default-depth route is served 200: {def}"
    );
    assert_eq!(
        depth_of(&def),
        2,
        "an omitted depth defaults to DEFAULT_GRAPH_DEPTH: {def}"
    );
    assert_eq!(
        ids(&def),
        ["n0", "n1", "n2"].iter().map(|s| s.to_string()).collect(),
        "the default-depth neighborhood is the 2-hop reach: {def}"
    );

    // An over-large / hostile depth -> clamped to MAX_GRAPH_DEPTH = 6: reaches {n0..n6}, NEVER n7/n8,
    // and echoes the clamped 6 - the guard that a hostile `depth=` cannot churn the whole graph.
    let big = fetch_served("/api/graph?seed=n0&depth=99999", &graph);
    assert_eq!(
        depth_of(&big),
        6,
        "an over-large depth is clamped to MAX_GRAPH_DEPTH: {big}"
    );
    assert_eq!(
        ids(&big),
        (0..=6).map(|i| format!("n{i}")).collect(),
        "a hostile depth is clamped to 6 hops, never churning the whole chain: {big}"
    );

    // A non-numeric depth -> falls back to the default (never a 500).
    let bad = fetch_served("/api/graph?seed=n0&depth=notanumber", &graph);
    assert!(
        bad.starts_with("HTTP/1.1 200 OK"),
        "a non-numeric depth is a graceful 200, not a 500: {bad}"
    );
    assert_eq!(
        depth_of(&bad),
        2,
        "a non-numeric depth falls back to the default: {bad}"
    );

    // depth=0 -> the seed node alone (depth is honored down to zero).
    let zero = fetch_served("/api/graph?seed=n0&depth=0", &graph);
    assert_eq!(
        ids(&zero),
        ["n0"].iter().map(|s| s.to_string()).collect(),
        "depth=0 is the seed node alone: {zero}"
    );

    // A bare node's served label is its id (node_label's id fallback), so the panel always has a
    // handle to render even for a node with no summary / title / name.
    let json: serde_json::Value = serde_json::from_str(body_of(&def)).unwrap();
    let n1 = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "n1")
        .unwrap();
    assert_eq!(n1["label"], "n1", "a bare node's label is its id: {def}");
}

/// The served `/api/graph` percent-decodes a special-char seed that crossed the REAL socket (spec 30
/// c5 select-to-seed): a graph node id carries `#` (a rationale's `<file>#L<line>`), `::`, and `/`,
/// which the client `encodeURIComponent`s onto `?seed=`. Over the wire the request-line + query
/// parsing must preserve the escapes and the route must decode them back to the EXACT node id, or
/// selecting such a node would seed an empty neighborhood. The in-process route test decodes
/// in-memory; this proves the decode survives the HTTP framing end-to-end.
#[test]
fn the_served_graph_route_percent_decodes_a_special_char_seed_over_the_socket() {
    let raw_id = "src/conductor.rs#L19930";
    let node = |id: &str| Node {
        id: id.to_string(),
        kind: "rationale".to_string(),
        attrs: BTreeMap::new(),
    };
    let graph = Graph {
        nodes: vec![node(raw_id), node("src/conductor.rs")],
        edges: vec![Edge {
            from: raw_id.to_string(),
            to: "src/conductor.rs".to_string(),
            rel: "explains".to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        }],
    };
    // encodeURIComponent("src/conductor.rs#L19930") == "src%2Fconductor.rs%23L19930".
    let resp = fetch_served(
        "/api/graph?seed=src%2Fconductor.rs%23L19930&depth=1",
        &graph,
    );
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the special-char seed is served 200: {resp}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served body is valid JSON");
    assert_eq!(
        json["seed"], raw_id,
        "the route percent-decodes the seed back to the exact id over the socket: {json}"
    );
    let ids: BTreeSet<&str> = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(raw_id) && ids.contains("src/conductor.rs"),
        "the decoded seed reaches its own node and its neighbor: {json}"
    );
}

/// A star graph: one `hub` wired to `spokes` bare leaf nodes (each edge `extracted`). A depth-1 walk
/// from the hub carries every hub-spoke edge, so the hub's in-neighborhood degree is exactly `spokes`
/// - the fixture the served GOD-NODE flag reads off the wire.
fn star_graph(hub: &str, spokes: usize) -> Graph {
    let mut nodes = vec![Node {
        id: hub.to_string(),
        kind: KIND_UNIT.to_string(),
        attrs: BTreeMap::new(),
    }];
    let mut edges = Vec::new();
    for i in 0..spokes {
        let spoke = format!("{hub}-s{i}");
        nodes.push(Node {
            id: spoke.clone(),
            kind: "code-entity".to_string(),
            attrs: BTreeMap::new(),
        });
        edges.push(Edge {
            from: hub.to_string(),
            to: spoke,
            rel: REL_REFERENCES.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        });
    }
    Graph { nodes, edges }
}

/// The SERVED `/api/graph` route carries the c6 QUERY-PATH + GOD-NODE analysis over the real socket:
/// (a) a seeded neighborhood flags a high-degree hub as a god-node (with its in-neighborhood degree)
/// on every node, and a seed-only request omits the path; (b) a `from=&to=` request also returns the
/// shortest query-path between the two selected nodes. Guards the serve/route/framing seam the pure
/// in-process route test is blind to, and proves the c6 fields cross the wire in both feature lanes.
#[test]
fn the_served_graph_route_flags_god_nodes_and_returns_the_query_path() {
    // A hub wired to one more than the threshold's worth of spokes: strictly above the threshold, so
    // it crosses the wire flagged god. Expressed off the public const so a threshold change reddens.
    let spokes = dash::GOD_NODE_DEGREE_THRESHOLD + 1;
    let graph = star_graph("hub", spokes);
    let resp = fetch_served("/api/graph?seed=hub&depth=1", &graph);
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the god-node neighborhood is served 200: {resp}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served body is valid JSON");

    let hub = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "hub")
        .unwrap();
    assert_eq!(
        hub["god"], true,
        "a high-degree hub crosses the wire flagged god: {json}"
    );
    assert_eq!(
        hub["degree"].as_u64().unwrap(),
        spokes as u64,
        "the hub's in-neighborhood degree crosses the wire: {json}"
    );
    let spoke = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "hub-s0")
        .unwrap();
    assert_eq!(
        spoke["god"], false,
        "a leaf spoke is not a god-node: {json}"
    );
    assert_eq!(
        spoke["degree"].as_u64().unwrap(),
        1,
        "a leaf spoke has degree 1: {json}"
    );
    // A seed-only request carries NO query path (the panel highlights a path only for two selections).
    assert!(
        json.get("path").is_none(),
        "a seed-only neighborhood omits the query path: {json}"
    );

    // A from=&to= request returns the shortest query-path between the two selected nodes on the wire.
    let chain = chain_graph(5); // n0 -> n1 -> n2 -> n3 -> n4
    let resp2 = fetch_served("/api/graph?seed=n0&depth=4&from=n0&to=n3", &chain);
    assert!(
        resp2.starts_with("HTTP/1.1 200 OK"),
        "the query-path request is served 200: {resp2}"
    );
    let json2: serde_json::Value = serde_json::from_str(body_of(&resp2)).unwrap();
    let got: Vec<&str> = json2["path"]
        .as_array()
        .expect("a from+to request carries the query path over the wire")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        got,
        vec!["n0", "n1", "n2", "n3"],
        "the served route returns the shortest path between the two selected nodes: {json2}"
    );
}

/// The SERVED root page ships the c6 client rendering: a GOD-NODE badge keyed off the server's `god`
/// flag + `degree`, the QUERY-PATH highlight keyed off the returned `path`, and the shift-click that
/// selects a second node and fetches the `from=&to=` path over the read-only route. Structural, but
/// bound to the c6 mechanism so some OTHER panel's markup cannot satisfy it.
#[test]
fn the_served_root_page_renders_god_nodes_and_the_query_path() {
    let resp = fetch_served("/", &fixture_graph());
    assert!(
        resp.starts_with("HTTP/1.1 200 OK") && resp.contains("text/html"),
        "GET / returns a 200 HTML page over the real serve socket:\n{resp}"
    );
    let page = body_of(&resp);

    // The god-node badge is conditional on the server's `god` flag and shows the `degree`.
    assert!(
        page.contains("n.god ?") && page.contains("kggod"),
        "the page must render a god-node badge conditioned on n.god"
    );
    assert!(
        page.contains("n.degree"),
        "the god-node badge must show the node's in-neighborhood degree"
    );
    // The query-path highlight is keyed off the returned `path`, applied to nodes AND edges.
    assert!(
        page.contains("g.path") && page.contains("onpath"),
        "the page must highlight the query path (nodes/edges) from the returned path"
    );
    // A shift-click selects the second endpoint and fetches the from=&to= path over the route.
    assert!(
        page.contains("shiftKey") && page.contains("pathTo("),
        "the page must wire a shift-click to trace the query path"
    );
    assert!(
        page.contains("&from=") && page.contains("&to="),
        "the path request must fetch /api/graph with from= and to= endpoints"
    );
    // The panel is NOT written by render(): render() must never touch el("kgpanel"), so the operator's
    // selection (and any traced path) survives the live poll - the c5 poll-survival invariant c6 keeps.
    let r = page
        .find("function render(state)")
        .expect("the served page carries render()");
    let render_end = page[r..]
        .find("\n// The run-tree spine")
        .map(|i| r + i)
        .expect("render() ends before the tree helpers");
    assert!(
        !page[r..render_end].contains("kgpanel"),
        "render() must NOT touch the KG panel, so a selection/path survives the live poll"
    );
}

/// A DOM shim + test driver (JavaScript) that RUNS the served page's OWN c6 rendering: (A) it calls
/// `renderGraph` with a neighborhood carrying a `god` node and a `path`, and asserts the panel shows
/// the god-node badge (with the degree) and highlights the path; (B) it seeds a node, then dispatches
/// a SHIFT-click on a second node through the KG panel's delegated listener, and asserts `pathTo`
/// fetched `/api/graph` with `from=&to=` and the panel highlighted the returned path. The
/// template-grep test above proves the TOKENS ship; this proves the RUNTIME behavior. Hermetic under
/// node's built-in `vm` (no npm; `fetch` is a fixture, `setTimeout` inert). Mutation-proven: dropping
/// the god badge (or the onpath highlight, or the shift-click branch) makes the driver throw.
const GOD_PATH_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

const SHIM = String.raw`
const __els = {};
let __fetchedGraphUrl = "";
const __NB_PATH = { seed: "a", depth: 2,
  nodes: [ { id: "a", kind: "unit", label: "a", degree: 1, god: false },
           { id: "b", kind: "unit", label: "b", degree: 2, god: false },
           { id: "h", kind: "unit", label: "h", degree: 6, god: true } ],
  edges: [ { from: "a", to: "b", rel: "REFERENCES", tier: "extracted" },
           { from: "b", to: "h", rel: "REFERENCES", tier: "inferred" } ],
  path: [ "a", "b", "h" ] };
const __NB_SEED = { seed: "a", depth: 2,
  nodes: [ { id: "a", kind: "unit", label: "a", degree: 1, god: false },
           { id: "b", kind: "unit", label: "b", degree: 1, god: false } ],
  edges: [ { from: "a", to: "b", rel: "REFERENCES", tier: "extracted" } ] };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={}; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    __fetchedGraphUrl = String(url);
    const body = String(url).indexOf("from=") !== -1 ? __NB_PATH : __NB_SEED;
    return Promise.resolve({ json: function(){ return Promise.resolve(body); } });
  }
  return Promise.reject(new Error("no network for " + url));
};
const setTimeout = function(){ return 0; };
`;

const DRIVER = String.raw`
;(async function(){
  // (A) Direct render of a neighborhood carrying a god node + a query path: the badge + highlight show.
  renderGraph(__NB_PATH);
  let panel = el("kgpanel")._html;
  if (panel.indexOf("kggod") === -1) throw new Error("no god-node badge rendered: " + panel);
  if (panel.indexOf("deg 6") === -1) throw new Error("god badge missing the degree: " + panel);
  if (panel.indexOf("onpath") === -1) throw new Error("the query path was not highlighted: " + panel);
  // The badge is CONDITIONAL on n.god: exactly one of the three nodes (h) is a god-node.
  if ((panel.match(/kggod/g) || []).length !== 1)
    throw new Error("the god badge must be conditional on n.god (expected exactly one): " + panel);

  // (B) Seed a node, then SHIFT-click a second: pathTo fetches from=&to= and highlights the path.
  await seedGraph("a");
  for (let k = 0; k < 12; k++) { await Promise.resolve(); }
  if (el("kgpanel")._html.indexOf("onpath") !== -1)
    throw new Error("a seed-only render must not highlight a path");
  const panelEl = el("kgpanel");
  const handlers = (panelEl._listeners && panelEl._listeners.click) || [];
  if (!handlers.length) throw new Error("no delegated click listener on the KG panel");
  const target = { dataset: { seed: "h" }, closest: function(sel){ return sel === "[data-seed]" ? this : null; } };
  handlers.forEach(function(fn){ fn({ target: target, shiftKey: true }); });
  for (let k = 0; k < 12; k++) { await Promise.resolve(); }
  if (__fetchedGraphUrl.indexOf("from=") === -1 || __fetchedGraphUrl.indexOf("to=h") === -1)
    throw new Error("a shift-click did not request the query path (from/to): " + __fetchedGraphUrl);
  if (el("kgpanel")._html.indexOf("onpath") === -1)
    throw new Error("the shift-click query path was not highlighted: " + el("kgpanel")._html);

  console.log("OK god-node-and-query-path");
})().catch(function(e){ console.error(String((e && e.stack) || e)); throw e; });
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-kg-c6-harness.js" });
"##;

/// RUNTIME guard for spec 30 c6's client rendering: a GOD-NODE flagged by the server renders a badge
/// (with its degree), the returned QUERY-PATH highlights the nodes/edges on it, and a SHIFT-click on a
/// second node fetches the `from=&to=` path and highlights it. Drives the SERVED page's real
/// `renderGraph` + `pathTo` + delegated listener under a DOM shim (node's `vm`) - the runtime check
/// the grep test cannot make.
#[test]
fn a_god_node_renders_a_badge_and_a_shift_click_traces_the_query_path() {
    if !node_available() {
        eprintln!(
            "SKIP a_god_node_renders_a_badge_and_a_shift_click_traces_the_query_path: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the c6 KG harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, GOD_PATH_HARNESS).expect("write the c6 KG harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served god-node + query-path rendering");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a god-node must render a badge and a shift-click must trace the query path, but the \
         runtime harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK god-node-and-query-path"),
        "the c6 KG harness must confirm the god-node badge + query-path highlight:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// The served `/api/graph` percent-decodes the c6 `from=`/`to=` PATH ENDPOINTS that crossed the REAL
/// socket, exactly as it does the seed (spec 30 c6). A node id carries `#` (a rationale's
/// `<file>#L<line>`) and `/`, which the client `pathTo` `encodeURIComponent`s onto `?from=&to=`. Over
/// the wire those escapes must survive the request-line + query parsing and the route must decode BOTH
/// endpoints back to the EXACT ids, or shift-clicking such a node would trace no path. Percent-decode
/// of `from`/`to` happens ONLY in the route arm - the unit `path`/`graph_json` tests pass plain ids and
/// are structurally blind to it - so this proves the decode end-to-end for the c6 endpoints.
#[test]
fn the_served_graph_route_percent_decodes_special_char_path_endpoints() {
    let from_id = "src/conductor.rs#L100";
    let mid_id = "mid";
    let to_id = "src/dash.rs#L200";
    let node = |id: &str| Node {
        id: id.to_string(),
        kind: "rationale".to_string(),
        attrs: BTreeMap::new(),
    };
    let edge = |from: &str, to: &str| Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: "explains".to_string(),
        valid_from: 0,
        valid_to: None,
        source: 0,
        tier: TIER_EXTRACTED.to_string(),
    };
    // A 3-node chain of special-char endpoints: from -> mid -> to, so the shortest path is all three.
    let graph = Graph {
        nodes: vec![node(from_id), node(mid_id), node(to_id)],
        edges: vec![edge(from_id, mid_id), edge(mid_id, to_id)],
    };
    // encodeURIComponent("src/conductor.rs#L100") == "src%2Fconductor.rs%23L100" (and likewise `to`).
    let resp = fetch_served(
        "/api/graph?seed=src%2Fconductor.rs%23L100&depth=2&from=src%2Fconductor.rs%23L100&to=src%2Fdash.rs%23L200",
        &graph,
    );
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the special-char path request is served 200: {resp}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served body is valid JSON");
    let got: Vec<&str> = json["path"]
        .as_array()
        .expect("a from+to request with decodable endpoints carries the query path")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        got,
        vec![from_id, mid_id, to_id],
        "the route percent-decodes BOTH endpoints back to their exact ids and traces the path \
         between them over the socket: {json}"
    );
}

/// The served `/api/graph` returns the query path ONLY when BOTH endpoints are given AND a route
/// between them exists; otherwise the `path` key is OMITTED from the JSON entirely (never `[]`), so the
/// client's `g.path` is falsy and nothing is highlighted (spec 30 c6). Four omission cases + one
/// control, all over the real socket: (1) `from=` alone, (2) `to=` alone, (3) both given but the target
/// is unreachable, (4) both given but an endpoint is absent - each a graceful 200 with NO path key; the
/// control (both given, reachable) DOES carry it. The `skip_serializing_if` omission is observable only
/// through serialization, so the unit `path` test (which returns a Vec) is blind to it.
#[test]
fn the_served_graph_route_omits_the_path_for_a_partial_or_unreachable_selection() {
    // A connected chain n0 -> n1 -> n2, plus an ISOLATED node with no edges: reachable within the
    // chain, never from it.
    let mut graph = chain_graph(3);
    graph.nodes.push(Node {
        id: "iso".to_string(),
        kind: KIND_UNIT.to_string(),
        attrs: BTreeMap::new(),
    });

    let has_path_key = |path: &str| -> bool {
        let resp = fetch_served(path, &graph);
        assert!(
            resp.starts_with("HTTP/1.1 200 OK"),
            "{path} is a graceful 200: {resp}"
        );
        let json: serde_json::Value = serde_json::from_str(body_of(&resp)).unwrap();
        json.get("path").is_some()
    };

    // (1) from= alone and (2) to= alone: an incomplete two-node selection carries no path.
    assert!(
        !has_path_key("/api/graph?seed=n0&depth=4&from=n0"),
        "a from= without a to= omits the path key"
    );
    assert!(
        !has_path_key("/api/graph?seed=n0&depth=4&to=n2"),
        "a to= without a from= omits the path key"
    );
    // (3) both given, but iso is unreachable from n0: graceful-empty, path key omitted (not []).
    assert!(
        !has_path_key("/api/graph?seed=n0&depth=4&from=n0&to=iso"),
        "an unreachable target omits the path key rather than serializing []"
    );
    // (4) both given, but the target is not even a node: still a graceful 200 with no path key.
    assert!(
        !has_path_key("/api/graph?seed=n0&depth=4&from=n0&to=ghost"),
        "an absent endpoint omits the path key"
    );

    // Control: both given AND reachable -> the path IS present, so the omissions above are the
    // specific partial/unreachable behavior, not a route that never returns a path.
    let resp = fetch_served("/api/graph?seed=n0&depth=4&from=n0&to=n2", &graph);
    let json: serde_json::Value = serde_json::from_str(body_of(&resp)).unwrap();
    let got: Vec<&str> = json["path"]
        .as_array()
        .expect("a complete, reachable selection carries the path")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        got,
        vec!["n0", "n1", "n2"],
        "the control selection carries the shortest path over the wire: {json}"
    );
}

/// The SERVED `/api/graph` route carries the c7 EXPLAIN PROVENANCE of the seed node over the real
/// socket: `explain(<seed>)` returns the events/decisions that produced the selected node as the
/// source-stamped edges incident to it. Over the wire against a graph whose seed's incident edges
/// carry DISTINCT source event positions and tiers, the served body's `explain` names the seed and
/// lists those provenance edges with their `source` positions - so the panel answers explain with no
/// new route param and no second query. Guards the serve/route/framing seam the pure route test is
/// blind to, and proves the c7 explain DTO crosses the wire in both feature lanes.
#[test]
fn the_served_graph_route_carries_the_seed_nodes_explain_provenance_over_the_socket() {
    let node = |id: &str, kind: &str| Node {
        id: id.to_string(),
        kind: kind.to_string(),
        attrs: BTreeMap::new(),
    };
    let edge = |from: &str, to: &str, rel: &str, tier: &str, source: u64| Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source,
        tier: tier.to_string(),
    };
    // The seed decision `d1` DECIDED `u1` (event 7, extracted) and REFERENCES `c1` (event 9,
    // inferred): its provenance is two edges folded by two DISTINCT events at two tiers.
    let graph = Graph {
        nodes: vec![
            node("d1", KIND_DECISION),
            node("u1", KIND_UNIT),
            node("c1", "code-entity"),
        ],
        edges: vec![
            edge("d1", "u1", REL_DECIDED, TIER_EXTRACTED, 7),
            edge("d1", "c1", REL_REFERENCES, TIER_INFERRED, 9),
        ],
    };
    let resp = fetch_served("/api/graph?seed=d1&depth=2", &graph);
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the explain request is served 200: {resp}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served body is valid JSON");

    assert_eq!(
        json["explain"]["node"], "d1",
        "the served response explains the seed node: {json}"
    );
    let sources: BTreeSet<u64> = json["explain"]["sources"]
        .as_array()
        .expect("the explain provenance carries its sources over the wire")
        .iter()
        .map(|s| s["source"].as_u64().unwrap())
        .collect();
    assert_eq!(
        sources,
        [7, 9].into_iter().collect(),
        "the seed's provenance carries the distinct SOURCE EVENT positions that produced it: {json}"
    );
    let tiers: BTreeSet<&str> = json["explain"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["tier"].as_str().unwrap())
        .collect();
    assert_eq!(
        tiers,
        [TIER_EXTRACTED, TIER_INFERRED].into_iter().collect(),
        "each provenance edge carries its confidence tier over the wire: {json}"
    );
    assert!(
        json["explain"]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["rel"].is_string() && s["from"].is_string() && s["to"].is_string()),
        "each provenance edge carries its relation + endpoints: {json}"
    );
}

/// The SERVED `/api/graph` route degrades the c7 EXPLAIN provenance GRACEFULLY over the real socket,
/// pinning the two graceful-empty boundaries the present-case test above cannot see. The `explain`
/// field is `Option<Explanation>` with `skip_serializing_if = "Option::is_none"`, so its omission is
/// observable ONLY through the wire serialization - the in-process `route` unit test asserts on
/// `r.body` and is blind to the serve/framing seam. Two cases + one control, all over the socket:
/// (A) an UNKNOWN seed (not a graph node) has nothing to explain -> the `explain` key is OMITTED
/// entirely from the JSON (never `null`), so the client's `g.explain` is falsy and the panel shows no
/// provenance section (the graceful-empty it degrades to); (B) a seed node that EXISTS but is
/// ISOLATED (no incident edges) is present-but-empty -> `explain` IS carried, names the seed, and its
/// `sources` is `[]` (distinct from the OMITTED unknown-seed case, so the two graceful states do not
/// collapse into one); and the control (a seed with real provenance) DOES carry non-empty sources.
/// Guards the skip_serializing_if omission end-to-end over the socket in both feature lanes.
#[test]
fn the_served_graph_route_omits_explain_for_an_unknown_seed_and_is_empty_for_an_isolated_node() {
    // A connected pair `d1 -> u1` (real provenance), plus an ISOLATED node `iso` that is a graph node
    // but carries no incident edges - so `iso` explains to an EMPTY sources list, while an id that is
    // not a node at all explains to NOTHING (the key omitted).
    let graph = Graph {
        nodes: vec![
            Node {
                id: "d1".to_string(),
                kind: KIND_DECISION.to_string(),
                attrs: BTreeMap::new(),
            },
            Node {
                id: "u1".to_string(),
                kind: KIND_UNIT.to_string(),
                attrs: BTreeMap::new(),
            },
            Node {
                id: "iso".to_string(),
                kind: KIND_UNIT.to_string(),
                attrs: BTreeMap::new(),
            },
        ],
        edges: vec![Edge {
            from: "d1".to_string(),
            to: "u1".to_string(),
            rel: REL_DECIDED.to_string(),
            valid_from: 0,
            valid_to: None,
            source: 0,
            tier: TIER_EXTRACTED.to_string(),
        }],
    };

    let served_json = |path: &str| -> serde_json::Value {
        let resp = fetch_served(path, &graph);
        assert!(
            resp.starts_with("HTTP/1.1 200 OK"),
            "{path} is a graceful 200 (never an error for an unknown/isolated seed): {resp}"
        );
        serde_json::from_str(body_of(&resp)).expect("the served body is valid JSON")
    };

    // (A) An unknown seed has no node to explain: the `explain` key is OMITTED, not serialized `null`.
    let unknown = served_json("/api/graph?seed=ghost&depth=2");
    assert!(
        unknown.get("explain").is_none(),
        "an unknown seed omits the explain key over the socket (skip_serializing_if), rather than \
         serializing null: {unknown}"
    );

    // (B) An isolated node IS explained, but with an EMPTY sources list - present-but-empty, the
    // distinct graceful state from the OMITTED unknown-seed case above.
    let isolated = served_json("/api/graph?seed=iso&depth=2");
    assert_eq!(
        isolated["explain"]["node"], "iso",
        "an isolated seed still carries an explanation naming it: {isolated}"
    );
    let iso_sources = isolated["explain"]["sources"]
        .as_array()
        .expect("an isolated node's explain carries an (empty) sources array, not a missing key");
    assert!(
        iso_sources.is_empty(),
        "an isolated node's provenance is present but empty over the socket: {isolated}"
    );

    // Control: a seed with real provenance carries a NON-empty sources list, so the omission/empty
    // above are the specific graceful-empty behaviors, not a route that never carries provenance.
    let control = served_json("/api/graph?seed=d1&depth=2");
    assert!(
        !control["explain"]["sources"]
            .as_array()
            .expect("a seed with provenance carries its sources")
            .is_empty(),
        "the control seed carries non-empty provenance over the socket: {control}"
    );
}

/// A DOM shim + test driver (JavaScript) that RUNS the served page's OWN c7 rendering: it calls
/// `renderGraph` with a tier-tagged neighborhood that ALSO carries a c6 god node, a c6 query path,
/// and the c7 `explain` provenance, then (A) asserts both tier edges draw and the god badge, path
/// highlight, and provenance section all render; (B) toggles the INFERRED tier OFF through the KG
/// panel's delegated `change` listener and asserts that tier's edge is HIDDEN while the extracted
/// edge and the c6 god/path render survive (a visibility toggle that COEXISTS with c6, per
/// d30-c6-client-god-path-render); (C) toggles it back ON to prove the filter is reversible; and (D)
/// asserts a live-poll `render()` leaves the panel untouched, so the tier filter/selection survive
/// the poll. Hermetic under node's built-in `vm` (no npm; `fetch` unused here, `setTimeout` inert).
/// Mutation-proven: dropping the tier filter (or the explain render) in `renderGraph` makes it throw.
const TIER_FILTER_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

const SHIM = String.raw`
const __els = {};
const __NB = { seed: "u1", depth: 2,
  nodes: [ { id: "u1", kind: "unit", label: "u1", degree: 6, god: true },
           { id: "d1", kind: "decision", label: "the d1 decision", degree: 2, god: false },
           { id: "c1", kind: "code-entity", label: "c1", degree: 1, god: false } ],
  edges: [ { from: "d1", to: "u1", rel: "DECIDED", tier: "extracted" },
           { from: "d1", to: "c1", rel: "REFERENCES", tier: "inferred" } ],
  path: [ "u1", "d1" ],
  explain: { node: "u1", sources: [ { rel: "DECIDED", from: "d1", to: "u1", tier: "extracted", source: 42 } ] } };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={}; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const fetch = function(url){ return Promise.reject(new Error("no network for " + url)); };
const setTimeout = function(){ return 0; };
`;

const DRIVER = String.raw`
;(async function(){
  const panel = function(){ return el("kgpanel")._html; };
  const edgeCount = function(){ return (panel().match(/class="kgedge/g) || []).length; };

  // (A) Render the c7-extended neighborhood: both tier edges draw; the c6 god badge + path highlight
  // COEXIST; the c7 explain provenance (with its source event position) renders; a tier toggle ships.
  renderGraph(__NB);
  if (edgeCount() !== 2) throw new Error("both tier edges must draw initially: " + panel());
  if (panel().indexOf("REFERENCES") === -1) throw new Error("the inferred edge must draw initially: " + panel());
  if (panel().indexOf("kggod") === -1) throw new Error("the c6 god badge must COEXIST with the tier filter: " + panel());
  if (panel().indexOf("onpath") === -1) throw new Error("the c6 path highlight must COEXIST with the tier filter: " + panel());
  if (panel().indexOf("kgprov") === -1) throw new Error("the c7 explain provenance section must render: " + panel());
  if (panel().indexOf("42") === -1) throw new Error("the explain provenance must show the source event position: " + panel());
  if (panel().indexOf('data-tier="inferred"') === -1) throw new Error("a tier toggle checkbox must ship per tier: " + panel());

  // (B) Toggle the INFERRED tier OFF via the panel's delegated change listener: its edge is HIDDEN,
  // the extracted edge stays, and the c6 god badge + path highlight survive (visibility, not replace).
  const panelEl = el("kgpanel");
  const changers = (panelEl._listeners && panelEl._listeners.change) || [];
  if (!changers.length) throw new Error("no delegated change listener on the KG panel (tier toggle unwired)");
  const mk = function(checked){ return { target: { dataset: { tier: "inferred" }, checked: checked,
    closest: function(sel){ return sel === "[data-tier]" ? this : null; } } }; };
  changers.forEach(function(fn){ fn(mk(false)); });
  if (edgeCount() !== 1) throw new Error("toggling inferred OFF must hide its edge: " + panel());
  if (panel().indexOf("REFERENCES") !== -1) throw new Error("the inferred edge must be HIDDEN after toggling it off: " + panel());
  if (panel().indexOf("DECIDED") === -1) throw new Error("the extracted edge must stay visible: " + panel());
  if (panel().indexOf("kggod") === -1) throw new Error("the god badge must survive the tier toggle: " + panel());
  if (panel().indexOf("onpath") === -1) throw new Error("the path highlight must survive the tier toggle: " + panel());

  // (C) Toggle it back ON: the hidden edge returns (a reversible visibility toggle).
  changers.forEach(function(fn){ fn(mk(true)); });
  if (edgeCount() !== 2) throw new Error("toggling inferred back ON must restore its edge: " + panel());

  // (D) The tier filter + selection SURVIVE the live poll: render() never touches kgpanel.
  const before = panel();
  render({ run: { units: [] }, metrics: {}, step: { wave: [] }, graph: { decisions: [], findings: [] },
           tree: [], blockers: [], events: [], generated_at: 0, position: 1 });
  if (panel() !== before) throw new Error("REGRESSION: the live poll wiped the KG panel tier filter/selection");

  console.log("OK tier-filter-and-explain");
})().catch(function(e){ console.error(String((e && e.stack) || e)); throw e; });
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-kg-c7-harness.js" });
"##;

/// RUNTIME guard for spec 30 c7's client rendering: the confidence-TIER FILTER is a client-side
/// visibility toggle over the c5 tier tags (toggling a tier HIDES its edges and is reversible), and
/// the c7 EXPLAIN provenance section renders the seed's origin - both COEXISTING with the c6 god
/// badge + path highlight and SURVIVING the live poll. Drives the SERVED page's real `renderGraph` +
/// the delegated tier-toggle listener under a DOM shim (node's `vm`) - the runtime check the grep
/// test cannot make.
#[test]
fn toggling_a_tier_hides_that_tiers_edges_and_the_explain_provenance_renders() {
    if !node_available() {
        eprintln!(
            "SKIP toggling_a_tier_hides_that_tiers_edges_and_the_explain_provenance_renders: no \
             `node` runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the c7 KG harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, TIER_FILTER_HARNESS).expect("write the c7 KG harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served tier-filter + explain rendering");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "toggling a tier must hide its edges and the explain provenance must render, but the \
         runtime harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK tier-filter-and-explain"),
        "the c7 KG harness must confirm the tier filter + explain render:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// The SERVED root page ships the c7 client mechanisms: the tier-filter TOGGLES (a `data-tier`
/// checkbox per confidence tier, backed by a client-side visible-tier set that `renderGraph` filters
/// the drawn edges by) and the EXPLAIN provenance render (keyed off the server's `explain` DTO), the
/// toggle wired via a delegated `change` listener on the stable panel container. Structural, but
/// bound to the c7 mechanism so some OTHER markup cannot satisfy it. The c6 god/path tokens must
/// remain in `renderGraph`, proving the tier filter COEXISTS with (does not replace) the c6 render.
#[test]
fn the_served_root_page_ships_the_tier_toggles_and_the_explain_provenance() {
    let resp = fetch_served("/", &fixture_graph());
    assert!(
        resp.starts_with("HTTP/1.1 200 OK") && resp.contains("text/html"),
        "GET / returns a 200 HTML page over the real serve socket:\n{resp}"
    );
    let page = body_of(&resp);

    // The tier filter is a CLIENT-side visibility toggle: a data-tier checkbox per tier, a client
    // visible-tier set, and renderGraph filtering the DRAWN edges by it (never a server-side drop).
    assert!(
        page.contains("data-tier="),
        "the page must ship a data-tier toggle handle per confidence tier"
    );
    assert!(
        page.contains("kgTiers"),
        "the page must carry the client-side visible-tier set (kgTiers)"
    );
    assert!(
        page.contains("kgTiers.has"),
        "renderGraph must FILTER the drawn edges by the visible-tier set"
    );
    // The three confidence tiers are the toggle vocabulary.
    assert!(
        page.contains("\"extracted\"")
            && page.contains("\"inferred\"")
            && page.contains("\"ambiguous\""),
        "the tier toggles must cover extracted / inferred / ambiguous"
    );
    // The toggle is wired via a delegated `change` listener on the stable panel container, so it
    // survives the renderGraph innerHTML swaps (the same delegation the c5 select-to-seed uses).
    assert!(
        page.contains("\"change\"") && page.contains("closest(\"[data-tier]\")"),
        "a delegated change listener must map a tier-checkbox toggle to the visible-tier set"
    );

    // The explain provenance renders from the server's `explain` DTO into its own panel section.
    assert!(
        page.contains("g.explain"),
        "the panel must render the seed's provenance from the server explain DTO"
    );
    assert!(
        page.contains("kgprov"),
        "the explain provenance must render in its own section (kgprov)"
    );

    // The tier filter COEXISTS with the c6 render: the god badge + path highlight tokens remain.
    assert!(
        page.contains("kggod") && page.contains("onpath"),
        "the c7 tier filter must coexist with (not replace) the c6 god/path render"
    );
}
