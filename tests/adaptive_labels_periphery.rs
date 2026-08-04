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
//!     the HOVER-NAMES-EVERY-NODE contract (spec 59 c3 done-when): a decluttered force node ALWAYS names
//!     itself on hover EVEN when it carries an explicit tooltip - the hover is the node's BARE name
//!     (`opts.name`) followed by the tooltip note, so a name-less note never suppresses the name; the
//!     bare name drops the display-only `[shared]` tag so it stays a single occurrence; and a force view
//!     WITHOUT a title falls back to the label as its hover surface.
//!
//!   * REAL-DRILL INTEGRATION (the name-less-title seam end-to-end): the regression proof above mirrors
//!     the concepts drill with a SYNTHETIC inline fixture. This drives the served page's OWN
//!     `renderKgDrill` - the sole force view that supplies an explicit, generic, NAME-LESS `opts.title` -
//!     and asserts the rendered SVG's shared-member `<title>` carries the node's BARE name AND the note.
//!     It closes exactly the seam a decluttered shared member's hover was proven to drop: the real drill's
//!     `name`/`title`/`label` wiring must NAME the node, never surface the bare note alone, with `[shared]`
//!     a single occurrence (the spec-54 `count([shared])===1` invariant), and the member stamped a
//!     `data-nid` (a real force node the live zoom toggles).
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

  // HOVER NAMES EVERY FORCE NODE (spec 59 c3 done-when): a decluttered force node ALWAYS names itself
  // on hover, EVEN when it carries an explicit tooltip. This mirrors the REAL concepts drill exactly:
  // the on-canvas display label carries a [shared] tag, the BARE identity is exposed via opts.name, and
  // the explicit title is the production NAME-LESS note (it never contains the node's own name). So the
  // hover must carry the node's BARE name AND the note - the operator can identify a node whose canvas
  // label is decluttered away - while the [shared] display tag stays a SINGLE occurrence (the bare name
  // in the hover drops it), so this does not regress the spec-54 count([shared])===1 invariant.
  var NOTE = "shared member: realizes multiple concepts (folded under its primary bucket)";
  var titledSvg = kgSvg(spec.nodes, spec.edges, KG_W, KG_H, {
    radius: spec.radiusOf, fill: function(){ return "#888"; },
    nodeClass: function(){ return "kgcluster"; },
    label: function(n){ return n.label + " [shared]"; },   // the on-canvas display label carries the tag
    name: function(n){ return n.label; },                  // the BARE identity kgSvg folds into the hover
    title: function(){ return NOTE; }                      // the REAL name-LESS generic tooltip
  });
  spec.nodes.forEach(function(n){
    // the hover carries the node's BARE name AND the note, so a titled node still identifies itself.
    if (titledSvg.indexOf("<title>" + n.label + " - " + NOTE + "</title>") === -1)
      throw new Error("a titled force node's hover must carry its BARE name AND the note: missing " + n.label);
    // the pre-fix defect: the explicit note must NOT be the WHOLE hover (that suppressed the name); a
    // bare-note title with nothing before it is exactly the regression this asserts is gone.
    if (titledSvg.indexOf("<title>" + NOTE + "</title>") !== -1)
      throw new Error("an explicit tooltip must NOT suppress the node name on hover (bare note found)");
  });
  // the [shared] display tag stays a SINGLE occurrence per node - the on-canvas label carries it, the
  // hover (bare name) does not - so the count still matches the spec-54 concepts_lens_view_periphery invariant.
  var shared = (titledSvg.match(/\[shared\]/g) || []).length;
  if (shared !== spec.nodes.length)
    throw new Error("the [shared] display tag must appear once per node (never duplicated into the hover): got "
      + shared + " for " + spec.nodes.length);

  // FALLBACK: a titleless force view still names every node - its hover is the display label.
  spec.nodes.forEach(function(n){
    if (forceSvg.indexOf("<title>" + spec.labelOf(n) + "</title>") === -1)
      throw new Error("a titleless force view must name every node on hover (the label): " + spec.labelOf(n));
  });

  console.log("OK adaptive-labels-regression");
})();
"##;

