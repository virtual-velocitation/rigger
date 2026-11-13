//! Periphery (integration) test for the explorer's OPENING VIEW and the DRAWING'S HEIGHT.
//!
//! An overview opens FITTED to its panel: every mark it drew (each cluster's circle and label, each
//! directory hull and its label) lies inside the panel at the opening pan/zoom, scaled down as far as
//! that needs and never up past 1, and the wheel zooms on from that scale. Spec 59 lets a dense layout
//! spread past the panel, for what the user reaches by zooming in; the opening view still shows the
//! whole drawing, at every lens, every viewport width and every redraw (a lens switch, a
//! re-projection).
//!
//! And every drawing (an overview, a re-projection, a drilled cluster, a call walk) fills the room
//! its panel leaves below the toolbar that view shows, however many lines that toolbar wraps to on a
//! narrow screen, so the panel's content never runs past its own height.
//!
//! Driven by the served page's own script under node's built-in `vm` (hermetic, no npm), through a
//! DOM shim that keeps the panel's geometry (its client box, padding and toolbar heights, as a
//! browser lays them out at each viewport width) and records the transform the page applies to
//! `#kgzoom`. Each drawn mark's box is read back from the emitted markup with the page's own
//! label-size model (`kgNodeBody`, and a hull label at its 10px monospace advance), so the check
//! never trusts the extent the page computes for itself. `dash` compiles on both lanes, so this guards
//! both.

use crate::common;

use common::served::node_harness_claims;

use crate::vm_harness;
use vm_harness::vm_harness;

/// The DOM shim: the panel (`#kgpanel`) keeps the client box, padding and per-view toolbar height
/// `__geom` holds, and each markup it is given gets a fresh `svg` (recording its wheel listener, its
/// slot boxed below that markup's toolbar) and a fresh `#kgzoom` (recording its transform), as a
/// browser replaces them; every other element is a bare stand-in. Free of backticks and `${` so it
/// embeds in the harness's `String.raw` template.
const PANEL_SHIM: &str = r##"
const __els = {};
const __geom = { width: 800, height: 300, padTop: 10, padBottom: 10,
  toolbar: { overview: 24, reproject: 24, drill: 24, calls: 24 } };
const __classList = { add: function(){}, remove: function(){}, toggle: function(){}, contains: function(){ return false; } };
function __Stub(){ this._attrs = {}; this._listeners = {}; this.dataset = {}; this.classList = __classList; }
__Stub.prototype.setAttribute = function(k, v){ this._attrs[k] = String(v); };
__Stub.prototype.getAttribute = function(k){ return k in this._attrs ? this._attrs[k] : null; };
__Stub.prototype.addEventListener = function(t, f){ (this._listeners[t] = this._listeners[t] || []).push(f); };
__Stub.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 0, height: 0 }; };
function __viewOf(html){
  if (html.indexOf("data-kgclear") !== -1) return "reproject";
  if (html.indexOf("data-calls-down") !== -1) return "calls";
  if (html.indexOf("data-kgback") !== -1) return "drill";
  return "overview";
}
function __slotTop(html){ return __geom.padTop + __geom.toolbar[__viewOf(html)]; }
function __El(id){ this.id = id; this._html = ""; this._text = ""; this._listeners = {}; this.dataset = {};
  this.hidden = false; this.className = ""; this.checked = false; this.classList = __classList;
  this.clientTop = 0; this.clientLeft = 0; this._zoom = new __Stub(); this._svg = new __Stub(); }
Object.defineProperty(__El.prototype, "clientWidth", { get(){ return __geom.width; } });
Object.defineProperty(__El.prototype, "clientHeight", { get(){ return __geom.height; } });
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){
  const html = String(v);
  this._html = html;
  this._zoom = new __Stub();
  this._svg = new __Stub();
  const high = html.match(/<svg class="kgsvg"[^>]* height="([^"]*)"/);
  this._svg.getBoundingClientRect = function(){
    return { left: 12, top: __slotTop(html), width: __geom.width - 24, height: high ? Number(high[1]) : 0 };
  };
} });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: __geom.width, height: __geom.height }; };
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.querySelector = function(sel){
  if (this.id !== "kgpanel") return new __Stub();
  if (sel === "#kgzoom") return this._html.indexOf('id="kgzoom"') === -1 ? null : this._zoom;
  if (sel === "svg" || sel === ".kgsvg" || sel === "svg.kgsvg") return this._html.indexOf("<svg") === -1 ? null : this._svg;
  return new __Stub();
};
__El.prototype.addEventListener = function(t, f){ (this._listeners[t] = this._listeners[t] || []).push(f); };
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };
const getComputedStyle = function(){ return { paddingTop: __geom.padTop + "px", paddingBottom: __geom.padBottom + "px" }; };
"##;

