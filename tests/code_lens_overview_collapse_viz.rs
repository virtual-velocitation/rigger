//! Periphery (integration) test for the code lens's OVERVIEW COLLAPSE client seam (spec 63,
//! criterion 6): zoomed out, the coupling-community hulls collapse into genuine super-nodes -
//! one per community, never one per member - sized by member count, labelled by the community's
//! own deterministic name (falling back to its own opaque id, never to its kind), and carrying no
//! storage-schema name anywhere on the collapsed canvas.
//!
//! `clustered_overview`/`cluster_detail` (spec 42/53) and the served page's `renderKgOverview`
//! already implement this mechanism, and criterion 1's `code_lens_view_periphery.rs` already pins
//! the RUST payload's purity/sizing/labelling (`Cluster { key, count, kind, label }`) under
//! `Lens::Code`. What is NOT yet pinned is the CLIENT half: the spec's own "Test seams" note commits
//! "overview collapse" to a node-gated runtime harness (spec 42/55/59 precedent), and peer finding
//! `sdet-u63c1-hull-treatment-and-js-purity-untested` names the exact gap this file closes - "no
//! test could catch a future JS regression that leaked a file/bucket node into the live dash for
//! this lens, only the Rust payload is proven pure". This file drives the served page's OWN
//! `renderKgOverview` under node's built-in `vm` (hermetic, no npm) and proves the client-side
//! render contract independently of the server payload's own correctness.
//!
//! `dash` compiles on BOTH the default and the `--no-default-features` lane (the viz is not
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

/// The minimal DOM shim the overview render path touches (node `vm`, no npm): innerHTML on the
/// panel element, a stubbed `querySelector` so `bindKgView`'s post-render lookups resolve without
/// throwing, and a `clientWidth`/`clientHeight` so `kgDims` sizes the drawing box. `renderKgOverview`
/// is called directly (no network involved), so no `fetch` stub is needed.
const DOM_SHIM: &str = r#"
function __Stub(){ this._attrs = {}; }
__Stub.prototype.setAttribute = function(k,v){ this._attrs[k] = String(v); };
__Stub.prototype.getAttribute = function(k){ return this._attrs[k]; };
__Stub.prototype.addEventListener = function(){};
__Stub.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; };
function __El(id){ this.id=id; this._html=""; this._text=""; this._listeners={}; this.dataset={};
  this.clientWidth = 800; this.clientHeight = 300;
  this.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 800, height: 300 }; }; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.querySelector = function(){ return new __Stub(); };
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
const __els = {};
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };
"#;

