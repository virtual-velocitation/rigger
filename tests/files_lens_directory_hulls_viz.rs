//! Periphery (integration) test for spec 63 CRITERION 3's own Done-when clause naming DIRECTORY
//! HULLS: "a test proves the FILES LENS: file nodes sized by entity count with weighted uses edges,
//! DIRECTORY HULLS, and a card listing the file's top entities". The fold/purity surface (file nodes,
//! weighted uses edges) is pinned by [`files_lens_view_periphery`] and `dash.rs`'s own unit tests;
//! this file owns the one remaining piece those never touch - the SERVED PAGE'S OWN CLIENT RENDERING
//! of a directory hull: a low-contrast, labelled region drawn BEHIND the file nodes sharing a
//! directory, mirroring the mockup (`docs/design/kg-lens-mockups.html`) and the Design bullet
//! ("directories are the same hull treatment" as the code lens's community hulls).
//!
//! `kgSvg`'s existing per-node radius/colour/class/label opts (spec 42 c5, spec 52 c5) gain two new
//! OPT-IN ones: `opts.groupOf(n)` keys a node into a group, `opts.groupLabel(key)` names it. Only the
//! FILES lens (the default, and the only lens whose clusters ARE files) supplies them - `renderKgOverview`
//! / `renderReprojection` derive the group from the cluster's OWN key (its file path)'s parent
//! directory, via a new pure `kgFilesDirOf` helper. Code / concepts clusters are communities / concepts,
//! not files, so they never pass `groupOf` and draw no hull - this criterion's own directory grouping
//! must never leak onto a taxonomy that owns no directory concept.
//!
//! Proven by driving the served page's OWN script under node's built-in `vm` (hermetic, no npm) - the
//! behavioral proof a structural grep on the served bytes cannot make (a grep sees `groupOf` exists;
//! only running the renderer proves a hull actually gets DRAWN, positioned around the right members,
//! and labelled with the right directory). `dash` compiles on BOTH the default and the
//! `--no-default-features` lane (the viz is not feature-gated), so this guards the served page in
//! both lanes.
//!
//! BOTH wired call sites get their own dedicated harness ([`HULL_HARNESS`] for `renderKgOverview`,
//! [`HULL_HARNESS_REPROJECTION`] for `renderReprojection`): each carries its own `groupOf`/`groupLabel`
//! ternary, so a mistake at ONE call site (a typo'd lens comparison, a dropped opt) would not redden
//! the other's test - proving the overview alone would leave the re-projection panel's directory hulls
//! (spec 55's subject x lens surface, e.g. a concept re-grained by files) structurally unguarded.

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

/// True when a `node` runtime can be spawned (present on dev machines and on GitHub `ubuntu-latest`,
/// which ships Node.js on PATH, so this runtime guard runs in CI).
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A DOM shim + test driver (JavaScript) that RUNS the served page's OWN renderers under node's built-in
/// `vm` (no npm, hermetic). It drives `renderKgOverview` directly against a fixture `ClusterOverview` (no
/// fetch involved - this criterion owns the RENDER, not the fetch dispatch spec 42 c4/c5 already proves),
/// asserting in turn that: (a) under the default (files) lens, three file clusters split across two
/// directories draw TWO hull regions, each labelled "DIR · <directory>"; (b) a repo-root file (no `/`)
/// groups under the `(root)` sentinel, matching the server's own directory-fold convention; (c) each
/// hull's `<rect>` renders BEFORE any node `<g class="kgn ...">` in the SVG markup (behind, never
/// occluding); and (d) flipping to the CODE lens draws NO hull at all (directory grouping is a files-lens
/// concept only - it must never leak onto a taxonomy that has no directory).
const HULL_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

const SHIM = String.raw`
const __els = {};
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
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };
const fetch = function(){ return Promise.reject(new Error("no network in this harness")); };
`;

