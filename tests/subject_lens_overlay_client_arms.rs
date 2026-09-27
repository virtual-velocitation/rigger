//! Periphery (integration) test for two DISPATCH ARMS of the spec 55 c4 unified-inspector client seam
//! that the unit's own served-page and serving-seam tests leave behaviorally UNDRIVEN. The client seam
//! adds `onLensPick` (a subject-STICKY lens dispatch) and `reprojectSubject` (the subject x lens
//! re-request); the served-page runtime harness drives their PRESENT-subject / successful-fetch arms
//! only. This layer closes the two remaining arms, each a boundary the seam's own contract claims:
//!
//!   * onLensPick's NO-SUBJECT arm: with no subject focused, flipping the lens must reload the
//!     WHOLE-GRAPH overview under the new lens (the byte-identical spec-53 behavior), NOT re-project a
//!     non-existent subject. The served-page runtime only ever flips the lens WITH a subject focused
//!     (the re-projection arm), so the `else loadKgOverview()` branch is asserted structurally but never
//!     dispatched. A mutant that dropped the branch (always re-projecting) would break the overview's
//!     own inline lens selector and no existing test would redden.
//!
//!   * reprojectSubject's LIVE fetch-FAILURE degrade: the re-request runs against a live server that can
//!     fail; the code's own contract is "a static export / failed fetch degrades to a message". The
//!     serving-seam test covers the `!LIVE` static-export degrade; the LIVE `catch` (the panel-never-
//!     throws guarantee on the NEW re-request path) is driven by neither layer, whose reprojection fetch
//!     always resolves.
//!
//! Both arms are proven by driving the served page's OWN script under node's built-in `vm` (hermetic,
//! no npm) through a DOM + fetch shim - the behavioral proof a structural grep cannot make. `dash`
//! compiles on BOTH the default and the `--no-default-features` lane (the seam is not feature-gated),
//! so this guards the client seam in both lanes.

mod common;

use common::served::node_harness_passes;

#[path = "common/vm_harness.rs"]
mod vm_harness;
use vm_harness::vm_harness;

/// The DOM shim every driver in this file runs under (node `vm`, no npm): the handful of element
/// surfaces the client seam touches (innerHTML / textContent / dataset / .hidden / .className /
/// addEventListener, a stubbed querySelector so bindKgView's lookups resolve without throwing). Kept
/// free of backticks / `${` so it embeds verbatim inside a `String.raw` template. Each driver appends
/// its OWN `fetch` + fixtures (they differ: one resolves every view, one FAILS the re-projection).
const DOM_SHIM: &str = r#"
const __els = {};
const __fetched = [];
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

/// The node `vm` program name this suite's harness runs under.
const HARNESS_FILE: &str = "dash-client-arm-harness.js";

/// A `fetch` + fixtures prelude whose EVERY `/api/graph` view resolves: the whole-graph overview (the
/// no-argument route, and any `lens=` overview reload), a seeded neighborhood, a subject x lens
/// re-projection, and an empty rationale batch. Every other fetch (the load-time state / instances poll)
/// rejects, matching the live page's own network shape.
const RESOLVING_FETCH: &str = r#"
const __OVERVIEW = { clusters: [ { key: "src", count: 2, kind: "code-entity" },
                                 { key: "docs", count: 1, kind: "design-doc" } ],
                     edges: [ { from: "docs", to: "src", weight: 1 } ], total: 3 };
const __NEIGH = { seed: "concept:auth", depth: 2,
  nodes: [ { id: "concept:auth", kind: "concept", label: "auth" } ], edges: [] };
const __REPROJ = { subject: "concept:auth", total: 2,
  clusters: [ { key: "community/1/0", count: 2, kind: "code-entity", label: "auth core" } ],
  edges: [], unresolved: [], shared: [] };
function __view(url){
  const s = String(url);
  if (s.indexOf("explain=") !== -1) return { nodes: [] };
  if (s.indexOf("lens=") !== -1 && s.indexOf("seed=") !== -1 && s.indexOf("seed=&") === -1) return __REPROJ;
  if (s.indexOf("cluster=") !== -1) return __OVERVIEW;
  if (s.indexOf("seed=") !== -1 && s.indexOf("seed=&") === -1) return __NEIGH;
  return __OVERVIEW;
}
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    __fetched.push(String(url));
    return Promise.resolve({ json: function(){ return Promise.resolve(__view(url)); } });
  }
  return Promise.reject(new Error("no network for " + url));
};
"#;

