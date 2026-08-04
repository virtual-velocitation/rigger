//! Periphery (integration) tests for spec 59 criterion 1 - the BOUNDARY behavior of the collision-free
//! SEPARATION PASS. Criterion 1 owns the separation pass, whose done-when (no collision-body overlap at
//! real densities, deterministically) is proven by `readable_graph_layout_separation.rs`. This file
//! adds the OUTSIDE-IN layer that the density proof is structurally blind to: the exact collision-body
//! CONTRACT and the pass's DEGENERATE branches.
//!
//! Two properties, each a distinct branch of the served page's own JS (`src/dash.html`:
//! `kgLabelDims` / `kgNodeBody` / `kgSeparate`):
//!   * the COLLISION BODY encloses the circle PLUS the label box - a longer label widens the body
//!     horizontally beyond the bare circle (the label is part of the node), the label box hangs BELOW
//!     the circle (the body is asymmetric in y, so text can never sit above the node), and an empty
//!     label collapses the horizontal extent back to the padded circle (no phantom label width); and
//!   * the separation pass's DEGENERATE branches - fewer than two positioned nodes is a safe no-op,
//!     and two nodes seeded at the EXACT same point (a force-pass degeneracy) are pushed to disjoint
//!     bodies DETERMINISTICALLY, the id tie-break fixing the direction so two polls never diverge.
//!
//! The density proof never places exactly-coincident centres and never asserts on the body's own
//! geometry, so these branches would regress silently without this layer.
//!
//! Same runtime harness as the density proof and the wider dash viz suite: node's built-in `vm`
//! (hermetic, no package install) EXECUTES the served page's `<script>` and the driver calls the
//! page's PURE layout functions directly. The runtime guard SKIPs (does not fail) when no `node` is on
//! PATH. `dash` compiles on BOTH the default and the `--no-default-features` lane (the viz is not
//! feature-gated), so this guards the served page in both lanes.

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

/// The head of the node-vm harness: a minimal DOM shim so the page's top-level wiring
/// (`el(...).addEventListener`, `loadKgOverview()`) does not throw, then the opening of the driver's
/// `String.raw` template. The served page script and the driver body are spliced in by `run_driver`.
const HARNESS_HEAD: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

const SHIM = String.raw`
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
`;

const DRIVER = String.raw`
"##;

/// The tail: close the driver template, then run the shim, the served page, and the driver together in
/// one sandbox that only exposes `console`, so the driver drives the page's real layout code.
const HARNESS_TAIL: &str = r##"
`;
const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-collision-harness.js" });
"##;

/// Splice `driver` (a JS body, no backticks / no `${...}`) into the harness, run it against the served
/// page's script under node, and return (success, stdout, stderr). The caller asserts on the sentinel
/// the driver prints so a silent early return can never masquerade as a pass.
fn run_driver(page: &str, driver: &str) -> (bool, String, String) {
    let script = page_script(page);
    let harness = format!("{HARNESS_HEAD}{driver}{HARNESS_TAIL}");

    let dir = tempfile::tempdir().expect("a scratch dir for the collision-body harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, &harness).expect("write the collision-body harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served layout");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The collision body (`kgNodeBody`, built from `kgLabelDims`) encloses the circle PLUS the label box:
/// a long label widens the body horizontally beyond the bare circle, the label box hangs BELOW the
/// circle (asymmetric in y), and an empty label leaves the body at the padded circle with no phantom
/// label width. This binds the "label is part of the node" contract directly - the density proof only
/// asserts that composed bodies do not overlap, never that the body itself encloses the label.
const DRIVER_COLLISION_BODY: &str = r##";(function(){
  var r = 10;
  var empty = kgNodeBody(0, 0, r, "");
  var longLabel = "";
  for (var i = 0; i < 40; i++) longLabel += "x";   // 40 glyphs, far wider than the r=10 circle
  var wide = kgNodeBody(0, 0, r, longLabel);

  // (1) The label is PART of the node: a long label widens the collision body horizontally beyond the
  //     bare circle. Were kgNodeBody to ignore the label this half-width would not grow.
  var emptyHalfW = (empty.x1 - empty.x0) / 2;
  var wideHalfW = (wide.x1 - wide.x0) / 2;
  if (!(wideHalfW > emptyHalfW))
    throw new Error("a long label must widen the collision body beyond the circle: wideHalfW=" + wideHalfW + " emptyHalfW=" + emptyHalfW);

  // (2) An EMPTY label contributes no horizontal width: the body half-width is the circle plus only a
  //     hair of pad, never the phantom width of a label.
  if (!(emptyHalfW >= r && emptyHalfW <= r + 4))
    throw new Error("an empty label must leave the body at the padded circle (halfW=" + emptyHalfW + " for r=" + r + ")");

  // (3) The label box hangs BELOW the circle: for ANY node the body reaches farther DOWN than UP, by
  //     at least a text line's height - the label sits under the node, so text can never cover it.
  [["empty", empty], ["wide", wide]].forEach(function(pair){
    var name = pair[0], b = pair[1];
    var up = 0 - b.y0;     // reach above the centre (the centre is at y = 0)
    var down = b.y1 - 0;   // reach below the centre
    if (!(down > up && (down - up) >= 10))
      throw new Error("the collision body must hang below the circle by a label line for the " + name + " label: up=" + up + " down=" + down);
  });

  console.log("OK collision-body-encloses-circle-and-label");
})();"##;

