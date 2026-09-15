//! Periphery (integration) test for spec 86 criterion 2's METADATA CARD client seam: the served
//! page's own `renderCard` draws a PROOF row on a code-entity subject's card - "proven by N tests"
//! with the evidence `file:line`s on expand, or the explicit amber "no test reaches this entity"
//! state when `proven_by` is `0`. Absence of proof is itself information the audit (spec 85) and
//! reviewers want, so the empty state renders explicitly rather than hiding the row - the ONE
//! behavioral fact a structural grep on the served bytes cannot make (a grep sees the markup exists;
//! it cannot prove `renderCard` actually chooses the right branch for a given `Card`). Driven by the
//! served page's OWN script under node's built-in `vm` (hermetic, no npm) through a DOM shim,
//! mirroring `metadata_card_handoff_viz.rs`'s own harness verbatim - the established per-file
//! duplication convention for this class of client-seam test in this codebase (each `tests/*.rs`
//! integration test compiles as its own independent crate, so the harness plumbing cannot be shared
//! via a plain `use`).

use std::process::Command;

use rigger::dash;

/// Extract the single inline `<script>` body from the served page (the slice the runtime harness
/// drives). Mirrors `metadata_card_handoff_viz.rs::page_script` verbatim.
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

/// No network needed for this seam (every driver calls `renderCard` directly with fixture data,
/// never through `loadCard`'s fetch) - a rejecting stub is enough to satisfy the page script's own
/// top-level references to `fetch`, matching the shape (never the content) of
/// `metadata_card_handoff_viz.rs::RESOLVING_FETCH`.
const REJECTING_FETCH: &str = r#"
const fetch = function(url){ return Promise.reject(new Error("no network needed for this seam: " + url)); };
"#;

/// Assemble a complete node `vm` program: the shared DOM shim, the fetch stub, the served page
/// script (read from `argv[2]`), then the driver - which shares the page's scope, so it calls the
/// page's own `renderCard` directly. Mirrors `metadata_card_handoff_viz.rs::build_harness`.
fn build_harness(driver: &str) -> String {
    const TEMPLATE: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const SHIM = String.raw`__CARD_SHIM__`;
const DRIVER = String.raw`__CARD_DRIVER__`;
const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "proof-row-harness.js" });
"##;
    let shim = format!("{DOM_SHIM}\n{REJECTING_FETCH}");
    TEMPLATE
        .replace("__CARD_SHIM__", &shim)
        .replace("__CARD_DRIVER__", driver)
}

/// Spawn `node` on a self-contained vm harness, asserting it exits 0 and prints `ok_token`.
fn run_node_harness(harness_src: &str, ok_token: &str) {
    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the runtime harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, harness_src).expect("write the runtime harness");
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
        "the runtime harness must drive the PROOF-row client seam, but node failed:\n\
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains(ok_token),
        "the runtime harness must confirm '{ok_token}':\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// Driver: (1) a proven code entity's card names its count and both evidence `file:line`s inside an
/// expandable detail, never eagerly visible text outside it; (2) an UNPROVEN code entity's card
/// carries the explicit amber `kgcard-proof-empty` state, never a silently absent row; (3) a
/// FILE/CONCEPT card - which carries no proof of its own - renders NO "PROOF" heading at all, the
/// row being a code-entity-only fact.
const PROOF_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  await flush();

  // --- a PROVEN code entity ---------------------------------------------------------------
  renderCard({ id: "product.rs::product_fn", kind: "code-entity", label: "product_fn",
    file: "product.rs", line: "1", degree: 1, community: null, concepts: [],
    decisions: 0, findings: 0, top_entities: [], top_evidence: [],
    proven_by: 2, proof_evidence: ["product.rs:7", "tests/integration.rs:4"] });
  let html = el("kgcard")._html;
  if (html.indexOf("PROOF") === -1) throw new Error("REGRESSION: a code-entity card must carry a PROOF row: " + html);
  if (html.indexOf("proven by 2 tests") === -1)
    throw new Error("REGRESSION: the spec's own literal wording must render: " + html);
  if (html.indexOf("product.rs:7") === -1 || html.indexOf("tests/integration.rs:4") === -1)
    throw new Error("REGRESSION: both evidence file:lines must render: " + html);
  if (html.indexOf("kgcard-proof-empty") !== -1)
    throw new Error("REGRESSION: a proven entity must NOT carry the empty-state class: " + html);
  // The evidence lives inside an expandable <details>, not eagerly visible text.
  const proofIdx = html.indexOf("PROOF");
  const detailsIdx = html.indexOf("<details", proofIdx);
  const evidenceIdx = html.indexOf("product.rs:7", proofIdx);
  if (detailsIdx === -1 || detailsIdx > evidenceIdx)
    throw new Error("REGRESSION: the evidence list must be inside a <details> (expand-to-reveal): " + html);

  // --- an UNPROVEN code entity: the explicit amber state, never a silent absence ----------
  renderCard({ id: "product.rs::unused_fn", kind: "code-entity", label: "unused_fn",
    file: "product.rs", line: "5", degree: 0, community: null, concepts: [],
    decisions: 0, findings: 0, top_entities: [], top_evidence: [],
    proven_by: 0, proof_evidence: [] });
  html = el("kgcard")._html;
  if (html.indexOf("PROOF") === -1) throw new Error("REGRESSION: an unproven entity must still carry a PROOF row: " + html);
  if (html.indexOf("no test reaches this entity") === -1)
    throw new Error("REGRESSION: the explicit no-test state must render: " + html);
  if (html.indexOf("kgcard-proof-empty") === -1)
    throw new Error("REGRESSION: the no-test state must carry the amber empty-state class: " + html);

  // --- a FILE card: no PROOF row at all - it is a code-entity-only fact -------------------
  renderCard({ id: "product.rs", kind: "file", label: "product.rs", degree: 1,
    concepts: [], decisions: 0, findings: 0,
    top_entities: [ { id: "product.rs::product_fn", kind: "code-entity", label: "product_fn" } ],
    top_evidence: [] });
  html = el("kgcard")._html;
  if (html.indexOf("PROOF") !== -1) throw new Error("REGRESSION: a file card must carry no PROOF row: " + html);

  console.log("OK proof-row-renders-count-evidence-and-the-explicit-empty-state");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// RUNTIME guard for spec 86 criterion 2's own "WHERE PROOF RENDERS" Design clause: a card gains a
/// PROOF row - "proven by N tests" with the list on expand - and an explicit "no test reaches this
/// entity" state (amber, not silent).
#[test]
fn proof_row_renders_count_evidence_and_the_explicit_empty_state() {
    if !node_available() {
        eprintln!(
            "SKIP proof_row_renders_count_evidence_and_the_explicit_empty_state: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }
    run_node_harness(
        &build_harness(PROOF_DRIVER),
        "OK proof-row-renders-count-evidence-and-the-explicit-empty-state",
    );
}