/// Driver: onLensPick's NO-SUBJECT arm reloads the whole-graph overview under the new lens. On load no
/// subject is focused (the subject-lens is hidden, the view is the overview); flipping the lens on the
/// overview's OWN inline selector (the panel's delegated `data-lens` dispatch) must re-fetch the
/// whole-graph overview (a bare `/api/graph`, never a `seed=` re-projection) under the new lens and must
/// NOT invent a subject or reveal the subject-lens. Mutation-proven: an onLensPick that always
/// re-projects would fetch a `seed=` URL and flip the view to the re-projection, reddening the driver.
const NO_SUBJECT_LENS_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  function fire(listeners, target){
    (listeners||[]).forEach(function(fn){ fn({ target: target, preventDefault: function(){} }); });
  }
  function __attr(k){ return k.replace(/[A-Z]/g, function(c){ return "-" + c.toLowerCase(); }); }
  function handle(name, value){
    return { dataset: (function(){ var d = {}; d[name] = value; return d; })(),
             closest: function(sel){ return sel === "[data-" + __attr(name) + "]" ? this : null; } };
  }

  // (baseline) On load the whole-graph OVERVIEW is fetched, NO subject is focused, the subject-lens is
  // hidden, and the current view is the overview - the subject-LESS altitude the no-subject arm keeps.
  await flush();
  if (kgSubject !== null)
    throw new Error("no subject must be focused on load, got " + JSON.stringify(kgSubject));
  if (el("kgsubjectlens").hidden !== true)
    throw new Error("the subject-lens must be HIDDEN with no subject focused");
  if (typeof kgMode === "undefined" || kgMode !== "overview")
    throw new Error("the load-time view must be the whole-graph overview, got kgMode=" + (typeof kgMode === "undefined" ? null : kgMode));
  const panelListeners = (el("kgpanel")._listeners && el("kgpanel")._listeners.click) || [];
  if (!panelListeners.length) throw new Error("no delegated click listener on the KG panel (seam unwired)");

  // Flip the lens on the overview's OWN inline selector while NO subject is focused: the panel's
  // delegated data-lens dispatch routes to onLensPick, whose no-subject arm must RELOAD the whole-graph
  // overview under the new lens - never re-project a (non-existent) subject.
  __fetched.length = 0;
  fire(panelListeners, handle("lens", "code"));
  await flush();

  if (kgLens !== "code")
    throw new Error("a no-subject lens flip must switch the active lens to code, got " + JSON.stringify(kgLens));
  const last = __fetched[__fetched.length - 1] || "";
  if (!last.length)
    throw new Error("a no-subject lens flip must re-fetch a graph view: " + JSON.stringify(__fetched));
  if (last.indexOf("seed=") !== -1 || last.indexOf("cluster=") !== -1)
    throw new Error("REGRESSION: with NO subject focused a lens flip must reload the WHOLE-GRAPH overview (a bare /api/graph), never a subject re-projection: " + last);
  if (kgSubject !== null)
    throw new Error("a no-subject lens flip must NOT invent a focused subject, got " + JSON.stringify(kgSubject));
  if (el("kgsubjectlens").hidden !== true)
    throw new Error("a no-subject lens flip must NOT reveal the subject-lens control");
  if (kgMode !== "overview")
    throw new Error("a no-subject lens flip must stay on the whole-graph overview, got kgMode=" + kgMode);

  console.log("OK no-subject-lens-flip-reloads-overview");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// A `fetch` + fixtures prelude that resolves the neighborhood but FAILS the subject x lens
/// re-projection request (seed=<id> AND lens=), so reprojectSubject's LIVE `catch` is the arm that runs.
const REPROJECT_FAILS_FETCH: &str = r#"
const __NEIGH = { seed: "concept:auth", depth: 2,
  nodes: [ { id: "concept:auth", kind: "concept", label: "auth" } ], edges: [] };
const __OVERVIEW = { clusters: [ { key: "src", count: 2, kind: "code-entity" } ], edges: [], total: 2 };
function __view(url){
  const s = String(url);
  if (s.indexOf("explain=") !== -1) return { nodes: [] };
  if (s.indexOf("seed=") !== -1 && s.indexOf("seed=&") === -1) return __NEIGH;
  return __OVERVIEW;
}
const fetch = function(url){
  const s = String(url);
  if (s.indexOf("/api/graph") === -1) return Promise.reject(new Error("no network for " + s));
  __fetched.push(s);
  // The subject x lens RE-PROJECTION request (seed=<id> AND lens=) fails at the server - the exact
  // boundary this guard pins: the client must DEGRADE to a message, never throw or leave a dead panel.
  if (s.indexOf("lens=") !== -1 && s.indexOf("seed=") !== -1 && s.indexOf("seed=&") === -1)
    return Promise.reject(new Error("simulated re-projection fetch failure"));
  return Promise.resolve({ json: function(){ return Promise.resolve(__view(s)); } });
};
"#;

