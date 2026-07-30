//! Periphery (integration) test for the dash's DIRECTED-CALL LAYERED RENDER (spec 52, criterion c5):
//! the SERVED-PAGE render wiring for the execution-path / call-sites DAG. Spec 52 c1/c3 built the
//! store-side `Projection::calls` traversal and c4 the `/api/graph?view=calls&dir=` route; this
//! criterion OWNS the served page's library-free layered viz that DRAWS the route's body - the
//! server-`layer`-keyed left-to-right layout (barycenter within a layer), the SVG ARROWHEAD marker
//! that makes direction legible, the DISTINCT curved BACK-edge arc for a recursion edge, and the
//! FRONTIER expand-and-reseed (a multi-candidate hop the human picks, the graph never guesses) -
//! reached from a code-entity neighborhood's two directed queries.
//!
//! Two layers, the same shape spec 42 c5's `dash_graph_exploration_viz` uses:
//!   * a STRUCTURAL assertion (this criterion's OWN done-when) that the served page SHIPS the layered
//!     layout, the arrowhead marker definition, the back-edge arc, and the frontier expand-and-reseed
//!     wiring (grep on the served bytes, each bound to the c5 mechanism so some OTHER panel's tokens
//!     cannot satisfy it), and that the direction is an INJECTED opt so the exploration views stay
//!     byte-identical; and
//!   * a RUNTIME harness (node's built-in `vm`, hermetic, no npm) that DRIVES the served page's OWN
//!     render: a code-entity neighborhood offers the two directed queries; `seedCalls` fetches
//!     `view=calls` and `renderKgCalls` DRAWS the layered SVG with the arrowhead marker, a back edge
//!     as a distinct curved arc, and a frontier node with its candidate badge; a frontier click
//!     EXPANDS its candidates and a chosen candidate RE-SEEDS the SAME-direction walk; an UP walk
//!     draws the "referenced but not called" sidecar; `layeredLayout` yields FINITE positions that
//!     increase with the server layer; and an empty walk degrades to a message rather than throwing.
//!     The runtime layer is what the grep cannot make - that the render actually LAYS OUT and
//!     DISPATCHES - and is mutation-proven: dropping the marker, the back-edge arc, or the frontier
//!     dispatch reddens the driver.
//!
//! `dash` compiles on BOTH the default and the `--no-default-features` lane (the render is not
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

