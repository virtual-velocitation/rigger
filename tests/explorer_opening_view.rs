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
//! And when the panel's size changes after a view is drawn (a window resize, or the instances strip
//! above it growing after the first draw), the view redraws at the new size with no new request:
//! re-fitted by the opening rule when the user has not panned or zoomed it, and when they have,
//! keeping its transform and its marks while only its drawing surface takes the new size. One size
//! observer drives it, redrawing at most once a frame and only when the measured size changed.
//!
//! Every drawing is made at its surface's own size (the svg's box, which takes no border and no
//! padding), so it is seen at a scale of 1 on both axes: an opened view shows at its fit, and a moved
//! view's nodes keep their places on screen through a resize. That size is the one the surface has
//! with the drawing in place, beside the scrollbar the panel shows when what it holds runs past its
//! height (a drawing kept at its 200px floor, notes below the drawing).
//!
//! And a redraw keeps everything the panel was showing of the view it holds (the call view's
//! frontier choice list, an opened rationale disclosure, a grain being typed), shown and answering,
//! whether the panel's size changes with the window or by the scrollbar what it shows makes it take.
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
use vm_harness::{vm_harness, EVENT_PHASES};

/// The DOM shim: the panel (`#kgpanel`) keeps the client box, padding and per-view toolbar height
/// `__geom` holds, and each markup it is given gets a fresh `svg` (its height and viewBox read from
/// that markup and kept as attributes the page may set again, its listeners recorded, its slot boxed
/// below that markup's toolbar and frontier list as tall as its height) and a fresh `#kgzoom`
/// (recording its transform), as a browser replaces them; every other element is a bare stand-in.
/// The panel shows a 12px scrollbar of its own, narrowing its client box and the svg's slot, while
/// what it holds - its padding, the toolbar, the frontier list, the svg at the height it has now and
/// any markup below the svg, 40px of it - runs past its height, as a classic scrollbar does. The call
/// view's frontier slot (`#kgfrontier`) is a view onto the panel's markup, written in place there
/// with the drawing left as it is, and takes an 18px line for its head and each candidate it lists;
/// `_gen` counts the markups the panel is given, so a driver can tell what the page wrote from what
/// it drew again. The grain box a markup writes (`[data-lens-res]`) is a text box as a browser makes
/// it: its value the markup's, its caret, `focus()` making it `document.activeElement`; committed (by
/// Enter, by leaving it, or by a markup replacing it while it holds the focus) it sends `change` when
/// its text differs from the text it held when it took the focus or last sent `change`, and left it
/// gives up the focus and sends `focusout`. A markup that replaces the box holding the focus leaves
/// it while it is still in the panel, `change` then `focusout`, as Chromium does (Firefox sends none). Its elements record their listeners, and the drivers deliver
/// events to them, by [`EVENT_PHASES`]. `window` records its listeners, a size observer is recorded
/// with what it watches for the driver to notify, and an animation frame waits in `__frames` until
/// the driver runs it.
/// Free of backticks and `${` so it embeds in the harness's `String.raw` template.
const PANEL_SHIM: &str = r##"
const __els = {};
const __observers = [];
function ResizeObserver(cb){ this._cb = cb; this._targets = []; __observers.push(this); }
ResizeObserver.prototype.observe = function(t){ this._targets.push(t); };
const __frames = [];
const requestAnimationFrame = function(f){ __frames.push(f); return __frames.length; };
const __winListeners = {};
// The panel's client box with no scrollbar of its own, its padding, the height each view's toolbar
// takes, the width of the scrollbar it shows while what it holds overflows, and the height of the
// markup a view puts below its drawing.
const __geom = { width: 800, height: 300, padTop: 10, padBottom: 10, padSide: 12,
  toolbar: { overview: 24, reproject: 24, drill: 24, calls: 24 }, scrollbar: 12, below: 40 };
const __classList = { add: function(){}, remove: function(){}, toggle: function(){}, contains: function(){ return false; } };
function __Stub(){ this._attrs = {}; this._listeners = {}; this.dataset = {}; this.classList = __classList; }
__Stub.prototype.setAttribute = function(k, v){ this._attrs[k] = String(v); };
__Stub.prototype.getAttribute = function(k){ return k in this._attrs ? this._attrs[k] : null; };
__Stub.prototype.addEventListener = function(t, f, o){ __listen(this, t, f, o); };
__Stub.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: 0, height: 0 }; };
function __viewOf(html){
  if (html.indexOf("data-kgclear") !== -1) return "reproject";
  if (html.indexOf("data-calls-down") !== -1) return "calls";
  if (html.indexOf("data-kgback") !== -1) return "drill";
  return "overview";
}
// The call view's frontier slot in a panel's markup: the span of its content, from its opening tag to
// the </div> that closes it, or null when the markup holds no slot.
const __SLOT_OPEN = '<div id="kgfrontier" class="kgfrontier">';
function __slotSpan(html){
  const at = html.indexOf(__SLOT_OPEN);
  if (at === -1) return null;
  const from = at + __SLOT_OPEN.length, re = /<div\b|<\/div>/g;
  re.lastIndex = from;
  let depth = 1, m;
  while ((m = re.exec(html)) !== null) {
    depth += m[0] === "</div>" ? -1 : 1;
    if (depth === 0) return [from, m.index];
  }
  return null;
}
// The height the frontier slot takes: none while empty, else its 6px padding above and below and an
// 18px line for its head and for each candidate it lists.
function __listHeight(html){
  const span = __slotSpan(html);
  const n = span ? (html.slice(span[0], span[1]).match(/class="kgfr-cand"/g) || []).length : 0;
  return n ? 12 + 18 * (1 + n) : 0;
}
function __slotTop(html){ return __geom.padTop + __geom.toolbar[__viewOf(html)] + __listHeight(html); }
// The width of the scrollbar the panel shows now: what it holds - its padding, the toolbar, the svg at
// the height it has now and the markup below the svg - runs past its height, or it shows none.
function __scrollbar(panel){
  const html = panel._html, end = html.lastIndexOf("</svg>");
  if (html.indexOf('<svg class="kgsvg"') === -1) return 0;
  const below = html.slice(end + "</svg>".length).trim() === "" ? 0 : __geom.below;
  const holds = __slotTop(html) + Number(panel._svg.getAttribute("height") || 0) + below + __geom.padBottom;
  return holds > __geom.height ? __geom.scrollbar : 0;
}
function __unesc(s){ return s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"')
  .replace(/&#39;/g, "'").replace(/&amp;/g, "&"); }
// The grain box the panel's markup writes: its value and default the value the markup wrote, its
// caret at its end, connected while it is the box the panel hands out. It keeps the text it held when
// it took the focus or last sent change (_from): committed, it sends change when its text differs
// from that; left, it commits, gives up the focus and sends focusout.
function __Box(panel){
  const tag = panel._html.match(/<input [^>]*data-lens-res[^>]*>/), v = tag && tag[0].match(/ value="([^"]*)"/);
  this._panel = panel;
  this.value = this.defaultValue = this._from = v ? __unesc(v[1]) : "";
  this.selectionStart = this.selectionEnd = this.value.length;
}
__Box.prototype.closest = function(sel){ return sel === "[data-lens-res]" ? this : null; };
__Box.prototype.setSelectionRange = function(a, b){ this.selectionStart = a; this.selectionEnd = b; };
__Box.prototype.focus = function(){
  document.activeElement = this;
  this._from = this.value;
  __reach(this._panel, { type: "focusin", target: this });
};
__Box.prototype._commit = function(){
  if (this.value === this._from) return;
  this._from = this.value;
  __reach(this._panel, { type: "change", target: this });
};
__Box.prototype._leave = function(){
  this._commit();
  document.activeElement = null;
  __reach(this._panel, { type: "focusout", target: this });
};
Object.defineProperty(__Box.prototype, "isConnected", { get(){ return this._panel._box === this; } });
function __El(id){ this.id = id; this._html = ""; this._text = ""; this._listeners = {}; this.dataset = {};
  this.hidden = false; this.className = ""; this.checked = false; this.classList = __classList;
  this.clientTop = 0; this.clientLeft = 0; this._zoom = new __Stub(); this._svg = new __Stub(); this._gen = 0;
  this._box = null; }
Object.defineProperty(__El.prototype, "clientWidth", { get(){
  return __geom.width - (this.id === "kgpanel" ? __scrollbar(this) : 0); } });
Object.defineProperty(__El.prototype, "clientHeight", { get(){ return __geom.height; } });
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){
  const html = String(v), panel = this, held = this._box;
  // The box holding the focus is left while it is still in the panel, then replaced.
  if (held && document.activeElement === held) held._leave();
  this._html = html;
  this._gen++;
  this._zoom = new __Stub();
  const svg = this._svg = new __Stub();
  const tag = html.match(/<svg class="kgsvg"[^>]*>/);
  const high = tag && tag[0].match(/ height="([^"]*)"/), box = tag && tag[0].match(/ viewBox="([^"]*)"/);
  if (high) svg.setAttribute("height", high[1]);
  if (box) svg.setAttribute("viewBox", box[1]);
  svg.getBoundingClientRect = function(){
    return { left: __geom.padSide, top: __slotTop(panel._html), width: __geom.width - 2 * __geom.padSide - __scrollbar(panel),
      height: Number(svg.getAttribute("height") || 0) };
  };
  this._box = null;
} });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.getBoundingClientRect = function(){ return { left: 0, top: 0, width: __geom.width, height: __geom.height }; };
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.querySelector = function(sel){
  if (this.id !== "kgpanel") return new __Stub();
  if (sel === "#kgzoom") return this._html.indexOf('id="kgzoom"') === -1 ? null : this._zoom;
  if (sel === "svg" || sel === ".kgsvg" || sel === "svg.kgsvg") return this._html.indexOf("<svg") === -1 ? null : this._svg;
  if (sel === "[data-lens-res]") return this._html.indexOf("data-lens-res") === -1 ? null : this._box || (this._box = new __Box(this));
  return new __Stub();
};
__El.prototype.addEventListener = function(t, f, o){ __listen(this, t, f, o); };
// The frontier slot as an element: what the panel's markup holds in the slot, written in place there
// with the drawing left as it is, as a browser does.
function __Slot(panel){ this.id = "kgfrontier"; this._panel = panel; }
Object.defineProperty(__Slot.prototype, "innerHTML", { get(){
  const html = this._panel._html, span = __slotSpan(html);
  return span ? html.slice(span[0], span[1]) : "";
}, set(v){
  const html = this._panel._html, span = __slotSpan(html);
  this._panel._html = html.slice(0, span[0]) + String(v) + html.slice(span[1]);
} });
function __element(id){ return __els[id] || (__els[id] = new __El(id)); }
const document = { activeElement: null, getElementById: function(id){
  if (id !== "kgfrontier") return __element(id);
  const panel = __element("kgpanel");
  return __slotSpan(panel._html) ? new __Slot(panel) : null;
} };
const window = { addEventListener: function(t, f){ (__winListeners[t] = __winListeners[t] || []).push(f); } };
const setTimeout = function(){ return 0; };
const getComputedStyle = function(){ return { paddingTop: __geom.padTop + "px", paddingBottom: __geom.padBottom + "px" }; };
"##;

