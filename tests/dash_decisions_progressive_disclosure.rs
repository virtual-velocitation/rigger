//! Periphery (integration) test for the dash's DECISION history render (spec 30, criterion c4):
//! the decision history renders as PROGRESSIVE DISCLOSURE - each decision a native `<details>`
//! whose `<summary>` previews `id + a one-line summary` and whose expandable body carries the FULL
//! reasoning, so a multi-KB decision collapses to one line but expands whole (the dash charter: no
//! framework, no inline multi-KB dumps).
//!
//! This runs OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::serve_on`), and
//! crosses the REAL loopback HTTP socket the operator's browser actually hits. The implementer's
//! inside-out unit test in `dash.rs` greps `live_page()` IN-PROCESS: it is structurally blind to
//! the serve path (the `route` dispatch of `GET /` -> `Response::rendered(200, HTML_CONTENT_TYPE, live_page())` and the
//! HTTP framing the socket delivers). This layer proves the SERVED root page - the bytes a client
//! receives from the public `serve` entrypoint - carries the c4 progressive-disclosure decisions
//! region end-to-end, not merely that the in-process template string does.
//!
//! Interactive expand/collapse and the `preview()` truncation are BROWSER behaviors (rule 4), so
//! this is a STRUCTURAL guard on the render MECHANISMS the served page ships that deliver them; it
//! binds to the decisions render region (`el("decisions")` .. the empty-state sentinel) so a
//! `<details>` some OTHER panel emits cannot satisfy it. This criterion OWNS progressive
//! disclosure; the run-tree section is criterion 3's, so this test does NOT touch the tree render.
//!
//! `dash`, `spawn`, `contextgraph` are compiled on BOTH the default and the `--no-default-features`
//! lane (none feature-gated), so this guards the served boundary in both lanes.

mod common;

use common::fixtures::assert_decisions_region_discloses_progressively;
use common::served::node_harness_passes;
use common::served::{fetch_with_retry, graph_provider_of, try_fetch_over};
use rigger::contextgraph::Graph;

/// Drive the hand-rolled dash server over a REAL loopback socket through the public `serve_on`
/// entrypoint and fetch `GET /` (the root page), returning the full raw HTTP response (status line
/// + headers + body).
///
/// This RETRIES on a socket-level transient (see [`try_fetch_over`], which owns its port
/// from `bind` through `serve_on` so an attempt can never return another server's response). Each
/// attempt is independent, so the guard is deterministic without weakening what it proves (the
/// served bytes over the real socket): a cleanly-served response is returned to the caller's
/// assertions unchanged, so a genuine content regression still fails.
fn fetch_served_root_page() -> String {
    fetch_with_retry("GET /", || {
        try_fetch_over("/", graph_provider_of(Graph::default()), Graph::default())
    })
}

/// The SERVED root page carries the c4 progressive-disclosure decisions region over the real HTTP
/// `serve` socket: a well-formed `200 text/html` response whose decisions render region emits a
/// native `<details>`/`<summary>` per decision (id + a one-line `preview`) with the FULL summary in
/// the expandable body - NOT the old flat `<table>` that dumped every summary inline - plus the
/// one-line `preview()` helper the summary line depends on. Guards the serve/route/framing seam the
/// in-process `live_page()` grep is structurally blind to.
#[test]
fn the_served_root_page_ships_the_decisions_progressive_disclosure_region() {
    let resp = fetch_served_root_page();

    // The served response is a well-formed HTML page, not a 404/405/500 - the browser's entrypoint.
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET / returns 200 over the real serve socket:\n{resp}"
    );
    assert!(
        resp.contains("text/html"),
        "the served root page is HTML:\n{resp}"
    );
    let page = resp
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body");

    assert_decisions_region_discloses_progressively(page);
}