/// The SERVED root page SHIPS the directed-call layered render (spec 52 c5): the server-`layer`-keyed
/// left-to-right layout with a within-layer barycenter sweep, the SVG arrowhead marker that draws
/// direction, the distinct curved back-edge arc for a recursion edge, and the frontier
/// expand-and-reseed wiring - reached from a code-entity neighborhood's two directed queries. Each
/// assertion is bound to the c5 mechanism so an unrelated token cannot satisfy it.
#[test]
fn the_served_page_ships_the_directed_call_layered_render() {
    let page = dash::live_page();

    // The KG detail panel region still ships (spec 30) - the directed-call DAG fills it.
    assert!(
        page.contains("id=\"kgpanel\""),
        "the served page must carry the KG panel region (id=kgpanel) the render fills"
    );

    // The LAYERED left-to-right layout (spec 52 c5): x by the SERVER-provided `layer`, within-layer
    // order by a one-pass BARYCENTER sweep. It is a drop-in position map for the shared emitter.
    assert!(
        page.contains("function layeredLayout("),
        "the served page must ship the layered layout (layeredLayout)"
    );
    let l = page
        .find("function layeredLayout(")
        .expect("the served page carries layeredLayout");
    // Slice the WHOLE function body (to the next top-level `function`), so the assertion binds to
    // layeredLayout's own body regardless of how long it grows.
    let layout_end = page[l + 1..]
        .find("\nfunction ")
        .map(|i| l + 1 + i)
        .unwrap_or(page.len());
    let layout = &page[l..layout_end];
    assert!(
        layout.contains("n.layer") || layout.contains(".layer"),
        "layeredLayout must place x by the SERVER-provided layer: {layout}"
    );
    assert!(
        layout.contains("barycenter"),
        "layeredLayout must order within a layer by a barycenter sweep: {layout}"
    );

    // The DIRECTION is an INJECTED opt on the shared emitter, so the exploration views (which pass
    // none) stay byte-identical: the layout swaps in via `opts.layout`, defaulting to `forceLayout`.
    assert!(
        page.contains("opts.layout || forceLayout"),
        "the shared emitter must take the layout as an opt (default forceLayout) so exploration stays byte-identical"
    );

    // The ARROWHEAD marker (spec 52 c5): an SVG <defs><marker> DEFINED ONLY when the caller asks for
    // direction (opts.marker), with the forward edge pointed at it via marker-end. The marker is the
    // page's ONLY <marker> - it does not exist for the exploration views.
    assert!(
        page.contains("<marker id=\"kgarrow\""),
        "the served page must define the SVG arrowhead marker (<marker id=kgarrow>)"
    );
    assert!(
        page.contains("orient=\"auto-start-reverse\""),
        "the arrowhead marker must orient to the edge so the callee end draws the head"
    );
    assert!(
        page.contains("marker-end=\"url(#kgarrow)\""),
        "the forward directed edge must point its callee end at the arrowhead marker (marker-end)"
    );
    assert!(
        page.contains("opts.marker"),
        "the arrowhead marker must be GATED on opts.marker so the exploration views emit no <defs>"
    );

    // The BACK edge (recursion / mutual call): rendered as a DISTINCT curved return arc (a <path>
    // through a perpendicular control point) with its own class, gated on an injected edgeBack opt.
    assert!(
        page.contains("opts.edgeBack"),
        "the emitter must take a back-edge predicate as an opt (opts.edgeBack)"
    );
    assert!(
        page.contains("kgline back"),
        "a back edge must render with the distinct back class (kgline back) so it reads apart from a forward edge"
    );

    // The directed-call renderer draws through the SHARED emitter with the layered layout + marker +
    // back-edge arc injected (never a second graph-drawing engine).
    assert!(
        page.contains("function renderKgCalls("),
        "the served page must ship the directed-call renderer (renderKgCalls)"
    );
    let r = page
        .find("function renderKgCalls(")
        .expect("the served page carries renderKgCalls");
    // Slice the WHOLE function body (to the next top-level `function`), so the assertion binds to
    // renderKgCalls's own injected opts regardless of how long the body grows.
    let render_end = page[r + 1..]
        .find("\nfunction ")
        .map(|i| r + 1 + i)
        .unwrap_or(page.len());
    let render = &page[r..render_end];
    assert!(
        render.contains("layout: layeredLayout")
            && render.contains("marker: true")
            && render.contains("edgeBack"),
        "renderKgCalls must inject the layered layout, the arrowhead marker, and the back-edge arc into the shared emitter: {render}"
    );

    // The FETCH: seedCalls hits the c4 route `view=calls` with the direction, through the lazy provider.
    assert!(
        page.contains("function seedCalls("),
        "the served page must ship the directed-call fetch (seedCalls)"
    );
    assert!(
        page.contains("view=calls"),
        "seedCalls must fetch the c4 directed-call route (view=calls)"
    );

    // The FRONTIER expand-and-reseed (spec 52 c5): a multi-candidate node carries its candidates and
    // renders a candidate badge; a click EXPANDS the candidate list; choosing a candidate RE-SEEDS the
    // SAME-direction walk on that definition - the human picks, the graph never guesses.
    assert!(
        page.contains("function showFrontierChoices("),
        "the served page must ship the frontier expander (showFrontierChoices)"
    );
    assert!(
        page.contains("data-frontier=") && page.contains("data-candidates="),
        "a frontier node must carry its frontier marker and candidate ids (data-frontier / data-candidates)"
    );
    assert!(
        page.contains("data-candidate="),
        "an expanded candidate must be a re-seed handle (data-candidate)"
    );
    assert!(
        page.contains("closest(\"[data-frontier]\")"),
        "a delegated listener must dispatch a data-frontier click to the expander"
    );
    assert!(
        page.contains("closest(\"[data-candidate]\")"),
        "a delegated listener must dispatch a data-candidate click to the re-seed"
    );
    assert!(
        page.contains("seedCalls(cand.dataset.candidate, kgCallsDir)"),
        "a chosen candidate must re-seed the SAME-direction walk (seedCalls with the current dir)"
    );

    // The ENTRY: a code-entity neighborhood (a `<file>::<name>` id) offers the two directed queries;
    // a non-code node offers none (the offer is gated on the `::` in the seed id).
    assert!(
        page.contains("data-calls-down=")
            && page.contains("data-calls-up=")
            && page.contains("data-calls-both="),
        "a code-entity neighborhood must offer the execution-path / call-sites / flow queries"
    );
    assert!(
        page.contains("indexOf(\"::\")"),
        "the directed-call offer must be gated on a code-entity id (the `::` in the seed)"
    );
    assert!(
        page.contains("closest(\"[data-calls-down]\")")
            && page.contains("closest(\"[data-calls-up]\")"),
        "a delegated listener must dispatch the two directed queries (data-calls-down / data-calls-up)"
    );

    // The UP-walk SIDECAR: the flat "referenced but not called" who-uses list (files that import the
    // seed's name but call it from no function) rides beside the traversed DAG.
    assert!(
        page.contains("referenced_not_called"),
        "the UP render must read the route's referenced_not_called sidecar"
    );
    assert!(
        page.contains("referenced but not called"),
        "the UP render must caption the who-uses (referenced but not called) sidecar"
    );
}

