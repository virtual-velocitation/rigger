//! PERIPHERY layer for spec 59 criterion 3 - ADAPTIVE LABELS. This file adds the contract, boundary,
//! and integration coverage the unit's own runtime test (`tests/readable_graph_adaptive_labels.rs`) is
//! structurally blind to. That test proves the happy path over one laid-out overview at two fixed
//! scales; this layer proves the SEAMS around it:
//!
//!   * INTEGRATION (the live DOM toggle): the unit test only GREPS that the served page wires
//!     `kgVisibleLabels` into the zoom path - it never EXECUTES that wire. Here a DOM shim (real
//!     `classList` + a `querySelectorAll` that returns the force nodes) drives the actual live handler
//!     `applyKgView` -> `kgApplyLabels`, and asserts the `.kg-nolabel` class on every node matches the
//!     visible-label set exactly, that a deeper `kgView.k` reveals strictly MORE labels in the DOM
//!     monotonically, and that a node with an empty `data-nid` is SKIPPED by the toggle guard.
//!
//!   * CONTRACT (the declutter arithmetic at its edges): the degenerate inputs the comment promises
//!     (zero / one positioned node leaves every threshold 0, so nothing declutters), coincident node
//!     centres (`kgClearScale` -> Infinity, so the less-important label is NEVER revealed at any finite
//!     scale), the most-important node's reveal threshold being EXACTLY 0 (labelled at an arbitrarily
//!     tiny scale), and - stronger than the two fixed scales the unit test samples - the pairwise-
//!     disjoint and monotone-superset invariants swept across a whole ascending scale range.
//!
//!   * REGRESSION (the byte-identity boundary): a force view SETS `kgLabelState` and stamps `data-nid`;
//!     a LAYERED view (`opts.layout`) must CLEAR the state, stamp no `data-nid`, and - given no explicit
//!     `opts.title` - emit no auto `<title>`, so the layered call-DAG render stays byte-identical. Plus
//!     the hover-title PRECEDENCE: when a caller supplies an explicit `opts.title`, that tooltip wins
//!     verbatim over the auto label (a single occurrence per node), and a force view WITHOUT a title
//!     falls back to the label as its hover surface.
//!
//! Like the unit test, the proof is a hermetic node `vm` harness that extracts the served page's own
//! `<script>` and executes its real functions - no npm, no browser - and it SKIPs (not fails) when no
//! `node` runtime is present (the shim-only lane), while `cargo test` on the always-on lanes runs it.
//! `dash` compiles on both the default and `--no-default-features` lane (the viz is not feature-gated),
//! so this guards the served page in both. Every harness is RED unless the seam it guards is real: with
//! no live wire the integration harness reddens (`kg-nolabel` never toggles); a declutter that ignores
//! coincidence or importance reddens the contract harness; and a layered render that leaks `data-nid`
//! or an auto `<title>` reddens the regression harness.

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

/// True when a `node` runtime can be spawned (present on dev machines and on the `ubuntu-latest` CI
/// image, absent on the shim-only lane); the runtime guards SKIP rather than fail when it is missing.
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Wrap a DOM shim + a driver into a hermetic node program: it reads the served page script from
/// `argv[2]`, then runs the shim, the page, and the driver in one `vm` context (so the driver reaches
/// the page's own top-level functions and `let` state).
fn program(shim: &str, driver: &str) -> String {
    let mut p = String::new();
    p.push_str("\"use strict\";\n");
    p.push_str("const vm = require(\"vm\");\n");
    p.push_str("const fs = require(\"fs\");\n");
    p.push_str("const pageScript = fs.readFileSync(process.argv[2], \"utf8\");\n");
    p.push_str("const SHIM = String.raw`");
    p.push_str(shim);
    p.push_str("`;\n");
    p.push_str("const DRIVER = String.raw`");
    p.push_str(driver);
    p.push_str("`;\n");
    p.push_str("const sandbox = { console: console };\n");
    p.push_str("vm.createContext(sandbox);\n");
    p.push_str("vm.runInContext(SHIM + \"\\n\" + pageScript + \"\\n\" + DRIVER, sandbox, { filename: \"periphery-harness.js\" });\n");
    p
}

/// Run a node program that extracts and drives the served page script, returning its output.
fn run_node(driver_program: &str) -> std::process::Output {
    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the periphery harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, driver_program).expect("write the periphery harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served declutter")
}

