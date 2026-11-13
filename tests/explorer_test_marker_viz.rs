//! Periphery (integration) test for the explorer's TEST MARKER client seam: with tests drawn, every
//! view that draws nodes - the seeded neighborhood's list, the cluster drill, the call walk and its
//! "referenced but not called" sidecar, a re-projection's unresolved and shared members, and a
//! card's subject and node chips - marks each node its answer names in `test_nodes` apart from
//! product code (a `kgtest` class and a visible test tag), and a body that names none draws no
//! marker. Driven by the served page's OWN script under node's built-in `vm` (hermetic, no npm)
//! through a DOM shim mirroring `metadata_card_handoff_viz.rs`'s harness verbatim - the established
//! per-file duplication convention for this class of client-seam test (each suite's harness plumbing
//! is its own).

use crate::common;

use common::served::node_harness_passes;

use crate::vm_harness;
use vm_harness::vm_harness;

/// The DOM shim (node `vm`, no npm): the element surfaces the client seam touches (innerHTML /
/// dataset / .hidden / addEventListener). Mirrors `metadata_card_handoff_viz.rs::DOM_SHIM` verbatim
/// - the same handful of surfaces every dash client-seam harness in this repo needs.
const DOM_SHIM: &str = r#"
const __els = {};
function __Stub(){ this._attrs = {}; }
__Stub.prototype.setAttribute = function(k,v){ this._attrs[k] = String(v); };
__Stub.prototype.getAttribute = function(k){ return this._attrs[k]; };
__Stub.prototype.addEventListener = function(){};
__Stub.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={};
  this.hidden=false; this.className=""; this.checked=false;
  this.clientWidth = 800; this.clientHeight = 300; this.clientTop = 0;
  this.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; }; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
// The drawing's slot: kgDims sizes the drawing to the room below it, here under the panel's
// 10px top padding and a one-line 24px toolbar, so a 300px panel leaves the drawing 256px.
const __slot = { getBoundingClientRect: function(){ return { left: 12, top: 34, width: 776, height: 0 }; } };
const getComputedStyle = function(){ return { paddingBottom: "10px" }; };
__El.prototype.querySelector = function(sel){ return sel === ".kgsvg" ? __slot : new __Stub(); };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };
"#;

/// A `fetch` shim answering a `card=` request from the driver's `__cardBodies` (keyed by the
/// decoded subject id); every other request rejects, as every other view is drawn from a fixture
/// body handed straight to its renderer.
const CARD_FETCH: &str = r#"
const __cardBodies = {};
const fetch = function(url){
  const m = /[?&]card=([^&]*)/.exec(String(url));
  const body = m && __cardBodies[decodeURIComponent(m[1])];
  if (body) return Promise.resolve({ headers: { get: function(){ return null; } }, json: function(){ return Promise.resolve(body); } });
  return Promise.reject(new Error("no network needed for this seam: " + url));
};
"#;

/// The node `vm` program name this suite's harness runs under.
const HARNESS_FILE: &str = "explorer-test-marker-harness.js";

