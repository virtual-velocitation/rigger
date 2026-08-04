//! Periphery (integration) test for spec 59 criterion 2 - DENSITY-SCALED SPACING. Spec 59 makes the
//! knowledge-graph views readable at real densities; the force layout used to compress every overview
//! into the panel's centre regardless of node count, so at the Concepts lens's ~56 clusters and the
//! Code lens's ~160 the nodes piled up and their relationships (edges) vanished under a clump. This
//! criterion OWNS the SPACING RULE: the force layout's extent grows with the node count AND the total
//! label area, so a denser overview spreads to the room its labels need instead of compressing - and
//! every edge's endpoints end up separated by at least the sum of their node radii, so an edge is
//! drawable as a visible relationship rather than a zero-length stub inside the clump.
//!
//! The layout is client-side JS in `src/dash.html` (`forceLayout` scaled by `kgSpread`, then the c1
//! `kgSeparate`). Following the spec-42/55/59-c1 precedent this is a read-only presentation change, so
//! the proof is a RUNTIME harness (node's built-in `vm`, hermetic, no npm) that extracts the served
//! page's own `<script>` and EXECUTES the layout to assert on the resulting positions. Two layers:
//!   * a STRUCTURAL assertion (grep on the served bytes) that the page SHIPS the density lever
//!     (`kgSpread` / `KG_EXTENT_SPREAD`) and that `kgSvg` actually feeds the radius/label accessors
//!     into `forceLayout` (so the scaling is on the shipped force path, not dead code), and
//!   * the RUNTIME proof of the done-when: over synthetic clustered overviews the layout's extent is
//!     STRICTLY larger at 160 nodes than at 60 (grows with node count), STRICTLY larger for long
//!     labels than short at a FIXED node count (grows with label area), and after the full pipeline
//!     every edge's endpoints are at least the sum of their node radii apart - DETERMINISTICALLY
//!     across two independent runs.
//!
//! Non-vacuous + mutation-proven: the harness is RED unless the scaling is real. Before the c2 change
//! `forceLayout` ignores the accessors and fits both densities to the same panel, so the two extents
//! are EQUAL and the "strictly larger" assertions fail; likewise a `kgSpread` that returns 1
//! unconditionally (no scaling) reddens the density and label-area assertions, and a no-op
//! `kgSeparate` reddens the edge-separation assertion.
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
/// absent on the shim-only lane); the runtime harness SKIPs rather than fails when it is missing.
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// STRUCTURAL: the served page SHIPS the density-spread lever AND wires the radius/label accessors
/// into the force layout, so the scaling governs the actual force-laid views. Bound to the c2
/// mechanism (the exact function/constant names + the accessor-carrying `forceLayout(` call) so an
/// unrelated token cannot satisfy it.
#[test]
fn the_served_page_ships_the_density_spread_lever() {
    let page = dash::live_page();

    assert!(
        page.contains("function kgSpread("),
        "the served page must ship the density spread factor (kgSpread)"
    );
    assert!(
        page.contains("KG_EXTENT_SPREAD"),
        "the served page must ship the extent-spread constant (KG_EXTENT_SPREAD)"
    );
    // The scaling is only real if kgSvg feeds the per-node radius/label accessors into forceLayout;
    // without them kgSpread cannot size the collision bodies and the canvas stays panel-sized.
    assert!(
        page.contains("forceLayout(nodes, edges, width, height, opts.radius, opts.label)"),
        "kgSvg must feed the radius/label accessors into forceLayout so the density scaling is live"
    );
}