/// A minimal, inert DOM so the page's top-level wiring does not throw; the drivers call the page's real
/// pure functions (and, for the integration driver, install their own panel over `getElementById`).
const SHIM_MIN: &str = r##"
const __els = {};
function __Stub(){}
__Stub.prototype.setAttribute = function(){};
__Stub.prototype.getAttribute = function(){ return null; };
__Stub.prototype.addEventListener = function(){};
__Stub.prototype.getBoundingClientRect = function(){ return { left:0, top:0, width:800, height:300 }; };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={};
  this.clientWidth=800; this.clientHeight=300; }
Object.defineProperty(__El.prototype,"innerHTML",{ get(){ return this._html; }, set(v){ this._html=String(v); } });
Object.defineProperty(__El.prototype,"textContent",{ get(){ return this._text; }, set(v){ this._text=String(v); } });
__El.prototype.querySelector = function(){ return new __Stub(); };
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
__El.prototype.getBoundingClientRect = function(){ return { left:0, top:0, width:800, height:300 }; };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const location = { href: "" };
const fetch = function(){ return Promise.resolve({ json: function(){ return Promise.resolve({ clusters:[], edges:[], total:0, nodes:[] }); } }); };
const setTimeout = function(){ return 0; };
const setInterval = function(){ return 0; };
"##;

/// Shared synthetic-overview generator + the real (density force + separation) layout pipeline the
/// served page draws, prepended to the drivers that need a laid-out graph.
const OVERVIEW_JS: &str = r##"
var KG_W = 900, KG_H = 520;
function overview(N){
  var nodes = [], edges = [], maxCount = 1;
  var kinds = ["src/driver", "docs/design-notes", "core/model", "adapter/eventstore", "gate/runner"];
  for (var i=0;i<N;i++){
    var count = 1 + (i*7) % 40;
    if (count > maxCount) maxCount = count;
    var key = kinds[i % kinds.length] + "/component-" + i;
    nodes.push({ id: "n" + String(1000 + i), count: count, kind: kinds[i % kinds.length], label: key });
  }
  for (var j=0;j<N;j++){
    edges.push({ from: nodes[j].id, to: nodes[(j+1)%N].id });
    edges.push({ from: nodes[j].id, to: nodes[(j+7)%N].id });
  }
  return {
    nodes: nodes, edges: edges,
    radiusOf: function(n){ return 7 + 22 * (Math.sqrt(n.count) / Math.sqrt(maxCount)); },
    labelOf:  function(n){ return n.label + " (" + n.count + ")"; },
  };
}
function laidOut(spec){
  var pos = forceLayout(spec.nodes, spec.edges, KG_W, KG_H, spec.radiusOf, spec.labelOf);
  kgSeparate(pos, spec.nodes, spec.radiusOf, spec.labelOf);
  return pos;
}
function nodeById(spec, id){ var r=null; spec.nodes.forEach(function(n){ if(n.id===id) r=n; }); return r; }
"##;

