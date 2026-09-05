//! Periphery (integration) test for the SUBJECT VIEW's docked MEMORY RAIL client seam (spec 63,
//! criterion 5): clicking a node re-seeds the panel to its neighborhood (the EXISTING spec 30/55
//! `seedGraph` mechanism, unchanged) and the served page's own `renderMemoryRail` must ALSO reveal a
//! docked rail listing the subject's governing decisions/findings/concepts (the response's new
//! `memory` field), WITHOUT inserting anything into the neighborhood panel itself - the rail is a
//! separate, static DOM sibling (`#kgrail`), never folded into `#kgpanel`'s rendered nodes.
//!
//! The implementer's inside-out `dash.rs` tests pin the pure `memory_rail`/`Neighborhood::memory`
//! surface; this layer proves the CLIENT actually renders it - the behavioral half a structural grep
//! cannot make, and the boundary the spec's "Test seams" note assigns to a node-gated runtime
//! harness (spec 42/55/59 precedent). Driven by the served page's OWN script under node's built-in
//! `vm` (hermetic, no npm) through a DOM + fetch shim, mirroring `subject_lens_overlay_client_arms.rs`.
//! `dash` compiles on BOTH the default and the `--no-default-features` lane (the seam is not
//! feature-gated), so this guards the client seam in both lanes.

use std::process::Command;

use rigger::dash;

/// Extract the single inline `<script>` body from the served page (the slice the runtime harness drives).
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

/// The DOM shim every driver in this file runs under (node `vm`, no npm): the handful of element
/// surfaces the client seam touches (innerHTML / textContent / dataset / .hidden / .className /
/// addEventListener, a stubbed querySelector so bindKgView's lookups resolve without throwing). Kept
/// free of backticks / `${` so it embeds verbatim inside a `String.raw` template.
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

/// A `fetch` + fixtures prelude: the whole-graph overview (the no-argument route) and a seeded
/// neighborhood carrying a `memory` rail ({decisions, findings, concepts}) for subject
/// `combat.rs::fire`. Every other fetch (explain overlay, instances/state poll) resolves gracefully
/// empty, matching the live page's own network shape.
const RESOLVING_FETCH: &str = r#"
const __OVERVIEW = { clusters: [ { key: "src", count: 2, kind: "code-entity" } ], edges: [], total: 2 };
const __NEIGH = { seed: "combat.rs::fire", depth: 2,
  nodes: [ { id: "combat.rs::fire", kind: "code-entity", label: "fire" } ], edges: [],
  memory: {
    decisions: [ { id: "d1", kind: "decision", summary: "use the shared authority" } ],
    findings: [ { id: "f1", kind: "finding", summary: "the finding content" } ],
    concepts: [ { id: "concept/combat", label: "combat resolution" } ]
  } };
