# 30 - Dash redesign: responsive shell, the run tree, and the unified-KG detail panel

**Goal:** rebuild the always-on `rigger dash` (`src/dash.html` + `src/dash.rs`) as a responsive,
self-contained observability surface whose SPINE is the unit -> stage -> role -> agent tree, and
whose unified knowledge graph (section 6) renders as the DETAIL VIEW of the selected node. This
replaces today's fixed-1200px, left-aligned, flat-panel dash whose id cells render one char per line,
and it lands the KG's inspectability in the SAME coherent page - selecting a unit in the tree drives
the graph panel, so an operator drills from "what is running" straight into "what knowledge governs
it". The dash stays a read-only projection over the existing event store and context graph.

## Design

The dash is one `include_str!` page (`dash.html`) served by `dash.rs`, which polls the event store,
projects it, and renders. The redesign is a projection + template change; it adds no event type and
no external dependency (the dash charter: one self-contained page, no CDN, no JS toolchain, all CSS
and JS inline).

- **Responsive shell.** `main` drops its `max-width: 1200px` cap and becomes a width-reflowing grid
  (`repeat(auto-fit, minmax(<sane>, 1fr))`) that fills the viewport and collapses to one column when
  narrow - no dead space, no cap, and the body never scrolls horizontally (wide content scrolls or
  wraps inside its own cell). The left region is the live SPINE (tree + selected node's KG
  neighborhood); a right rail holds the supporting panels (needs-you, metrics, event feed,
  decisions) that do not need the tree's width.
- **Cells fit or wrap.** Ids and content size to their content or wrap at hyphens; the wide cells
  (event JSON, the agent doing-line) wrap or scroll WITHIN their cell via an `overflow-x:auto`
  container - never char-by-char, never forcing a page-level horizontal scrollbar.
- **The run tree is the spine.** `dash.rs` projects the run's events into a
  spec -> unit -> stage (Implement/Gates/Review/Integrate) -> role
  (implementer/lens/adversary/adjudicator/integrator) -> agent (e.g. sdet/arch/attempt#N) tree. It
  subsumes today's in-flight-units, live-agent-activity, and swimlane panels into one navigable
  thing: single-child levels auto-collapse, the path to whatever is RUNNING auto-expands, and the
  step couriers collapse to a single "driver" line. Each node carries its live status
  (building/reviewing/integrated/reject-recurrence) so the tree is the at-a-glance run state.
- **Decisions: progressive disclosure.** Decision history renders each entry as a native
  `<details>` - a one-line `<summary>` preview (id + summary) that expands to the full reasoning on
  click. No framework, no inline multi-KB dumps.
- **The KG panel is the detail of the selected node.** `dash.rs` adds a read-only
  `GET /api/graph?seed=<node>&depth=<n>&tier=<filter>` route that returns the seeded neighborhood
  (nodes + edges, each tagged with its confidence tier) as self-contained JSON. Selecting a tree
  node (or a graph node) sets the seed - there is NO hand-seeding; the panel always shows the
  neighborhood of what is selected. The panel renders that neighborhood inline (nodes carry their
  own labels/kinds/tiers), highlights the query PATH between two selected nodes, flags GOD-NODES
  (high-degree hubs), answers `explain(node)` with the node's provenance (the events/decisions that
  produced it), and offers confidence-TIER filters (EXTRACTED / INFERRED / AMBIGUOUS) that toggle
  edge visibility.

## Global constraints

- Self-contained (dash charter): ONE `include_str!` page, no CDN, no external fonts/scripts/styles,
  no JS build step; all CSS and JS inline. A strict "no network except same-origin `/api/*`" page.
- The body NEVER scrolls horizontally; any intentionally-wide content scrolls inside its own
  `overflow-x:auto` container.
- Read-only: the dash adds NO event type and never mutates the store; it projects the existing event
  store and context graph (the same projection the run uses).
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`. The dash + `/api/graph` route
  serve in BOTH lanes; with the KG feature off / an empty graph the panel degrades gracefully (an
  empty-neighborhood message), never an error.
- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.

## Done when

- [ ] a test proves the RESPONSIVE SHELL: the served page's layout has no fixed `max-width` cap on
  the content region and uses a width-reflowing grid (fills wide, one column narrow), with a
  page-level `overflow-x` backstop so the body can never scroll horizontally. This criterion OWNS the
  responsive layout shell.
- [ ] a test proves CELLS FIT OR WRAP: id and long-text cells carry the wrap/size-to-content styling
  (wrap at hyphens / `overflow-wrap`) and the wide cells (event JSON, agent doing-line) are inside an
  in-cell scroll/wrap container, so nothing renders char-by-char and no cell forces a page-level
  horizontal scrollbar. This criterion OWNS cell fit/wrap; it does NOT own the shell (criterion 1).
- [ ] a test proves the RUN TREE projection: `dash.rs` projects a run's events into a
  spec -> unit -> stage -> role -> agent tree with correct nesting; single-child levels are marked
  auto-collapse, the running path is marked auto-expand, and step couriers collapse to one "driver"
  line; each node carries its live status. This criterion OWNS the tree projection (the spine).
- [ ] a test proves DECISION PREVIEW/EXPAND: each decision renders as a native `<details>` with a
  one-line `<summary>` preview (id + summary) and the full reasoning in the expandable body. This
  criterion OWNS progressive disclosure; it does NOT own the tree (criterion 3).
- [ ] a test proves the KG ROUTE + SEEDED NEIGHBORHOOD: `GET /api/graph?seed=&depth=&tier=` returns
  the seeded neighborhood (nodes + tier-tagged edges) as self-contained JSON for a given seed node,
  and the served panel is wired so selecting a tree/graph node sets that seed (no hand-seeding). This
  criterion OWNS the graph route and select-to-seed.
- [ ] a test proves QUERY-PATH + GOD-NODES: the graph projection computes the path between two
  selected nodes and flags god-nodes (degree above a threshold) in the returned neighborhood. This
  criterion OWNS path + god-node analysis; it does NOT own the route (criterion 5).
- [ ] a test proves EXPLAIN + TIER FILTERS: `explain(<node>)` returns the node's provenance (the
  events/decisions that produced it), and the neighborhood's edges are partitioned by confidence
  tier so a tier filter (EXTRACTED / INFERRED / AMBIGUOUS) toggles their visibility. This criterion
  OWNS explain + tier filtering; it does NOT own the route or path analysis (criteria 5-6).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