/// Driver: each row draws one view from a body whose `test_nodes` names one test node, then reads the
/// drawn element of the test node and of a product node (the element carrying its `data-seed`, or a
/// shared member's list item): the test node's element carries the `kgtest` class and a visible test
/// tag (`[test]` on a drawn label or chip, a `test` badge on a list row), the product node's carries
/// neither. A card marks its subject's title and its node chips, never its MEMORY chips (which seed
/// the subject itself). A body naming no test node (tests hidden) draws no marker anywhere.
const MARKER_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  await flush();
  const P = "src/a.rs::f", T = "src/a.rs::tests::checks", F = "tests/it.rs", R = "src/b.rs";
  const product = { id: P, kind: "code-entity", label: "f", degree: 1 };
  const test = { id: T, kind: "code-entity", label: "checks", degree: 1 };
  // The drawn element of node id in element box: the nearest tag opening before its data-seed, to
  // its first close.
  function seeded(tag, box) {
    return (view, id) => {
      const html = el(box || "kgpanel")._html;
      const at = html.indexOf('data-seed="' + id + '"');
      if (at === -1) throw new Error(view + ": no drawn element for " + id + ": " + html);
      const open = html.lastIndexOf("<" + tag + " ", at);
      return html.slice(open, html.indexOf("</" + tag + ">", at));
    };
  }
  // A shared member's item in the re-projection's shared list: the element whose text opens with id.
  function sharedItem(view, id) {
    const html = el("kgpanel")._html, list = html.indexOf('class="kgsharedlist"');
    const at = list === -1 ? -1 : html.indexOf(">" + id, list);
    if (at === -1) throw new Error(view + ": no listed element for " + id + ": " + html);
    const open = html.lastIndexOf("<span ", at);
    return html.slice(open, html.indexOf("</span>", at));
  }
  function marked(e) {
    const opening = e.slice(0, e.indexOf(">"));
    return { cls: / class="[^"]*\bkgtest\b/.test(opening), tag: e.indexOf("[test]") !== -1 || e.indexOf(">test<") !== -1 };
  }
  function expectMarked(view, element, id) {
    const m = marked(element);
    if (!m.cls || !m.tag)
      throw new Error("REGRESSION: " + view + " must mark its test node " + id + " (kgtest class: " + m.cls + ", test tag: " + m.tag + "): " + element);
  }
  function expectUnmarked(view, element, id) {
    const m = marked(element);
    if (m.cls || m.tag)
      throw new Error("REGRESSION: " + view + " must not mark " + id + ": " + element);
  }
  function check(view, locate, testId, productId) {
    expectMarked(view, locate(view, testId), testId);
    expectUnmarked(view, locate(view, productId), productId);
  }

  renderGraph({ seed: P, depth: 1, nodes: [product, test],
    edges: [ { from: T, to: P, rel: "calls", tier: "extracted" } ], test_nodes: [T] });
  check("the seeded neighborhood", seeded("div"), T, P);

  renderKgDrill({ seed: "src/a.rs", nodes: [product, test],
    edges: [ { from: T, to: P, rel: "calls", tier: "extracted" } ], test_nodes: [T] });
  check("the cluster drill", seeded("g"), T, P);

  renderKgCalls({ seed: P, dir: "up", depth: 6,
    nodes: [ { id: P, kind: "code-entity", label: "f", layer: 0 }, { id: T, kind: "code-entity", label: "checks", layer: -1 } ],
    edges: [ { from: T, to: P, back: false } ],
    referenced_not_called: [ { id: F, kind: "file", label: "it.rs" }, { id: R, kind: "file", label: "b.rs" } ],
    test_nodes: [T, F] });
  check("the call walk", seeded("g"), T, P);
  check("the referenced-not-called sidecar", seeded("div"), F, R);

  renderReprojection({ subject: "concept/1/0", clusters: [], edges: [], total: 2,
    unresolved: [ { id: T, candidates: [] }, { id: P, candidates: [R] } ], shared: [T, P], test_nodes: [T] });
  check("the re-projection's unresolved members", seeded("div"), T, P);
  check("the re-projection's shared members", sharedItem, T, P);

  const C = "concept/1/0";
  const member = (id, label) => ({ id: id, kind: "code-entity", label: label });
  __cardBodies["src/a.rs"] = { card: { id: "src/a.rs", kind: "file", label: "a.rs", degree: 2, concepts: [],
    decisions: 0, findings: 0, top_entities: [member(T, "checks"), member(P, "f")], proven_by: 0 }, test_nodes: [T] };
  __cardBodies[C] = { card: { id: C, kind: "concept", label: "the graph", degree: 2, concepts: [],
    decisions: 0, findings: 0, top_evidence: [member(T, "checks"), member(P, "f")], proven_by: 0 }, test_nodes: [T] };
  const CASE = "tests/it.rs::case";
  __cardBodies[CASE] = { card: { id: CASE, kind: "code-entity", label: "case", file: F, line: "3", degree: 1,
    concepts: [], decisions: 1, findings: 0, proven_by: 0 }, test_nodes: [F, CASE] };
  // Each card is the card of the view drawn for its subject.
  const seed = async id => { renderGraph({ seed: id, depth: 1, nodes: [], edges: [] }); await flush(); };
  await seed("src/a.rs");
  check("a file card's top entities", seeded("a", "kgcard"), T, P);
  await seed(C);
  check("a concept card's top evidence", seeded("a", "kgcard"), T, P);
  await seed(CASE);
  const card = el("kgcard")._html;
  const title = card.slice(card.indexOf('<div class="kgcard-title'), card.indexOf("</div>", card.indexOf('<div class="kgcard-title')));
  expectMarked("a test entity's card title", title, CASE);
  expectMarked("a test entity's FILE chip", seeded("a", "kgcard")("a test entity's card", F), F);
  expectUnmarked("a test entity's MEMORY chip", seeded("a", "kgcard")("a test entity's card", CASE), CASE);

  renderGraph({ seed: P, depth: 1, nodes: [product, test], edges: [] });
  if (/\bkgtest\b|\[test\]|>test</.test(el("kgpanel")._html))
    throw new Error("REGRESSION: a body naming no test node must draw no test marker: " + el("kgpanel")._html);

  console.log("OK every-drawn-test-node-is-marked");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

rigger::test_cases! {
    /// RUNTIME guard for the explorer's test marker: with tests drawn, the seeded neighborhood, the
    /// cluster drill, the call walk and its sidecar, a re-projection's unresolved and shared members
    /// and a card's subject and node chips each mark the nodes their answer names as test code apart
    /// from product code, and draw no marker when the answer names none.
    every_drawn_test_node_is_marked_apart_from_product_code:
        node_harness_passes(&vm_harness(&[DOM_SHIM, CARD_FETCH], MARKER_DRIVER, HARNESS_FILE), "OK every-drawn-test-node-is-marked");
}
