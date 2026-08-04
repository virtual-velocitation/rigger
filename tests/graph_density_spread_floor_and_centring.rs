//! Periphery (integration) tests for spec 59 criterion 2 - the BOUNDARY behavior of DENSITY-SCALED
//! SPACING. Criterion 2 owns the spacing rule, whose done-when (the force extent grows with node count
//! AND label area, and every edge is drawable at real densities, deterministically) is proven by
//! `readable_graph_density_scaled_spacing.rs`. That proof only ever exercises the DENSE, accessors-
//! present path where the spread factor is > 1; it is structurally blind to the three other branches
//! of the served page's own JS (`src/dash.html`: `kgSpread` / `forceLayout`) that this file adds:
//!
//!   * the SPREAD FLOOR + degenerate inputs - `kgSpread` returns EXACTLY 1 when the radius/label
//!     accessors are absent (the layered call-DAG and the bare structural harness path), when the node
//!     set is empty, and when the graph is SPARSE (demand below the panel), so a sparse overview keeps
//!     the panel and is never shrunk below it (the floor), with a dense graph proving > 1 so the
//!     assertions are not vacuously satisfied by a kgSpread that always returns 1;
//!   * the BARE 4-arg BACK-COMPAT path - `forceLayout(nodes, edges, width, height)` with no accessors
//!     lays out on the panel EXACTLY as before c2 (every node within the panel box, the enlarged-canvas
//!     centring a no-op because the factor is 1), while the SAME graph through the 6-arg accessor path
//!     grows past the panel - so it is the accessors, not the arg count, that drive the scaling; and
//!   * the CENTRING translation - the enlarged canvas is centred on the panel so the reset pan/zoom
//!     view opens on the MIDDLE of the drawing (the drawing's bounding-box centre sits on the panel
//!     centre). The done-when proof measures only the EXTENT, which a translation leaves unchanged, so
//!     it would stay green even if the centring were dropped and the drawing clumped to a corner.
//!
//! The done-when proof never routes an absent/sparse graph through kgSpread, never calls the bare
//! 4-arg forceLayout, and never asserts on the drawing's position on the panel, so these branches
//! would regress silently without this layer.
//!
//! Same runtime harness as the density proof and the wider dash viz suite: node's built-in `vm`
//! (hermetic, no package install) EXECUTES the served page's `<script>` and the driver calls the
//! page's PURE layout functions directly. The runtime guards SKIP (do not fail) when no `node` is on
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
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-density-spread-harness.js" });
"##;

