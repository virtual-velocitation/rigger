//! Periphery (integration) test for spec 59 criterion 3 - ADAPTIVE LABELS. Spec 59 makes the
//! knowledge-graph views readable at real densities; c1 spaces the nodes and c2 grows the canvas, but
//! at the DEFAULT (overview) zoom the whole density-scaled canvas is shrunk to fit the panel, so every
//! label crowds onto its neighbours and the text is unreadable. This criterion OWNS the DECLUTTER
//! behavior: at a given effective screen scale only the labels that MATTER are drawn - the largest
//! (highest-degree proxy) nodes first, lexicographic id tie-break - and the visible set is chosen so
//! their collision bodies (the SAME `kgNodeBody` geometry the separation pass and the renderer use) are
//! pairwise DISJOINT. Zooming IN raises the effective scale, so more labels cross their reveal threshold
//! and appear (a monotone reveal); and hovering ANY node always surfaces its label through a native
//! `<title>` tooltip that needs no layout room, so a hidden label is one hover away.
//!
//! The declutter is client-side JS in `src/dash.html`: `kgLabelThresholds` assigns each node the
//! smallest effective scale at which its body clears every MORE-IMPORTANT node's body, `kgVisibleLabels`
//! is the visible set at a scale (every node whose threshold the scale has crossed), and the zoom
//! handler (`kgApplyLabels`, off `applyKgView`) toggles a `.kg-nolabel` class by comparing the live
//! effective scale against those thresholds. Following the c1/c2 precedent this is a read-only
//! presentation change, so the proof is a RUNTIME harness (node's built-in `vm`, hermetic, no npm) that
//! extracts the served page's own `<script>` and EXECUTES the declutter functions over a synthetic
//! overview. Two layers:
//!   * a STRUCTURAL assertion (grep on the served bytes) that the page SHIPS the declutter authority
//!     (`kgVisibleLabels` / `kgLabelThresholds`), wires it live into the zoom path with the per-node
//!     radius/label accessors, and ships the `.kg-nolabel` toggle class and the per-node `data-nid`
//!     handle - so the selection is on the shipped render path, not dead code; and
//!   * the RUNTIME proof of the done-when: over a real-density laid-out overview the DEFAULT-zoom
//!     visible set is a non-empty STRICT SUBSET whose label boxes are pairwise DISJOINT and which keeps
//!     the most-important node's label; a DEEPER zoom reveals strictly MORE labels and does so
//!     MONOTONICALLY (a superset); the selection is DETERMINISTIC across two runs; and the rendered
//!     force view carries a `<title>` naming EVERY node's label regardless of whether that label is
//!     drawn.
//!
//! Non-vacuous + mutation-proven: the harness is RED unless the declutter is real. With no visible-label
//! selection the page never ships `kgVisibleLabels`, so both the structural grep and the runtime
//! (`kgVisibleLabels is not defined`) redden; a selection that returns EVERY node reddens the
//! strict-subset assertion; one that ignores overlap reddens the pairwise-disjoint assertion; one that
//! ignores importance reddens the most-important-node assertion; and a render that drops the hover
//! `<title>` reddens the hover-surface assertion.
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

/// True when a `node` runtime can be spawned (present on dev machines and on the `ubuntu-latest` CI
/// image, absent on the shim-only lane); the runtime guard SKIPs rather than fails when it is missing.
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// STRUCTURAL: the served page SHIPS the declutter authority AND wires it live. Bound to the c3
/// mechanism (the exact function names, the accessor-carrying `kgVisibleLabels(` call in the zoom path,
/// the `.kg-nolabel` toggle class and the per-node `data-nid` handle) so an unrelated token cannot
/// satisfy it.
#[test]
fn the_served_page_ships_the_adaptive_label_declutter() {
    let page = dash::live_page();

    assert!(
        page.contains("function kgVisibleLabels("),
        "the served page must ship the visible-label selector (kgVisibleLabels)"
    );
    assert!(
        page.contains("function kgLabelThresholds("),
        "the served page must ship the per-node reveal thresholds (kgLabelThresholds)"
    );
    // The selection is only real if the live zoom handler recomputes it at the CURRENT effective scale
    // with the per-node radius/label accessors; without that the declutter is dead code and the page
    // would draw every label as before.
    assert!(
        page.contains("kgVisibleLabels(st.nodes, st.pos, eff, st.radius, st.label)"),
        "the zoom handler must recompute kgVisibleLabels at the live effective scale with the accessors"
    );
    assert!(
        page.contains("kg-nolabel"),
        "the served page must ship the hidden-label toggle class (.kg-nolabel)"
    );
    assert!(
        page.contains("data-nid="),
        "each force node must carry a data-nid handle so the zoom handler can toggle its label"
    );
}

