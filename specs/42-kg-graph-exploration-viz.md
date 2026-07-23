# 42 - KG graph-exploration viz: whole-graph clustered overview and drill-down

**Goal:** give the always-on dash a way to explore the WHOLE knowledge graph, not just the
seeded-neighborhood fragment spec 30 already renders. Spec 30's KG panel answers "what governs THIS
node" (seed a node, see its depth-bounded neighborhood); it has no answer to "what is in the graph at
all, and how is it organized". A 7k+ node graph cannot be drawn node-for-node, so the exploration
view renders the graph as a few dozen CLUSTER super-nodes (a module directory, or a node kind) that
the operator DRILLS INTO on demand. This is the parameterized-exploration workstream of the
concept-graph architecture addendum: the default KG view becomes the whole-graph clustered overview,
a click drills a cluster to its member nodes, and a member click hands off to spec 30's existing
seeded neighborhood - one coherent path from "the whole graph" down to "the provenance of one node".
The dash stays a read-only projection over the existing event store and context graph; this adds no
event type and no external dependency.

## Design

The dash is one `include_str!` page (`src/dash.html`) served by `src/dash.rs`, which projects the
event store + context graph and renders. Spec 30 added the seeded route
`GET /api/graph?seed=&depth=&tier=` and the panel that renders a `Neighborhood`. This spec adds the
two projections that make the SAME panel a whole-graph explorer, and the library-free SVG viz that
draws them. It is a projection + template change: no new event type, no store write, no CDN, no JS
build step (the dash charter: one self-contained page, all CSS and JS inline, same-origin `/api/*`
only).

### Data layer (`src/dash.rs`)

- **`cluster_key(id, kind) -> String`** folds a node into its super-node. A node whose id names a
  file - a code entity (`<file>::<name>`), a rationale anchor (`<file>#L<n>`), or a path id (a file
  or design-doc whose last segment has an extension) - clusters by that file's DIRECTORY (its
  module); its directory-less root falls back to a `(root)` bucket. Every other node (a decision,
  finding, unit, agent, gate, lesson - the dev-loop nodes with no path id) clusters by its KIND. This
  folds thousands of nodes into a few dozen module/kind buckets, deterministically.
- **`clustered_overview(graph) -> ClusterOverview`** aggregates the whole graph: each `cluster_key`
  bucket becomes a `Cluster { key, count, kind }` (its member count and its DOMINANT member kind, for
  colour), every currently-valid (`valid_to IS NULL`) edge whose endpoints fall in two DIFFERENT
  clusters adds weight to a symmetric `ClusterEdge { from, to, weight }`, and `total` carries the full
  node count so the panel can say "N nodes in M clusters". Bounded by the module/kind count, not the
  node count, so it renders at any graph size.
- **`cluster_detail(graph, key) -> Neighborhood`** drills a cluster to its members: the nodes whose
  `cluster_key` equals `key`, the currently-valid edges among them, each node carrying its
  intra-cluster degree and god-node flag. A cluster at or under `CLUSTER_RENDER_BUDGET` renders whole;
  a bigger one (e.g. a `src` module of 1000+ code entities) keeps only its highest-degree members -
  the hubs worth seeing - ties broken by id for a stable pick across polls, and sets
  `Neighborhood::truncated = Some(total)` so the panel can say "showing the N most-connected of M".
  It reuses spec 30's `Neighborhood` shape so the same renderer draws it.
- **`Neighborhood`** gains an optional `truncated: Option<usize>` (omitted when the node set is
  complete; only `cluster_detail` ever sets it). New serializable carriers `Cluster`, `ClusterEdge`,
  `ClusterOverview`, and the constant `CLUSTER_RENDER_BUDGET` (the drill render cap).
- **Route.** `GET /api/graph` gains a `cluster=<key>` parameter and a no-argument default: `cluster=`
  returns `cluster_detail`; an empty `seed` with no `cluster` returns `clustered_overview` (the new
  DEFAULT KG view); a non-empty `seed` returns the spec-30 seeded neighborhood UNCHANGED. One route,
  three views, selected by parameter.

### Visual layer (`src/dash.html`)

A library-free, JS-driven, SVG-rendered viz fills the KG panel. It computes node positions with a
force layout and emits `<circle>`/`<line>`/`<text>` from them (what a graph library does, minus the
library, honoring the dash charter):