/// CONTRACT: the declutter arithmetic at its edges + its invariants across a scale sweep.
const CONTRACT_DRIVER: &str = r##"
;(function(){
  function box(id, spec, s){
    var n = nodeById(spec, id), p = spec.pos[id];
    return kgNodeBody(p.x * s, p.y * s, spec.radiusOf(n), spec.labelOf(n));
  }
  function disjoint(a, b){ return a.x1 <= b.x0 || a.x0 >= b.x1 || a.y1 <= b.y0 || a.y0 >= b.y1; }
  function count(o){ return Object.keys(o).length; }
  var R = function(){ return 12; }, L = function(n){ return n.id; };

  // (a) DEGENERATE: zero positioned nodes -> empty visible set (nothing to declutter, no throw).
  if (count(kgVisibleLabels([], {}, 0.5, R, L)) !== 0)
    throw new Error("empty graph must yield an empty visible set");

  // (b) DEGENERATE: a single positioned node has threshold 0 and is labelled at an arbitrarily tiny
  //     scale (fewer than two nodes leaves nothing to declutter).
  var one = [{ id: "solo" }], onePos = { solo: { x: 5, y: 5 } };
  if (kgLabelThresholds(one, onePos, R, L).solo !== 0)
    throw new Error("a lone node must carry threshold 0");
  if (!kgVisibleLabels(one, onePos, 1e-9, R, L).solo)
    throw new Error("a lone node's label must show at any scale");

  // (c) kgClearScale edges: coincident centres never separate (Infinity); a real x-gap clears finitely.
  var A = { p:{x:0,y:0}, hw:20, up:20, down:20 }, B = { p:{x:0,y:0}, hw:10, up:10, down:10 };
  if (kgClearScale(A, B) !== Infinity)
    throw new Error("coincident bodies must never become disjoint (Infinity)");
  var C = { p:{x:100,y:0}, hw:10, up:10, down:10 };
  if (!isFinite(kgClearScale(A, C)))
    throw new Error("bodies with a positive centre gap must clear at a finite scale");

  // (d) COINCIDENCE behavior: two nodes at the SAME centre - the more-important keeps its label at
  //     every scale, the less-important is NEVER revealed (threshold Infinity), even zoomed way in.
  var coNodes = [{ id: "big" }, { id: "small" }], coPos = { big:{x:50,y:50}, small:{x:50,y:50} };
  var coRad = function(n){ return n.id === "big" ? 30 : 10; };
  var coThr = kgLabelThresholds(coNodes, coPos, coRad, L);
  if (coThr.big !== 0) throw new Error("the more-important coincident node must have threshold 0");
  if (isFinite(coThr.small)) throw new Error("a coincident less-important node must have Infinity threshold");
  var coVis = kgVisibleLabels(coNodes, coPos, 1e9, coRad, L);
  if (!coVis.big || coVis.small)
    throw new Error("at a coincident centre only the more-important label ever shows");

  // (e) IMPORTANCE: over a real overview the single most-important node (max radius, id tie-break) has
  //     threshold EXACTLY 0 and is labelled at an arbitrarily tiny scale.
  var spec = overview(80);
  spec.pos = laidOut(spec);
  var thr = kgLabelThresholds(spec.nodes, spec.pos, spec.radiusOf, spec.labelOf);
  var top = spec.nodes.slice().sort(function(x, y){
    return (spec.radiusOf(y) - spec.radiusOf(x)) || (x.id < y.id ? -1 : x.id > y.id ? 1 : 0);
  })[0];
  if (thr[top.id] !== 0) throw new Error("the most-important node must have reveal threshold 0: " + top.id);
  var tiny = kgVisibleLabels(spec.nodes, spec.pos, 1e-9, spec.radiusOf, spec.labelOf);
  if (!tiny[top.id]) throw new Error("the most-important node must be labelled at a tiny scale");

  // (f) INVARIANTS across an ASCENDING scale sweep: at EVERY scale the visible boxes are pairwise
  //     disjoint; the set grows monotonically (a superset of every smaller scale); its size never
  //     shrinks on zoom-in.
  var scales = [0.05, 0.1, 0.2, 0.35, 0.6, 1.0, 2.0];
  var prev = null, prevN = -1;
  scales.forEach(function(s){
    var vis = kgVisibleLabels(spec.nodes, spec.pos, s, spec.radiusOf, spec.labelOf);
    var ids = spec.nodes.map(function(n){ return n.id; }).filter(function(id){ return vis[id]; });
    for (var a=0;a<ids.length;a++) for (var b=a+1;b<ids.length;b++)
      if (!disjoint(box(ids[a], spec, s), box(ids[b], spec, s)))
        throw new Error("visible labels overlap at scale " + s + ": " + ids[a] + " vs " + ids[b]);
    if (prev) prev.forEach(function(id){
      if (!vis[id]) throw new Error("reveal not monotone: " + id + " visible at a smaller scale, hidden at " + s);
    });
    if (ids.length < prevN) throw new Error("visible-label count shrank on zoom-in at scale " + s);
    prev = ids; prevN = ids.length;
  });

  console.log("OK adaptive-labels-contract");
})();
"##;