const DRIVER = String.raw`
;(function(){
  // Three file clusters: two share "src/alpha" (a.rs, b.rs), one is "src/beta/c.rs", and one is a
  // repo-root file (no "/") - directory-less, so it must group under the (root) sentinel.
  const OVERVIEW = { clusters: [
      { key: "src/alpha/a.rs", count: 2, kind: "code-entity" },
      { key: "src/alpha/b.rs", count: 1, kind: "code-entity" },
      { key: "src/beta/c.rs",  count: 1, kind: "code-entity" },
      { key: "build.rs",       count: 1, kind: "code-entity" },
    ], edges: [], total: 5 };

  if (typeof kgLens === "undefined") throw new Error("the served page must declare kgLens");
  if (kgLens !== "files") throw new Error("the served page must default kgLens to files, got " + kgLens);

  renderKgOverview(OVERVIEW);
  const html = el("kgpanel")._html;

  // Two hull regions (src/alpha, src/beta) - a repo-root file's own hull groups it under (root), so
  // build.rs does NOT spawn a third real-directory hull.
  const hullCount = (html.match(/<g class="kghull">/g) || []).length;
  if (hullCount !== 3)
    throw new Error("expected 3 hull groups (src/alpha, src/beta, (root)), got " + hullCount + ": " + html);
  if (html.indexOf("DIR · src/alpha") === -1)
    throw new Error("missing the src/alpha directory hull label: " + html);
  if (html.indexOf("DIR · src/beta") === -1)
    throw new Error("missing the src/beta directory hull label: " + html);
  if (html.indexOf("DIR · (root)") === -1)
    throw new Error("a repo-root file must group under the (root) sentinel: " + html);

  // Every hull <rect> must render BEFORE every node <g class="kgn ..."> - behind, never occluding.
  const firstNode = html.indexOf('<g class="kgn ');
  const lastHull = html.lastIndexOf('<g class="kghull">');
  if (firstNode === -1) throw new Error("the overview did not render any node: " + html);
  if (lastHull === -1 || lastHull > firstNode)
    throw new Error("every hull must render BEFORE the node circles (drawn behind them): " + html);

  // Flipping to the CODE lens draws NO hull at all - directory grouping is a files-lens-only concept.
  kgLens = "code";
  const CODE_OVERVIEW = { clusters: [
      { key: "community/1/0", count: 2, kind: "code-entity", label: "alpha" },
      { key: "community/1/1", count: 1, kind: "code-entity", label: "beta" },
    ], edges: [], total: 3 };
  renderKgOverview(CODE_OVERVIEW);
  const codeHtml = el("kgpanel")._html;
  if (codeHtml.indexOf('<g class="kghull">') !== -1)
    throw new Error("the CODE lens must draw NO hull - directory grouping is a files-lens concept only: " + codeHtml);
  kgLens = "files";

  console.log("OK directory-hulls-draw-group-and-label-file-clusters-by-directory");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-hull-harness.js" });
"##;

/// RUNTIME guard for spec 63 c3's directory hulls (its own Done-when clause): the served page's OWN
/// `renderKgOverview` draws a low-contrast, labelled hull region behind each directory's file nodes
/// under the default (files) lens - a repo-root file groups under `(root)`, every hull renders BEHIND
/// the node circles, and the CODE lens (a different taxonomy, no directory concept) draws none. This
/// is the behavioral proof a structural grep on the served bytes cannot make.
#[test]
fn the_files_lens_draws_directory_hulls_behind_its_file_nodes() {
    if !node_available() {
        eprintln!(
            "SKIP the_files_lens_draws_directory_hulls_behind_its_file_nodes: no `node` runtime on \
             PATH. This runtime guard needs node (present on dev machines and on ubuntu-latest CI); \
             install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the hull harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, HULL_HARNESS).expect("write the hull harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served files-lens directory hulls");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the files lens must draw directory hulls behind its file nodes, but the runtime harness \
         failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK directory-hulls-draw-group-and-label-file-clusters-by-directory"),
        "the hull harness must confirm the directory-hull render:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// A DOM shim + test driver (JavaScript) proving the OTHER wired call site: `renderReprojection` (the
/// subject x lens re-projection panel, spec 55) gets the IDENTICAL `groupOf`/`groupLabel` directory-hull
/// treatment as `renderKgOverview` under the files lens - a SEPARATE conditional at a SEPARATE call
/// site, so a mistake there (a typo'd lens comparison, a dropped opt) would NOT redden
/// [`HULL_HARNESS`] above, which drives only the overview. Drives `renderReprojection` directly against
/// a fixture `Reprojection` (no fetch, no route dispatch - this criterion owns the RENDER, not the
/// subject/lens composition spec 55 already proves) with three file-keyed clusters split across two
/// directories plus a root file, asserting: (a) three hull regions draw, correctly labelled
/// ("DIR · <dir>" / "DIR · (root)"); (b) every hull renders BEFORE any node circle (behind, never
/// occluding); and (c) flipping `kgLens` to `code` draws NO hull for this same renderer (directory
/// grouping is a files-lens concept only, at EITHER call site).
const HULL_HARNESS_REPROJECTION: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

const SHIM = String.raw`
const __els = {};
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
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const window = { addEventListener: function(){} };
const setTimeout = function(){ return 0; };
const fetch = function(){ return Promise.reject(new Error("no network in this harness")); };
`;

const DRIVER = String.raw`
;(function(){
  // The same three-directory shape as the overview harness, but as a Reprojection body (subject +
  // clusters + total + edges): two files share "src/alpha", one is "src/beta/c.rs", one is repo-root.
  const REPROJECTION = { subject: "concept/1/0", total: 4, edges: [], clusters: [
      { key: "src/alpha/a.rs", count: 2, kind: "code-entity" },
      { key: "src/alpha/b.rs", count: 1, kind: "code-entity" },
      { key: "src/beta/c.rs",  count: 1, kind: "code-entity" },
      { key: "build.rs",       count: 1, kind: "code-entity" },
    ] };

  if (typeof kgLens === "undefined") throw new Error("the served page must declare kgLens");
  if (kgLens !== "files") throw new Error("the served page must default kgLens to files, got " + kgLens);

  renderReprojection(REPROJECTION);
  const html = el("kgpanel")._html;

  const hullCount = (html.match(/<g class="kghull">/g) || []).length;
  if (hullCount !== 3)
    throw new Error("expected 3 hull groups (src/alpha, src/beta, (root)) from renderReprojection, got " + hullCount + ": " + html);
  if (html.indexOf("DIR · src/alpha") === -1)
    throw new Error("renderReprojection is missing the src/alpha directory hull label: " + html);
  if (html.indexOf("DIR · src/beta") === -1)
    throw new Error("renderReprojection is missing the src/beta directory hull label: " + html);
  if (html.indexOf("DIR · (root)") === -1)
    throw new Error("renderReprojection: a repo-root file must group under the (root) sentinel: " + html);

  const firstNode = html.indexOf('<g class="kgn ');
  const lastHull = html.lastIndexOf('<g class="kghull">');
  if (firstNode === -1) throw new Error("renderReprojection did not render any node: " + html);
  if (lastHull === -1 || lastHull > firstNode)
    throw new Error("renderReprojection: every hull must render BEFORE the node circles: " + html);

  // Flipping to the CODE lens draws NO hull at THIS call site either.
  kgLens = "code";
  const CODE_REPROJECTION = { subject: "concept/1/0", total: 2, edges: [], clusters: [
      { key: "community/1/0", count: 2, kind: "code-entity", label: "alpha" },
    ] };
  renderReprojection(CODE_REPROJECTION);
  const codeHtml = el("kgpanel")._html;
  if (codeHtml.indexOf('<g class="kghull">') !== -1)
    throw new Error("renderReprojection under the CODE lens must draw NO hull: " + codeHtml);
  kgLens = "files";

  console.log("OK reprojection-draws-group-and-label-file-clusters-by-directory");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-hull-reprojection-harness.js" });
"##;

/// RUNTIME guard for the SECOND call site spec 63 c3's directory hulls wires: `renderReprojection`
/// (the subject x lens re-projection panel) draws the identical directory-hull treatment as the
/// overview - a defect at this call site alone (its own `groupOf`/`groupLabel` ternary) would not
/// redden [`the_files_lens_draws_directory_hulls_behind_its_file_nodes`] above, which never calls it.
#[test]
fn the_reprojection_view_draws_directory_hulls_behind_its_file_clusters() {
    if !node_available() {
        eprintln!(
            "SKIP the_reprojection_view_draws_directory_hulls_behind_its_file_clusters: no `node` \
             runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the reprojection hull harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, HULL_HARNESS_REPROJECTION).expect("write the hull harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served renderReprojection directory hulls");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "renderReprojection must draw directory hulls behind its file clusters, but the runtime \
         harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK reprojection-draws-group-and-label-file-clusters-by-directory"),
        "the reprojection hull harness must confirm the directory-hull render:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