function __view(url){
  const s = String(url);
  if (s.indexOf("explain=") !== -1) return { nodes: [] };
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

/// Assemble a complete node `vm` program from a per-test `fetch` + fixtures prelude and a driver: the
/// shared DOM shim, then the fetch prelude, then the served page script (read from `argv[2]`), then the
/// driver - which shares the page's scope, so it calls the page's own functions and reads its module
/// state directly.
fn build_harness(fetch_prelude: &str, driver: &str) -> String {
    const TEMPLATE: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const SHIM = String.raw`__RAIL_SHIM__`;
const DRIVER = String.raw`__RAIL_DRIVER__`;
const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-memory-rail-harness.js" });
"##;
    let shim = format!("{DOM_SHIM}\n{fetch_prelude}");
    TEMPLATE
        .replace("__RAIL_SHIM__", &shim)
        .replace("__RAIL_DRIVER__", driver)
}

/// Spawn `node` on a self-contained vm harness (a complete node program that reads the served page
/// script from `argv[2]` and drives it under the DOM shim), asserting it exits 0 and prints `ok_token`.
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
        "the runtime harness must drive the client seam, but node failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains(ok_token),
        "the runtime harness must confirm '{ok_token}':\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// Driver: on load the rail is hidden (no subject); clicking a node (`seedGraph`) reveals the rail
/// and renders the decision/finding/concept content from the response's `memory` field, WITHOUT that
/// content ever appearing inside `#kgpanel`'s own innerHTML (proving the rail adds no node to the
/// neighborhood panel - a separate static sibling, never folded into the drawn layout); returning to
/// the whole graph (the delegated `data-kgclear` handle) hides the rail again.
const RAIL_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  function fire(listeners, target){
    (listeners||[]).forEach(function(fn){ fn({ target: target, preventDefault: function(){} }); });
  }
  function handle(name, value){
    return { dataset: (function(){ var d = {}; d[name] = value; return d; })(),
             closest: function(sel){ return sel === "[data-kgclear]" ? this : null; } };
  }

  await flush();
  if (el("kgrail").hidden !== true)
    throw new Error("the memory rail must be HIDDEN with no subject focused on load");

  // Click a node: seedGraph re-seeds the panel to its neighborhood (unchanged) AND must reveal the
  // rail, populated from THIS SAME response's memory field.
  seedGraph("combat.rs::fire");
  await flush();

  if (el("kgrail").hidden !== false)
    throw new Error("clicking a node must reveal the docked memory rail");
  const rail = el("kgrail")._html;
  if (rail.indexOf("d1") === -1 || rail.indexOf("use the shared authority") === -1)
    throw new Error("REGRESSION: the rail must list the subject's governing decision: " + rail);
  if (rail.indexOf("f1") === -1 || rail.indexOf("the finding content") === -1)
    throw new Error("REGRESSION: the rail must list the subject's ABOUT finding: " + rail);
  if (rail.indexOf("combat resolution") === -1)
    throw new Error("REGRESSION: the rail must list the concept the subject realizes: " + rail);

  // The rail is a SEPARATE static sibling: none of its content leaks into the neighborhood panel,
  // and the panel keeps drawing only the walked neighborhood's own node.
  const panel = el("kgpanel")._html;
  if (panel.indexOf("use the shared authority") !== -1 || panel.indexOf("combat resolution") !== -1)
    throw new Error("REGRESSION: the memory rail's content must NOT be folded into the neighborhood panel: " + panel);
  if (panel.indexOf("combat.rs::fire") === -1)
    throw new Error("the neighborhood panel still draws its own seeded node: " + panel);

  // Return to the whole graph: the rail hides again, exactly like the subject-lens control.
  const headListeners = (el("kghead")._listeners && el("kghead")._listeners.click) || [];
  if (!headListeners.length) throw new Error("no delegated click listener on the KG header (seam unwired)");
  fire(headListeners, handle("kgclear", "1"));
  await flush();
  if (el("kgrail").hidden !== true)
    throw new Error("returning to the whole graph must hide the memory rail again");

  console.log("OK memory-rail-shows-on-seed-and-hides-on-clear");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// RUNTIME guard (spec 63 c5, SUBJECT VIEW): clicking a node reveals the docked memory rail with the
/// subject's decisions/findings/concepts, never leaking that content into the neighborhood panel
/// itself, and clearing the subject hides the rail again. Dropping `renderMemoryRail`'s wiring (or
/// folding its content into `#kgpanel`) reddens this.
#[test]
fn clicking_a_node_reveals_the_memory_rail_without_touching_the_neighborhood_panel() {
    if !node_available() {
        eprintln!(
            "SKIP clicking_a_node_reveals_the_memory_rail_without_touching_the_neighborhood_panel: \
             no `node` runtime on PATH. This runtime guard needs node (present on dev machines and \
             on ubuntu-latest CI); install node to run it."
        );
        return;
    }
    run_node_harness(
        &build_harness(RESOLVING_FETCH, RAIL_DRIVER),
        "OK memory-rail-shows-on-seed-and-hides-on-clear",
    );
}