/// Splice `driver` (a JS body, no backticks / no `${...}`) into the harness, run it against the served
/// page's script under node, and return (success, stdout, stderr). The caller asserts on the sentinel
/// the driver prints so a silent early return can never masquerade as a pass.
fn run_driver(page: &str, driver: &str) -> (bool, String, String) {
    let script = page_script(page);
    let harness = format!("{HARNESS_HEAD}{driver}{HARNESS_TAIL}");

    let dir = tempfile::tempdir().expect("a scratch dir for the density-spread harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, &harness).expect("write the density-spread harness");
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

/// STRUCTURAL: the served page wires the LAYERED call-DAG path (`opts.layout`) with the BARE 4-arg
/// `forceLayout` signature - no radius/label accessors - so that path never density-scales. This is the
/// complement of the density proof's assertion that the FORCE path is fed the 6-arg accessor call; both
/// together pin the c2 wiring: force scales, layered does not. Runs in the shim-only lane (no node).
#[test]
fn the_served_page_wires_the_layered_path_without_the_density_accessors() {
    let page = dash::live_page();
    assert!(
        page.contains("opts.layout(nodes, edges, width, height)"),
        "the layered call-DAG path must call forceLayout's stand-in with the bare 4-arg signature so \
         it never density-scales (the force path carries the accessors instead)"
    );
}

/// The SPREAD FLOOR and the degenerate inputs: `kgSpread` returns EXACTLY 1 without accessors, for an
/// empty node set, and for a SPARSE graph (the floor keeps a sparse overview at the panel and never
/// shrinks it below), while a dense graph is > 1 so the floor assertions are non-vacuous.
const DRIVER_SPREAD_FLOOR: &str = r##";(function(){
  var W = 900, H = 520;
  function rad(n){ return n.r; }
  function lab(n){ return n.label; }

  // (1) Accessors ABSENT (the layered / bare-structural path) -> the factor is EXACTLY 1, so that
  //     caller lays out on the panel exactly as before c2. A strict === guards against any drift off 1.
  var some = [{ id: "a", r: 20, label: "x" }, { id: "b", r: 20, label: "x" }, { id: "c", r: 20, label: "x" }];
  var fNoAcc = kgSpread(some, W, H);
  if (fNoAcc !== 1)
    throw new Error("kgSpread must be exactly 1 without accessors, got " + fNoAcc);

  // (2) EMPTY node set -> exactly 1 even when accessors are supplied (no division by a zero demand).
  var fEmpty = kgSpread([], W, H, rad, lab);
  if (fEmpty !== 1)
    throw new Error("kgSpread must be exactly 1 for an empty node set, got " + fEmpty);

  // (3) SPARSE graph: a few small nodes on a large panel, so the total collision-body demand is far
  //     below the panel area and the raw sqrt is < 1. The Math.max(1, ...) FLOOR pins it at 1 - a
  //     sparse overview keeps the panel and is NEVER shrunk below it.
  var sparse = [];
  for (var i = 0; i < 3; i++) sparse.push({ id: "s" + i, r: 5, label: "a" });
  var fSparse = kgSpread(sparse, W, H, rad, lab);
  if (fSparse !== 1)
    throw new Error("kgSpread must FLOOR a sparse graph to exactly 1, got " + fSparse);

  // (4) NON-VACUITY: a dense graph must exceed 1, so (1)-(3) are not trivially satisfied by a kgSpread
  //     that returns 1 unconditionally (a mutation the density proof also catches, pinned here too).
  var dense = [];
  for (var j = 0; j < 160; j++) dense.push({ id: "d" + j, r: 26, label: "src/some/long-module-name/component-" + j });
  var fDense = kgSpread(dense, W, H, rad, lab);
  if (!(fDense > 1))
    throw new Error("a dense graph must spread past the panel (factor > 1), got " + fDense);

  console.log("OK spread-floor");
})();
"##;