/// The separation pass's DEGENERATE branches: fewer than two positioned nodes is a safe no-op, and two
/// nodes at the EXACT same point are pushed to disjoint bodies DETERMINISTICALLY, the id tie-break
/// fixing the direction. A coincident pair is precisely where a random or order-dependent resolver
/// would diverge across polls; the id tie-break makes it reproducible.
const DRIVER_TIEBREAK: &str = r##";(function(){
  var r = 10;
  function radiusOf(){ return r; }
  function labelOf(){ return ""; }
  function bodiesOverlap(A, B){
    var ox = Math.min(A.x1, B.x1) - Math.max(A.x0, B.x0);
    var oy = Math.min(A.y1, B.y1) - Math.max(A.y0, B.y0);
    return ox > 0 && oy > 0;
  }

  // (1) NO-OP boundary: fewer than two positioned nodes must never throw and never move anything.
  var single = { only: { x: 5, y: 9 } };
  kgSeparate(single, [{ id: "only" }], radiusOf, labelOf);
  if (single.only.x !== 5 || single.only.y !== 9)
    throw new Error("a single node must be left untouched by the separation pass");
  kgSeparate({}, [], radiusOf, labelOf);   // an empty layout must not throw

  // A pair seeded at the EXACT same point - the degeneracy a force pass can leave behind.
  function coincidentPair(){
    return {
      nodes: [{ id: "node-a" }, { id: "node-b" }],
      pos: { "node-a": { x: 100, y: 100 }, "node-b": { x: 100, y: 100 } },
    };
  }

  // (2) Coincident nodes are pushed to DISJOINT bodies and end at distinct points.
  var first = coincidentPair();
  kgSeparate(first.pos, first.nodes, radiusOf, labelOf);
  var A = kgNodeBody(first.pos["node-a"].x, first.pos["node-a"].y, r, "");
  var B = kgNodeBody(first.pos["node-b"].x, first.pos["node-b"].y, r, "");
  if (bodiesOverlap(A, B))
    throw new Error("coincident nodes must be separated to disjoint bodies");
  if (first.pos["node-a"].x === first.pos["node-b"].x && first.pos["node-a"].y === first.pos["node-b"].y)
    throw new Error("coincident nodes must end at distinct positions");

  // (3) DETERMINISM via the id tie-break: an independent run is byte-identical, and the lower id
  //     (node-a) settles on the -x side - a stable direction chosen by id, not by chance.
  var second = coincidentPair();
  kgSeparate(second.pos, second.nodes, radiusOf, labelOf);
  ["node-a", "node-b"].forEach(function(id){
    if (first.pos[id].x !== second.pos[id].x || first.pos[id].y !== second.pos[id].y)
      throw new Error("the separation of coincident nodes must be deterministic across runs for " + id);
  });
  if (!(first.pos["node-a"].x < first.pos["node-b"].x))
    throw new Error("the id tie-break must send the lower id to the -x side: node-a.x=" + first.pos["node-a"].x + " node-b.x=" + first.pos["node-b"].x);

  console.log("OK coincident-separation-deterministic");
})();"##;

/// CONTRACT: the served page's collision body encloses the circle plus the label box - a long label
/// widens it horizontally, the label box hangs below the circle, and an empty label collapses back to
/// the padded circle. Guards `kgNodeBody` / `kgLabelDims` directly (the density proof exercises them
/// only indirectly).
#[test]
fn the_collision_body_encloses_the_circle_and_its_label() {
    if !node_available() {
        eprintln!(
            "SKIP the_collision_body_encloses_the_circle_and_its_label: no `node` runtime on PATH. \
             This runtime guard needs node (present on dev machines and on ubuntu-latest CI); \
             install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let (ok, stdout, stderr) = run_driver(&page, DRIVER_COLLISION_BODY);
    assert!(
        ok,
        "the collision body must enclose the circle plus its label box, but the runtime harness \
         failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK collision-body-encloses-circle-and-label"),
        "the harness must confirm the collision-body contract:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// BOUNDARY: the separation pass is a safe no-op below two positioned nodes, and it resolves an
/// exactly-coincident pair to disjoint bodies deterministically (the id tie-break fixes the direction).
/// Guards the degenerate branches the clustered-density proof never places.
#[test]
fn the_separation_pass_resolves_coincident_nodes_deterministically() {
    if !node_available() {
        eprintln!(
            "SKIP the_separation_pass_resolves_coincident_nodes_deterministically: no `node` runtime \
             on PATH. This runtime guard needs node (present on dev machines and on ubuntu-latest \
             CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let (ok, stdout, stderr) = run_driver(&page, DRIVER_TIEBREAK);
    assert!(
        ok,
        "the separation pass must resolve coincident nodes deterministically and no-op below two \
         nodes, but the runtime harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK coincident-separation-deterministic"),
        "the harness must confirm the coincident-separation tie-break:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