/// Driver: reprojectSubject degrades a FAILED live re-projection fetch to a message. Focus a subject
/// (the lens-absent neighborhood renders, the subject-lens reveals), then flip the lens on the focused
/// subject: onLensPick re-requests seed=<subject>&lens=code, which FAILS at the server here. The client
/// must degrade the panel to the documented "unavailable" message - never throw, never leave the stale
/// neighborhood body. Mutation-proven: dropping reprojectSubject's try/catch leaves the panel without
/// the message (and lets the rejection escape), reddening the driver.
const REPROJECT_FAILURE_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  function fire(listeners, target){
    (listeners||[]).forEach(function(fn){ fn({ target: target, preventDefault: function(){} }); });
  }
  function __attr(k){ return k.replace(/[A-Z]/g, function(c){ return "-" + c.toLowerCase(); }); }
  function handle(name, value){
    return { dataset: (function(){ var d = {}; d[name] = value; return d; })(),
             closest: function(sel){ return sel === "[data-" + __attr(name) + "]" ? this : null; } };
  }

  await flush();
  // Focus a subject: the lens-absent neighborhood renders and the subject-sticky lens control reveals.
  seedGraph("concept:auth");
  await flush();
  if (kgSubject !== "concept:auth")
    throw new Error("focusing a node must set the subject, got " + JSON.stringify(typeof kgSubject === "undefined" ? null : kgSubject));
  if (el("kgsubjectlens").hidden !== false)
    throw new Error("focusing a node must reveal the subject-lens");

  // Flip the lens on the focused subject: onLensPick re-requests seed=<subject>&lens=code (a
  // re-projection). That fetch FAILS at the server here - the client must degrade the panel to the
  // documented "unavailable" message, never throw and never leave the stale neighborhood body.
  const headListeners = (el("kghead")._listeners && el("kghead")._listeners.click) || [];
  if (!headListeners.length) throw new Error("no delegated click listener on the KG header (seam unwired)");
  __fetched.length = 0;
  fire(headListeners, handle("lens", "code"));
  await flush();

  const reprojFetch = __fetched.filter(function(u){ return u.indexOf("lens=code") !== -1 && u.indexOf("seed=") !== -1 && u.indexOf("seed=&") === -1; });
  if (reprojFetch.length !== 1)
    throw new Error("the lens flip must issue exactly one subject re-projection request (seed=<subject>&lens=code): " + JSON.stringify(__fetched));
  const panel = el("kgpanel")._html;
  if (panel.indexOf("the subject re-projection is unavailable") === -1)
    throw new Error("REGRESSION: a FAILED subject re-projection fetch must degrade the panel to the 'unavailable' message (the panel must never throw or leave a dead view): " + panel);

  console.log("OK reprojection-fetch-failure-degrades");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

rigger::test_cases! {
    /// RUNTIME guard (spec 55 c4, dispatch arm): onLensPick's NO-SUBJECT arm reloads the whole-graph
    /// overview under the new lens rather than re-projecting a non-existent subject - the other half of the
    /// subject-sticky rule, which the served-page runtime (always flipping the lens WITH a subject) never
    /// drives. Dropping the `else loadKgOverview()` branch reddens it.
    a_lens_flip_with_no_subject_reloads_the_whole_graph_overview:
        node_harness_passes(&vm_harness(&[DOM_SHIM, RESOLVING_FETCH], NO_SUBJECT_LENS_DRIVER, HARNESS_FILE), "OK no-subject-lens-flip-reloads-overview");
    /// RUNTIME guard (spec 55 c4, degrade arm): a FAILED live subject-re-projection fetch degrades the panel
    /// to the documented "unavailable" message (the panel-never-throws contract on the NEW re-request path),
    /// the LIVE `catch` neither the served-page test (fetch always resolves) nor the serving-seam test
    /// (`!LIVE` static-export degrade) reaches. Dropping reprojectSubject's try/catch reddens it.
    a_failed_live_reprojection_fetch_degrades_to_a_message:
        node_harness_passes(&vm_harness(&[DOM_SHIM, REPROJECT_FAILS_FETCH], REPROJECT_FAILURE_DRIVER, HARNESS_FILE), "OK reprojection-fetch-failure-degrades");
}
