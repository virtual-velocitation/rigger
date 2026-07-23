//! Periphery (integration) test for the dash's WHOLE-GRAPH EXPLORATION VIZ (spec 42, criterion c5):
//! the SERVED-PAGE VIZ WIRING. Spec 42 c1-c4 built the projections (`cluster_key`,
//! `clustered_overview`, `cluster_detail`) and the `/api/graph` three-view route dispatch; this
//! criterion OWNS the served page's library-free SVG viz that DRAWS them - the deterministic
//! force layout, the SVG emit, the overview/drill renderers, the pan+zoom, and the delegated
//! `data-cluster` (drill) / `data-kgback` (overview) dispatch that rides alongside the existing
//! spec-30 `data-seed` (seed) - plus defaulting the KG panel to the clustered overview on load.
//!
//! Two layers, spec-30 style:
//!   * a STRUCTURAL assertion (this criterion's OWN done-when) that the served page SHIPS the viz
//!     functions + the on-load default + the three-way delegated dispatch (grep on the served bytes,
//!     bound to the c5 mechanism so some OTHER panel's tokens cannot satisfy it), and
//!   * a RUNTIME harness (node's built-in `vm`, hermetic, no npm) that DRIVES the served page's own
//!     viz: `loadKgOverview` renders clickable clusters, a `data-cluster` click DRILLS, a
//!     `data-kgback` click returns to the overview, a member's `data-seed` click hands off to the
//!     spec-30 seeded panel UNCHANGED, `forceLayout` yields FINITE positions with NO `Math.random`,
//!     and an EMPTY graph degrades to a message rather than throwing. The runtime layer is what the
//!     grep cannot make - that the viz actually LAYS OUT and DISPATCHES - and is mutation-proven:
//!     dropping the `data-cluster` dispatch, or a non-finite layout, makes the driver throw.
//!
//! `dash` compiles on BOTH the default and the `--no-default-features` lane (the viz is not
//! feature-gated), so this guards the served page in both lanes.

use std::process::Command;

use rigger::dash;

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