/// A `fetch` serving deterministic overviews as dense as a real project's - the files lens's 158
/// file clusters in 32 directories (one at the repo root), the code lens's 200 communities, the
/// concepts lens's 98 concepts - a subject's re-projection under each lens (40 files, a single
/// community, 12 concepts), a drilled cluster's 12 members and a 4-layer call walk; a card or
/// rationale read finds nothing. `__fetches` counts every request the page makes.
const OVERVIEWS_FETCH: &str = r##"
let __fetches = 0;
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
  __fetches++;
  if (String(url).indexOf("/api/graph") === -1) return Promise.reject(new Error("no network for " + url));
  return Promise.resolve({ headers: { get: function(){ return null; } }, json: function(){ return Promise.resolve(__graph(url)); } });
};
"##;

/// What every driver reads back from the page and does to it: settling its fetches; the drawing the
/// panel holds, refusing a message in its place and a drawing box that is not a finite, positive
/// size; the transform on that drawing and a wheel notch over it; and the drawing filling the room
/// the panel leaves below its toolbar.
const PAGE_READERS: &str = r##"
// Let every pending promise the page chained settle (its fetches resolve on the microtask queue).
function __flush(){ return (async function(){ for (let k = 0; k < 80; k++) await Promise.resolve(); })(); }
// The drawing the panel holds: its markup, and its svg's viewBox size and height as the svg carries
// them now, refusing a panel that holds no drawing (a message such as "unavailable" in its place) and
// a viewBox that is not a finite, positive size.
function __svgBox(what){
  const panel = el("kgpanel"), html = panel._html;
  if (html.indexOf('<svg class="kgsvg"') === -1) throw new Error(what + " drew no graph: " + html.slice(0, 300));
  const svg = panel._svg, high = svg.getAttribute("height"), box = svg.getAttribute("viewBox");
  const vb = box && box.match(/^0 0 (\S+) (\S+)$/);
  const height = Number(high), W = vb ? Number(vb[1]) : NaN, H = vb ? Number(vb[2]) : NaN;
  if (!(isFinite(W) && isFinite(H) && W > 0 && H > 0 && height === H))
    throw new Error(what + " drew a degenerate drawing box, height " + high + " and viewBox " + box);
  return { html: html, W: W, H: H, height: height };
}
// The marks the drawing holds: the markup of its #kgzoom group.
function __marks(){
  const html = el("kgpanel")._html, at = html.indexOf('<g id="kgzoom">');
  return at === -1 ? "" : html.slice(at, html.lastIndexOf("</svg>"));
}
// The transform the page applied to the drawing's #kgzoom group.
function __view(what){
  const t = el("kgpanel")._zoom.getAttribute("transform");
  const m = t && t.match(/^translate\(([^,]+),([^)]+)\) scale\(([^)]+)\)$/);
  if (!m) throw new Error(what + " applied no opening transform: " + t);
  return { x: Number(m[1]), y: Number(m[2]), k: Number(m[3]) };
}
// The scale the drawing is seen at on each axis: its svg's box, which is the drawing's surface (the
// stylesheet gives the svg no border and no padding), over the viewBox the drawing was made at.
function __seenAt(what){
  const d = __svgBox(what), r = el("kgpanel")._svg.getBoundingClientRect();
  return { x: r.width / d.W, y: r.height / d.H };
}
// The drawing, made at its surface's own size - as wide as the panel's content box, its client width
// less its padding on each side - so seen at a scale of 1 on both axes. Answers the drawing.
function __atItsSurface(what){
  const d = __svgBox(what), seen = __seenAt(what);
  if (seen.x !== 1 || seen.y !== 1)
    throw new Error("REGRESSION: " + what + " is drawn at " + d.W + "x" + d.H + " and seen at " + JSON.stringify(seen)
      + " on its surface, not at 1");
  return d;
}
// Where each node the drawing holds shows on screen, relative to the top-left corner of its surface:
// the viewBox shows in the svg's box at the larger scale that fits it whole, centred on the axis it
// leaves room on (preserveAspectRatio xMidYMid meet), under the transform on #kgzoom.
function __onSurface(what){
  const d = __svgBox(what), t = __view(what), seen = __seenAt(what), s = Math.min(seen.x, seen.y);
  const r = el("kgpanel")._svg.getBoundingClientRect(), ox = (r.width - d.W * s) / 2, oy = (r.height - d.H * s) / 2;
  const re = /<g class="kgn [^"]*"[^>]* transform="translate\(([-\d.]+),([-\d.]+)\)"/g, marks = __marks(), at = [];
  let m;
  while ((m = re.exec(marks)) !== null) at.push([ox + s * (t.x + t.k * Number(m[1])), oy + s * (t.y + t.k * Number(m[2]))]);
  if (!at.length) throw new Error(what + " holds no node to place on screen");
  return at;
}
// One wheel notch over the middle of the drawing (a negative deltaY zooms in); answers the scale.
function __wheel(deltaY){
  const svg = el("kgpanel")._svg, r = svg.getBoundingClientRect(), on = svg._listeners.wheel || [];
  if (on.length !== 1) throw new Error("the drawn graph must carry one wheel listener, not " + on.length);
  on[0]({ deltaY: deltaY, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, preventDefault: function(){} });
  return __view("the wheel").k;
}
// The drawing of the mode view fills the room the panel leaves below the toolbar it is drawn under,
// and never runs past it: the content (padding, toolbar, drawing, padding) stays inside the panel.
// Below 200px of room the drawing keeps its 200px floor and the panel scrolls. Either way it is made
// at its surface's own size, beside any scrollbar the panel then shows. Answers the drawing.
function __fillsItsRoom(at, what, mode){
  if (kgMode !== mode) throw new Error(at + "the " + what + " must draw, not leave kgMode " + kgMode);
  const d = __svgBox(at + "the " + what);
  const ch = __geom.height, bar = __geom.toolbar[mode];
  const room = ch - __geom.padTop - bar - __geom.padBottom;
  const content = __geom.padTop + bar + d.height + __geom.padBottom;
  if (room >= 200 && !(d.height <= room && d.height > room - 1))
    throw new Error("REGRESSION: " + at + "the " + what + " draws " + d.height + "px tall below its " + bar
      + "px toolbar where the panel leaves " + room + "px, so its " + content + "px of content "
      + (content > ch ? "runs " + (content - ch) + "px past" : "falls " + (ch - content) + "px short of") + " the " + ch + "px panel");
  if (room < 200 && d.height !== 200)
    throw new Error("REGRESSION: " + at + "the " + what + " draws " + d.height + "px tall where the panel leaves "
      + room + "px, below the drawing's 200px floor, which it must keep");
  return __atItsSurface(at + "the " + what);
}
"##;