/// A `fetch` serving deterministic overviews as dense as a real project's - the files lens's 158
/// file clusters in 32 directories (one at the repo root), the code lens's 200 communities, the
/// concepts lens's 98 concepts - a subject's re-projection under each lens (40 files, a single
/// community, 12 concepts), a drilled cluster's 12 members and a 4-layer call walk; a card or
/// rationale read finds nothing.
const OVERVIEWS_FETCH: &str = r##"
function __chain(keys, step){
  const edges = [];
  keys.forEach(function(k, i){
    edges.push({ from: k, to: keys[(i + 1) % keys.length], weight: 1 + i % 7 });
    const j = (i * step + 3) % keys.length;
    if (j !== i) edges.push({ from: k, to: keys[j], weight: 1 + i % 3 });
  });
  return keys.length > 1 ? edges : [];
}
function __fileKey(i){
  const d = i % 32;
  if (d === 0) return "build_" + i + ".rs";
  return "crates/crate-" + d + "/src" + (d % 3 === 0 ? "/nested" : "") + "/module_" + i + ".rs";
}
function __body(keys, cluster, step){
  return { clusters: keys.map(cluster), edges: __chain(keys, step), total: keys.length * 9 };
}
function __files(n){
  const keys = []; for (let i = 0; i < n; i++) keys.push(__fileKey(i));
  return __body(keys, function(k, i){ return { key: k, count: 1 + (i * 37) % 90, kind: i % 5 === 0 ? "design-doc" : "file" }; }, 7);
}
function __code(n){
  const keys = []; for (let i = 0; i < n; i++) keys.push("community-" + i);
  return __body(keys, function(k, i){ return { key: k, count: 1 + (i * 13) % 60, kind: "code-entity",
    label: "module_" + (i % 40) + "::function_name_" + i }; }, 11);
}
function __concepts(n){
  const keys = []; for (let i = 0; i < n; i++) keys.push("concept/c" + i);
  return __body(keys, function(k, i){ return { key: k, count: 1 + (i * 17) % 40, kind: "rationale",
    label: "concept about subsystem " + i }; }, 5);
}
// A drilled cluster's 12 members, the first its hub, and a call walk of 8 functions over 4 layers.
function __members(n, layered){
  const keys = []; for (let i = 0; i < n; i++) keys.push("crates/crate-1/src/module_1.rs::item_" + i);
  const nodes = keys.map(function(k, i){ return { id: k, label: "item_" + i, kind: "code-entity",
    degree: 1 + i % 5, god: i === 0, layer: layered ? Math.floor(i / 2) : undefined }; });
  const edges = layered
    ? keys.slice(2).map(function(k, i){ return { from: keys[i], to: k }; })
    : __chain(keys, 3);
  return { nodes: nodes, edges: edges };
}
const __SUBJECT = "crates/crate-1/src/module_1.rs::Thing";
const __CLUSTER = "crates/crate-1/src";
const __DRILL = __members(12, false);
const __CALLS = __members(8, true);
const __OVERVIEWS = { files: __files(158), code: __code(200), concepts: __concepts(98) };
const __REPROJECTIONS = { files: __files(40), code: __code(1), concepts: __concepts(12) };
function __graph(url){
  const s = String(url);
  if (s.indexOf("explain=") !== -1) return { nodes: [] };
  if (s.indexOf("card=") !== -1) return {};
  if (s.indexOf("view=calls") !== -1) return { seed: __SUBJECT, dir: "down", nodes: __CALLS.nodes, edges: __CALLS.edges };
  if (s.indexOf("cluster=") !== -1) return { seed: __CLUSTER, nodes: __DRILL.nodes, edges: __DRILL.edges };
  const lens = (s.match(/[?&]lens=([a-z]+)/) || [null, "files"])[1];
  if (s.indexOf("seed=") !== -1) {
    const rp = __REPROJECTIONS[lens];
    return { subject: __SUBJECT, lens: lens, clusters: rp.clusters, edges: rp.edges, total: rp.total };
  }
  return __OVERVIEWS[lens];
}
const fetch = function(url){
  if (String(url).indexOf("/api/graph") === -1) return Promise.reject(new Error("no network for " + url));
  return Promise.resolve({ headers: { get: function(){ return null; } }, json: function(){ return Promise.resolve(__graph(url)); } });
};
"##;

