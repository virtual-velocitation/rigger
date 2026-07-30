//! Periphery (integration) test for the SERVED-PAGE WIRING of the unified inspector (spec 55,
//! criterion 4): the CLIENT SEAM. Criteria 1-3 built the SERVER data paths - the subject x lens
//! re-projection route (`GET /api/graph?seed=<id>&lens=<lens>` -> `Reprojection`), the defined-cell
//! fields on it, and the rationale-overlay batch (`GET /api/graph?explain=<id>[,<id>...]` ->
//! `RationaleBatch`). This criterion OWNS the served page that DRIVES them: the lens selector with a
//! subject-STICKY re-request (flip the lens on a focused subject -> re-request the SAME subject under
//! the new lens), the rationale-overlay TOGGLE with badge-and-expand disclosure, and the ADDITIVE
//! guarantee that with the seed+lens composition absent and the overlay off every existing view
//! (overview / drill / neighborhood / calls) renders byte-identical.
//!
//! Two layers, the way `dash_graph_exploration_viz.rs` proves spec 42 c5:
//!   * a STRUCTURAL layer (grep on the served bytes of `dash::live_page()`, each assertion bound to
//!     the c4 mechanism so an unrelated token cannot satisfy it) - that the page SHIPS the subject-
//!     sticky lens control, the overlay toggle, the re-projection renderer + fetch, and the overlay
//!     data path; and
//!   * a RUNTIME layer (node's built-in `vm`, hermetic, no npm) that DRIVES the served page's own
//!     JavaScript: focusing a node reveals the subject lens; flipping it RE-REQUESTS the subject
//!     under the new lens (`seed=...&lens=...`, the re-projection - NOT the whole-graph overview and
//!     NOT the lens-absent neighborhood); clearing the subject returns to the whole-graph overview;
//!     the overlay toggle batch-fetches `explain=` for the visible nodes and badges ONLY the nodes
//!     that carry rationale, each badge expanding to its leaves; and - the additive proof - the
//!     neighborhood with the overlay OFF is BYTE-IDENTICAL before the overlay was ever toggled and
//!     after it is toggled back off. The runtime layer is what the grep cannot make (that the wiring
//!     actually DISPATCHES and stays additive) and is mutation-proven: dropping the subject-sticky
//!     branch, or ungating a badge, reddens the driver.
//!
//! `dash` compiles on BOTH the default and the `--no-default-features` lane (the seam is not
//! feature-gated), so this guards the served page in both lanes.

use std::process::Command;

use rigger::dash;

/// Extract the single inline `<script>` body from the served page (the same slice the viz test drives).
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