/// A DOM shim + test driver (JavaScript source) that RUNS the served page's own `render()` twice -
/// one live-poll cycle - to prove an operator-expanded decision stays open across the re-render.
///
/// The template-grep tests above are structurally blind to the RUNTIME behavior: `render()` re-runs
/// every `POLL_MS` and wholesale-replaces the decisions region's `innerHTML`, which destroys and
/// recreates the `<details>` subtree; a grep over the served string cannot see that a body the
/// operator expanded snaps shut on the next poll. This harness executes the real page script under
/// node's built-in `vm` (no npm, fully hermetic - `fetch`/`setTimeout` are inert so the live tail
/// never touches the network), so it observes the actual open-state across two renders.
///
/// It shares the page script's lexical scope (appended after it), so it calls `render()`/`el()`
/// directly. The DOM shim covers exactly what the render path touches: `getElementById`,
/// `innerHTML`, `querySelectorAll("details.decision")`, `addEventListener`, and each `<details>`
/// `open`/`data-did` state. Mutation-proven: removing the render-side `open` re-application (or the
/// whole tracking mechanism) re-collapses `d-alpha` and the driver throws.
const RENDER_TWICE_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script).
const SHIM = String.raw`
const __els = {};
function __El(id){ this.id=id; this._html=""; this._text=""; this._children=[]; this._listeners={}; this.open=false; this.dataset={}; }
Object.defineProperty(__El.prototype, "innerHTML", {
  get(){ return this._html; },
  set(v){ this._html = String(v); this._children = __parseDecisions(this._html); }
});
Object.defineProperty(__El.prototype, "textContent", {
  get(){ return this._text; },
  set(v){ this._text = String(v); }
});
__El.prototype.querySelectorAll = function(sel){
  return this._children.filter(function(c){ return /(^|\s)decision(\s|$)/.test(c.cls); });
};
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
function __parseDecisions(html){
  const out = [];
  const blocks = html.match(/<details\b[\s\S]*?<\/details>/g) || [];
  for(const blk of blocks){
    const open = /<details\b[^>]*\sopen(?=[\s>])/.test(blk);
    const clsM = blk.match(/<details\b[^>]*class="([^"]*)"/);
    const cls = clsM ? clsM[1] : "";
    const didM = blk.match(/data-did="([^"]*)"/);
    const codeM = blk.match(/<code class="did">([^<]*)<\/code>/);
    const did = didM ? didM[1] : (codeM ? codeM[1] : "");
    out.push({ open: open, cls: cls, did: did, dataset: { did: didM ? didM[1] : undefined },
      _listeners: {}, addEventListener: function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); } });
  }
  return out;
}
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
// The page (spec 42 c5) installs its drag-pan handlers on the window global at load; a faithful
// browser shim provides it (a no-op addEventListener; this harness only drives the decisions region).
const window = { addEventListener: function(){} };
const fetch = function(){ return Promise.reject(new Error("no network in the render harness")); };
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope).
const DRIVER = String.raw`
;(function(){
  const long = "d-alpha reasoning line one\nline two spans multiple lines and is long enough that the one-line preview truncates while the expandable body carries the whole thing so the operator can actually read it";
  const state = {
    run: { units: [] }, metrics: {}, step: { wave: [] },
    graph: { decisions: [
      { id: "d-alpha", summary: long, superseded: false },
      { id: "d-beta", summary: "d-beta reasoning", superseded: false }
    ], findings: [] },
    tree: [], blockers: [], events: [], generated_at: 0, position: 1
  };

  // Poll render #1: the operator's browser paints the decision list.
  render(state);
  const dec = el("decisions");
  const a1 = dec._children.find(function(c){ return c.did === "d-alpha"; });
  if(!a1) throw new Error("render#1 produced no <details> for d-alpha");

  // The operator expands d-alpha (native <details> toggle).
  a1.open = true;
  (a1._listeners.toggle || []).forEach(function(fn){ fn({}); });

  // Poll render #2: the live poll re-renders the region (wholesale innerHTML swap).
  render(state);
  const a2 = el("decisions")._children.find(function(c){ return c.did === "d-alpha"; });
  const b2 = el("decisions")._children.find(function(c){ return c.did === "d-beta"; });
  if(!a2) throw new Error("render#2 produced no <details> for d-alpha");
  if(a2.open !== true) throw new Error("REGRESSION: an operator-expanded decision (d-alpha) re-collapsed after the live poll re-render");
  if(b2 && b2.open === true) throw new Error("an untouched decision (d-beta) must stay collapsed across the poll");
  console.log("OK expanded-decision-survives-poll");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-render-harness.js" });
"##;

rigger::test_cases! {
    /// RUNTIME guard for spec 30 c4's charter: a decision the operator expands must stay open across the
    /// 1.5s live poll so a multi-KB reasoning body can actually be READ in the primary `rigger dash`
    /// mode. The live poll re-runs `render()`, which wholesale-replaces the decisions region's
    /// `innerHTML` (destroying + recreating the `<details>` subtree); the fix tracks expanded ids and
    /// re-applies `open` on every render so the operator's expansion survives.
    ///
    /// This drives the SERVED page's real `render()` twice under a DOM shim (via node's `vm`), expands
    /// `d-alpha` between the renders, and asserts it is still open after the second render while an
    /// untouched `d-beta` stays collapsed. It is the runtime check the grep tests cannot make: reverting
    /// the render-side `open` re-application re-collapses `d-alpha` and this test goes red.
    an_operator_expanded_decision_survives_the_live_poll_re_render: node_harness_passes(RENDER_TWICE_HARNESS, "OK expanded-decision-survives-poll");
}

/// A DOM shim + test driver (JavaScript source) that LOADS the served page's own script and RUNS
/// its `preview()` helper on a multi-line, over-long summary, proving the always-visible `<summary>`
/// line really collapses to ONE truncated line - the core c4 charter that a decision "collapses to
/// one line but expands whole", so a multi-KB reasoning body is NEVER dumped inline (spec 30:36).
///
/// The template-grep tests above only prove the `.slice(`/`...` TOKENS exist in the shipped
/// `preview()`; they are blind to what the function actually COMPUTES. A mutation that guts the
/// truncation (raising the cap so no summary is ever cut) or drops the `/g` flag (so only the first
/// whitespace run collapses and newlines survive) keeps every one of those tokens - so all three
/// grep tests stay green while a multi-KB summary dumps inline on the always-visible line. This
/// harness RUNS the real helper under node's built-in `vm` (no npm, hermetic; `fetch`/`setTimeout`
/// are inert so the page's load-time poll never touches the network) and asserts the OUTPUT: a
/// long, multi-line input yields a single line (no `\n`/`\r`/`\t`, no residual whitespace run)
/// truncated with an ellipsis, and a short one passes through untouched. Mutation-proven: raising
/// the cap or dropping the `/g` flag makes the driver throw.
const PREVIEW_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script): just enough for the page script to
// LOAD and run its load-time bootstrap harmlessly. `preview()` itself touches no DOM.
const SHIM = String.raw`
const __els = {};
function __El(id){ this.id=id; this._html=""; this._text=""; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.addEventListener = function(){};
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
// The page (spec 42 c5) installs its drag-pan handlers on the window global at load; a faithful
// browser shim provides it (a no-op addEventListener; this harness only drives the decisions region).
const window = { addEventListener: function(){} };
const fetch = function(){ return Promise.reject(new Error("no network in the preview harness")); };
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope, so `preview` is in scope).
const DRIVER = String.raw`
;(function(){
  // A multi-LINE, over-long summary: hard newlines + doubled internal whitespace + a tab + a long
  // unbroken tail, exactly the multi-KB reasoning a decision carries. The one-line preview MUST
  // collapse every whitespace run to a single space (no hard break survives) and TRUNCATE with an
  // ellipsis, so the always-visible <summary> line is never a multi-KB inline dump.
  const multi = "line one\n\nline two   has   runs\tand a tab\n" + "x".repeat(300);
  const p = preview(multi);
  if(typeof p !== "string") throw new Error("preview() did not return a string: " + p);
  if(/[\n\r\t]/.test(p)) throw new Error("REGRESSION: preview kept a hard line break/tab (not one line): " + JSON.stringify(p));
  if(/\s{2,}/.test(p)) throw new Error("REGRESSION: preview kept a collapsed-whitespace run (not one line): " + JSON.stringify(p));
  if(p.length > 123) throw new Error("REGRESSION: preview did not truncate a long summary (len=" + p.length + "); a multi-KB body would dump inline on the always-visible line");
  if(!p.endsWith("...")) throw new Error("REGRESSION: preview did not mark truncation with an ellipsis: " + JSON.stringify(p));
  // A short, single-line summary passes through unchanged (no spurious truncation/ellipsis).
  const short = preview("a tidy one-liner");
  if(short !== "a tidy one-liner") throw new Error("preview() mangled a short single-line summary: " + JSON.stringify(short));
  console.log("OK preview-collapses-and-truncates");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-preview-harness.js" });
"##;

rigger::test_cases! {
    /// RUNTIME guard for spec 30 c4's core charter (the summary is a PREVIEW, never an inline dump):
    /// the `preview()` helper the `<summary>` line depends on must collapse a multi-line, multi-KB
    /// summary to ONE truncated line. The grep tests above only prove `preview()` CONTAINS the
    /// `.slice(`/`...` idiom - they cannot see that raising the truncation cap (so nothing is ever cut)
    /// or dropping the `/g` flag (so only the first whitespace run collapses) leaves a multi-KB summary
    /// dumped inline on the always-visible line while every grep stays green. This drives the real
    /// helper under node's `vm` and asserts its OUTPUT; reverting either behavior makes it go red.
    the_summary_preview_collapses_a_multiline_summary_to_one_truncated_line: node_harness_passes(PREVIEW_HARNESS, "OK preview-collapses-and-truncates");
}
