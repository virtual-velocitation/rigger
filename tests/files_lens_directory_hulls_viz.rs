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
//! BOTH wired call sites get their own dedicated driver ([`HULL_DRIVER`] for `renderKgOverview`,
//! [`HULL_REPROJECTION_DRIVER`] for `renderReprojection`): each carries its own `groupOf`/`groupLabel`
//! ternary, so a mistake at ONE call site (a typo'd lens comparison, a dropped opt) would not redden
//! the other's test - proving the overview alone would leave the re-projection panel's directory hulls
//! (spec 55's subject x lens surface, e.g. a concept re-grained by files) structurally unguarded.

use crate::common;
use crate::vm_harness::{vm_harness, DomShim, Fetch};

use common::served::node_harness_passes;

/// The DOM both hull drivers run in: the drivers call the renderers directly, so every fetch is refused.
const HULL_DOM: DomShim = DomShim {
    fetch: Fetch::Refused(r#""no network in this harness""#),
    ..DomShim::PANEL
};

/// A test driver (JavaScript) that RUNS the served page's OWN renderers under node's built-in
/// `vm` (no npm, hermetic). It drives `renderKgOverview` directly against a fixture `ClusterOverview` (no
/// fetch involved - this criterion owns the RENDER, not the fetch dispatch spec 42 c4/c5 already proves),
/// asserting in turn that: (a) under the default (files) lens, three file clusters split across two
/// directories draw TWO hull regions, each labelled "DIR · <directory>"; (b) a repo-root file (no `/`)
/// groups under the `(root)` sentinel, matching the server's own directory-fold convention; (c) each
/// hull's `<rect>` renders BEFORE any node `<g class="kgn ...">` in the SVG markup (behind, never
/// occluding); and (d) flipping to the CODE lens draws NO hull at all (directory grouping is a files-lens
/// concept only - it must never leak onto a taxonomy that has no directory).
const HULL_DRIVER: &str = r#"
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
"#;

rigger::test_cases! {
    /// RUNTIME guard for spec 63 c3's directory hulls (its own Done-when clause): the served page's OWN
    /// `renderKgOverview` draws a low-contrast, labelled hull region behind each directory's file nodes
    /// under the default (files) lens - a repo-root file groups under `(root)`, every hull renders BEHIND
    /// the node circles, and the CODE lens (a different taxonomy, no directory concept) draws none. This
    /// is the behavioral proof a structural grep on the served bytes cannot make.
    the_files_lens_draws_directory_hulls_behind_its_file_nodes: node_harness_passes(&vm_harness(&[&HULL_DOM.render()], HULL_DRIVER, "dash-hull-harness.js"), "OK directory-hulls-draw-group-and-label-file-clusters-by-directory");
}

/// A test driver (JavaScript) proving the OTHER wired call site: `renderReprojection` (the
/// subject x lens re-projection panel, spec 55) gets the IDENTICAL `groupOf`/`groupLabel` directory-hull
/// treatment as `renderKgOverview` under the files lens - a SEPARATE conditional at a SEPARATE call
/// site, so a mistake there (a typo'd lens comparison, a dropped opt) would NOT redden
/// [`HULL_DRIVER`] above, which drives only the overview. Drives `renderReprojection` directly against
/// a fixture `Reprojection` (no fetch, no route dispatch - this criterion owns the RENDER, not the
/// subject/lens composition spec 55 already proves) with three file-keyed clusters split across two
/// directories plus a root file, asserting: (a) three hull regions draw, correctly labelled
/// ("DIR · <dir>" / "DIR · (root)"); (b) every hull renders BEFORE any node circle (behind, never
/// occluding); and (c) flipping `kgLens` to `code` draws NO hull for this same renderer (directory
/// grouping is a files-lens concept only, at EITHER call site).
const HULL_REPROJECTION_DRIVER: &str = r#"
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
"#;

rigger::test_cases! {
    /// RUNTIME guard for the SECOND call site spec 63 c3's directory hulls wires: `renderReprojection`
    /// (the subject x lens re-projection panel) draws the identical directory-hull treatment as the
    /// overview - a defect at this call site alone (its own `groupOf`/`groupLabel` ternary) would not
    /// redden [`the_files_lens_draws_directory_hulls_behind_its_file_nodes`] above, which never calls it.
    the_reprojection_view_draws_directory_hulls_behind_its_file_clusters: node_harness_passes(&vm_harness(&[&HULL_DOM.render()], HULL_REPROJECTION_DRIVER, "dash-hull-reprojection-harness.js"), "OK reprojection-draws-group-and-label-file-clusters-by-directory");
}