/// What both drivers read back from the page: settling its fetches, and the drawing the panel holds,
/// refusing a message in its place and a drawing box that is not a finite, positive size.
const PAGE_READERS: &str = r##"
// Let every pending promise the page chained settle (its fetches resolve on the microtask queue).
function __flush(){ return (async function(){ for (let k = 0; k < 80; k++) await Promise.resolve(); })(); }
// The drawing the panel holds: its viewBox size and its svg's height attribute, refusing a panel that
// holds no drawing (a message such as "unavailable" in its place) and a viewBox that is not a finite,
// positive size.
function __svgBox(what){
  const html = el("kgpanel")._html;
  const svg = html.match(/<svg class="kgsvg"[^>]* height="([^"]*)" viewBox="0 0 ([^ "]+) ([^ "]+)"/);
  if (!svg) throw new Error(what + " drew no graph: " + html.slice(0, 300));
  const height = Number(svg[1]), W = Number(svg[2]), H = Number(svg[3]);
  if (!(isFinite(W) && isFinite(H) && W > 0 && H > 0 && height === H))
    throw new Error(what + " drew a degenerate drawing box " + svg[0]);
  return { html: html, W: W, H: H, height: height };
}
"##;

/// The node `vm` program name this suite's harness runs under.
const HARNESS_FILE: &str = "dash-opening-view-harness.js";