/// Assemble a complete node `vm` program: the DOM shim, then the served page script (read from
/// `argv[2]`), then the driver - which shares the page's own scope, so it calls the page's real
/// `renderKgOverview` and reads its module state directly.
fn build_harness(driver: &str) -> String {
    const TEMPLATE: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const SHIM = String.raw`__COLLAPSE_SHIM__`;
const DRIVER = String.raw`__COLLAPSE_DRIVER__`;
const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-overview-collapse-harness.js" });
"##;
    TEMPLATE
        .replace("__COLLAPSE_SHIM__", DOM_SHIM)
        .replace("__COLLAPSE_DRIVER__", driver)
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
        .expect("spawn node to drive the overview collapse client seam");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the runtime harness must drive the overview collapse seam, but node failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains(ok_token),
        "the runtime harness must confirm '{ok_token}':\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// Driver: feeds `renderKgOverview` a `Lens::Code`-shaped overview - TWO communities of eight total
/// members (six and two), the larger one carrying a deterministic display label, the smaller one
/// carrying none (a real, documented case: the fold's highest-degree member has no nameable label
/// attr) - and proves the collapsed render:
///   (1) COLLAPSES each community to exactly ONE super-node (two `data-cluster` handles for eight
///       underlying members - the hulls are gone, not merely tinted);
///   (2) SIZES each super-node by its own member count (the six-member community draws a strictly
///       bigger circle than the two-member one);
///   (3) LABELS the named community by its own name, and the unnamed one by its own opaque id -
///       NEVER by its kind (the fallback a future regression could reach for, which would leak a
///       storage-schema name the moment any lens's fold ever regressed to admit one); and
///   (4) carries NO storage-schema kind literal anywhere on the collapsed canvas.
const COLLAPSE_DRIVER: &str = r#"
;(async function(){
  const OVERVIEW = {
    clusters: [
      { key: "community/1/0", count: 6, kind: "code-entity", label: "orchestrator" },
      { key: "community/1/1", count: 2, kind: "code-entity" }
    ],
    edges: [ { from: "community/1/0", to: "community/1/1", weight: 3 } ],
    total: 8
  };

  renderKgOverview(OVERVIEW);
  const panel = el("kgpanel")._html;

  // Scope every check below to the DRAWN graph region only (the #kgzoom group's own subtree) -
  // never the surrounding toolbar chrome, whose lens SWITCHER legitimately reads "files" (the tab
  // name for a DIFFERENT lens a human can pick) without that being a node or group label on canvas.
  const zoomAt = panel.indexOf('<g id="kgzoom">');
  if (zoomAt === -1) throw new Error("no #kgzoom drawing group rendered: " + panel);
  const html = panel.slice(zoomAt);

  // (1) COLLAPSE: exactly one super-node per community - eight members render as TWO handles, not eight.
  const handles = (html.match(/data-cluster="/g) || []).length;
  if (handles !== 2)
    throw new Error("the overview must collapse each community to exactly ONE super-node, not one per member (8 members, 2 communities): " + handles + " handles in " + html);

  // (2) SIZED BY MEMBER COUNT: the six-member community draws a strictly bigger circle than the two-member one.
  function radiusOf(key) {
    const marker = 'data-cluster="' + key + '"';
    const i = html.indexOf(marker);
    if (i === -1) throw new Error("no rendered super-node for community " + key + ": " + html);
    const m = html.slice(i, i + 400).match(/<circle r="([0-9.]+)"/);
    if (!m) throw new Error("no circle radius found for community " + key + ": " + html.slice(i, i + 400));
    return parseFloat(m[1]);
  }
  const rBig = radiusOf("community/1/0");
  const rSmall = radiusOf("community/1/1");
  if (!(rBig > rSmall))
    throw new Error("the six-member community must draw a BIGGER super-node than the two-member one: " + rBig + " vs " + rSmall);

  // (3) LABELLED BY COMMUNITY NAME, safely: the named community shows "name (count)"; the unnamed one
  // falls back to its own opaque id "(count)" - never to its kind.
  if (html.indexOf("orchestrator (6)") === -1)
    throw new Error("a community WITH a deterministic label must be captioned by that name: " + html);
  if (html.indexOf("community/1/1 (2)") === -1)
    throw new Error("a community with NO label must fall back to its own id, not its kind: " + html);

  // (4) NO STORAGE-SCHEMA NAME ever appears as a node or group label at the collapsed zoom. Matched
  // as a WHOLE WORD (word-boundary regex): the community key "community/1/1" legitimately contains
  // "unit" as a substring (comm-UNIT-y), which is not the storage-schema kind "unit" leaking - only
  // an isolated occurrence of the schema token itself is a genuine purity violation.
  const forbidden = ["file", "decision", "design-doc", "artifact", "agent", "gate", "unit", "lesson",
                      "finding", "arch-decision", "handbook-rule", "rationale"];
  for (let k = 0; k < forbidden.length; k++) {
    const re = new RegExp("\\b" + forbidden[k] + "\\b");
    if (re.test(html))
      throw new Error("REGRESSION: a storage-schema name ('" + forbidden[k] + "') leaked into the collapsed overview: " + html);
  }

  console.log("OK overview-collapses-to-sized-labelled-community-super-nodes-purely");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"#;

/// RUNTIME guard (spec 63 c6, OVERVIEW COLLAPSE): the served page's `renderKgOverview` collapses a
/// `Lens::Code` overview into sized, labelled community super-nodes and never a storage-schema name.
/// Dropping the collapse (rendering members instead of clusters), the count-based sizing, the
/// id-not-kind label fallback, or leaking a schema token onto the canvas each reddens this.
#[test]
fn the_overview_collapses_to_sized_labelled_community_super_nodes_purely() {
    if !node_available() {
        eprintln!(
            "SKIP the_overview_collapses_to_sized_labelled_community_super_nodes_purely: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }
    run_node_harness(
        &build_harness(COLLAPSE_DRIVER),
        "OK overview-collapses-to-sized-labelled-community-super-nodes-purely",
    );
}

/// STRUCTURAL companion (the fallback proof when `node` is unavailable): the served page's
/// `renderKgOverview` sizes by `Math.sqrt(n.count)` and labels via `n.label || n.id` - bound to the
/// exact mechanism text, so an unrelated token elsewhere on the page cannot satisfy it, and a
/// regression to a raw-kind fallback (`n.label || n.kind`) would flip the negative assertion.
#[test]
fn the_served_overview_renderer_sizes_by_count_and_falls_back_to_id_not_kind() {
    let page = dash::live_page();
    let r = page
        .find("function renderKgOverview(")
        .expect("the served page carries renderKgOverview");
    let body = &page[r..(r + 2000).min(page.len())];
    assert!(
        body.contains("Math.sqrt(n.count)"),
        "the overview must size each super-node by its own member count: {body}"
    );
    assert!(
        body.contains("n.label || n.id"),
        "an unlabelled community must fall back to its own opaque id: {body}"
    );
    assert!(
        !body.contains("n.label || n.kind") && !body.contains("n.kind + \" (\""),
        "the label must never fall back to the raw kind - a storage-schema name for any lens whose \
         fold ever regressed to admit one: {body}"
    );
}
