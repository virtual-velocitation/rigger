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

use crate::common;

use common::served::node_harness_passes;

use crate::vm_harness::{vm_harness, DomShim, Fields};

/// The DOM every driver in this file runs under (node `vm`, no npm): the element surfaces the client
/// seam touches (innerHTML, textContent, dataset, .hidden, .className, addEventListener, a stubbed
/// querySelector so bindKgView's lookups resolve without throwing), and the `__fetched` list each
/// driver's own `fetch` records its URLs in.
const DOM: DomShim = DomShim {
    fields: Fields::Form,
    fetch_log: true,
    ..DomShim::PANEL
};

/// A `fetch` + fixtures prelude: the whole-graph overview (the no-argument route) and the seeded
/// neighborhoods, each carrying its `memory` rail ({decisions, findings, concepts}): subject
/// `combat.rs::fire` with an entry in every section, `combat.rs::aim` with one decision alone,
/// `combat.rs::reload` with nothing to list, and any other seed answered as the route answers a
/// seed the graph does not hold (no node, no edge, every section empty). Every other fetch
/// (explain overlay, instances/state poll) resolves gracefully empty, matching the live page's
/// own network shape.
const RESOLVING_FETCH: &str = r#"
const __OVERVIEW = { clusters: [ { key: "src", count: 2, kind: "code-entity" } ], edges: [], total: 2 };
const __NOTHING_TO_LIST = { decisions: [], findings: [], concepts: [] };
function __held(id, memory){
  return { seed: id, depth: 2, nodes: [ { id: id, kind: "code-entity", label: id.split("::")[1] } ], edges: [], memory: memory };
}
const __NEIGHBORHOODS = {
  "combat.rs::fire": __held("combat.rs::fire", {
    decisions: [ { id: "d1", kind: "decision", summary: "use the shared authority" } ],
    findings: [ { id: "f1", kind: "finding", summary: "the finding content" } ],
    concepts: [ { id: "concept/combat", label: "combat resolution" } ]
  }),
  "combat.rs::aim": __held("combat.rs::aim", {
    decisions: [ { id: "d2", kind: "decision", summary: "aim before firing" } ], findings: [], concepts: []
  }),
  "combat.rs::reload": __held("combat.rs::reload", __NOTHING_TO_LIST)
};
function __view(url){
  const s = String(url);
  if (s.indexOf("explain=") !== -1) return { nodes: [] };
  const m = s.match(/[?&]seed=([^&]+)/);
  if (m === null) return __OVERVIEW;
  const seed = decodeURIComponent(m[1]);
  return __NEIGHBORHOODS[seed] || { seed: seed, depth: 2, nodes: [], edges: [], memory: __NOTHING_TO_LIST };
}
const fetch = function(url){
  if (String(url).indexOf("/api/graph") !== -1) {
    __fetched.push(String(url));
    return Promise.resolve({ headers: { get: function(){ return null; } }, json: function(){ return Promise.resolve(__view(url)); } });
  }
  return Promise.reject(new Error("no network for " + url));
};
"#;

/// The node `vm` program name this suite's harness runs under.
const HARNESS_FILE: &str = "dash-memory-rail-harness.js";

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

/// Driver: the rail shows only for a subject with an entry to list. A seed the graph does not
/// hold gets the panel's message and no rail; a held subject with nothing to list shows no rail
/// either; a subject with one entry shows the rail, its other sections saying there is none; and
/// a seed the graph does not hold after that one hides the shown rail.
const NOTHING_TO_LIST_DRIVER: &str = r#"
;(async function(){
  function flush(){ return (async()=>{ for (let k=0;k<40;k++) await Promise.resolve(); })(); }
  const noRail = function(what){
    if (el("kgrail").hidden !== true || el("kgrail")._html !== "")
      throw new Error("REGRESSION: " + what + " showed the memory rail: " + el("kgrail")._html);
  };
  await flush();

  seedGraph("nowhere.rs::ghost");
  await flush();
  noRail("a seed the graph does not hold");
  if (el("kgpanel")._html.indexOf("no knowledge-graph neighborhood for this node") === -1)
    throw new Error("a seed the graph does not hold must get the panel's message: " + el("kgpanel")._html);

  seedGraph("combat.rs::reload");
  await flush();
  noRail("a subject with nothing to list");

  seedGraph("combat.rs::aim");
  await flush();
  const rail = el("kgrail")._html;
  if (el("kgrail").hidden !== false || rail.indexOf("aim before firing") === -1)
    throw new Error("REGRESSION: a subject with one entry must show it in the rail: " + rail);
  if (rail.split(">none<").length - 1 !== 2)
    throw new Error("the rail's two empty sections must each say there is none: " + rail);

  seedGraph("nowhere.rs::ghost");
  await flush();
  noRail("a seed the graph does not hold, after a subject whose rail showed,");

  console.log("OK the-rail-shows-only-a-subject-with-an-entry-to-list");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

rigger::test_cases! {
    /// RUNTIME guard (spec 63 c5, SUBJECT VIEW): clicking a node reveals the docked memory rail with the
    /// subject's decisions/findings/concepts, never leaking that content into the neighborhood panel
    /// itself, and clearing the subject hides the rail again. Dropping `renderMemoryRail`'s wiring (or
    /// folding its content into `#kgpanel`) reddens this.
    clicking_a_node_reveals_the_memory_rail_without_touching_the_neighborhood_panel:
        node_harness_passes(&vm_harness(&[&DOM.render(), RESOLVING_FETCH], RAIL_DRIVER, HARNESS_FILE), "OK memory-rail-shows-on-seed-and-hides-on-clear");
    /// RUNTIME guard that the rail shows only for a subject with an entry to list: a seed the graph
    /// does not hold gets the panel's message and no rail, so the rail never narrows the panel
    /// for a subject it has nothing to say about.
    the_rail_shows_only_for_a_subject_with_an_entry_to_list:
        node_harness_passes(&vm_harness(&[&DOM.render(), RESOLVING_FETCH], NOTHING_TO_LIST_DRIVER, HARNESS_FILE), "OK the-rail-shows-only-a-subject-with-an-entry-to-list");
}