/// Driver, in the panel `__PANEL__` names: the files overview the page loads, the code and concepts
/// overviews a lens tab opens, the files overview a lens tab redraws after a zoom, and the subject's
/// re-projection under each lens each open with every drawn box inside the panel at a scale of at
/// most 1; on each overview the wheel zooms in and back from the opening scale, and out no further
/// than it or 0.2, whichever is less; and the redrawn overview opens where the first one did.
const OPENING_DRIVER: &str = r##"
;(async function(){
  const HULL_CHAR_W = 10 * (0.6 + 0.08);
  const TOL = 0.5;
  const unesc = function(s){ return s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'").replace(/&amp;/g, "&"); };
  const dirs = function(body){ return new Set(body.clusters.map(function(c){
    const i = c.key.lastIndexOf("/"); return i === -1 ? "(root)" : c.key.slice(0, i); })).size; };
  function drawn(what){
    const svg = __svgBox(what), html = svg.html, W = svg.W, H = svg.H;
    const boxes = [];
    let m, nodes = 0, hulls = 0;
    const nodeRe = /<g class="kgn [^"]*"[^>]* transform="translate\(([-\d.]+),([-\d.]+)\)">(?:<title>[^<]*<\/title>)?<circle r="([\d.]+)"[^>]*\/><text class="kglabel" y="[-\d.]+">([^<]*)<\/text><\/g>/g;
    while ((m = nodeRe.exec(html)) !== null) {
      nodes++;
      boxes.push({ what: "the node " + unesc(m[4]), b: kgNodeBody(Number(m[1]), Number(m[2]), Number(m[3]), unesc(m[4])) });
    }
    const hullRe = /<g class="kghull"><rect x="([-\d.]+)" y="([-\d.]+)" width="([\d.]+)" height="([\d.]+)"[^>]*\/><text class="kghull-l" x="([-\d.]+)" y="([-\d.]+)">([^<]*)<\/text><\/g>/g;
    while ((m = hullRe.exec(html)) !== null) {
      hulls++;
      const x = Number(m[1]), y = Number(m[2]), lx = Number(m[5]), ly = Number(m[6]), text = unesc(m[7]);
      boxes.push({ what: "the hull " + text, b: { x0: Math.min(x, lx), x1: Math.max(x + Number(m[3]), lx + text.length * HULL_CHAR_W),
        y0: Math.min(y, ly - 10), y1: Math.max(y + Number(m[4]), ly + 3) } });
    }
    return { W: W, H: H, boxes: boxes, nodes: nodes, hulls: hulls };
  }
  function view(what){
    const t = el("kgpanel")._zoom.getAttribute("transform");
    const m = t && t.match(/^translate\(([^,]+),([^)]+)\) scale\(([^)]+)\)$/);
    if (!m) throw new Error(what + " applied no opening transform: " + t);
    return { x: Number(m[1]), y: Number(m[2]), k: Number(m[3]) };
  }
  function opensFitted(what, body, hulls){
    const d = drawn(what);
    if (d.nodes !== body.clusters.length || d.hulls !== hulls)
      throw new Error(what + " drew " + d.nodes + " clusters and " + d.hulls + " hulls, not " + body.clusters.length + " and " + hulls);
    const t = view(what);
    if (!(t.k > 0 && t.k <= 1)) throw new Error("REGRESSION: " + what + " opens at scale " + t.k + ", never past 1");
    const out = d.boxes.filter(function(o){ const b = o.b;
      return b.x0 * t.k + t.x < -TOL || b.x1 * t.k + t.x > d.W + TOL || b.y0 * t.k + t.y < -TOL || b.y1 * t.k + t.y > d.H + TOL; });
    if (out.length)
      throw new Error("REGRESSION: " + what + " opens with " + out.length + " of " + d.boxes.length + " drawn boxes outside its "
        + d.W + "x" + d.H + " panel at " + JSON.stringify(t) + "; the first is " + out[0].what + " " + JSON.stringify(out[0].b));
    return t;
  }
  function pick(lens){
    const handle = { dataset: { lens: lens } };
    const target = { closest: function(sel){ return sel === "[data-lens]" ? handle : null; } };
    (el("kgpanel")._listeners.click || []).forEach(function(fn){ fn({ target: target, preventDefault: function(){} }); });
  }
  function wheel(deltaY){
    const svg = el("kgpanel")._svg, r = svg.getBoundingClientRect(), on = svg._listeners.wheel || [];
    if (on.length !== 1) throw new Error("the drawn graph must carry one wheel listener, not " + on.length);
    on[0]({ deltaY: deltaY, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, preventDefault: function(){} });
    return view("the wheel").k;
  }
  const near = function(a, b){ return Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(b)); };
  // The panel this case draws in, set before the page's own load draws (its fetch resolves after
  // this line): [viewport width, panel clientWidth, clientHeight, toolbar height].
  const [vw, cw, ch, bar] = __PANEL__;
  __geom.width = cw; __geom.height = ch;
  __geom.toolbar = { overview: bar, reproject: bar, drill: bar };
  const at = vw + "px: ";
  const scales = [];
  await __flush();
  const opening = opensFitted(at + "the files overview on load", __OVERVIEWS.files, dirs(__OVERVIEWS.files));
  scales.push("files " + opening.k.toFixed(3));
  for (const lens of ["code", "concepts"]) {
    pick(lens);
    await __flush();
    if (kgMode !== "overview" || kgLens !== lens) throw new Error(at + "the " + lens + " tab must open the " + lens + " overview");
    const t = opensFitted(at + "the " + lens + " overview after a lens switch", __OVERVIEWS[lens], 0);
    scales.push(lens + " " + t.k.toFixed(3));
    const zin = wheel(-1);
    if (!near(zin, t.k * 1.1)) throw new Error("REGRESSION: " + at + "zooming in on the " + lens + " overview must continue from its opening scale " + t.k + ", got " + zin);
    const back = wheel(1);
    if (!near(back, t.k)) throw new Error(at + "zooming back out must return to the opening scale " + t.k + ", got " + back);
    const outer = wheel(1);
    if (outer > t.k + 1e-12 || outer < Math.min(0.2, t.k) - 1e-12)
      throw new Error("REGRESSION: " + at + "zooming out past the opening scale " + t.k + " must not jump, got " + outer);
  }
  wheel(-1);
  pick("files");
  await __flush();
  const again = opensFitted(at + "the files overview after a lens switch from a zoomed view", __OVERVIEWS.files, dirs(__OVERVIEWS.files));
  if (!near(again.k, opening.k) || !near(again.x, opening.x) || !near(again.y, opening.y))
    throw new Error("REGRESSION: " + at + "a redraw must open fitted again, not where the last zoom left it: " + JSON.stringify(again));
  reprojectSubject(__SUBJECT);
  await __flush();
  if (kgMode !== "reproject") throw new Error(at + "the subject must re-project");
  opensFitted(at + "the files re-projection", __REPROJECTIONS.files, dirs(__REPROJECTIONS.files));
  for (const lens of ["code", "concepts"]) {
    pick(lens);
    await __flush();
    if (kgMode !== "reproject" || kgLens !== lens) throw new Error(at + "the " + lens + " tab must re-project the subject");
    opensFitted(at + "the " + lens + " re-projection", __REPROJECTIONS[lens], 0);
  }
  console.log(at + "opening scales " + scales.join(", "));
  console.log("OK every-overview-opens-fitted-to-its-panel");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// Driver, in the panel `__PANEL__` names: the files overview the page loads, the subject's