/// The node `vm` program name this suite's harness runs under.
const HARNESS_FILE: &str = "dash-opening-view-harness.js";

/// Driver, in the panel `__PANEL__` names: the files overview the page loads, the code and concepts
/// overviews a lens tab opens, the files overview a lens tab redraws after a zoom, and the subject's
/// re-projection under each lens are each drawn at their surface's own size, so seen at a scale of 1
/// on both axes, and open with every drawn box inside the panel at a scale of at most 1; on each
/// overview the wheel zooms in and back from the opening scale, and out no further
/// than it or 0.2, whichever is less; and the redrawn overview opens where the first one did.
const OPENING_DRIVER: &str = r##"
;(async function(){
  const HULL_CHAR_W = 10 * (0.6 + 0.08);
  const TOL = 0.5;
  const dirs = function(body){ return new Set(body.clusters.map(function(c){
    const i = c.key.lastIndexOf("/"); return i === -1 ? "(root)" : c.key.slice(0, i); })).size; };
  function drawn(what){
    const svg = __svgBox(what), html = svg.html, W = svg.W, H = svg.H;
    const boxes = [];
    let m, nodes = 0, hulls = 0;
    const nodeRe = /<g class="kgn [^"]*"[^>]* transform="translate\(([-\d.]+),([-\d.]+)\)">(?:<title>[^<]*<\/title>)?<circle r="([\d.]+)"[^>]*\/><text class="kglabel" y="[-\d.]+">([^<]*)<\/text><\/g>/g;
    while ((m = nodeRe.exec(html)) !== null) {
      nodes++;
      boxes.push({ what: "the node " + __unesc(m[4]), b: kgNodeBody(Number(m[1]), Number(m[2]), Number(m[3]), __unesc(m[4])) });
    }
    const hullRe = /<g class="kghull"><rect x="([-\d.]+)" y="([-\d.]+)" width="([\d.]+)" height="([\d.]+)"[^>]*\/><text class="kghull-l" x="([-\d.]+)" y="([-\d.]+)">([^<]*)<\/text><\/g>/g;
    while ((m = hullRe.exec(html)) !== null) {
      hulls++;
      const x = Number(m[1]), y = Number(m[2]), lx = Number(m[5]), ly = Number(m[6]), text = __unesc(m[7]);
      boxes.push({ what: "the hull " + text, b: { x0: Math.min(x, lx), x1: Math.max(x + Number(m[3]), lx + text.length * HULL_CHAR_W),
        y0: Math.min(y, ly - 10), y1: Math.max(y + Number(m[4]), ly + 3) } });
    }
    return { W: W, H: H, boxes: boxes, nodes: nodes, hulls: hulls };
  }
  function opensFitted(what, body, hulls){
    const d = drawn(what);
    if (d.nodes !== body.clusters.length || d.hulls !== hulls)
      throw new Error(what + " drew " + d.nodes + " clusters and " + d.hulls + " hulls, not " + body.clusters.length + " and " + hulls);
    __atItsSurface(what);
    const t = __view(what);
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
    const zin = __wheel(-1);
    if (!near(zin, t.k * 1.1)) throw new Error("REGRESSION: " + at + "zooming in on the " + lens + " overview must continue from its opening scale " + t.k + ", got " + zin);
    const back = __wheel(1);
    if (!near(back, t.k)) throw new Error(at + "zooming back out must return to the opening scale " + t.k + ", got " + back);
    const outer = __wheel(1);
    if (outer > t.k + 1e-12 || outer < Math.min(0.2, t.k) - 1e-12)
      throw new Error("REGRESSION: " + at + "zooming out past the opening scale " + t.k + " must not jump, got " + outer);
  }
  __wheel(-1);
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
/// re-projection, a drilled cluster, the same drill with the rationale overlay's notes below it, a
/// call walk and the overview again each draw a drawing that fills the room the panel leaves below
/// that view's own toolbar and never runs past it, each after a view whose toolbar stood at another
/// height; below 200px of room the drawing keeps its 200px floor. Each is made at its surface's own
/// size, beside the scrollbar the panel shows when the floor or the notes below run past its height.
const PANEL_HEIGHT_DRIVER: &str = r##"
;(async function(){
  // The panel this case draws in, set before the page's own load draws (its fetch resolves after
  // this line): [clientWidth, clientHeight, the height each view's toolbar takes above the drawing,
  // its 6px margin included].
  const [cw, ch, bar] = __PANEL__;
  __geom.width = cw; __geom.height = ch; __geom.toolbar = bar;
  const at = cw + "x" + ch + " " + JSON.stringify(bar) + ": ";
  await __flush();
  __fillsItsRoom(at, "files overview on load", "overview");
  // Each view in turn, each drawn after one whose toolbar stood at another height.
  reprojectSubject(__SUBJECT);
  await __flush();
  __fillsItsRoom(at, "subject's re-projection", "reproject");
  drillCluster(__CLUSTER);
  await __flush();
  __fillsItsRoom(at, "drilled cluster", "drill");
  // The rationale overlay on, with a reason recorded for the drill's first member, puts its notes
  // below the drawing.
  kgOverlay = true;
  kgRationale = {};
  kgRationale[kgViewData.nodes[0].id] = [{ kind: "decision", summary: "a recorded reason" }];
  rerenderCurrentView();
  if (el("kgpanel")._html.indexOf('<div class="kgwhy-notes">') === -1)
    throw new Error(at + "the overlay must put its notes below the drill");
  __fillsItsRoom(at, "drilled cluster with the rationale overlay's notes below it", "drill");
  kgOverlay = false;
  kgRationale = {};
  seedCalls(__SUBJECT, "down");
  await __flush();
  __fillsItsRoom(at, "call walk", "calls");
  loadKgOverview();
  await __flush();
  __fillsItsRoom(at, "files overview after the call walk", "overview");
  console.log("OK every-drawing-fits-below-its-toolbar");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// What the resize drivers do to the page: size the panel, resize it (the browser notifying each
/// observer watching it), run the animation frames the page queued, and press on the drawing; and
/// read back the overview the panel holds at the size it now has.
const RESIZE_READERS: &str = r##"
// The panel at a 1400px and at an 800px viewport: [clientWidth, clientHeight, toolbar height].
const __WIDE = [1358, 294, 24], __NARROW = [758, 414, 44];
// The panel at a phone's 390px viewport once the rail and the card stacked above it push the page
// past the viewport: the page's 12px scrollbar leaves main 378px, the panel 336px inside main's 20px
// padding and the section's 1px border on each side (its surface 312px inside its own 12px padding),
// and the call walk's toolbar wraps to four lines.
const __PHONE = [336, 414, 80];
// The panel takes the client box and toolbar height a [clientWidth, clientHeight, toolbar] row
// gives; __resize then notifies each observer watching the panel, as a browser does.
function __size(p){
  __geom.width = p[0]; __geom.height = p[1];
  __geom.toolbar = { overview: p[2], reproject: p[2], drill: p[2], calls: p[2] };
}
function __notify(){
  const panel = el("kgpanel");
  __observers.forEach(function(o){ if (o._targets.indexOf(panel) !== -1) o._cb([{ target: panel }], o); });
}
function __resize(p){ __size(p); __notify(); }
// Run the animation frames the page queued; answers how many ran.
function __frame(){ const run = __frames.splice(0); run.forEach(function(f){ f(0); }); return run.length; }
// Press on the drawing, move the pointer by (dx, dy) when given, and release: a drag, or a click.
function __press(dx, dy){
  const down = el("kgpanel")._svg._listeners.mousedown || [];
  if (down.length !== 1) throw new Error("the drawn graph must carry one mousedown listener, not " + down.length);
  down[0]({ clientX: 100, clientY: 100 });
  if (dx !== undefined) (__winListeners.mousemove || []).forEach(function(f){ f({ clientX: 100 + dx, clientY: 100 + dy }); });
  (__winListeners.mouseup || []).forEach(function(f){ f({}); });
}
// The overview the panel holds, drawn at the size the panel now has: filling the room below its
// toolbar, at its surface's own size. Answers its markup and its transform.
function __overviewAt(at, what){
  const d = __fillsItsRoom(at, what, "overview");
  return { html: d.html, t: JSON.stringify(__view(at + "the " + what)) };
}
"##;

/// Driver: at a 1400px viewport the panel is 300px tall when the overview first draws, and the
/// instances strip above it then grows, leaving it 294px. The overview redraws to the room the
/// 294px panel leaves, with no new request, as the drawing and opening view a fresh draw there gives.
const FIRST_DRAW_RESIZE_DRIVER: &str = r##"
;(async function(){
  const at = "1400px, the strip above growing after the first draw: ";
  __size([1358, 300, 24]);
  await __flush();
  __overviewAt(at, "files overview on load");
  const fetches = __fetches;
  __resize(__WIDE);
  __frame();
  const resized = __overviewAt(at, "files overview after the strip grew");
  if (__fetches !== fetches)
    throw new Error("REGRESSION: " + at + "the redraw must make no request, it made " + (__fetches - fetches));
  loadKgOverview();
  await __flush();
  const fresh = __overviewAt(at, "files overview drawn fresh in the 294px panel");
  if (resized.html !== fresh.html || resized.t !== fresh.t)
    throw new Error("REGRESSION: " + at + "the untouched overview must re-fit as a fresh draw at its size does, "
      + resized.t + " against " + fresh.t);
  console.log("OK the-first-draw-refits-to-the-panel-the-strip-leaves");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// Driver: an overview no one has panned or zoomed (a click on it does neither) redraws on each
/// resize from a 1400px viewport to 800px and back, with no new request: at 800px as the drawing and
/// opening view a fresh draw there gives, and back at 1400px as the one it first opened as.
const UNTOUCHED_RESIZE_DRIVER: &str = r##"
;(async function(){
  const at = "1400px to 800px and back, untouched: ";
  __size(__WIDE);
  await __flush();
  const wide = __overviewAt(at, "files overview on load");
  const fetches = __fetches;
  __press();
  __resize(__NARROW);
  __frame();
  const narrow = __overviewAt(at, "files overview resized to 800px");
  __resize(__WIDE);
  __frame();
  const back = __overviewAt(at, "files overview resized back to 1400px");
  if (back.html !== wide.html || back.t !== wide.t)
    throw new Error("REGRESSION: " + at + "back at 1400px the overview must be the one it opened as, " + back.t + " against " + wide.t);
  if (__fetches !== fetches)
    throw new Error("REGRESSION: " + at + "a resize must make no request, it made " + (__fetches - fetches));
  __resize(__NARROW);
  __frame();
  loadKgOverview();
  await __flush();
  const fresh = __overviewAt(at, "files overview drawn fresh at 800px");
  if (narrow.html !== fresh.html || narrow.t !== fresh.t)
    throw new Error("REGRESSION: " + at + "at 800px the untouched overview must re-fit as a fresh draw there does, "
      + narrow.t + " against " + fresh.t);
  console.log("OK an-untouched-view-refits-on-every-resize");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// Driver: an overview the user has panned keeps its transform and its marks, each node where it
/// shows on screen, through a resize from a 1400px viewport to 800px and back, and through a change
/// of its width alone and of its height alone and back, with no new request: only its drawing
/// surface takes each new size, and its labels follow the scale it is then seen at. It is seen at a
/// scale of 1 once the one frame has run, even where the drawing at its old height ran past a
/// shorter panel until that frame. A panned call walk does the same, through 800px and a phone's panel beside the page's
/// scrollbar. A fresh view
/// after it opens fitted and, untouched, re-fits on a resize. One the user zooms at 800px keeps its
/// zoom through a resize to a 1400px viewport whose panel is taller, where the fitted scale is
/// larger, and the wheel zooms on from the kept scale, in and back, never jumping above it.
const MOVED_RESIZE_DRIVER: &str = r##"
;(async function(){
  const at = "1400px to 800px and back, moved: ";
  // The scale the page last chose the shown labels at, as it asks kgVisibleLabels for them.
  let labelScale = null;
  const visibleLabels = kgVisibleLabels;
  kgVisibleLabels = function(nodes, pos, scale, radiusOf, labelOf){
    labelScale = scale;
    return visibleLabels(nodes, pos, scale, radiusOf, labelOf);
  };
  const near = function(a, b){ return Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(b)); };
  // The panel at a 1400px viewport in a taller window.
  const TALL = [1358, 340, 24];
  // A moved view, once the panel is resized to the row p and the frame has run: its transform as t
  // was, its marks as they were and each node where it showed on screen, relative to the surface's
  // top-left corner; its surface filling the room the panel leaves; on a force view, the labels
  // chosen at the scale the surface is now seen at. Answers the drawing.
  function keptThrough(p, what, mode, t, marks){
    labelScale = null;
    const shown = __onSurface(at + "the " + what + " before the resize");
    __resize(p);
    __frame();
    const d = __fillsItsRoom(at, what, mode);
    const now = JSON.stringify(__view(at + "the " + what));
    if (now !== t)
      throw new Error("REGRESSION: " + at + "the " + what + " must keep its transform, " + now + " against " + t);
    if (__marks() !== marks)
      throw new Error("REGRESSION: " + at + "the " + what + " must keep its marks where they were, not draw them again");
    const moved = __onSurface(at + "the " + what).map(function(q, i){ return [q[0] - shown[i][0], q[1] - shown[i][1]]; })
      .filter(function(dv){ return Math.abs(dv[0]) > 1e-6 || Math.abs(dv[1]) > 1e-6; });
    if (moved.length)
      throw new Error("REGRESSION: " + at + "the " + what + " must keep its marks where they are on screen, but " + moved.length
        + " of " + shown.length + " nodes moved, the first by " + moved[0].map(function(v){ return v.toFixed(1); }).join(", "));
    const seen = __seenAt(at + "the " + what).x * JSON.parse(t).k;
    if (mode !== "calls" && !(labelScale !== null && near(labelScale, seen)))
      throw new Error("REGRESSION: " + at + "the " + what + " must show the labels for the scale it is now seen at, "
        + seen + ", not " + labelScale);
    return d;
  }
  __size(__WIDE);
  await __flush();
  const wide = __overviewAt(at, "files overview on load");
  const wideMarks = __marks();
  const fetches = __fetches;
  __press(40, 25);
  const panned = JSON.stringify(__view(at + "the pan"));
  if (panned === wide.t) throw new Error(at + "the drag must pan the overview");
  keptThrough(__NARROW, "panned overview resized to 800px", "overview", panned, wideMarks);
  keptThrough(__WIDE, "panned overview resized back to 1400px", "overview", panned, wideMarks);
  // Each dimension alone, and back.
  [[1200, 294, 24], __WIDE, TALL, __WIDE].forEach(function(p){
    keptThrough(p, "panned overview resized to " + p[0] + "x" + p[1], "overview", panned, wideMarks);
  });
  if (__fetches !== fetches)
    throw new Error("REGRESSION: " + at + "a resize must make no request, it made " + (__fetches - fetches));
  seedCalls(__SUBJECT, "down");
  await __flush();
  __fillsItsRoom(at, "call walk", "calls");
  const walk = __marks(), walkFetches = __fetches;
  __press(-30, 15);
  const walked = JSON.stringify(__view(at + "the pan on the call walk"));
  keptThrough(__NARROW, "panned call walk resized to 800px", "calls", walked, walk);
  keptThrough(__PHONE, "panned call walk resized to a 390px phone's panel beside the page's scrollbar", "calls", walked, walk);
  keptThrough(__WIDE, "panned call walk resized back to 1400px", "calls", walked, walk);
  if (__fetches !== walkFetches)
    throw new Error("REGRESSION: " + at + "a resize of the call walk must make no request, it made " + (__fetches - walkFetches));
  loadKgOverview();
  await __flush();
  const fresh = __overviewAt(at, "files overview drawn fresh after the kept pan");
  if (fresh.t !== wide.t)
    throw new Error("REGRESSION: " + at + "a fresh view must open fitted, not where a kept pan left it, " + fresh.t + " against " + wide.t);
  __resize(TALL);
  __frame();
  const tall = __overviewAt(at, "fresh overview resized to the taller 1400px panel");
  __resize(__NARROW);
  __frame();
  const refit = __overviewAt(at, "fresh overview resized to 800px");
  loadKgOverview();
  await __flush();
  const narrow800 = __overviewAt(at, "files overview drawn fresh at 800px");
  if (refit.html !== narrow800.html || refit.t !== narrow800.t)
    throw new Error("REGRESSION: " + at + "a fresh view after a kept pan is untouched, so a resize must re-fit it as a fresh draw there does, "
      + refit.t + " against " + narrow800.t);
  const marks800 = __marks();
  const k = __wheel(-1);
  const zoomed = JSON.stringify(__view(at + "the zoom"));
  if (!(k < JSON.parse(tall.t).k))
    throw new Error(at + "this case needs the 800px zoom " + k + " below the taller 1400px panel's opening scale " + tall.t);
  keptThrough(TALL, "zoomed 800px overview resized to the taller 1400px panel", "overview", zoomed, marks800);
  const zin = __wheel(-1);
  if (Math.abs(zin - k * 1.1) > 1e-12)
    throw new Error("REGRESSION: " + at + "the wheel must zoom on from the kept scale " + k + ", got " + zin);
  const zback = __wheel(1);
  const zout = __wheel(1);
  if (Math.abs(zback - k) > 1e-12 || zout > k + 1e-12)
    throw new Error("REGRESSION: " + at + "zooming back out from the kept scale " + k + " must return to it and go no higher, got "
      + zback + " then " + zout);
  console.log("OK a-moved-view-keeps-its-transform-and-marks-through-a-resize");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// Driver: the page binds one size observer, on the panel, however many views it draws; a
/// notification at the size the drawing was drawn at redraws nothing, while a change of width alone
/// redraws; three size changes within a frame queue one redraw; and a panel holding a message, or the
/// empty slot a draw that threw left, rather than a drawing keeps what it holds.
const RESIZE_DISCIPLINE_DRIVER: &str = r##"
;(async function(){
  const at = "1400px: ";
  __size(__WIDE);
  await __flush();
  reprojectSubject(__SUBJECT);
  await __flush();
  drillCluster(__CLUSTER);
  await __flush();
  loadKgOverview();
  await __flush();
  const panel = el("kgpanel");
  const watching = __observers.map(function(o){ return o._targets.map(function(t){ return t.id; }).join("+"); });
  if (JSON.stringify(watching) !== JSON.stringify(["kgpanel"]))
    throw new Error("REGRESSION: " + at + "the page must bind one size observer, on the panel, not " + JSON.stringify(watching));
  const svg = panel._svg;
  __notify();
  __frame();
  if (panel._svg !== svg) throw new Error("REGRESSION: " + at + "a notification at the drawn size must redraw nothing");
  __resize([1200, 294, 24]);
  __frame();
  __overviewAt(at, "overview after its panel narrowed alone");
  __resize([1000, 294, 24]);
  __resize([900, 294, 24]);
  __resize(__NARROW);
  const frames = __frame();
  if (frames !== 1) throw new Error("REGRESSION: " + at + "three size changes in a frame must queue one redraw, not " + frames);
  __overviewAt(at, "overview after three size changes in a frame");
  const message = '<div class="empty">the knowledge graph is unavailable</div>';
  showKgMessage(message);
  __resize(__WIDE);
  __frame();
  if (panel._html !== message)
    throw new Error("REGRESSION: " + at + "a panel holding a message must keep it through a resize, not " + panel._html.slice(0, 200));
  loadKgOverview();
  await __flush();
  __overviewAt(at, "overview drawn before a draw that throws");
  try { kgDraw("", function(){ throw new Error("a draw that throws"); }, ""); } catch (e) {}
  const slot = panel._html;
  if (slot.indexOf('<svg class="kgsvg"') === -1) throw new Error(at + "the draw that throws must leave the drawing's empty slot");
  __resize([1200, 294, 24]);
  __frame();
  if (panel._html !== slot)
    throw new Error("REGRESSION: " + at + "a panel holding the empty slot a draw that threw left must keep it through a resize, not "
      + panel._html.slice(0, 200));
  console.log("OK the-panel-redraws-once-a-frame-and-only-on-a-new-size");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// Driver, for the row `__ROW__` (`{writer, pan, resize}`): what the panel shows of the view it
/// holds beyond the view's body - the `writer` row of `WRITERS` - is shown, still shown once the panel
/// has settled (its size observer told of the 12px the panel's own scrollbar takes when what it shows
/// makes it overflow), and still shown and answering after the panel is resized from a 1400px
/// viewport to 800px (`window`) or narrowed by the 12px a scrollbar takes (`scrollbar`), the drawing
/// filling the room below the toolbar and what it shows above the drawing, and the settling and the
/// resize making no request (a grain box a redraw replaces commits nothing). An untouched view (`pan`
/// `none`) redraws once from its held body on the resize. A moved one, panned or zoomed `after` it was
/// shown or `before`, keeps its transform and its marks through showing it, the panel settling and
/// the resize, the resize redrawing nothing.
const VIEW_STATE_DRIVER: &str = r##"
;(async function(){
  const ROW = __ROW__;
  const on = { none: "an untouched view", after: "a view moved after it was shown", before: "a view moved before it was shown" };
  if (!(ROW.pan in on)) throw new Error("a row pans none, after or before, not " + ROW.pan);
  const at = ROW.writer + " on " + on[ROW.pan] + ", "
    + (ROW.resize === "window" ? "resized from 1400px to 800px" : "narrowed by the 12px a scrollbar takes") + ": ";
  const A = "crates/crate-2/src/module_2.rs::Thing", B = "crates/crate-3/src/module_3.rs::Thing";
  __CALLS.nodes[5].frontier = [A, B];
  // The call node the rationale overlay finds a decision for.
  const WHY = __CALLS.nodes[3].id;
  // Every request the page makes, a call walk answered for the seed and direction it asks for, and a
  // rationale read answered with WHY's decision.
  const asked = [];
  const served = __graph;
  __graph = function(url){
    const s = String(url);
    asked.push(s);
    if (s.indexOf("explain=") !== -1)
      return { nodes: [{ node: WHY, leaves: [{ kind: "decision", summary: "it calls on the next layer" }] }] };
    if (s.indexOf("view=calls") === -1) return served(url);
    const seed = decodeURIComponent((s.match(/[?&]seed=([^&]*)/) || [null, ""])[1]);
    const dir = (s.match(/[?&]dir=([a-z]+)/) || [null, "down"])[1];
    return { seed: seed, dir: dir, nodes: __CALLS.nodes, edges: __CALLS.edges };
  };
  // The redraws of the held view the page makes.
  let redraws = 0;
  const redraw = rerenderCurrentView;
  rerenderCurrentView = function(){ redraws++; return redraw(); };
  const panel = el("kgpanel");
  const camel = function(s){ return s.replace(/-([a-z])/g, function(_, c){ return c.toUpperCase(); }); };
  // The element whose opening tag in the panel's markup carries mark, as the target of an event: its
  // attributes, its data-* attributes as its dataset, whether it is open, and closest() answering
  // itself for a [data-*] selector it carries. Null when no tag carries mark.
  function element(mark){
    const html = panel._html, i = html.indexOf(mark);
    if (i === -1) return null;
    const tag = html.slice(html.lastIndexOf("<", i), html.indexOf(">", i) + 1);
    const attrs = {}, dataset = {}, re = /\s([\w-]+)(?:="([^"]*)")?/g;
    let m;
    while ((m = re.exec(tag)) !== null) {
      attrs[m[1]] = m[2] === undefined ? "" : __unesc(m[2]);
      if (m[1].indexOf("data-") === 0) dataset[camel(m[1].slice(5))] = attrs[m[1]];
    }
    const node = { tag: tag, attrs: attrs, dataset: dataset, open: "open" in attrs, value: attrs.value };
    node.closest = function(sel){
      const want = sel.match(/^\[data-([\w-]+)\]$/);
      return want && camel(want[1]) in dataset ? node : null;
    };
    return node;
  }
  // Dispatch an event of type to the panel's listeners, as from target, carrying the fields of extra;
  // answers the event.
  function dispatch(type, target, extra){
    const ev = Object.assign({ type: type, target: target, defaultPrevented: false,
      preventDefault: function(){ ev.defaultPrevented = true; } }, extra || {});
    return __reach(panel, ev);
  }
  // What the browser keeps on the disclosure mark finds while the panel's markup is not replaced (the
  // same _gen): its open state; the disclosure a new markup makes takes it from its attributes.
  const native = {};
  function live(mark){ const s = native[mark]; return s && s.gen === panel._gen ? s : null; }
  function target(mark){
    const node = element(mark);
    if (!node) throw new Error(at + "the panel must hold " + mark + ", it holds " + panel._html.slice(0, 300));
    return node;
  }
  // The browser's own <details> toggle on the disclosure mark finds, and the toggle event it fires.
  function toggle(mark, open){
    const node = target(mark);
    native[mark] = { gen: panel._gen, open: open };
    node.open = open;
    dispatch("toggle", node);
  }
  function isOpen(mark){ const s = live(mark), node = element(mark); return s ? s.open : !!node && node.open; }
  // The grain box the panel's markup writes now.
  function grainBox(){ return panel.querySelector("[data-lens-res]"); }
  // A click into the grain box, then every character in it selected.
  function clickInto(b){
    b.focus();
    b.setSelectionRange(0, b.value.length);
  }
  // Text typed where the focus is: into the grain box at its caret, and the input event it fires,
  // while the box the panel holds has the focus; anywhere else it lands nowhere on the panel.
  function keys(text){
    const b = document.activeElement;
    if (!b || b !== grainBox()) return;
    const at = b.selectionStart;
    b.value = b.value.slice(0, at) + text + b.value.slice(b.selectionEnd);
    b.setSelectionRange(at + text.length, at + text.length);
    dispatch("input", b);
  }
  // Enter pressed where the focus is: the grain box's keydown, then the change it commits.
  function enter(){
    const b = document.activeElement;
    if (!b || b !== grainBox()) return;
    dispatch("keydown", b, { key: "Enter" });
    b._commit();
  }
  // Leaving the grain box that holds the focus: the change it commits, then its focusout.
  function leave(){
    const b = document.activeElement;
    if (b && b === grainBox()) b._leave();
  }
  // Let the panel settle as a browser does: while its client box differs from the one its size
  // observer was last told of, tell it, and run the frames the page queued. A panel still changing
  // after 8 rounds redraws in a loop. Answers the redraws the settling made.
  let told = "";
  const box = function(){ return panel.clientWidth + "x" + panel.clientHeight; };
  function settle(what){
    const before = redraws;
    for (let round = 0; round < 8; round++) {
      const now = box();
      if (now === told && !__frames.length) return redraws - before;
      if (now !== told) { told = now; __notify(); }
      __frame();
    }
    throw new Error("REGRESSION: " + at + what + ", the panel never settles: it redrew " + (redraws - before) + " times in 8 frames");
  }
  // The drawing fills the room the panel leaves below its toolbar and what it shows above the
  // drawing, at its surface's own size.
  function placed(what){
    const d = __atItsSurface(at + what);
    const room = Math.max(200, Math.floor(__geom.height - __geom.padBottom - __slotTop(panel._html)));
    if (d.height !== room)
      throw new Error("REGRESSION: " + at + what + ", the drawing is " + d.height + "px tall where the panel leaves it "
        + room + "px below its toolbar and what it shows above the drawing");
  }
  // What each writer shows: the view it shows it on, the act that shows it, what it shows now (to
  // equal expect), and the click it answers.
  const WRITERS = {
    frontier: {
      async open(){ seedCalls(__SUBJECT, "down"); await __flush(); },
      async show(){
        const node = element('data-frontier="1"');
        if (!node) throw new Error(at + "the call walk must draw a frontier node to click");
        dispatch("click", node);
        await __flush();
      },
      shown(){
        const span = __slotSpan(panel._html);
        const listed = span ? panel._html.slice(span[0], span[1]).match(/data-candidate="[^"]*"/g) || [] : [];
        return listed.map(function(a){ return __unesc(a.slice('data-candidate="'.length, -1)); }).join("|");
      },
      expect: A + "|" + B,
      async answer(){
        const cand = element('data-candidate="' + A + '"');
        if (!cand) throw new Error("REGRESSION: " + at + "the list must still offer " + A + " to choose");
        const n = asked.length;
        dispatch("click", cand);
        await __flush();
        const walk = asked.slice(n).filter(function(u){ return u.indexOf("view=calls") !== -1; })[0] || "";
        if (walk.indexOf("seed=" + encodeURIComponent(A)) === -1 || walk.indexOf("dir=down") === -1)
          throw new Error("REGRESSION: " + at + "choosing " + A + " must walk down from it (spec 52 c5), the page asked "
            + JSON.stringify(asked.slice(n)));
        if (kgMode !== "calls" || panel._html.indexOf("<b>" + A + "</b>") === -1)
          throw new Error("REGRESSION: " + at + "choosing " + A + " must draw its walk, the panel holds " + panel._html.slice(0, 300));
        if (this.shown() !== "")
          throw new Error("REGRESSION: " + at + "the choice made must dismiss the list, it still lists " + this.shown());
      }
    },
    // The rationale overlay's "why" badge below the call walk and the decision inside it: the overlay
    // turned on, then each opened by the browser's own <details> toggle.
    disclosure: {
      BADGE: 'data-explain="' + WHY + '"',
      LEAF: 'class="kgleaf k-',
      async open(){ seedCalls(__SUBJECT, "down"); await __flush(); },
      // A click on the badge is left to its own disclosure: nothing prevented, requested or redrawn.
      async click(){
        const n = asked.length, gen = panel._gen, ev = dispatch("click", target(this.BADGE));
        await __flush();
        if (ev.defaultPrevented || asked.length !== n || panel._gen !== gen)
          throw new Error("REGRESSION: " + at + "a click on the why badge must be left to its disclosure, it "
            + (ev.defaultPrevented ? "was prevented" : asked.length !== n ? "requested " + asked.slice(n) : "redrew the panel"));
      },
      async show(){
        await setOverlay(true);
        await __flush();
        await this.click();
        toggle(this.BADGE, true);
        toggle(this.LEAF, true);
      },
      shown(){ return [this.BADGE, this.LEAF].map(function(m){ return isOpen(m) ? "open" : "closed"; }).join(" "); },
      expect: "open open",
      async answer(){
        await this.click();
        toggle(this.BADGE, false);
        redraw();
        if (this.shown() !== "closed open")
          throw new Error("REGRESSION: " + at + "the badge closed must stay closed through a redraw, its decision kept open, "
            + "it shows " + this.shown());
      }
    },
    // A grain being typed into the code lens's resolution box above the overview: the box clicked into,
    // its text selected and "2." typed over it, the box keeping the focus and the caret after the "2.".
    grain: {
      async open(){ kgLens = "code"; loadKgOverview(); await __flush(); },
      async show(){ clickInto(grainBox()); keys("2."); },
      shown(){
        const b = grainBox();
        if (!b) return "no grain box";
        const focus = document.activeElement === b ? " focused, caret " + b.selectionStart + "-" + b.selectionEnd : " unfocused";
        return JSON.stringify(b.value) + focus;
      },
      expect: '"2." focused, caret 2-2',
      // The "5" typed next lands in the box, then Enter after a window resize, leaving the box after a
      // scrollbar's: either commits the grain typed, read once, the change the browser fires with it
      // reading nothing more.
      async answer(){
        const n = asked.length, how = ROW.resize === "window" ? "Enter" : "leaving the box";
        keys("5");
        if (ROW.resize === "window") enter(); else leave();
        await __flush();
        const reads = asked.slice(n).filter(function(u){ return u.indexOf("lens=code") !== -1; });
        if (reads.length !== 1 || reads[0].indexOf("resolution=2.5") === -1)
          throw new Error("REGRESSION: " + at + "the \"5\" typed must land in the box and " + how + " commit the grain "
            + "typed, reading the code lens once at 2.5, the page asked " + JSON.stringify(asked.slice(n)));
      }
    }
  };
  // The grain "2." typed as the grain writer types it, kept as a draft in the box an untouched view's
  // redraw writes in its place, whose markup holds it, so the browser fires no change for it: then Enter
  // after a window resize, leaving the box after a scrollbar's, with nothing more typed, commits it,
  // read once.
  WRITERS.draft = Object.assign({}, WRITERS.grain, {
    async answer(){
      const b = grainBox(), n = asked.length, how = ROW.resize === "window" ? "Enter" : "leaving the box";
      if (ROW.pan !== "none") throw new Error("a draft is kept in the box an untouched view's redraw writes, so its row pans none");
      if (b.value !== b.defaultValue)
        throw new Error(at + "the redraw must write the draft into the box's markup, the box holds " + JSON.stringify(b.value)
          + " over its markup's " + JSON.stringify(b.defaultValue));
      if (ROW.resize === "window") enter(); else leave();
      await __flush();
      const reads = asked.slice(n).filter(function(u){ return u.indexOf("lens=code") !== -1; });
      if (reads.length !== 1 || !/[?&]resolution=2\.(&|$)/.test(reads[0]))
        throw new Error("REGRESSION: " + at + how + " with nothing more typed must commit the draft kept through the redraw, "
          + "reading the code lens once at 2., the page asked " + JSON.stringify(asked.slice(n)));
    }
  });
  // The move, and the transform and marks it leaves, which a moved view keeps from then on: a drag on
  // the drawing, or a wheel notch while the grain box holds the focus, which a press on the drawing
  // would take from it (leaving the box, which commits the grain).
  let kept = null, marks = null;
  function pan(){
    if (document.activeElement) __wheel(-1); else __press(-30, 15);
    kept = JSON.stringify(__view(at + "the move")); marks = __marks();
  }
  function keeps(what){
    const now = JSON.stringify(__view(at + what));
    if (now !== kept || __marks() !== marks)
      throw new Error("REGRESSION: " + at + "the moved view must keep its transform and its marks through " + what + ", "
        + now + " against " + kept + (__marks() === marks ? "" : ", its marks laid out again"));
  }
  const W = WRITERS[ROW.writer];
  __size(__WIDE);
  await __flush();
  await W.open();
  told = box();
  settle("the view drawn");
  if (ROW.pan === "before") pan();
  await W.show();
  if (W.shown() !== W.expect)
    throw new Error(at + "showing it must show " + W.expect + ", it shows " + JSON.stringify(W.shown()));
  const fetches = __fetches;
  settle("showing it");
  if (W.shown() !== W.expect)
    throw new Error("REGRESSION: " + at + "it must stay shown once the panel settles after showing it, it shows "
      + JSON.stringify(W.shown()));
  placed("showing it");
  if (ROW.pan === "before") keeps("showing it");
  if (ROW.pan === "after") pan();
  __size(ROW.resize === "window" ? __NARROW : [__WIDE[0] - __geom.scrollbar, __WIDE[1], __WIDE[2]]);
  const made = settle("the resize");
  if (__fetches !== fetches)
    throw new Error("REGRESSION: " + at + "the panel settling and the redraw must make no request, they made " + (__fetches - fetches));
  if (W.shown() !== W.expect)
    throw new Error("REGRESSION: " + at + "it must stay shown through the redraw, it shows " + JSON.stringify(W.shown()));
  placed("the resize");
  if (ROW.pan !== "none") {
    keeps("the resize");
    if (made !== 0) throw new Error("REGRESSION: " + at + "the moved view must redraw nothing on the resize, it redrew " + made + " times");
  } else if (made !== 1) {
    throw new Error("REGRESSION: " + at + "the untouched view must redraw once, not " + made + " times");
  }
  await W.answer();
  console.log("OK the-redraw-keeps-what-the-panel-shows");
})().catch(function(e){ console.error(String((e && e.stack) || e)); process.exit(1); });
"##;

/// Runs `driver`, and claims it ends in `ok`.
fn drive(driver: &str, ok: &str, claim: &str) {
    node_harness_claims(
        &vm_harness(
            &[
                EVENT_PHASES,
                PANEL_SHIM,
                OVERVIEWS_FETCH,
                PAGE_READERS,
                RESIZE_READERS,
            ],
            driver,
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
        &OPENING_DRIVER.replace("__PANEL__", panel),
        "OK every-overview-opens-fitted-to-its-panel",
        "every overview must open fitted to its panel",
    );
}

/// Every view's drawing height, in the panel a `[clientWidth, clientHeight, {view: the height its
/// toolbar takes, 6px margin included}]` row describes.
fn fits_below_its_toolbar(panel: &str) {
    drive(
        &PANEL_HEIGHT_DRIVER.replace("__PANEL__", panel),
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
    /// The same phone once the rail and the card stacked above the panel push the page past the
    /// viewport, so the page's 12px scrollbar leaves the panel 336px and its surface 312px.
    the_overview_opens_fitted_at_390px_beside_the_page_scrollbar:
        opens_fitted("[390, 336, 414, 62]");
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
    /// toolbar it leaves 194px, under the drawing's 200px floor, so the panel scrolls and shows a
    /// scrollbar of its own, which leaves the drawing's surface 312px.
    the_drawing_keeps_its_floor_below_a_tall_toolbar_in_a_short_panel:
        fits_below_its_toolbar(r#"[348, 294, {"overview": 62, "reproject": 80, "drill": 60, "calls": 80}]"#);
    /// The instances strip grows after the first draw at 1400px, leaving 300px of content in a
    /// 294px panel until the overview redraws.
    the_first_draw_refits_when_the_strip_above_it_grows: drive(
        FIRST_DRAW_RESIZE_DRIVER,
        "OK the-first-draw-refits-to-the-panel-the-strip-leaves",
        "the first drawing must redraw to the room the panel leaves once the strip above it grows",
    );
    /// A window resize from 1400px to 800px and back.
    an_untouched_view_refits_on_a_resize_and_back: drive(
        UNTOUCHED_RESIZE_DRIVER,
        "OK an-untouched-view-refits-on-every-resize",
        "an untouched view must redraw and re-fit on every resize",
    );
    /// The same resize after a pan, on an overview and a call walk, and after a zoom.
    a_moved_view_keeps_its_transform_on_a_resize_and_back: drive(
        MOVED_RESIZE_DRIVER,
        "OK a-moved-view-keeps-its-transform-and-marks-through-a-resize",
        "a panned or zoomed view must keep its transform and marks through a resize, only its surface resized",
    );
    /// One observer, one redraw a frame, none at an unchanged size or over a message or an empty slot.
    the_panel_redraws_once_a_frame_and_only_on_a_new_size: drive(
        RESIZE_DISCIPLINE_DRIVER,
        "OK the-panel-redraws-once-a-frame-and-only-on-a-new-size",
        "the panel must redraw at most once a frame, only on a new size, and never over a message or an empty slot",
    );
    /// The call view's frontier choice list, the view untouched, through a window resize.
    the_frontier_list_stays_through_a_window_resize_of_an_untouched_view:
        keeps_what_it_shows(r#"{"writer": "frontier", "pan": "none", "resize": "window"}"#);
    /// The same list, the view panned after it was shown.
    the_frontier_list_stays_through_a_window_resize_of_a_view_panned_after_it:
        keeps_what_it_shows(r#"{"writer": "frontier", "pan": "after", "resize": "window"}"#);
    /// The same list shown on a view already panned, which keeps its pan through the list's redraw.
    the_frontier_list_stays_through_a_window_resize_of_a_view_panned_before_it:
        keeps_what_it_shows(r#"{"writer": "frontier", "pan": "before", "resize": "window"}"#);
    /// The same list, the panel narrowed by the 12px a scrollbar takes.
    the_frontier_list_stays_through_a_scrollbar_of_an_untouched_view:
        keeps_what_it_shows(r#"{"writer": "frontier", "pan": "none", "resize": "scrollbar"}"#);
    /// The same, the view panned after the list was shown.
    the_frontier_list_stays_through_a_scrollbar_of_a_view_panned_after_it:
        keeps_what_it_shows(r#"{"writer": "frontier", "pan": "after", "resize": "scrollbar"}"#);
    /// The same, the list shown on a view already panned.
    the_frontier_list_stays_through_a_scrollbar_of_a_view_panned_before_it:
        keeps_what_it_shows(r#"{"writer": "frontier", "pan": "before", "resize": "scrollbar"}"#);
    /// A rationale disclosure opened below the call walk, the view untouched, through a window resize.
    an_open_disclosure_stays_through_a_window_resize_of_an_untouched_view:
        keeps_what_it_shows(r#"{"writer": "disclosure", "pan": "none", "resize": "window"}"#);
    /// The same disclosure, the view panned after it was opened.
    an_open_disclosure_stays_through_a_window_resize_of_a_view_panned_after_it:
        keeps_what_it_shows(r#"{"writer": "disclosure", "pan": "after", "resize": "window"}"#);
    /// The same disclosure on a view already panned, which keeps its pan through the overlay's redraw.
    an_open_disclosure_stays_through_a_window_resize_of_a_view_panned_before_it:
        keeps_what_it_shows(r#"{"writer": "disclosure", "pan": "before", "resize": "window"}"#);
    /// The same disclosure, the panel narrowed by the 12px a scrollbar takes.
    an_open_disclosure_stays_through_a_scrollbar_of_an_untouched_view:
        keeps_what_it_shows(r#"{"writer": "disclosure", "pan": "none", "resize": "scrollbar"}"#);
    /// The same, the view panned after the disclosure was opened.
    an_open_disclosure_stays_through_a_scrollbar_of_a_view_panned_after_it:
        keeps_what_it_shows(r#"{"writer": "disclosure", "pan": "after", "resize": "scrollbar"}"#);
    /// The same, the disclosure opened on a view already panned.
    an_open_disclosure_stays_through_a_scrollbar_of_a_view_panned_before_it:
        keeps_what_it_shows(r#"{"writer": "disclosure", "pan": "before", "resize": "scrollbar"}"#);
    /// A grain typed into the code lens's resolution box, the box keeping the focus and the caret, the
    /// overview untouched, through a window resize.
    a_typed_grain_stays_through_a_window_resize_of_an_untouched_view:
        keeps_what_it_shows(r#"{"writer": "grain", "pan": "none", "resize": "window"}"#);
    /// The same grain, the overview zoomed by the wheel after it was typed, the box keeping the focus.
    a_typed_grain_stays_through_a_window_resize_of_a_view_zoomed_after_it:
        keeps_what_it_shows(r#"{"writer": "grain", "pan": "after", "resize": "window"}"#);
    /// The same grain typed on an overview already panned.
    a_typed_grain_stays_through_a_window_resize_of_a_view_panned_before_it:
        keeps_what_it_shows(r#"{"writer": "grain", "pan": "before", "resize": "window"}"#);
    /// The same grain, the panel narrowed by the 12px a scrollbar takes.
    a_typed_grain_stays_through_a_scrollbar_of_an_untouched_view:
        keeps_what_it_shows(r#"{"writer": "grain", "pan": "none", "resize": "scrollbar"}"#);
    /// The same, the overview zoomed by the wheel after the grain was typed.
    a_typed_grain_stays_through_a_scrollbar_of_a_view_zoomed_after_it:
        keeps_what_it_shows(r#"{"writer": "grain", "pan": "after", "resize": "scrollbar"}"#);
    /// The same, the grain typed on an overview already panned.
    a_typed_grain_stays_through_a_scrollbar_of_a_view_panned_before_it:
        keeps_what_it_shows(r#"{"writer": "grain", "pan": "before", "resize": "scrollbar"}"#);
    /// A grain typed, kept as a draft in the box the untouched overview's redraw writes, then Enter
    /// with nothing more typed: it commits the draft once.
    a_drafted_grain_commits_on_enter_after_a_window_resize:
        keeps_what_it_shows(r#"{"writer": "draft", "pan": "none", "resize": "window"}"#);
    /// The same draft committed once by leaving the box after the scrollbar's redraw.
    a_drafted_grain_commits_on_leaving_the_box_after_a_scrollbar:
        keeps_what_it_shows(r#"{"writer": "draft", "pan": "none", "resize": "scrollbar"}"#);
}

/// What the panel shows of the view it holds, in the `{writer, pan, resize}` row `VIEW_STATE_DRIVER`
/// reads, kept through the panel settling and a resize, and still answering.
fn keeps_what_it_shows(row: &str) {
    drive(
        &VIEW_STATE_DRIVER.replace("__ROW__", row),
        "OK the-redraw-keeps-what-the-panel-shows",
        "a redraw must keep what the panel was showing, still answering",
    );
}