/// The SERVED root page SHIPS the client seam (spec 55 c4): the subject-sticky lens control, the
/// re-projection fetch + renderer, the rationale-overlay toggle, and the overlay data path - each
/// assertion bound to the c4 mechanism so an unrelated token cannot satisfy it, and the existing
/// exploration handles are asserted still-present (the additive guarantee's structural half).
#[test]
fn the_served_page_ships_the_subject_lens_and_rationale_overlay_seam() {
    let page = dash::live_page();

    // --- The STATIC KG header the new controls live in, OUTSIDE the re-rendered #kgpanel (so the
    //     existing view bodies stay byte-identical). ---
    assert!(
        page.contains("id=\"kghead\""),
        "the served page must carry the static KG header (id=kghead) the subject-lens + overlay toggle live in"
    );
    assert!(
        page.contains("id=\"kgpanel\""),
        "the served page must still carry the KG panel region (id=kgpanel) the views render into"
    );

    // --- The SUBJECT-STICKY lens control: a hidden-until-focused subject-lens region carrying the
    //     files/code/concepts tabs (data-lens) and a whole-graph clear (data-kgclear). ---
    assert!(
        page.contains("id=\"kgsubjectlens\""),
        "the served page must carry the subject-lens control (id=kgsubjectlens) the subject-sticky re-request rides"
    );
    assert!(
        page.contains("function syncSubjectLens("),
        "the served page must ship syncSubjectLens (reveal the subject-lens only when a subject is focused)"
    );
    assert!(
        page.contains("data-kgclear"),
        "the subject-lens must carry a whole-graph CLEAR affordance (data-kgclear) that returns to the overview"
    );

    // --- The subject-STICKY dispatch: ONE lens picker whose flip re-requests the SUBJECT when one is
    //     focused, else reloads the whole-graph overview (the spec-55 c4 subject-sticky rule). ---
    assert!(
        page.contains("function onLensPick("),
        "the served page must ship the subject-aware lens dispatch (onLensPick)"
    );
    {
        let s = page
            .find("function onLensPick(")
            .expect("the served page carries onLensPick");
        let body = &page[s..(s + 400).min(page.len())];
        assert!(
            body.contains("kgSubject") && body.contains("reprojectSubject(") && body.contains("loadKgOverview("),
            "onLensPick must be subject-sticky: reproject the focused subject, else the whole-graph overview: {body}"
        );
    }

    // --- The re-projection fetch: `GET /api/graph?seed=<id>&lens=<lens>` (the c1 route), the
    //     subject-STICKY re-request. ---
    assert!(
        page.contains("function reprojectSubject("),
        "the served page must ship the subject re-projection fetch (reprojectSubject)"
    );
    {
        let s = page
            .find("function reprojectSubject(")
            .expect("the served page carries reprojectSubject");
        let body = &page[s..(s + 1000).min(page.len())];
        assert!(
            body.contains("/api/graph") && body.contains("seed=") && body.contains("lens=") && body.contains("encodeURIComponent"),
            "reprojectSubject must re-request the SAME subject under the new lens: /api/graph?seed=<id>&lens=<lens>: {body}"
        );
        assert!(
            body.contains("renderReprojection("),
            "reprojectSubject must render the re-projection body (renderReprojection): {body}"
        );
    }
    assert!(
        page.contains("function renderReprojection("),
        "the served page must ship the re-projection renderer (renderReprojection)"
    );
    // The re-projection surfaces the c1/c2 honesty markers it carries (unresolved / shared / truncated
    // / empty_state), so the client is not confidently wrong about a re-grained cell.
    {
        let s = page
            .find("function renderReprojection(")
            .expect("the served page carries renderReprojection");
        let end = page[s..]
            .find("\nfunction ")
            .map(|i| s + i)
            .unwrap_or(page.len());
        let body = &page[s..end];
        assert!(
            body.contains(".unresolved") && body.contains(".shared") && body.contains(".empty_state"),
            "renderReprojection must surface the re-projection honesty markers (unresolved / shared / empty_state)"
        );
        assert!(
            body.contains(".truncated"),
            "renderReprojection must caption a wide re-grain (the truncated showing-N-of-M marker)"
        );
    }

    // --- The RATIONALE OVERLAY toggle + data path. ---
    assert!(
        page.contains("data-overlay") && page.contains("id=\"kgoverlaytoggle\""),
        "the served page must carry the rationale-overlay TOGGLE (data-overlay, id=kgoverlaytoggle)"
    );
    assert!(
        page.contains("function setOverlay("),
        "the served page must ship the overlay toggle handler (setOverlay)"
    );
    assert!(
        page.contains("function loadRationale("),
        "the served page must ship the overlay data path (loadRationale)"
    );
    {
        let s = page
            .find("function loadRationale(")
            .expect("the served page carries loadRationale");
        let body = &page[s..(s + 600).min(page.len())];
        assert!(
            body.contains("/api/graph") && body.contains("explain="),
            "loadRationale must batch-fetch the rationale over GET /api/graph?explain=<id,...>: {body}"
        );
    }
    // The badge-and-expand disclosure: a per-node badge that only renders when the overlay is ON and
    // the node carries rationale (the additive gate), expanding to its leaves.
    assert!(
        page.contains("function rationaleBadge("),
        "the served page must ship the per-node rationale badge (rationaleBadge)"
    );
    {
        let s = page
            .find("function rationaleBadge(")
            .expect("the served page carries rationaleBadge");
        let body = &page[s..(s + 500).min(page.len())];
        assert!(
            body.contains("kgOverlay"),
            "rationaleBadge must be GATED on kgOverlay so a node renders byte-identical with the overlay off: {body}"
        );
    }
    assert!(
        page.contains("kgbadge") && page.contains("data-explain") && page.contains("kgleaf"),
        "the overlay must render a badge (kgbadge / data-explain) that expands to its leaves (kgleaf)"
    );

    // --- The additive guarantee's structural half: every existing exploration handle + renderer is
    //     still present, unregressed (the delegated dispatch and the four view renderers). ---
    assert!(
        page.contains("closest(\"[data-seed]\")"),
        "the spec-30 select-to-seed dispatch (data-seed -> the neighborhood) must remain, unregressed"
    );
    assert!(
        page.contains("closest(\"[data-cluster]\")") && page.contains("closest(\"[data-kgback]\")"),
        "the spec-42 drill (data-cluster) and back-to-overview (data-kgback) dispatch must remain, unregressed"
    );
    assert!(
        page.contains("function renderKgOverview(")
            && page.contains("function renderKgDrill(")
            && page.contains("function renderGraph(")
            && page.contains("function renderKgCalls("),
        "the four existing view renderers (overview / drill / neighborhood / calls) must remain, unregressed"
    );
    // Determinism by construction carries over: no Math.random() call anywhere on the page.
    assert!(
        !page.contains("Math.random("),
        "the seam must stay deterministic - NO Math.random() call anywhere on the page"
    );
}