/// re-projection, a drilled cluster, a call walk and the overview again each draw a drawing that fills
/// the room the panel leaves below that view's own toolbar and never runs past it, each after a view
/// whose toolbar stood at another height; below 200px of room the drawing keeps its 200px floor.
const PANEL_HEIGHT_DRIVER: &str = r##"
;(async function(){
  // The panel this case draws in, set before the page's own load draws (its fetch resolves after
  // this line): [clientWidth, clientHeight, the height each view's toolbar takes above the drawing,
  // its 6px margin included].
  const [cw, ch, bar] = __PANEL__;
  __geom.width = cw; __geom.height = ch; __geom.toolbar = bar;
  const at = cw + "x" + ch + " " + JSON.stringify(bar) + ": ";
  // The drawing fills the room the panel leaves below the toolbar it is drawn under, and never runs
  // past it: the content (padding, toolbar, drawing, padding) stays inside the panel. Below 200px of
  // room the drawing keeps its 200px floor and the panel scrolls.
  function fillsItsRoom(what, mode){
    if (kgMode !== mode) throw new Error(at + "the " + what + " must draw, not leave kgMode " + kgMode);
    const d = __svgBox(at + "the " + what);
    const room = ch - __geom.padTop - bar[mode] - __geom.padBottom;
    const content = __geom.padTop + bar[mode] + d.height + __geom.padBottom;
    if (room >= 200 && !(d.height <= room && d.height > room - 1))
      throw new Error("REGRESSION: " + at + "the " + what + " draws " + d.height + "px tall below its " + bar[mode]
        + "px toolbar where the panel leaves " + room + "px, so its " + content + "px of content "
        + (content > ch ? "runs " + (content - ch) + "px past" : "falls " + (ch - content) + "px short of") + " the " + ch + "px panel");
    if (room < 200 && d.height !== 200)
      throw new Error("REGRESSION: " + at + "the " + what + " draws " + d.height + "px tall where the panel leaves "
        + room + "px, below the drawing's 200px floor, which it must keep");
  }
  await __flush();
  fillsItsRoom("files overview on load", "overview");
  // Each view in turn, each drawn after one whose toolbar stood at another height.
  reprojectSubject(__SUBJECT);
  await __flush();
  fillsItsRoom("subject's re-projection", "reproject");
  drillCluster(__CLUSTER);
  await __flush();
  fillsItsRoom("drilled cluster", "drill");
  seedCalls(__SUBJECT, "down");
  await __flush();
  fillsItsRoom("call walk", "calls");
  loadKgOverview();
  await __flush();
  fillsItsRoom("files overview after the call walk", "overview");
  console.log("OK every-drawing-fits-below-its-toolbar");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// Runs `driver` in the panel `panel` describes, and claims it ends in `ok`.
fn drive(driver: &str, panel: &str, ok: &str, claim: &str) {
    node_harness_claims(
        &vm_harness(
            &[PANEL_SHIM, OVERVIEWS_FETCH, PAGE_READERS],
            &driver.replace("__PANEL__", panel),
            HARNESS_FILE,
        ),
        ok,
        claim,
    );
}

/// The opening view of every overview and re-projection, in the panel a `[viewport width, panel
/// clientWidth, clientHeight, toolbar height]` row describes.
fn opens_fitted(panel: &str) {
    drive(
        OPENING_DRIVER,
        panel,
        "OK every-overview-opens-fitted-to-its-panel",
        "every overview must open fitted to its panel",
    );
}

/// Every view's drawing height, in the panel a `[clientWidth, clientHeight, {view: the height its
/// toolbar takes, 6px margin included}]` row describes.
fn fits_below_its_toolbar(panel: &str) {
    drive(
        PANEL_HEIGHT_DRIVER,
        panel,
        "OK every-drawing-fits-below-its-toolbar",
        "every drawing must fill the room its panel leaves below its toolbar, and no more",
    );
}

rigger::test_cases! {
    /// A 1400px viewport: the panel fills the room the page leaves it, 1358 by 294px.
    the_overview_opens_fitted_at_1400px: opens_fitted("[1400, 1358, 294, 26]");
    /// A 1000px viewport: the narrow layout's 46vh panel, 958 by 414px.
    the_overview_opens_fitted_at_1000px: opens_fitted("[1000, 958, 414, 26]");
    /// An 800px viewport, where the toolbar wraps to two lines.
    the_overview_opens_fitted_at_800px: opens_fitted("[800, 758, 414, 44]");
    /// A phone's 390px viewport, where the toolbar wraps to three lines.
    the_overview_opens_fitted_at_390px: opens_fitted("[390, 348, 414, 62]");
    /// 1400px: every toolbar on one line.
    the_drawing_fits_below_a_one_line_toolbar_at_1400px:
        fits_below_its_toolbar(r#"[1358, 294, {"overview": 24, "reproject": 24, "drill": 24, "calls": 24}]"#);
    /// 1400px: one line, made 2px taller by the grain control the derived lenses show.
    the_drawing_fits_below_the_grain_controls_line_at_1400px:
        fits_below_its_toolbar(r#"[1358, 294, {"overview": 26, "reproject": 26, "drill": 26, "calls": 26}]"#);
    /// 1000px: the re-projection's and the call walk's longer toolbars wrap to two lines.
    the_drawing_fits_below_a_wrapped_toolbar_at_1000px:
        fits_below_its_toolbar(r#"[958, 414, {"overview": 26, "reproject": 42, "drill": 26, "calls": 44}]"#);
    /// 800px: two and three lines.
    the_drawing_fits_below_a_wrapped_toolbar_at_800px:
        fits_below_its_toolbar(r#"[758, 414, {"overview": 44, "reproject": 62, "drill": 42, "calls": 62}]"#);
    /// A phone's 390px: three and four lines.
    the_drawing_fits_below_a_wrapped_toolbar_at_390px:
        fits_below_its_toolbar(r#"[348, 414, {"overview": 62, "reproject": 80, "drill": 60, "calls": 80}]"#);
    /// A toolbar whose wrapped height falls between whole pixels.
    the_drawing_fits_below_a_fractional_toolbar:
        fits_below_its_toolbar(r#"[348, 414, {"overview": 42.5, "reproject": 60.5, "drill": 42.5, "calls": 60.5}]"#);
    /// A phone held in a 640px-tall viewport: the 46vh panel is 294px, and below a four-line
    /// toolbar it leaves 194px, under the drawing's 200px floor.
    the_drawing_keeps_its_floor_below_a_tall_toolbar_in_a_short_panel:
        fits_below_its_toolbar(r#"[348, 294, {"overview": 62, "reproject": 80, "drill": 60, "calls": 80}]"#);
}