/// Proof of the floor + degenerate branches (see `DRIVER_SPREAD_FLOOR`): `kgSpread` is exactly 1 off
/// the dense-with-accessors path, so a sparse or accessor-less caller keeps the panel unchanged.
#[test]
fn the_spread_factor_floors_at_one_off_the_dense_path() {
    if !node_available() {
        eprintln!(
            "SKIP the_spread_factor_floors_at_one_off_the_dense_path: no `node` runtime on PATH. This \
             runtime guard needs node (present on dev machines and on ubuntu-latest CI); install node \
             to run it."
        );
        return;
    }
    let page = dash::live_page();
    let (ok, stdout, stderr) = run_driver(&page, DRIVER_SPREAD_FLOOR);
    assert!(
        ok && stdout.contains("OK spread-floor"),
        "kgSpread must floor at 1 off the dense path:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// The BARE 4-arg BACK-COMPAT path: `forceLayout(nodes, edges, width, height)` with no accessors lays
/// out WITHIN the panel box (the pre-c2 behavior, the enlarged-canvas centring a no-op), the empty
/// graph returns `{}`, and the SAME graph through the 6-arg accessor path grows past the panel - so it
/// is the accessors, not merely a longer arg list, that drive the density scaling.
const DRIVER_BACKCOMPAT_PANEL: &str = r##";(function(){
  var W = 900, H = 520, maxCount = 40, N = 160;
  var nodes = [], edges = [];
  for (var i = 0; i < N; i++) nodes.push({ id: "n" + i, count: 1 + (i * 7) % maxCount, label: "src/module/component-" + i });
  for (var j = 0; j < N; j++) {
    edges.push({ from: "n" + j, to: "n" + ((j + 1) % N) });
    edges.push({ from: "n" + j, to: "n" + ((j + 7) % N) });
  }
  function rad(n){ return 7 + 22 * (Math.sqrt(n.count) / Math.sqrt(maxCount)); }
  function lab(n){ return n.label + " (" + n.count + ")"; }
  var ids = nodes.map(function(n){ return n.id; });

  function bbox(pos){
    var mnx = Infinity, mny = Infinity, mxx = -Infinity, mxy = -Infinity;
    ids.forEach(function(id){
      var p = pos[id];
      if (!p || !isFinite(p.x) || !isFinite(p.y)) throw new Error("non-finite position for " + id);
      mnx = Math.min(mnx, p.x); mny = Math.min(mny, p.y);
      mxx = Math.max(mxx, p.x); mxy = Math.max(mxy, p.y);
    });
    return { mnx: mnx, mny: mny, mxx: mxx, mxy: mxy, w: mxx - mnx, h: mxy - mny };
  }

  // (1) The EMPTY graph is a hard early-return contract: {} regardless of the arg count.
  if (Object.keys(forceLayout([], [], W, H)).length !== 0)
    throw new Error("empty forceLayout(4-arg) must be {}");
  if (Object.keys(forceLayout([], [], W, H, rad, lab)).length !== 0)
    throw new Error("empty forceLayout(6-arg) must be {}");

  // (2) BARE 4-arg (no accessors) -> factor 1 -> the drawing stays WITHIN the panel box [0,W]x[0,H]
  //     (fitToBox pads it to [pad, W-pad]); the enlarged-canvas centring is the no-op offset-zero
  //     branch. This is the pre-c2 behavior the layered call-DAG and the c1 harness still rely on.
  var bare = forceLayout(nodes, edges, W, H);
  var bb = bbox(bare);
  if (!(bb.mnx >= -0.001 && bb.mny >= -0.001 && bb.mxx <= W + 0.001 && bb.mxy <= H + 0.001))
    throw new Error("the bare 4-arg layout must stay within the panel box: " + JSON.stringify(bb));

  // (3) The SAME graph through the 6-arg ACCESSOR path density-scales: its extent is STRICTLY larger
  //     than the panel-sized one, and the enlarged, centred drawing ESCAPES the panel box (min < 0 or
  //     max > panel). So the accessors, not the arg count, drive the scaling.
  var dense = forceLayout(nodes, edges, W, H, rad, lab);
  var db = bbox(dense);
  if (!(db.w > bb.w && db.h > bb.h))
    throw new Error("the 6-arg extent must exceed the bare 4-arg extent: bare=" + JSON.stringify(bb) + " dense=" + JSON.stringify(db));
  if (!(db.mnx < -0.001 || db.mxx > W + 0.001 || db.mny < -0.001 || db.mxy > H + 0.001))
    throw new Error("the density-scaled drawing must escape the panel box: " + JSON.stringify(db));

  console.log("OK backcompat-panel");
})();
"##;