- **Deterministic layout.** A Fruchterman-Reingold force layout seeded on a spiral (NO `Math.random`)
  so the same graph lays out identically every poll. One connected graph is force-laid then stretched
  on x to fill the wide, short panel; a disconnected graph (a drilled cluster's separate files) is
  laid out per connected COMPONENT (union-find) and shelf-PACKED so the pieces tile the panel instead
  of flinging to the corners. The layout reads the panel's live aspect ratio so it fills whatever size
  the responsive shell (spec 30) gives it.
- **Overview render.** Each cluster is a super-node sized by `sqrt(count)`, coloured by its dominant
  kind, labelled `key (count)`; each inter-cluster edge's thickness scales with its weight. A toolbar
  line reports "N nodes in M clusters - click a cluster to drill in".
- **Drill render.** A cluster's member nodes, each clickable to SEED spec 30's neighborhood
  (`data-seed`), god-nodes emphasized; a "<- overview" back link; the truncated caption when capped.
- **Pan + zoom.** Scroll zooms toward the cursor, drag pans, transforming a `#kgzoom` group; a fresh
  render (drill / back) rebinds and resets the view. Drag handlers live on `window` (installed once)
  so a drag that leaves the svg still tracks.
- **Wiring that survives re-render.** The existing spec-30 delegated click listeners on the stable
  `tree` and `kgpanel` containers gain `data-cluster` (drill) and `data-kgback` (overview) handling
  alongside the existing `data-seed` (seed) - so select-to-drill and select-to-seed survive the
  `innerHTML` swaps that destroy individual nodes. On load the panel defaults to the clustered
  overview (`loadKgOverview`).
- **Graceful degradation.** An empty graph renders an "empty graph" message, not an error; the static
  export (non-LIVE) says the graph is available in the live dash; a failed fetch says the graph is
  unavailable. The panel never throws.

## Global constraints

- Dash charter: ONE `include_str!` page, no CDN, no external fonts/scripts/styles, no JS build step;
  all CSS and JS inline; a strict "no network except same-origin `/api/*`" page.
- The body NEVER scrolls horizontally; the SVG fills its panel and pans/zooms WITHIN it.
- Read-only: adds NO event type and never mutates the store; it projects the existing event store and
  context graph (the same projection the run and spec 30 use).
- Determinism by construction: the projections fold over `BTreeMap`/`BTreeSet`/sorted `Vec` and the
  drill cap ranks by degree with an id tie-break; the JS layout is seeded deterministically (no
  `Math.random`), so a given graph yields one stable overview and one stable drill.
- The event log stays the source of truth; the graph is a rebuildable projection. This spec reads the
  projection only.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`. The overview/drill routes serve in
  BOTH lanes; with the KG feature off / an empty graph the panel degrades gracefully (an empty-graph
  message), never an error.
- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Spec 30 is NOT regressed: a non-empty `seed` still returns the seeded neighborhood, and the panel's
  existing select-to-seed, path, god-node, explain, and tier-filter behaviors are unchanged.

## Done when

- [ ] a test proves `cluster_key` FOLDING: a code-entity id (`<file>::<name>`), a rationale id
  (`<file>#L<n>`), and a path id (a file/doc) fold to their file's DIRECTORY; a directory-less path
  folds to the root bucket; a non-path dev-loop node (a decision / finding / agent) folds to its
  KIND; the mapping is deterministic. This criterion OWNS the cluster-key folding.
- [ ] a test proves the CLUSTERED OVERVIEW: `clustered_overview` folds a fixture graph into the
  expected clusters (each with its member count and dominant kind), adds weight only to edges that
  CROSS clusters (an intra-cluster edge adds none), counts only currently-valid edges, and reports
  the full node `total`. This criterion OWNS the overview aggregation; it does NOT own the fold key
  (criterion 1).
- [ ] a test proves the DRILL + RENDER BUDGET: `cluster_detail` on a cluster at/under
  `CLUSTER_RENDER_BUDGET` returns ALL its members with `truncated` omitted; on a cluster OVER the
  budget it returns exactly `CLUSTER_RENDER_BUDGET` members - the highest-degree ones, ties broken by
  id - with `truncated = Some(total)`, and every returned edge has both endpoints in the returned set.
  This criterion OWNS the drill projection and the budget cap; it does NOT own the overview
  (criterion 2).
- [ ] a test proves the ROUTE dispatch: `GET /api/graph` with `cluster=<key>` returns the cluster
  detail, with an empty `seed` and no `cluster` returns the clustered overview (the default view),
  and with a non-empty `seed` returns the spec-30 seeded neighborhood unchanged. This criterion OWNS
  the route dispatch; it does NOT own the projections (criteria 2-3).
- [ ] a test proves the SERVED-PAGE VIZ WIRING: the served `dash.html` contains the exploration viz
  (the force-layout and SVG-emit functions, the overview/drill renderers, the pan/zoom handlers),
  defaults the KG panel to the clustered overview on load, and its delegated `tree`/`kgpanel` click
  listener dispatches `data-cluster` to drill and `data-kgback` to the overview alongside the existing
  `data-seed`. This is a structural assertion on the served page (spec-30 style); it does NOT own the
  projections or the route. This criterion OWNS the served-page wiring.
- [ ] a test proves GRACEFUL DEGRADATION: the overview route over an EMPTY graph returns a well-formed
  empty overview (zero clusters, zero total) rather than an error, in BOTH feature lanes, so the panel
  can render its empty-graph message. This criterion OWNS the empty/degraded path; it does NOT own the
  populated projections (criteria 2-3).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