/// INTEGRATION: drive the live `applyKgView` -> `kgApplyLabels` handler through a real DOM shim and
/// assert the `.kg-nolabel` toggle matches the visible set exactly and reveals more on a deeper zoom.
const INTEGRATION_DRIVER: &str = r##"
;(function(){
  var spec = overview(90);
  var SCREEN_W = 160;   // the on-screen panel width; the density canvas viewBox is KG_W (900), so the
                        // default (k=1) effective scale SCREEN_W/KG_W ~ 0.18 shrinks labels together.

  // Set the declutter state HONESTLY through the render path, then read it back.
  kgSvg(spec.nodes, spec.edges, KG_W, KG_H, {
    radius: spec.radiusOf, fill: function(){ return "#888"; },
    nodeClass: function(){ return "kgcluster"; }, label: spec.labelOf
  });
  if (!kgLabelState || kgLabelState.width !== KG_W)
    throw new Error("kgSvg must set kgLabelState for a force view");

  function mkNode(nid){
    var set = {};
    return {
      dataset: { nid: nid },
      classList: {
        add: function(c){ set[c] = true; },
        remove: function(c){ delete set[c]; },
        contains: function(c){ return !!set[c]; }
      },
      getAttribute: function(a){ return a === "data-nid" ? nid : null; }
    };
  }
  var domNodes = spec.nodes.map(function(n){ return mkNode(n.id); });
  var emptyIdNode = mkNode("");          // the data-nid guard: the handler must SKIP an empty id.
  domNodes.push(emptyIdNode);

  var panel = {
    querySelector: function(sel){
      if (sel === "svg") return { getBoundingClientRect: function(){ return { width: SCREEN_W }; } };
      if (sel === "#kgzoom") return { setAttribute: function(){} };
      return null;
    },
    querySelectorAll: function(){ return domNodes; }
  };
  var origGet = document.getElementById;
  document.getElementById = function(id){ return id === "kgpanel" ? panel : origGet.call(document, id); };

  function driveAt(k){
    kgView = { k: k, x: 0, y: 0 };
    applyKgView();                       // the REAL live path: applyKgView -> kgApplyLabels -> toggle.
    var eff = (SCREEN_W / kgLabelState.width) * k;
    return kgVisibleLabels(kgLabelState.nodes, kgLabelState.pos, eff, kgLabelState.radius, kgLabelState.label);
  }

  // DEFAULT zoom (k=1): every node's .kg-nolabel matches the visible set EXACTLY; a strict subset shows.
  var vis1 = driveAt(1);
  var shown1 = 0, hidden1 = 0;
  spec.nodes.forEach(function(n, i){
    var hasNo = domNodes[i].classList.contains("kg-nolabel");
    var wantHidden = !vis1[n.id];
    if (hasNo !== wantHidden)
      throw new Error("k=1 DOM toggle mismatch for " + n.id + ": kg-nolabel=" + hasNo + " expected-hidden=" + wantHidden);
    if (hasNo) hidden1++; else shown1++;
  });
  if (!(shown1 > 0 && hidden1 > 0))
    throw new Error("default zoom must reveal a strict subset in the DOM: shown=" + shown1 + " hidden=" + hidden1);
  if (emptyIdNode.classList.contains("kg-nolabel"))
    throw new Error("a node with an empty data-nid must be SKIPPED by the toggle, not hidden");

  var revealed1 = {};
  spec.nodes.forEach(function(n, i){ if (!domNodes[i].classList.contains("kg-nolabel")) revealed1[n.id] = true; });

  // DEEPER zoom (k=4): the DOM reveals strictly MORE labels, monotonically (nothing revealed vanishes).
  var vis4 = driveAt(4);
  var shown4 = 0;
  spec.nodes.forEach(function(n, i){
    var hasNo = domNodes[i].classList.contains("kg-nolabel");
    if (hasNo !== !vis4[n.id]) throw new Error("k=4 DOM toggle mismatch for " + n.id);
    if (!hasNo) shown4++;
  });
  if (!(shown4 > shown1))
    throw new Error("a deeper zoom must reveal MORE labels in the DOM: k1=" + shown1 + " k4=" + shown4);
  spec.nodes.forEach(function(n, i){
    if (revealed1[n.id] && domNodes[i].classList.contains("kg-nolabel"))
      throw new Error("DOM reveal not monotone: " + n.id + " shown at k=1 but hidden at k=4");
  });

  console.log("OK adaptive-labels-integration");
})();
"##;

