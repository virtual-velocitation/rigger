//! Periphery (integration) test for the explorer's TEST MARKER client seam: with tests drawn, every
//! view that draws nodes - the seeded neighborhood's list, the cluster drill, the call walk and its
//! "referenced but not called" sidecar - marks each node its answer names in `test_nodes` apart from
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
  this.clientWidth = 800; this.clientHeight = 300;
  this.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; }; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.querySelector = function(){ return new __Stub(); };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };
"#;

/// No network needed for this seam (every driver calls a renderer directly with fixture bodies) - a
/// rejecting stub satisfies the page script's own top-level references to `fetch`, matching
/// `proof_row_renders_on_the_card.rs::REJECTING_FETCH`.
const REJECTING_FETCH: &str = r#"
const fetch = function(url){ return Promise.reject(new Error("no network needed for this seam: " + url)); };
"#;

/// The node `vm` program name this suite's harness runs under.
const HARNESS_FILE: &str = "explorer-test-marker-harness.js";

/// Driver: each row draws one view from a body whose `test_nodes` names one test node, then reads the
/// drawn element of the test node and of a product node (the element carrying its `data-seed`): the
/// test node's element carries the `kgtest` class and a visible test tag (`[test]` on a drawn label, a
/// `test` badge on a list row), the product node's carries neither. A body naming no test node (tests
/// hidden) draws no marker anywhere.
const MARKER_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  await flush();
  const P = "src/a.rs::f", T = "src/a.rs::tests::checks", F = "tests/it.rs", R = "src/b.rs";
  const product = { id: P, kind: "code-entity", label: "f", degree: 1 };
  const test = { id: T, kind: "code-entity", label: "checks", degree: 1 };
  // The drawn element of node id: the nearest tag opening before its data-seed, to its first close.
  function drawn(view, tag, id) {
    const html = el("kgpanel")._html;
    const at = html.indexOf('data-seed="' + id + '"');
    if (at === -1) throw new Error(view + ": no drawn element for " + id + ": " + html);
    const open = html.lastIndexOf("<" + tag + " ", at);
    return html.slice(open, html.indexOf("</" + tag + ">", at));
  }
  function marked(e) {
    const opening = e.slice(0, e.indexOf(">"));
    return { cls: / class="[^"]*\bkgtest\b/.test(opening), tag: e.indexOf("[test]") !== -1 || e.indexOf(">test<") !== -1 };
  }
  function check(view, tag, testId, productId) {
    const t = marked(drawn(view, tag, testId)), p = marked(drawn(view, tag, productId));
    if (!t.cls || !t.tag)
      throw new Error("REGRESSION: " + view + " must mark its test node " + testId + " (kgtest class: " + t.cls + ", test tag: " + t.tag + "): " + drawn(view, tag, testId));
    if (p.cls || p.tag)
      throw new Error("REGRESSION: " + view + " must not mark the product node " + productId + ": " + drawn(view, tag, productId));
  }

  renderGraph({ seed: P, depth: 1, nodes: [product, test],
    edges: [ { from: T, to: P, rel: "calls", tier: "extracted" } ], test_nodes: [T] });
  check("the seeded neighborhood", "div", T, P);

  renderKgDrill({ seed: "src/a.rs", nodes: [product, test],
    edges: [ { from: T, to: P, rel: "calls", tier: "extracted" } ], test_nodes: [T] });
  check("the cluster drill", "g", T, P);

  renderKgCalls({ seed: P, dir: "up", depth: 6,
    nodes: [ { id: P, kind: "code-entity", label: "f", layer: 0 }, { id: T, kind: "code-entity", label: "checks", layer: -1 } ],
    edges: [ { from: T, to: P, back: false } ],
    referenced_not_called: [ { id: F, kind: "file", label: "it.rs" }, { id: R, kind: "file", label: "b.rs" } ],
    test_nodes: [T, F] });
  check("the call walk", "g", T, P);
  check("the referenced-not-called sidecar", "div", F, R);

  renderGraph({ seed: P, depth: 1, nodes: [product, test], edges: [] });
  if (/\bkgtest\b|\[test\]|>test</.test(el("kgpanel")._html))
    throw new Error("REGRESSION: a body naming no test node must draw no test marker: " + el("kgpanel")._html);

  console.log("OK every-drawn-test-node-is-marked");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

rigger::test_cases! {
    /// RUNTIME guard for the explorer's test marker: with tests drawn, the seeded neighborhood, the
    /// cluster drill, the call walk and its sidecar each mark the nodes their answer names as test
    /// code apart from product code, and draw no marker when the answer names none.
    every_drawn_test_node_is_marked_apart_from_product_code:
        node_harness_passes(&vm_harness(&[DOM_SHIM, REJECTING_FETCH], MARKER_DRIVER, HARNESS_FILE), "OK every-drawn-test-node-is-marked");
}