/// True when a `node` runtime can be spawned (present on dev machines and on GitHub `ubuntu-latest`,
/// which ships Node.js on PATH, so this runtime guard runs in CI).
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The SERVED root page SHIPS the whole-graph exploration viz (spec 42 c5): the deterministic
/// force layout, the SVG emit (`<circle>`/`<line>`/`<text>`), the overview + drill renderers, the
/// pan+zoom over a `#kgzoom` group, the on-load default to the clustered overview, and the delegated
/// `data-cluster` (drill) / `data-kgback` (overview) dispatch alongside the spec-30 `data-seed`
/// (seed). Structural, but each assertion is bound to the c5 mechanism so an unrelated token cannot
/// satisfy it.
#[test]
fn the_served_page_ships_the_exploration_viz_and_three_way_dispatch() {
    let page = dash::live_page();

    // The KG detail panel region still ships (spec 30) - the viz fills it.
    assert!(
        page.contains("id=\"kgpanel\""),
        "the served page must carry the KG panel region (id=kgpanel) the viz fills"
    );

    // The force layout: a deterministic Fruchterman-Reingold sim, seeded on a spiral, with NO
    // Math.random anywhere on the page (determinism by construction, dash charter).
    assert!(
        page.contains("function forceLayout("),
        "the served page must ship the force-layout function (forceLayout)"
    );
    // Determinism by construction: the layout is spiral-seeded, so NO Math.random() is ever CALLED
    // (a comment may name the API; the guard forbids the call - the `(` distinguishes them).
    assert!(
        !page.contains("Math.random("),
        "the layout must be seeded deterministically - NO Math.random() call anywhere on the page"
    );
    // Per-connected-component layout for a disconnected drill (union-find) + shelf packing, so the
    // pieces tile the panel instead of flinging to the corners.
    assert!(
        page.contains("function ufFind(") || page.contains("union") || page.contains("component"),
        "the layout must lay out each connected component (union-find) for a disconnected graph"
    );

    // The SVG emit: the viz draws circles/lines/text into an SVG (the library-free graph draw).
    assert!(
        page.contains("function kgSvg("),
        "the served page must ship the SVG-emit function (kgSvg)"
    );
    assert!(
        page.contains("<circle") && page.contains("<line") && page.contains("<text"),
        "the SVG emit must draw <circle>/<line>/<text> nodes, edges, and labels"
    );

    // The two renderers: the overview (clusters) and the drill (a cluster's members).
    assert!(
        page.contains("function renderKgOverview("),
        "the served page must ship the overview renderer (renderKgOverview)"
    );
    assert!(
        page.contains("function renderKgDrill("),
        "the served page must ship the drill renderer (renderKgDrill)"
    );

    // Pan + zoom: a transformed `#kgzoom` group, a wheel zoom, and drag handlers on window (installed
    // once) so a drag that leaves the svg still tracks.
    assert!(
        page.contains("id=\"kgzoom\""),
        "the viz must transform a #kgzoom group for pan/zoom"
    );
    assert!(
        page.contains("\"wheel\""),
        "the viz must zoom on the wheel event"
    );
    assert!(
        page.contains("window.addEventListener(\"mousemove\"")
            && page.contains("window.addEventListener(\"mouseup\""),
        "drag handlers must live on window (installed once) so a drag that leaves the svg still tracks"
    );

    // The on-load default: the panel opens on the CLUSTERED OVERVIEW (spec 42 c5), fetched from the
    // no-argument /api/graph route (the c4 default view). The load call is present.
    assert!(
        page.contains("function loadKgOverview("),
        "the served page must ship the overview loader (loadKgOverview)"
    );
    assert!(
        page.contains("loadKgOverview()"),
        "the served page must DEFAULT the KG panel to the clustered overview on load"
    );
    // The overview loader fetches /api/graph with NO seed and NO cluster (the c4 default view).
    let l = page
        .find("function loadKgOverview(")
        .expect("the served page carries loadKgOverview");
    let loader = &page[l..(l + 700).min(page.len())];
    assert!(
        loader.contains("/api/graph"),
        "loadKgOverview must fetch the default /api/graph overview: {loader}"
    );

    // The drill fetch: /api/graph?cluster=<key>, url-encoded like a seed.
    assert!(
        page.contains("function drillCluster("),
        "the served page must ship the cluster-drill fetch (drillCluster)"
    );
    let d = page
        .find("function drillCluster(")
        .expect("the served page carries drillCluster");
    let driller = &page[d..(d + 700).min(page.len())];
    assert!(
        driller.contains("/api/graph?cluster=") && driller.contains("encodeURIComponent"),
        "drillCluster must fetch /api/graph?cluster= for the url-encoded key: {driller}"
    );

    // The THREE-WAY delegated dispatch on the stable tree/kgpanel containers: a click on a
    // data-cluster node DRILLS, a data-kgback node returns to the OVERVIEW, and a data-seed node
    // SEEDS (spec 30, unchanged) - all three survive the innerHTML swaps that destroy the nodes.
    assert!(
        page.contains("closest(\"[data-cluster]\")"),
        "a delegated listener must dispatch a data-cluster click to the drill"
    );
    assert!(
        page.contains("closest(\"[data-kgback]\")"),
        "a delegated listener must dispatch a data-kgback click to the overview"
    );
    assert!(
        page.contains("closest(\"[data-seed]\")"),
        "the delegated data-seed dispatch (spec 30 select-to-seed) must remain, unregressed"
    );
    // The drill dispatch and the overview dispatch route to their handlers.
    assert!(
        page.contains("drillCluster(") && page.contains("loadKgOverview("),
        "the delegated dispatch must call drillCluster (drill) and loadKgOverview (back to overview)"
    );

    // The overview toolbar caption ("N nodes in M clusters ...") and the drill back link
    // ("overview") + the truncated caption ride the renderers.
    assert!(
        page.contains("data-cluster=") && page.contains("data-kgback="),
        "the renderers must emit the data-cluster (drill) and data-kgback (overview) handles"
    );

    // The panel is NOT written by render(): render() must never touch el("kgpanel"), so the operator's
    // exploration survives the 1.5s live poll (the same discipline spec 30 pinned).
    let r = page
        .find("function render(state)")
        .expect("the served page carries render()");
    let render_end = page[r..]
        .find("\n// The run-tree spine")
        .map(|i| r + i)
        .expect("render() ends before the tree helpers");
    assert!(
        !page[r..render_end].contains("kgpanel"),
        "render() must NOT touch the KG panel, so the operator's exploration survives the live poll"
    );
}