/// REAL-DRILL INTEGRATION: drive the served page's OWN `renderKgDrill` - the concepts drill is the sole
/// force view that supplies an explicit, generic, NAME-LESS `opts.title`, the exact view whose decluttered
/// shared member was proven to lose its name on hover - and assert its rendered SVG names that member.
const REAL_DRILL_HOVER_DRIVER: &str = r##"
;(function(){
  function count(hay, needle){ var i=0,n=0; while((i=hay.indexOf(needle,i))!==-1){ n++; i+=needle.length; } return n; }

  // The REAL concepts drill (spec 54 c3): three members, of which exactly ONE realizes multiple concepts
  // and folds here under its primary bucket (shared). Its shared-member title is the production generic,
  // NAME-LESS note - the precise fixture whose vacuity let the pre-fix hover bug ship. Drive the served
  // page's OWN renderKgDrill (not a synthetic mirror) so the real name/title/label wiring is under test.
  var NOTE = "shared member: realizes multiple concepts (folded under its primary bucket)";
  var CONCEPT_DRILL = { seed: "concept/1/0", depth: 0,
    nodes: [
      { id: "docs/store.md", kind: "design-doc", label: "the store", degree: 1, god: false },
      { id: "src/store/log.rs::append", kind: "code-entity", label: "append", degree: 2, god: false, shared: true },
      { id: "src/index/build.rs::index", kind: "code-entity", label: "index", degree: 1, god: false }
    ],
    edges: [ { from: "src/store/log.rs::append", to: "src/index/build.rs::index", rel: "CALLS", tier: "inferred" } ] };

  renderKgDrill(CONCEPT_DRILL);
  var html = el("kgpanel")._html;

  // (1) c3 done-when ON THE REAL DRILL: the decluttered shared member's hover carries its BARE NAME
  //     ("append") followed by the note, so an operator whose on-canvas label is decluttered away can
  //     still identify the node. This is the assertion the synthetic regression fixture cannot make -
  //     it proves the ACTUAL renderKgDrill name/title/label wiring composes a NAMING hover.
  if (html.indexOf("<title>append - " + NOTE + "</title>") === -1)
    throw new Error("the real concepts drill's shared member must name itself on hover (bare name + note): " + html);
  // (2) ANTI-REGRESSION (the exact adjudicated defect): the generic note must NOT stand as the WHOLE
  //     hover - a bare name-less note title is precisely the pre-fix bug that dropped the node's identity.
  if (html.indexOf("<title>" + NOTE + "</title>") !== -1)
    throw new Error("the real drill's shared-member hover must NOT be the bare name-less note (pre-fix defect): " + html);
  // (3) the [shared] display tag stays a SINGLE occurrence - the on-canvas label carries it, the hover's
  //     bare name drops it - preserving the spec-54 concepts_lens_view_periphery count([shared])===1 invariant.
  if (count(html, "[shared]") !== 1)
    throw new Error("the [shared] display tag must stay a single occurrence (never duplicated into the hover): " + html);
  // (4) the shared member is a real FORCE node - the declutter stamps its data-nid so the live zoom
  //     handler can toggle its on-canvas label (the hover is the reveal path for a decluttered label).
  if (html.indexOf('data-nid="src/store/log.rs::append"') === -1)
    throw new Error("the real drill's shared member must be a force node stamped with data-nid: " + html);

  console.log("OK real-drill-hover");
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
/// data-nid, no auto title), and a titled force node still NAMES itself on hover (bare name + note),
/// never suppressed by the explicit tooltip, with the [shared] display tag a single occurrence.
#[test]
fn a_layered_view_stays_byte_identical_and_a_titled_node_still_names_itself_on_hover() {
    if !node_available() {
        eprintln!(
            "SKIP a_layered_view_stays_byte_identical_and_a_titled_node_still_names_itself_on_hover: no \
             `node` runtime on PATH (present on dev machines and ubuntu-latest CI); install node to run it."
        );
        return;
    }
    assert_ok(
        &program(SHIM_MIN, &[OVERVIEW_JS, REGRESSION_DRIVER].concat()),
        "OK adaptive-labels-regression",
    );
}

/// REAL-DRILL INTEGRATION seam: the served page's OWN `renderKgDrill` - the sole force view with an
/// explicit, generic, NAME-LESS `opts.title` - names its decluttered shared member on hover (bare name +
/// note), never surfaces the bare note alone, keeps `[shared]` a single occurrence, and stamps the member
/// a `data-nid`. This drives the real drill end-to-end, closing the exact seam a synthetic fixture leaves
/// open (a name-containing or titleless fixture passes vacuously against the pre-fix name-dropping hover).
#[test]
fn the_real_concepts_drill_names_its_decluttered_shared_member_on_hover() {
    if !node_available() {
        eprintln!(
            "SKIP the_real_concepts_drill_names_its_decluttered_shared_member_on_hover: no `node` runtime \
             on PATH (present on dev machines and ubuntu-latest CI); install node to run it."
        );
        return;
    }
    assert_ok(
        &program(SHIM_MIN, REAL_DRILL_HOVER_DRIVER),
        "OK real-drill-hover",
    );
}
