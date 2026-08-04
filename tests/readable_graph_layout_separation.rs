//! Periphery (integration) test for spec 59 criterion 1 - the COLLISION-FREE SEPARATION PASS. Spec
//! 59 makes the knowledge-graph views readable at real densities; at the Concepts lens's ~56 clusters
//! and the Code lens's ~160 the force layout collapsed into a central clump with labels stacked on
//! top of one another. This criterion OWNS the separation pass: after the force pass a deterministic
//! pass displaces every overlapping node apart, where a node's collision body is its circle PLUS its
//! label's bounding box (the label is part of the node, so text can never sit under a neighbour),
//! until no two bodies intersect.
//!
//! The layout is client-side JS in `src/dash.html` (`forceLayout` / `kgSeparate` / `kgNodeBody`), a
//! read-only presentation change with no route/projection/data-shape change - so the proof follows
//! the spec-42/55 precedent: a RUNTIME harness (node's built-in `vm`, hermetic, no npm) extracts the
//! served page's own `<script>` and EXECUTES the layout to assert on the resulting positions. Two
//! layers:
//!   * a STRUCTURAL assertion (grep on the served bytes) that the page SHIPS the separation mechanism
//!     (`kgSeparate` / `kgNodeBody`) wired into the force path of `kgSvg`, and
//!   * the RUNTIME proof of the done-when: over synthetic clustered overviews at the real densities
//!     (60 and 160 nodes with realistic label lengths) the layout (force + separation) yields
//!     positions where NO two collision bodies intersect, with finite coordinates, DETERMINISTICALLY
//!     across two independent runs. The harness first checks that the force pass ALONE crowds the
//!     panel (bodies overlap), so the no-overlap proof is not vacuous - and it is mutation-proven:
//!     make `kgSeparate` a no-op and the "overlaps remain after separation" assertion reddens.
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

/// STRUCTURAL: the served page SHIPS the separation mechanism and wires it into the force path of the
/// SVG emitter. Bound to the c1 mechanism (the exact function names + the `if (!opts.layout)` force
/// gate) so an unrelated token cannot satisfy it.
#[test]
fn the_served_page_ships_the_collision_separation_pass() {
    let page = dash::live_page();

    assert!(
        page.contains("function kgSeparate("),
        "the served page must ship the separation pass (kgSeparate)"
    );
    assert!(
        page.contains("function kgNodeBody("),
        "the served page must ship the collision body (kgNodeBody: circle + label box)"
    );
    assert!(
        page.contains("function kgLabelDims("),
        "the served page must estimate the label box from char count + font metrics (kgLabelDims)"
    );
    // Wired into the FORCE path of kgSvg (not the layered call-DAG, which keeps its own layout).
    assert!(
        page.contains("if (!opts.layout) kgSeparate("),
        "the separation pass must run on the force-laid views inside kgSvg (if (!opts.layout))"
    );
}

/// The node harness: prepend a minimal DOM shim (the page runs some top-level wiring on load), then
/// the served page script, then a driver that exercises the layout over synthetic overviews and
/// asserts NO collision-body overlap, finite positions, and determinism.
const SEPARATION_HARNESS: &str = r##"
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
  function bodiesOverlap(A, B){
    var ox = Math.min(A.x1, B.x1) - Math.max(A.x0, B.x0);
    var oy = Math.min(A.y1, B.y1) - Math.max(A.y0, B.y0);
    return ox > 0 && oy > 0;
  }

  // A synthetic clustered overview of N nodes, mirroring the real overview: radius scales as
  // 7 + 22*sqrt(count)/sqrt(maxCount) and the label is the cluster key plus its count, so labels
  // carry realistic lengths. The graph is one connected component (a ring plus deterministic chords)
  // so the force pass fits it into the panel and genuinely crowds it before the separation pass runs.
  function overview(N){
    var nodes = [], edges = [], maxCount = 1;
    var kinds = ["src/driver", "docs/design-notes", "core/model", "adapter/eventstore", "gate/runner"];
    for (var i=0;i<N;i++){
      var count = 1 + (i*7) % 40;
      if (count > maxCount) maxCount = count;
      var key = kinds[i % kinds.length] + "/component-" + i;
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

  function laidOut(spec){
    var pos = forceLayout(spec.nodes, spec.edges, W, H);
    kgSeparate(pos, spec.nodes, spec.radiusOf, spec.labelOf);
    return pos;
  }
  function countOverlaps(spec, pos){
    var n = spec.nodes, bad = 0;
    for (var i=0;i<n.length;i++){
      var A = kgNodeBody(pos[n[i].id].x, pos[n[i].id].y, spec.radiusOf(n[i]), spec.labelOf(n[i]));
      for (var j=i+1;j<n.length;j++){
        var B = kgNodeBody(pos[n[j].id].x, pos[n[j].id].y, spec.radiusOf(n[j]), spec.labelOf(n[j]));
        if (bodiesOverlap(A, B)) bad++;
      }
    }
    return bad;
  }

  [60, 160].forEach(function(N){
    var spec = overview(N);

    // (a) The density is genuinely crowded WITHOUT the separation pass, else the no-overlap proof is
    // vacuous: the force pass alone piles collision bodies onto each other.
    var raw = forceLayout(spec.nodes, spec.edges, W, H);
    var rawBad = countOverlaps(spec, raw);
    if (rawBad === 0)
      throw new Error("density " + N + " is not crowded by the force pass alone - the test would be vacuous");

    // (b) The layout (force + separation) leaves EVERY collision body disjoint, positions finite.
    var pos = laidOut(spec);
    spec.nodes.forEach(function(nd){
      if (!isFin(pos[nd.id])) throw new Error("non-finite position at N=" + N + " for " + nd.id);
    });
    var bad = countOverlaps(spec, pos);
    if (bad !== 0)
      throw new Error(bad + " collision-body overlaps remain after separation at N=" + N + " (was " + rawBad + " before the pass)");

    // (c) DETERMINISM: a second independent run yields byte-identical positions.
    var pos2 = laidOut(overview(N));
    spec.nodes.forEach(function(nd){
      var a = pos[nd.id], b = pos2[nd.id];
      if (a.x !== b.x || a.y !== b.y)
        throw new Error("non-deterministic layout at N=" + N + " for " + nd.id + ": " + JSON.stringify(a) + " vs " + JSON.stringify(b));
    });
  });

  console.log("OK separation-no-overlap-at-density");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-separation-harness.js" });
"##;

/// RUNTIME proof of the done-when: over synthetic overviews at the real densities (60 and 160 nodes
/// with realistic labels) the served page's own layout (force + separation) yields positions with NO
/// two collision bodies (circle + label box) intersecting, deterministically across two runs. The
/// harness also proves the density is genuinely crowded without the pass, so the proof is not vacuous.
#[test]
fn the_layout_leaves_no_collision_body_overlap_at_density() {
    if !node_available() {
        eprintln!(
            "SKIP the_layout_leaves_no_collision_body_overlap_at_density: no `node` runtime on PATH. \
             This runtime guard needs node (present on dev machines and on ubuntu-latest CI); \
             install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the separation harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, SEPARATION_HARNESS).expect("write the separation harness");
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
        "the layout (force + separation) must leave no collision-body overlap at density, but the \
         runtime harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK separation-no-overlap-at-density"),
        "the separation harness must confirm no-overlap at density:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