/// A DOM shim + test driver (JavaScript) that RUNS the served page's OWN client seam under node's
/// built-in `vm` (no npm, hermetic). It drives the real page script, asserting in turn that:
/// (a) on load the subject-lens is HIDDEN (no subject) and the overview is fetched; (b) focusing a
/// node reveals the subject-lens with its data-lens tabs + data-kgclear; (c) flipping the lens on a
/// focused subject RE-REQUESTS that subject under the new lens (`seed=...&lens=...`, the
/// re-projection - not the overview, not the lens-absent neighborhood) and renders the buckets;
/// (d) clearing the subject returns to the whole-graph overview and re-hides the subject-lens;
/// (e) with the overlay OFF the neighborhood carries NO badge; toggling the overlay ON batch-fetches
/// `explain=` ONCE for the visible nodes and badges ONLY the node that carries rationale, its badge
/// expanding to the leaf's content; and (f) toggling the overlay back OFF renders the neighborhood
/// BYTE-IDENTICAL to before it was ever toggled (the additive proof). Mutation-proven: dropping the
/// subject-sticky branch, or ungating the badge, reddens the driver.
const SEAM_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script). `fetch` resolves the /api/graph views
// (overview default, seeded neighborhood, subject x lens re-projection, and the rationale batch) from
// fixtures and records the URLs so the driver can assert the RIGHT view was fetched; every other
// fetch (the load-time /api/state + /api/instances poll) rejects. setTimeout is inert (the live tail
// never touches the network). The element stub supports the handful of DOM surfaces the seam uses:
// innerHTML / textContent / dataset / .hidden / .className / addEventListener / a stubbed
// querySelector so bindKgView's `#kgzoom` / `svg` lookups resolve without throwing.
const SHIM = String.raw`
const __els = {};
const __fetched = [];
// The whole-graph overview: two clusters joined by one weighted cross-edge, three nodes total.
const __OVERVIEW = { clusters: [ { key: "src", count: 2, kind: "code-entity" },
                                 { key: "docs", count: 1, kind: "design-doc" } ],
                     edges: [ { from: "docs", to: "src", weight: 1 } ], total: 3 };
// The seeded NEIGHBORHOOD (lens-absent) of the focused subject: three nodes, ONE of which (d1) will
// carry rationale in the overlay batch; the other two carry none.
const __NEIGH = { seed: "concept:auth", depth: 2,
  nodes: [ { id: "d1", kind: "decision", label: "route through the shared authority" },
           { id: "src/a.rs::f", kind: "code-entity", label: "f" },
           { id: "plain.rs", kind: "file", label: "plain.rs" } ],
  edges: [ { from: "d1", to: "src/a.rs::f", rel: "GOVERNS", tier: "extracted" } ] };
// The SUBJECT x LENS re-projection of the SAME subject under the code lens: one community bucket.
const __REPROJ = { subject: "concept:auth", total: 2,
  clusters: [ { key: "community/1/0", count: 2, kind: "code-entity", label: "auth core" } ],
  edges: [], unresolved: [], shared: [] };
// The rationale batch: only d1 carries a leaf; the other visible nodes carry none.
const __RATIONALE = { nodes: [ { node: "d1",
  leaves: [ { id: "dec1", kind: "decision",
              summary: "route the write through the shared authority so there is exactly one writer" } ] } ] };
const __EMPTY = { clusters: [], edges: [], total: 0 };
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
const window = { addEventListener: function(){}, };
function __view(url){
  const s = String(url);
  if (s.indexOf("explain=") !== -1) return __RATIONALE;
  if (s.indexOf("lens=") !== -1 && s.indexOf("seed=") !== -1 && s.indexOf("seed=&") === -1) return __REPROJ;
  if (s.indexOf("cluster=") !== -1) return __DRILL_UNUSED();
  if (s.indexOf("seed=") !== -1 && s.indexOf("seed=&") === -1) return __NEIGH;
  return __graphOverview;
}
function __DRILL_UNUSED(){ return __OVERVIEW; }
let __graphOverview = __OVERVIEW;
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    __fetched.push(String(url));
    return Promise.resolve({ json: function(){ return Promise.resolve(__view(url)); } });
  }
  return Promise.reject(new Error("no network for " + url));
};
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope, so it calls the page's
// own functions and reads its module state directly).
const DRIVER = String.raw`
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  function fire(listeners, target){
    (listeners||[]).forEach(function(fn){ fn({ target: target, preventDefault: function(){} }); });
  }
  function handle(name, value){
    return { dataset: (function(){ var d = {}; d[name] = value; return d; })(),
             closest: function(sel){ return sel === "[data-" + __attr(name) + "]" ? this : null; } };
  }
  // dataset key camelCase -> the data-ATTR kebab the page's closest() selector uses.
  function __attr(k){ return k.replace(/[A-Z]/g, function(c){ return "-" + c.toLowerCase(); }); }

  // (a) On load the subject-lens is HIDDEN (no subject yet) and the overview was fetched.
  await flush();
  const subjectLens = el("kgsubjectlens");
  if (subjectLens.hidden !== true)
    throw new Error("the subject-lens must be HIDDEN before a subject is focused");
  if (__fetched.length === 0 || __fetched[0].indexOf("explain=") !== -1)
    throw new Error("the panel did not fetch a graph view on load: " + JSON.stringify(__fetched));

  // (b) Focusing a node reveals the subject-lens with its data-lens tabs + data-kgclear.
  __fetched.length = 0;
  seedGraph("concept:auth");
  await flush();
  if (typeof kgSubject === "undefined" || kgSubject !== "concept:auth")
    throw new Error("focusing a node did not set the subject (kgSubject): " + JSON.stringify(typeof kgSubject === "undefined" ? null : kgSubject));
  if (el("kgsubjectlens").hidden !== false)
    throw new Error("focusing a node did not REVEAL the subject-lens");
  const slHtml = el("kgsubjectlens")._html;
  if (slHtml.indexOf("data-lens=") === -1 || slHtml.indexOf("data-kgclear") === -1)
    throw new Error("the subject-lens did not render its lens tabs + whole-graph clear: " + slHtml);
  // The neighborhood was fetched WITHOUT a lens param (composition absent - the byte-identical view).
  const neighFetch = __fetched[__fetched.length - 1] || "";
  if (neighFetch.indexOf("seed=") === -1 || neighFetch.indexOf("lens=") !== -1)
    throw new Error("focusing a node must fetch the LENS-ABSENT neighborhood: " + neighFetch);
  // Capture the neighborhood BASELINE (overlay never toggled) for the additive proof in (f).
  const neighBaseline = el("kgpanel")._html;
  if (neighBaseline.indexOf("kgbadge") !== -1)
    throw new Error("the neighborhood must carry NO badge before the overlay is toggled: " + neighBaseline);

  // (c) Flipping the lens on the focused subject RE-REQUESTS that subject under the new lens (the
  // subject-sticky re-projection): the header dispatches data-lens -> onLensPick -> reprojectSubject.
  __fetched.length = 0;
  const headListeners = (el("kghead")._listeners && el("kghead")._listeners.click) || [];
  if (!headListeners.length) throw new Error("no delegated click listener on the KG header (seam unwired)");
  fire(headListeners, handle("lens", "code"));
  await flush();
  const reprojFetch = __fetched[__fetched.length - 1] || "";
  if (reprojFetch.indexOf("seed=") === -1 || reprojFetch.indexOf("lens=code") === -1)
    throw new Error("flipping the lens on a subject must RE-REQUEST seed=<subject>&lens=code (re-projection): " + JSON.stringify(__fetched));
  const reproj = el("kgpanel")._html;
  if (reproj.indexOf("concept:auth") === -1 || reproj.indexOf("data-cluster=") === -1)
    throw new Error("the re-projection did not render the subject's buckets: " + reproj);

  // (d) Clearing the subject returns to the whole-graph overview and re-hides the subject-lens.
  __fetched.length = 0;
  fire(headListeners, handle("kgclear", "1"));
  await flush();
  if (kgSubject !== null)
    throw new Error("clearing the subject must reset kgSubject to null, got " + JSON.stringify(kgSubject));
  if (el("kgsubjectlens").hidden !== true)
    throw new Error("clearing the subject must re-HIDE the subject-lens");
  const backFetch = __fetched[__fetched.length - 1] || "";
  if (backFetch.indexOf("seed=") !== -1 || backFetch.indexOf("cluster=") !== -1)
    throw new Error("clearing the subject must re-fetch the whole-graph overview: " + JSON.stringify(__fetched));

  // (e) Re-focus the subject (lens-absent neighborhood), then toggle the OVERLAY on: it batch-fetches
  // explain= ONCE for the visible nodes and badges ONLY the node that carries rationale (d1).
  seedGraph("concept:auth");
  await flush();
  __fetched.length = 0;
  fire(headListeners, handle("overlay", "1"));
  await flush();
  const explainFetches = __fetched.filter(function(u){ return u.indexOf("explain=") !== -1; });
  if (explainFetches.length !== 1)
    throw new Error("the overlay must batch the rationale in ONE explain= request, got: " + JSON.stringify(__fetched));
  if (explainFetches[0].indexOf("d1") === -1)
    throw new Error("the overlay batch must cover the visible node ids: " + explainFetches[0]);
  const withOverlay = el("kgpanel")._html;
  if (withOverlay.indexOf("kgbadge") === -1 || withOverlay.indexOf("data-explain=\"d1\"") === -1)
    throw new Error("the overlay must badge the node that carries rationale (d1): " + withOverlay);
  if (withOverlay.indexOf("one writer") === -1)
    throw new Error("expanding the badge must disclose the leaf content (the decision summary): " + withOverlay);
  // ONLY the rationale-bearing node is badged: exactly one badge for the three visible nodes.
  const badgeCount = (withOverlay.match(/data-explain=/g) || []).length;
  if (badgeCount !== 1)
    throw new Error("the overlay must badge ONLY the nodes that carry rationale (expected 1, got " + badgeCount + "): " + withOverlay);

  // (f) The ADDITIVE proof: toggling the overlay back OFF renders the neighborhood BYTE-IDENTICAL to
  // the baseline captured before the overlay was ever toggled.
  fire(headListeners, handle("overlay", "0"));
  await flush();
  const overlayOff = el("kgpanel")._html;
  if (overlayOff !== neighBaseline)
    throw new Error("REGRESSION: the neighborhood with the overlay OFF must be BYTE-IDENTICAL to its pre-overlay baseline\n--- baseline ---\n" + neighBaseline + "\n--- overlay-off ---\n" + overlayOff);

  console.log("OK subject-lens-and-overlay-seam-drives");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
`;

const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-seam-harness.js" });
"##;

/// RUNTIME guard for spec 55 c4's client seam: the served page's OWN wiring DISPATCHES and stays
/// additive. It drives the real page script under a DOM shim (node's `vm`): focusing a node reveals
/// the subject lens, flipping it re-requests the subject under the new lens (the re-projection),
/// clearing the subject returns to the overview, the overlay toggle batches `explain=` and badges
/// only the rationale-bearing node, and the overlay-off neighborhood is byte-identical to its
/// pre-overlay baseline. This is the behavioral proof the grep cannot make - dropping the
/// subject-sticky branch, or ungating the badge, makes it go red.
#[test]
fn the_client_seam_dispatches_subject_sticky_lens_and_additive_overlay() {
    if !node_available() {
        eprintln!(
            "SKIP the_client_seam_dispatches_subject_sticky_lens_and_additive_overlay: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the seam harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, SEAM_HARNESS).expect("write the seam harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served client seam");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the client seam must dispatch the subject-sticky lens + additive overlay, but the runtime \
         harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK subject-lens-and-overlay-seam-drives"),
        "the seam harness must confirm the subject-sticky + additive-overlay path:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
