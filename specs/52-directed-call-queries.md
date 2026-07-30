# 52 - Directed call queries: execution path and call sites

**Goal:** the knowledge-graph inspector answers the two directed questions a person actually asks
about a function: "what does this call, transitively" (its EXECUTION PATH) and "who calls this" (its
CALL SITES) - as first-class views, honestly resolved and legibly drawn. The graph already stores
caller-attributed `CALLS` edges (`<file>::<caller> --CALLS--> <file>::<callee>`), but there is no
directed traversal over them: the only walks are undirected neighborhoods, and a cross-file call
points at a BARE placeholder node in the caller's file namespace (the definition lives elsewhere), so
a naive forward walk stops silently at the first file boundary - and on the live graph roughly a
third of cross-file calls resolve to a name with MULTIPLE definitions (`new`, `run`, `spawn`), so
following every candidate would drag whole unrelated call trees into the answer. This spec adds the
store-side directed traversal with CONSERVATIVE resolution, the route views over it, and the layered
left-to-right rendering that makes direction legible (the graph-inspector addendum, section 3).

## Design

### The traversal: `Projection::calls` (`src/contextgraph/`)

A new directed traversal beside the undirected `subgraph`:

- `calls(seed, direction, depth, tier_floor) -> Graph`-shaped result, where `direction` is DOWN
  (callees: the execution path) or UP (callers: the call sites), depth-bounded and clamped like the
  existing graph views, live edges only, project-scoped.
- **Cross-file resolution at each hop.** A hop that lands on a BARE callee node (no name attribute -
  the existing bare-node test) resolves it by the pinned name-suffix expression (the one the spec-45
  expression index serves) to the DEFINITION node(s) sharing the name. The walk auto-continues ONLY
  through unambiguous hops: a same-file extracted call, or a cross-file call whose name has EXACTLY
  ONE definition.
- **Multi-candidate hops become a FRONTIER, not a fan-out.** A name with N definitions is returned as
  a marked frontier node carrying its candidate definitions ("fans out to N candidates"); the
  traversal does NOT descend into any candidate. The client re-seeds on a chosen candidate to
  continue - the human picks, the graph never guesses.
- **Cycles yield a DAG.** Reached nodes are deduped (recursion and mutual calls terminate); each node
  carries its hop distance (layer) from the seed; an edge whose target layer is not deeper than its
  source is marked as a BACK edge (recursion) rather than duplicated.
- **Tier floor.** The walk defaults to the resolvable tiers (extracted + inferred) and excludes
  `ambiguous`; the floor is a parameter so the client can opt the unresolved tier in per-request.
  The UP direction additionally returns, as a flat non-traversed list, the file-level `REFERENCES`
  edges to the seed's name that carry no caller (imports/uses) - "referenced but not called" sites a
  who-uses-this reader cares about.

### The route (`src/dash.rs`)

`GET /api/graph` gains the call views on the seeded branch, exactly the parameter scheme the
inspector addendum fixes: `seed=<id>&view=calls&dir=down|up|both` (+ the existing `depth=`, and
`tier=` as the floor). `view` absent keeps today's neighborhood byte-identical. `dir=both` returns
callers and callees around a centered seed in one body (the "flow through this function" view). The
response reuses the `Neighborhood` shape with additive per-node fields: `layer` (hop distance),
`frontier` (multi-candidate marker with the candidate ids), and per-edge `back` (recursion marker).
It serves through the spec-45 lazy direct-projection provider, never the state poll.

### The rendering (`src/dash.html`)

A second layout behind the shared SVG emitter (the emitter already takes a position map):

- **Layered left-to-right DAG:** x by server-provided layer (seed at the left for DOWN, at the right
  for UP, centered for BOTH), within-layer ordering by a one-pass barycenter sweep (average of parent
  positions) - readable, not a full graph-drawing engine.
- **Direction is drawn:** edges carry ARROWHEADS (an SVG marker definition, which the page does not
  have today); BACK edges render as visually distinct curved return arcs.
- **Frontiers are actionable:** a frontier node renders with its "N candidates" badge and expands on
  click into its candidate list; choosing a candidate re-seeds the view on it. High fan-out within a
  layer caps at the existing render budget with a "+K more" expander.
- The call views are reachable from any code-entity node: selecting one in any existing view offers
  the two directed queries alongside the neighborhood (the delegated click wiring the views already
  share). Pan/zoom behaves as in the exploration views.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`. The call views serve in both lanes;
  an empty graph or a seed with no calls degrades to an empty view, never an error.
- Determinism by construction: candidate lists and within-layer orderings sort deterministically (by
  id); the same graph and seed yield a byte-identical response across polls.
- Honest by construction: the traversal never auto-descends a multi-candidate resolution, and a
  frontier is always visibly marked - the view may be INCOMPLETE but never confidently wrong.
- Read-only over the projection; no event type, no store write; the existing seeded-neighborhood and
  exploration views are byte-identical when the new parameters are absent.

## Done when

- [ ] a test proves the DOWN traversal: from a seed with same-file and single-candidate cross-file
  callees, `calls` returns the transitive DAG with correct per-node layers, deduped nodes under a
  cycle, and the recursive edge marked as a back edge. This criterion OWNS the directed walk and
  cycle handling.
- [ ] a test proves CONSERVATIVE RESOLUTION: a hop whose callee name has multiple definitions is
  returned as a marked frontier carrying the sorted candidate ids and is NOT descended; a
  single-definition hop IS followed. This criterion OWNS the resolution policy; it does NOT own the
  walk (criterion 1).
- [ ] a test proves the UP traversal: callers resolve through bare placeholder nodes to the seed's
  definition (the reverse name-match), transitively with layers, and the response carries the
  non-traversed "referenced but not called" list. This criterion OWNS call-sites.
- [ ] a test proves the ROUTE: `view=calls&dir=down|up|both` dispatches to the traversal through the
  lazy provider with depth/tier clamping, `dir=both` returns both sides around the seed, and absent
  `view` keeps the neighborhood byte-identical. This criterion OWNS the route dispatch.
- [ ] a test proves the RENDERING is wired: the served page carries the layered layout, the SVG
  arrowhead marker definition, distinct back-edge rendering, and the frontier expand-and-reseed
  wiring (structural assertions on the served page, as the exploration viz tests do). This criterion
  OWNS the presentation seam.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