/// The node harness: prepend a minimal DOM shim (the page runs some top-level wiring on load), then
/// the served page script, then a driver that exercises the density-scaled layout over synthetic
/// overviews and asserts the extent grows with node count and label area, and that edges are drawable.
const SPACING_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim so the page's top-level wiring (el(...).addEventListener, loadKgOverview()) does
// not throw. Everything is inert; the driver calls the page's PURE layout functions directly.
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
  function isFin(p){ return p && isFinite(p.x) && isFinite(p.y); }

  // A synthetic clustered overview of N nodes, mirroring the real overview: radius scales as
  // 7 + 22*sqrt(count)/sqrt(maxCount) and the label is the cluster key plus its count (realistic
  // lengths, or forced to a fixed length by labelLen). One connected component (a ring plus chords).
  function overview(N, labelLen){
    var nodes = [], edges = [], maxCount = 1;
    var kinds = ["src/driver", "docs/design-notes", "core/model", "adapter/eventstore", "gate/runner"];
    for (var i=0;i<N;i++){
      var count = 1 + (i*7) % 40;
      if (count > maxCount) maxCount = count;
      var key = labelLen ? "x".repeat(labelLen) : (kinds[i % kinds.length] + "/component-" + i);
      nodes.push({ id: "n" + String(1000 + i), count: count, label: key });
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

  // The FORCE-LAYOUT extent (the spacing rule's own output, before the c1 separation refinement):
  // forceLayout given the radius/label accessors sizes the canvas to the density, then fits the graph
  // to it, so the bounding box of the returned centres IS the density-scaled extent.
  function forceExtent(spec){
    var pos = forceLayout(spec.nodes, spec.edges, W, H, spec.radiusOf, spec.labelOf);
    var minx=Infinity,miny=Infinity,maxx=-Infinity,maxy=-Infinity;
    spec.nodes.forEach(function(nd){
      var p = pos[nd.id];
      if (!isFin(p)) throw new Error("non-finite force position for " + nd.id);
      minx=Math.min(minx,p.x); miny=Math.min(miny,p.y); maxx=Math.max(maxx,p.x); maxy=Math.max(maxy,p.y);
    });
    return { w: maxx-minx, h: maxy-miny };
  }

  // (1) GROWS WITH NODE COUNT: the 160-node overview occupies a strictly larger extent (both axes)
  // than the 60-node one. Before the c2 scaling both fit to the same panel and these are EQUAL.
  var e60  = forceExtent(overview(60));
  var e160 = forceExtent(overview(160));
  if (!(e160.w > e60.w && e160.h > e60.h))
    throw new Error("extent did not grow with node count: 60=" + JSON.stringify(e60) + " 160=" + JSON.stringify(e160));

  // (2) GROWS WITH LABEL AREA: at a FIXED node count, long labels take strictly more room than short
  // ones (the label is part of the node, so wider text enlarges the demand and thus the canvas).
  var eShort = forceExtent(overview(100, 3));
  var eLong  = forceExtent(overview(100, 44));
  if (!(eLong.w > eShort.w && eLong.h > eShort.h))
    throw new Error("extent did not grow with label area: short=" + JSON.stringify(eShort) + " long=" + JSON.stringify(eLong));

  // (3) EDGES ARE DRAWABLE: after the full pipeline (density-scaled force + c1 separation) every
  // edge's endpoints are at least the sum of their node radii apart, so the edge is a visible
  // relationship rather than a hidden zero-length stub. Proven at both real densities.
  function laidOut(spec){
    var pos = forceLayout(spec.nodes, spec.edges, W, H, spec.radiusOf, spec.labelOf);
    kgSeparate(pos, spec.nodes, spec.radiusOf, spec.labelOf);
    return pos;
  }
  [60, 160].forEach(function(N){
    var spec = overview(N);
    var pos = laidOut(spec);
    var radById = {};
    spec.nodes.forEach(function(n){ radById[n.id] = spec.radiusOf(n); });
    spec.edges.forEach(function(e){
      var a = pos[e.from], b = pos[e.to];
      if (!a || !b) return;
      var d = Math.hypot(a.x-b.x, a.y-b.y);
      var need = radById[e.from] + radById[e.to];
      if (d + 1e-6 < need)
        throw new Error("edge " + e.from + "->" + e.to + " endpoints too close at N=" + N + ": dist=" + d.toFixed(2) + " < r+r=" + need.toFixed(2));
    });
  });

  // (4) DETERMINISM: a second independent full run yields byte-identical positions.
  [60, 160].forEach(function(N){
    var a = laidOut(overview(N)), b = laidOut(overview(N));
    overview(N).nodes.forEach(function(nd){
      if (a[nd.id].x !== b[nd.id].x || a[nd.id].y !== b[nd.id].y)
        throw new Error("non-deterministic layout at N=" + N + " for " + nd.id);
    });
  });

  console.log("OK density-scaled-spacing");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-spacing-harness.js" });
"##;

/// RUNTIME proof of the done-when: the served page's own force layout spreads with density - a 160-node
/// overview occupies a strictly larger extent than a 60-node one, longer labels take strictly more room
/// than short ones at a fixed node count, and after the full pipeline every edge's endpoints are at
/// least the sum of their node radii apart - deterministically across two runs.
#[test]
fn the_layout_extent_scales_with_density_and_edges_are_drawable() {
    if !node_available() {
        eprintln!(
            "SKIP the_layout_extent_scales_with_density_and_edges_are_drawable: no `node` runtime on \
             PATH. This runtime guard needs node (present on dev machines and on ubuntu-latest CI); \
             install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the spacing harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, SPACING_HARNESS).expect("write the spacing harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served layout");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the layout must scale its extent with density and keep edges drawable, but the runtime \
         harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK density-scaled-spacing"),
        "the spacing harness must confirm density-scaled spacing:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