/// A DOM shim + test driver (JavaScript) that RUNS the served page's OWN directed-call render under
/// node's built-in `vm` (no npm, hermetic). It drives the real page script, asserting in turn that:
/// (a) a code-entity neighborhood offers the two directed queries and a `data-calls-down` click drives
/// `seedCalls` down (fetches `view=calls&dir=down`); (b) `renderKgCalls` DRAWS the layered SVG with the
/// arrowhead marker definition + a `marker-end`, a BACK edge as a distinct curved `<path class="kgline
/// back">` arc, and a FRONTIER node carrying its `data-frontier` + candidate badge; (c) a delegated
/// `data-frontier` click EXPANDS the candidate list (each `data-candidate`) and a chosen `data-candidate`
/// RE-SEEDS the same-direction walk on that definition; (d) an UP walk draws the "referenced but not
/// called" sidecar; (e) `layeredLayout` yields FINITE positions that INCREASE with the server `layer`;
/// and (f) an EMPTY walk degrades to a message and does NOT throw. Mutation-proven: dropping the marker,
/// the back-edge arc, or the frontier dispatch reddens the driver.
const CALLS_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script). fetch resolves the /api/graph?view=calls
// views (down / up) from fixtures and records the URLs so the driver can assert the walk it invoked;
// every other fetch (the load-time /api/state poll, the overview default) resolves to an empty body so
// nothing throws. setTimeout is inert. A tiny element stub lets bindKgView's querySelector resolve.
const SHIM = String.raw`
const __els = {};
const __fetched = [];
// The DOWN execution-path DAG of src/a.rs::f: a same-file callee (layer 1), a cross-file callee
// (layer 2), a RECURSION back edge (h -> f), and a multi-candidate cross-file FRONTIER (new, 2 defs)
// the walk did NOT descend.
const __CALLS_DOWN = { seed: "src/a.rs::f", dir: "down", depth: 6,
  nodes: [ { id: "src/a.rs::f", kind: "code-entity", label: "f", layer: 0 },
           { id: "src/a.rs::g", kind: "code-entity", label: "g", layer: 1 },
           { id: "src/b.rs::h", kind: "code-entity", label: "h", layer: 2 },
           { id: "src/x.rs::new", kind: "code-entity", label: "new", layer: 2,
             frontier: [ "src/p.rs::new", "src/q.rs::new" ] } ],
  edges: [ { from: "src/a.rs::f", to: "src/a.rs::g", back: false },
           { from: "src/a.rs::g", to: "src/b.rs::h", back: false },
           { from: "src/b.rs::h", to: "src/a.rs::f", back: true },
           { from: "src/a.rs::g", to: "src/x.rs::new", back: false } ],
  referenced_not_called: [] };
// The UP call-sites DAG of src/a.rs::f: one caller (layer -1, so the seed draws at the RIGHT), plus a
// flat "referenced but not called" file (imports the name, calls it from no function).
const __CALLS_UP = { seed: "src/a.rs::f", dir: "up", depth: 6,
  nodes: [ { id: "src/a.rs::f", kind: "code-entity", label: "f", layer: 0 },
           { id: "src/c.rs::caller", kind: "code-entity", label: "caller", layer: -1 } ],
  edges: [ { from: "src/c.rs::caller", to: "src/a.rs::f", back: false } ],
  referenced_not_called: [ { id: "src/d.rs", kind: "file", label: "d.rs" } ] };
const __CALLS_EMPTY = { seed: "src/a.rs::lonely", dir: "down", depth: 6,
  nodes: [], edges: [], referenced_not_called: [] };
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
let __callsView = function(url){
  const s = String(url);
  if (s.indexOf("dir=up") !== -1) return __CALLS_UP;
  return __CALLS_DOWN;
};
const fetch = function(url){
  const s = String(url);
  if (s.indexOf("/api/graph") !== -1) {
    __fetched.push(s);
    if (s.indexOf("view=calls") !== -1) {
      return Promise.resolve({ json: function(){ return Promise.resolve(__callsView(s)); } });
    }
    return Promise.resolve({ json: function(){ return Promise.resolve({ clusters: [], edges: [], total: 0 }); } });
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

  // (e) layeredLayout places x by the SERVER layer and yields FINITE positions: a seed (layer 0) left
  // of its layer-1 and layer-2 callees (the DOWN direction draws the seed at the LEFT).
  const ll = layeredLayout(
    [ { id: "s", layer: 0 }, { id: "a", layer: 1 }, { id: "b", layer: 2 } ],
    [ { from: "s", to: "a" }, { from: "a", to: "b" } ], 800, 300);
  for (const id of ["s","a","b"]) {
    const p = ll[id];
    if (!p || !isFinite(p.x) || !isFinite(p.y))
      throw new Error("layeredLayout produced a non-finite position for " + id + ": " + JSON.stringify(p));
  }
  if (!(ll["s"].x < ll["a"].x && ll["a"].x < ll["b"].x))
    throw new Error("layeredLayout did not order x by the server layer (seed left for DOWN): " + JSON.stringify(ll));

  // (a) A code-entity neighborhood OFFERS the two directed queries; a NON-code node offers none.
  renderGraph({ seed: "src/a.rs::f", depth: 2,
    nodes: [ { id: "src/a.rs::f", kind: "code-entity", label: "f" } ], edges: [] });
  let nb = el("kgpanel")._html;
  if (nb.indexOf("data-calls-down=") === -1 || nb.indexOf("data-calls-up=") === -1)
    throw new Error("a code-entity neighborhood did not offer the directed queries: " + nb);
  renderGraph({ seed: "some-decision-id", depth: 2,
    nodes: [ { id: "some-decision-id", kind: "decision", label: "d" } ], edges: [] });
  if (el("kgpanel")._html.indexOf("data-calls-down=") !== -1)
    throw new Error("a NON-code node must not offer the directed-call queries: " + el("kgpanel")._html);

  // A delegated data-calls-down click drives seedCalls DOWN: fetches view=calls&dir=down and draws.
  const panel = el("kgpanel");
  const clickHandlers = (panel._listeners && panel._listeners.click) || [];
  if (!clickHandlers.length) throw new Error("no delegated click listener on the kg panel (render unwired)");
  __fetched.length = 0;
  const downTarget = { dataset: { callsDown: "src/a.rs::f" },
    closest: function(sel){ return sel === "[data-calls-down]" ? this : null; } };
  clickHandlers.forEach(function(fn){ fn({ target: downTarget, preventDefault: function(){} }); });
  await flush();
  if (!__fetched.some(function(u){ return u.indexOf("view=calls") !== -1 && u.indexOf("dir=down") !== -1; }))
    throw new Error("a data-calls-down click did not fetch the DOWN calls view: " + JSON.stringify(__fetched));

  // (b) renderKgCalls DREW the layered SVG: the arrowhead marker definition + a marker-end, a BACK edge
  // as a distinct curved <path class="kgline back"> arc, and a FRONTIER node with its candidate badge.
  const dag = el("kgpanel")._html;
  if (dag.indexOf("<svg") === -1)
    throw new Error("renderKgCalls did not draw an SVG: " + dag);
  if (dag.indexOf('<marker id="kgarrow"') === -1)
    throw new Error("the layered render did not define the arrowhead marker: " + dag);
  if (dag.indexOf('marker-end="url(#kgarrow)"') === -1)
    throw new Error("the forward edges did not point at the arrowhead marker (marker-end): " + dag);
  if (dag.indexOf('class="kgline back"') === -1 || dag.indexOf("<path") === -1)
    throw new Error("the recursion edge did not draw as a distinct curved back-edge arc: " + dag);
  if (dag.indexOf('data-frontier="1"') === -1 || dag.indexOf("2 candidates") === -1)
    throw new Error("the multi-candidate hop did not draw as a frontier node with its candidate badge: " + dag);
  // A resolved (non-frontier) callee is a re-seed handle back into the neighborhood.
  if (dag.indexOf('data-seed="src/a.rs::g"') === -1)
    throw new Error("a resolved callee must be a re-seed handle (data-seed): " + dag);

  // (c) A delegated data-frontier click EXPANDS the candidate list; each candidate is a re-seed handle.
  const frontierTarget = { dataset: { candidates: "src/p.rs::new|src/q.rs::new" },
    closest: function(sel){ return sel === "[data-frontier]" ? this : null; } };
  clickHandlers.forEach(function(fn){ fn({ target: frontierTarget, preventDefault: function(){} }); });
  const fr = el("kgfrontier")._html;
  if (fr.indexOf('data-candidate="src/p.rs::new"') === -1 || fr.indexOf('data-candidate="src/q.rs::new"') === -1)
    throw new Error("a frontier click did not expand its candidate definitions (data-candidate): " + fr);

  // A chosen candidate RE-SEEDS the SAME-direction (down) walk on that definition.
  __fetched.length = 0;
  const candTarget = { dataset: { candidate: "src/p.rs::new" },
    closest: function(sel){ return sel === "[data-candidate]" ? this : null; } };
  clickHandlers.forEach(function(fn){ fn({ target: candTarget, preventDefault: function(){} }); });
  await flush();
  if (!__fetched.some(function(u){
        return u.indexOf("view=calls") !== -1 && u.indexOf("dir=down") !== -1
          && decodeURIComponent(u).indexOf("src/p.rs::new") !== -1; }))
    throw new Error("a chosen candidate did not re-seed the same-direction walk on it: " + JSON.stringify(__fetched));

  // (d) An UP walk draws the "referenced but not called" who-uses sidecar beside the traversed DAG.
  __fetched.length = 0;
  const upTarget = { dataset: { callsUp: "src/a.rs::f" },
    closest: function(sel){ return sel === "[data-calls-up]" ? this : null; } };
  clickHandlers.forEach(function(fn){ fn({ target: upTarget, preventDefault: function(){} }); });
  await flush();
  const up = el("kgpanel")._html;
  if (up.indexOf("referenced but not called") === -1 || up.indexOf('data-seed="src/d.rs"') === -1)
    throw new Error("the UP render did not draw the referenced-but-not-called sidecar: " + up);

  // (f) An EMPTY walk degrades to a message, never throws.
  __callsView = function(){ return __CALLS_EMPTY; };
  __fetched.length = 0;
  seedCalls("src/a.rs::lonely", "down");
  await flush();
  const empty = el("kgpanel")._html;
  if (empty.indexOf("no calls") === -1 && empty.indexOf("empty") === -1)
    throw new Error("an empty walk did not degrade to a message: " + empty);

  console.log("OK calls-render-lays-out-and-dispatches");
})().catch(function(e){ console.error(String((e && e.stack) || e)); throw e; });
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-calls-render-harness.js" });
"##;

/// RUNTIME guard for spec 52 c5's directed-call render: the served page's OWN render LAYS OUT and
/// DISPATCHES. It drives the real page script under a DOM shim (node's `vm`): a code-entity
/// neighborhood offers the two directed queries, a `data-calls-down` click fetches the DOWN calls view
/// and `renderKgCalls` draws the layered SVG with the arrowhead marker, a distinct back-edge arc, and a
/// frontier badge; a frontier click expands its candidates and a chosen candidate re-seeds the
/// same-direction walk; an UP walk draws the referenced-but-not-called sidecar; `layeredLayout`
/// converges to finite layer-ordered positions; and an empty walk degrades to a message. This is the
/// behavioral proof the grep test cannot make - dropping the marker, the back-edge arc, or the frontier
/// dispatch makes it go red.
#[test]
fn the_directed_call_render_lays_out_and_dispatches_the_layered_dag() {
    if !node_available() {
        eprintln!(
            "SKIP the_directed_call_render_lays_out_and_dispatches_the_layered_dag: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the calls-render harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, CALLS_HARNESS).expect("write the calls-render harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served directed-call render");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the directed-call render must lay out and dispatch the layered DAG, but the runtime harness \
         failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK calls-render-lays-out-and-dispatches"),
        "the calls-render harness must confirm the layered-DAG render path:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