/// A DOM shim + test driver (JavaScript) that RUNS the served page's OWN exploration viz under
/// node's built-in `vm` (no npm, hermetic). It drives the real page script, asserting in turn that:
/// (a) `loadKgOverview()` fetches the default `/api/graph` and renders an SVG with CLICKABLE clusters
/// (each `data-cluster`), captioned "N nodes in M clusters"; (b) a delegated `data-cluster` click
/// DRILLS - `drillCluster` fetches `/api/graph?cluster=<key>` and renders the members (each
/// `data-seed`) with a `data-kgback` back link; (c) a delegated `data-kgback` click returns to the
/// OVERVIEW (re-fetches the default view); (d) `forceLayout` yields FINITE numeric positions (the sim
/// converges, no NaN); and (e) an EMPTY overview degrades to a message and does NOT throw.
/// Mutation-proven: dropping the data-cluster dispatch, or a non-finite layout, reddens the driver.
const VIZ_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script). fetch resolves the /api/graph views
// (overview default, cluster drill) from fixtures and records the URLs so the driver can assert the
// right view was fetched; every other fetch (the load-time /api/state poll) rejects. setTimeout is
// inert (the live tail never touches the network). A tiny SVG-ish element stub lets the pan/zoom
// binder's querySelector("#kgzoom") / querySelector("svg") resolve without throwing.
const SHIM = String.raw`
const __els = {};
const __fetched = [];
// The whole-graph overview: two clusters joined by one weighted cross-edge, three nodes total.
const __OVERVIEW = { clusters: [ { key: "src", count: 2, kind: "code-entity" },
                                 { key: "docs", count: 1, kind: "design-doc" } ],
                     edges: [ { from: "docs", to: "src", weight: 3 } ], total: 3 };
// The drill of the "src" cluster: two member nodes joined by one edge.
const __DRILL = { seed: "src", depth: 0,
  nodes: [ { id: "src/a.rs::f", kind: "code-entity", label: "f", degree: 1, god: false },
           { id: "src/b.rs::g", kind: "code-entity", label: "g", degree: 1, god: false } ],
  edges: [ { from: "src/a.rs::f", to: "src/b.rs::g", rel: "CALLS", tier: "extracted" } ] };
const __EMPTY = { clusters: [], edges: [], total: 0 };
function __Stub(){ this._attrs = {}; }
__Stub.prototype.setAttribute = function(k,v){ this._attrs[k] = String(v); };
__Stub.prototype.getAttribute = function(k){ return this._attrs[k]; };
__Stub.prototype.addEventListener = function(){};
__Stub.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={};
  this.clientWidth = 800; this.clientHeight = 300;
  this.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; }; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.querySelector = function(){ return new __Stub(); };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){}, };
function __view(url){
  const s = String(url);
  if (s.indexOf("cluster=") !== -1) return __DRILL;
  if (s.indexOf("seed=") !== -1 && s.indexOf("seed=&") === -1) return __DRILL; // a member seed handoff (spec 30)
  return __OVERVIEW;
}
let __graphView = __view;
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    __fetched.push(String(url));
    return Promise.resolve({ json: function(){ return Promise.resolve(__graphView(url)); } });
  }
  return Promise.reject(new Error("no network for " + url));
};
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope, so it calls the page's
// own functions and reads its module state directly).
const DRIVER = String.raw`
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<20;k++) await Promise.resolve(); })(); }

  // (4) forceLayout converges to FINITE positions with no NaN (drive it directly on a small graph).
  const fl = forceLayout(
    [ { id: "a" }, { id: "b" }, { id: "c" } ],
    [ { from: "a", to: "b" }, { from: "b", to: "c" } ], 800, 300);
  for (const id of ["a","b","c"]) {
    const p = fl[id];
    if (!p || !isFinite(p.x) || !isFinite(p.y))
      throw new Error("forceLayout produced a non-finite position for " + id + ": " + JSON.stringify(p));
  }

  // (1) On load the panel DEFAULTS to the clustered overview: loadKgOverview fetched the default view
  // and rendered clickable clusters.
  __fetched.length = 0;
  loadKgOverview();
  await flush();
  const ov = el("kgpanel")._html;
  if (__fetched.length === 0 || __fetched[0].indexOf("cluster=") !== -1)
    throw new Error("loadKgOverview did not fetch the default overview view: " + JSON.stringify(__fetched));
  if (ov.indexOf("data-cluster=") === -1)
    throw new Error("the overview did not render clickable clusters (data-cluster): " + ov);
  if (ov.indexOf("2 clusters") === -1 && ov.indexOf("in 2") === -1)
    throw new Error("the overview toolbar did not report the cluster count: " + ov);
  if (ov.indexOf("<circle") === -1 || ov.indexOf("<svg") === -1)
    throw new Error("the overview did not draw an SVG of super-nodes: " + ov);

  // (2) A delegated data-cluster click DRILLS the "src" cluster: drillCluster fetches cluster=src and
  // renders member nodes (each data-seed) with a data-kgback back link.
  const panel = el("kgpanel");
  const clickHandlers = (panel._listeners && panel._listeners.click) || [];
  if (!clickHandlers.length) throw new Error("no delegated click listener on the kg panel (viz unwired)");
  __fetched.length = 0;
  const clusterTarget = { dataset: { cluster: "src" },
    closest: function(sel){ return sel === "[data-cluster]" ? this : null; } };
  clickHandlers.forEach(function(fn){ fn({ target: clusterTarget, preventDefault: function(){} }); });
  await flush();
  if (!__fetched.some(function(u){ return u.indexOf("cluster=src") !== -1; }))
    throw new Error("a data-cluster click did not drill (fetch cluster=src): " + JSON.stringify(__fetched));
  const drill = el("kgpanel")._html;
  if (drill.indexOf("data-seed=") === -1)
    throw new Error("the drill did not render members as select-to-seed handles (data-seed): " + drill);
  if (drill.indexOf("data-kgback=") === -1)
    throw new Error("the drill did not render a back-to-overview link (data-kgback): " + drill);

  // (3) A delegated data-kgback click returns to the OVERVIEW (re-fetches the default view).
  __fetched.length = 0;
  const backTarget = { dataset: { kgback: "1" },
    closest: function(sel){ return sel === "[data-kgback]" ? this : null; } };
  clickHandlers.forEach(function(fn){ fn({ target: backTarget, preventDefault: function(){} }); });
  await flush();
  if (__fetched.length === 0 || __fetched[__fetched.length-1].indexOf("cluster=") !== -1)
    throw new Error("a data-kgback click did not return to the overview view: " + JSON.stringify(__fetched));
  if (el("kgpanel")._html.indexOf("data-cluster=") === -1)
    throw new Error("returning to the overview did not re-render the clusters: " + el("kgpanel")._html);

  // (5) An EMPTY overview degrades to a message, never throws.
  __graphView = function(){ return __EMPTY; };
  __fetched.length = 0;
  loadKgOverview();
  await flush();
  const empty = el("kgpanel")._html;
  if (empty.indexOf("empty") === -1 && empty.indexOf("no ") === -1)
    throw new Error("an empty graph did not degrade to an empty-graph message: " + empty);

  // (6) A live-poll render() must NOT wipe the operator's exploration (render never touches kgpanel).
  __graphView = __view;
  loadKgOverview();
  await flush();
  const before = el("kgpanel")._html;
  render({ run: { units: [] }, metrics: {}, step: { wave: [] }, graph: { decisions: [], findings: [] },
           tree: [], blockers: [], events: [], generated_at: 0, position: 1 });
  if (el("kgpanel")._html !== before)
    throw new Error("REGRESSION: the live poll re-render wiped the operator's KG exploration");

  console.log("OK exploration-viz-drives-and-dispatches");
})().catch(function(e){ console.error(String((e && e.stack) || e)); throw e; });
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-viz-harness.js" });
"##;

/// RUNTIME guard for spec 42 c5's exploration viz: the served page's OWN viz LAYS OUT and DISPATCHES.
/// It drives the real page script under a DOM shim (node's `vm`): the on-load default renders the
/// clustered overview with clickable clusters, a `data-cluster` click drills, a `data-kgback` click
/// returns to the overview, `forceLayout` converges to finite positions, and an empty graph degrades
/// to a message. This is the behavioral proof the grep test cannot make - dropping the data-cluster
/// dispatch, or a non-finite layout, makes it go red.
#[test]
fn the_exploration_viz_lays_out_and_dispatches_overview_drill_and_back() {
    if !node_available() {
        eprintln!(
            "SKIP the_exploration_viz_lays_out_and_dispatches_overview_drill_and_back: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the viz harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, VIZ_HARNESS).expect("write the viz harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served exploration viz");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the exploration viz must lay out and dispatch overview/drill/back, but the runtime harness \
         failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK exploration-viz-drives-and-dispatches"),
        "the viz harness must confirm the overview/drill/back path:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