/// Proof that the bare 4-arg `forceLayout` preserves the pre-c2 panel-sized layout while the 6-arg
/// accessor path scales past it (see `DRIVER_BACKCOMPAT_PANEL`).
#[test]
fn the_bare_four_arg_layout_stays_panel_sized_and_the_accessor_path_grows_past_it() {
    if !node_available() {
        eprintln!(
            "SKIP the_bare_four_arg_layout_stays_panel_sized_and_the_accessor_path_grows_past_it: no \
             `node` runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }
    let page = dash::live_page();
    let (ok, stdout, stderr) = run_driver(&page, DRIVER_BACKCOMPAT_PANEL);
    assert!(
        ok && stdout.contains("OK backcompat-panel"),
        "the bare 4-arg layout must stay panel-sized while the accessor path grows past it:\n--- \
         stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// The CENTRING translation: the enlarged canvas is centred on the panel, so the drawing's bounding-box
/// centre sits on the panel centre (W/2, H/2) - the reset pan/zoom view opens on the MIDDLE of the
/// drawing. Non-vacuously on the enlarged path (the spread factor is proven > 1, so the centring is a
/// real, non-zero translation). Without it the drawing would fill [pad, cw-pad] and its centre would be
/// cw/2, far to the bottom-right of the panel - which the extent-only done-when proof cannot see.
const DRIVER_CENTRED_CANVAS: &str = r##";(function(){
  var W = 900, H = 520, maxCount = 40, N = 120;
  var nodes = [], edges = [];
  for (var i = 0; i < N; i++) nodes.push({ id: "n" + i, count: 1 + (i * 7) % maxCount, label: "src/module/component-" + i });
  for (var j = 0; j < N; j++) {
    edges.push({ from: "n" + j, to: "n" + ((j + 1) % N) });
    edges.push({ from: "n" + j, to: "n" + ((j + 7) % N) });
  }
  function rad(n){ return 7 + 22 * (Math.sqrt(n.count) / Math.sqrt(maxCount)); }
  function lab(n){ return n.label + " (" + n.count + ")"; }
  var ids = nodes.map(function(n){ return n.id; });

  // Non-vacuity: this density genuinely enlarges the canvas (factor > 1), so the centring below is a
  // real translation, not the trivial factor-1 no-op that would also sit at the panel centre.
  var f = kgSpread(nodes, W, H, rad, lab);
  if (!(f > 1.05))
    throw new Error("expected an enlarged canvas (factor > 1) for this density, got f=" + f);

  var pos = forceLayout(nodes, edges, W, H, rad, lab);
  var mnx = Infinity, mny = Infinity, mxx = -Infinity, mxy = -Infinity;
  ids.forEach(function(id){
    var p = pos[id];
    if (!p || !isFinite(p.x) || !isFinite(p.y)) throw new Error("non-finite position for " + id);
    mnx = Math.min(mnx, p.x); mny = Math.min(mny, p.y);
    mxx = Math.max(mxx, p.x); mxy = Math.max(mxy, p.y);
  });
  var cx = (mnx + mxx) / 2, cy = (mny + mxy) / 2;

  // The enlarged canvas is CENTRED on the panel: the drawing's bbox centre sits on (W/2, H/2). Without
  // the centring translation the bbox would be [pad, cw-pad] and its centre cw/2 >> W/2.
  if (Math.abs(cx - W / 2) > 1.0 || Math.abs(cy - H / 2) > 1.0)
    throw new Error("the enlarged canvas must be centred on the panel middle: centre=(" +
      cx.toFixed(3) + "," + cy.toFixed(3) + ") panel-centre=(" + (W / 2) + "," + (H / 2) + ")");

  console.log("OK centred-canvas");
})();
"##;

/// Proof that the enlarged canvas is centred on the panel middle (see `DRIVER_CENTRED_CANVAS`) - the
/// reset view opens on the drawing's centre, a claim the extent-only done-when proof is blind to.
#[test]
fn the_enlarged_canvas_is_centred_on_the_panel_middle() {
    if !node_available() {
        eprintln!(
            "SKIP the_enlarged_canvas_is_centred_on_the_panel_middle: no `node` runtime on PATH. This \
             runtime guard needs node (present on dev machines and on ubuntu-latest CI); install node \
             to run it."
        );
        return;
    }
    let page = dash::live_page();
    let (ok, stdout, stderr) = run_driver(&page, DRIVER_CENTRED_CANVAS);
    assert!(
        ok && stdout.contains("OK centred-canvas"),
        "the enlarged canvas must be centred on the panel middle:\n--- stdout ---\n{stdout}\n--- \
         stderr ---\n{stderr}"
    );
}