/// The node harness: prepend a minimal DOM shim (the page runs some top-level wiring on load), then the
/// served page script, then a driver that exercises the declutter over a synthetic real-density overview
/// and asserts the done-when.
const ADAPTIVE_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim so the page's top-level wiring (el(...).addEventListener, loadKgOverview()) does not
// throw. Everything is inert; the driver calls the page's PURE declutter/render functions directly.
const SHIM = String.raw`
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
`;

const DRIVER = String.raw`
;(function(){
  // A synthetic clustered overview of N nodes, mirroring the real overview: radius scales as
  // 7 + 22*sqrt(count)/sqrt(maxCount) and the label is the cluster key plus its count. One connected
  // component (a ring plus chords), so the force layout gives a realistic spread.
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

  var W = 900, H = 520;

  // The laid-out positions: the density-scaled force pass then the c1 separation pass - the SAME
  // pipeline the served page draws. The declutter then runs over these positions at a given scale.
  function laidOut(spec){
    var pos = forceLayout(spec.nodes, spec.edges, W, H, spec.radiusOf, spec.labelOf);
    kgSeparate(pos, spec.nodes, spec.radiusOf, spec.labelOf);
    return pos;
  }

  // A node's collision body at effective SCALE s: the fixed-size kgNodeBody centred at the
  // scale-multiplied position - the exact geometry the declutter's clear-scale reasons about.
  function nodeById(spec, id){ var r=null; spec.nodes.forEach(function(n){ if(n.id===id) r=n; }); return r; }
  function boxAt(spec, id, s){
    var n = nodeById(spec, id), p = spec.pos[id];
    return kgNodeBody(p.x * s, p.y * s, spec.radiusOf(n), spec.labelOf(n));
  }
  function disjoint(a, b){ return a.x1 <= b.x0 || a.x0 >= b.x1 || a.y1 <= b.y0 || a.y0 >= b.y1; }
  function count(o){ return Object.keys(o).length; }

  var spec = overview(90);
  spec.pos = laidOut(spec);

  // A DEFAULT (overview) effective scale where the density-scaled canvas is shrunk to fit the panel, so
  // the labels crowd and the declutter must hide most; and a DEEPER (zoomed-in) scale where more room
  // appears. The reveal is a pure function of the scale, so any pair with default < deeper works.
  var sDefault = 0.18, sDeep = 0.75;

  var visDefault = kgVisibleLabels(spec.nodes, spec.pos, sDefault, spec.radiusOf, spec.labelOf);
  var visDeep    = kgVisibleLabels(spec.nodes, spec.pos, sDeep,    spec.radiusOf, spec.labelOf);

  // (1) DECLUTTER IS NON-VACUOUS at default zoom: some labels shown, but NOT all.
  var nDefault = count(visDefault), nDeep = count(visDeep), total = spec.nodes.length;
  if (!(nDefault > 0 && nDefault < total))
    throw new Error("default-zoom visible set must be a non-empty strict subset: shown=" + nDefault + " of " + total);

  // (2) VISIBLE BOXES ARE PAIRWISE DISJOINT at default zoom - no label overlaps another.
  var vids = spec.nodes.map(function(n){ return n.id; }).filter(function(id){ return visDefault[id]; });
  for (var a=0;a<vids.length;a++) for (var b=a+1;b<vids.length;b++){
    if (!disjoint(boxAt(spec, vids[a], sDefault), boxAt(spec, vids[b], sDefault)))
      throw new Error("visible labels overlap at default zoom: " + vids[a] + " vs " + vids[b]);
  }

  // (3) IMPORTANCE: the largest node (top radius, lexicographic id tie-break) always keeps its label.
  var top = spec.nodes.slice().sort(function(x, y){
    return (spec.radiusOf(y) - spec.radiusOf(x)) || (x.id < y.id ? -1 : x.id > y.id ? 1 : 0);
  })[0];
  if (!visDefault[top.id])
    throw new Error("the most important node must keep its label at default zoom: " + top.id);

  // (4) DEEPER ZOOM ADMITS MORE, MONOTONICALLY (a superset - a label never vanishes on zoom-in).
  if (!(nDeep > nDefault))
    throw new Error("a deeper zoom must reveal MORE labels: default=" + nDefault + " deep=" + nDeep);
  spec.nodes.forEach(function(n){
    if (visDefault[n.id] && !visDeep[n.id])
      throw new Error("zoom reveal is not monotone: " + n.id + " visible at default but hidden deeper");
  });

  // (5) DETERMINISM: a second independent selection at the same scale is identical.
  var again = kgVisibleLabels(spec.nodes, spec.pos, sDefault, spec.radiusOf, spec.labelOf);
  spec.nodes.forEach(function(n){
    if (!!visDefault[n.id] !== !!again[n.id])
      throw new Error("non-deterministic visible-label selection for " + n.id);
  });

  // (6) HOVER SURFACE: a force view emits a <title> carrying EVERY node's label, whether or not that
  // label is drawn - so a hidden label is one hover away - and every force node carries the data-nid the
  // live zoom handler toggles.
  var svg = kgSvg(spec.nodes, spec.edges, W, H, {
    radius: spec.radiusOf, fill: function(){ return "#888"; },
    nodeClass: function(){ return "kgcluster"; },
    nodeAttrs: function(n){ return ' data-cluster="' + n.id + '"'; },
    label: spec.labelOf, edgeWidth: function(){ return 1; }
  });
  var titles = (svg.match(/<title>/g) || []).length;
  if (titles !== spec.nodes.length)
    throw new Error("hover surface must carry one <title> per node: titles=" + titles + " nodes=" + spec.nodes.length);
  spec.nodes.forEach(function(n){
    if (svg.indexOf("<title>" + spec.labelOf(n) + "</title>") === -1)
      throw new Error("hover <title> missing node label: " + spec.labelOf(n));
    if (svg.indexOf('data-nid="' + n.id + '"') === -1)
      throw new Error("force node missing data-nid for the label toggle: " + n.id);
  });

  console.log("OK adaptive-labels");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-adaptive-labels-harness.js" });
"##;

/// RUNTIME proof of the done-when: over a real-density laid-out overview the default-zoom visible set is
/// a non-empty strict subset with pairwise-disjoint label boxes that keeps the most-important node; a
/// deeper zoom reveals strictly more labels, monotonically; the selection is deterministic; and the
/// rendered force view carries a hover `<title>` for every node's label.
#[test]
fn adaptive_labels_declutter_by_importance_and_reveal_on_zoom() {
    if !node_available() {
        eprintln!(
            "SKIP adaptive_labels_declutter_by_importance_and_reveal_on_zoom: no `node` runtime on \
             PATH. This runtime guard needs node (present on dev machines and on ubuntu-latest CI); \
             install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the adaptive-labels harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, ADAPTIVE_HARNESS).expect("write the adaptive-labels harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served declutter");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the declutter must show only the disjoint top labels at default zoom, reveal more on zoom, and \
         carry every label on hover, but the runtime harness failed:\n--- stdout ---\n{stdout}\n--- \
         stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK adaptive-labels"),
        "the adaptive-labels harness must confirm the declutter:\n--- stdout ---\n{stdout}\n--- stderr \
         ---\n{stderr}"
    );
}