/// REGRESSION: the byte-identity boundary (force sets state + data-nid; layered clears state + stamps
/// none + emits no auto title) and the hover-title precedence (explicit opts.title wins over the label).
const REGRESSION_DRIVER: &str = r##"
;(function(){
  var spec = overview(30);

  // A FORCE view SETS the declutter state and stamps a data-nid on every node.
  var forceSvg = kgSvg(spec.nodes, spec.edges, KG_W, KG_H, {
    radius: spec.radiusOf, fill: function(){ return "#888"; },
    nodeClass: function(){ return "kgcluster"; }, label: spec.labelOf
  });
  if (kgLabelState === null) throw new Error("a force render must set kgLabelState");
  if (!kgLabelState.nodes || !kgLabelState.pos || !kgLabelState.radius || !kgLabelState.label || !kgLabelState.width)
    throw new Error("kgLabelState must carry the nodes/pos/radius/label/width the zoom handler needs");
  if (forceSvg.indexOf("data-nid=") === -1) throw new Error("a force render must stamp data-nid");

  // A LAYERED view CLEARS the state, stamps no data-nid, and (no explicit title) emits no auto <title>,
  // so the layered call-DAG render stays byte-identical to before this change.
  var lnodes = [];
  for (var i=0;i<8;i++) lnodes.push({ id: "L" + i, layer: i % 4, label: "L" + i });
  var ledges = [ {from:"L0",to:"L1"}, {from:"L1",to:"L2"}, {from:"L2",to:"L3"}, {from:"L4",to:"L5"} ];
  var layeredSvg = kgSvg(lnodes, ledges, KG_W, KG_H, {
    radius: function(){ return 12; }, fill: function(){ return "#888"; },
    nodeClass: function(){ return "kgdot"; }, label: function(n){ return n.id; },
    layout: layeredLayout
  });
  if (kgLabelState !== null) throw new Error("a layered render must CLEAR kgLabelState (byte-identity)");
  if (layeredSvg.indexOf("data-nid=") !== -1) throw new Error("a layered render must NOT stamp data-nid");
  if ((layeredSvg.match(/<title>/g) || []).length !== 0)
    throw new Error("a layered render with no opts.title must emit no auto <title>");

  // HOVER-TITLE PRECEDENCE: an explicit opts.title wins verbatim over the auto label, exactly once per
  // node - and the auto label must NOT also leak as a <title>.
  var titledSvg = kgSvg(spec.nodes, spec.edges, KG_W, KG_H, {
    radius: spec.radiusOf, fill: function(){ return "#888"; },
    nodeClass: function(){ return "kgcluster"; }, label: spec.labelOf,
    title: function(n){ return n.label + " [shared]"; }
  });
  spec.nodes.forEach(function(n){
    var tip = n.label + " [shared]";
    if (titledSvg.indexOf("<title>" + tip + "</title>") === -1)
      throw new Error("explicit opts.title must win as the hover surface: missing " + tip);
    if (titledSvg.indexOf("<title>" + spec.labelOf(n) + "</title>") !== -1)
      throw new Error("the auto label must not leak as a <title> when an explicit title is set: " + spec.labelOf(n));
  });
  var shared = (titledSvg.match(/\[shared\]/g) || []).length;
  if (shared !== spec.nodes.length)
    throw new Error("the explicit tooltip tag must appear once per node: got " + shared + " for " + spec.nodes.length);

  // FALLBACK: a force view with NO explicit title falls back to the label as its hover surface.
  spec.nodes.forEach(function(n){
    if (forceSvg.indexOf("<title>" + spec.labelOf(n) + "</title>") === -1)
      throw new Error("a titleless force view must fall back to the label on hover: " + spec.labelOf(n));
  });

  console.log("OK adaptive-labels-regression");
})();
"##;

/// Assert a driver program runs green under node and prints its OK marker.
fn assert_ok(driver_program: &str, marker: &str) {
    let out = run_node(driver_program);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the periphery harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains(marker),
        "the periphery harness must confirm the seam ({marker}):\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// INTEGRATION seam: the live `applyKgView` -> `kgApplyLabels` handler toggles `.kg-nolabel` to match
/// the visible-label set exactly, reveals strictly more on a deeper zoom, and skips an empty data-nid.
#[test]
fn the_live_zoom_handler_toggles_labels_to_match_the_visible_set() {
    if !node_available() {
        eprintln!(
            "SKIP the_live_zoom_handler_toggles_labels_to_match_the_visible_set: no `node` runtime on \
             PATH (present on dev machines and ubuntu-latest CI); install node to run it."
        );
        return;
    }
    assert_ok(
        &program(SHIM_MIN, &[OVERVIEW_JS, INTEGRATION_DRIVER].concat()),
        "OK adaptive-labels-integration",
    );
}

/// CONTRACT edges: degenerate inputs, coincident centres (never revealed), the most-important node's
/// threshold-0, and the pairwise-disjoint + monotone invariants swept across an ascending scale range.
#[test]
fn the_declutter_holds_its_contract_at_the_edges_and_across_scales() {
    if !node_available() {
        eprintln!(
            "SKIP the_declutter_holds_its_contract_at_the_edges_and_across_scales: no `node` runtime on \
             PATH (present on dev machines and ubuntu-latest CI); install node to run it."
        );
        return;
    }
    assert_ok(
        &program(SHIM_MIN, &[OVERVIEW_JS, CONTRACT_DRIVER].concat()),
        "OK adaptive-labels-contract",
    );
}

/// REGRESSION boundary: a layered view clears the declutter state and stays byte-identical (no
/// data-nid, no auto title), and an explicit opts.title wins over the auto label on hover.
#[test]
fn a_layered_view_stays_byte_identical_and_an_explicit_title_wins() {
    if !node_available() {
        eprintln!(
            "SKIP a_layered_view_stays_byte_identical_and_an_explicit_title_wins: no `node` runtime on \
             PATH (present on dev machines and ubuntu-latest CI); install node to run it."
        );
        return;
    }
    assert_ok(
        &program(SHIM_MIN, &[OVERVIEW_JS, REGRESSION_DRIVER].concat()),
        "OK adaptive-labels-regression",
    );
}
