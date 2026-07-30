# 43 - De-noise the knowledge graph: model the target project, not the harness

**Goal:** the context graph's projection models the TARGET PROJECT - its code, its design, and the
decisions/findings/lessons about that code - not the loop's own run machinery. Stop folding the
harness's agent, unit, and gate NODES and the agent-touched-file edges into the graph, so every
consumer that reads the graph (the dash inspector, grounding, blast-radius) sees the project, not the
bookkeeping. The machinery is not lost: the event log keeps every event, and the dash run-tree projects
units/stages/gates straight from the log (its proper home). This is a fold-level SCOPING change - what
the projection includes - never a mutation or drop of a log event, and never a removal of the content
or lifecycle those same events also carry.

The self-hosted graph shows this worst (`rust-engineer`, `gate`, `unit` render as top-level super-nodes
above the actual code), but the principle is general: on any project the loop builds, its run machinery
is noise on top of the code the graph is meant to be about.

## Design

The fold (`fold`/`apply` in `src/contextgraph/sqlite.rs`) turns run events into graph nodes and edges.
Several arms create machinery: `TYPE_FILE_TOUCHED` (~503) makes an `agent --TOUCHES--> file`
node-and-edge; `TYPE_GATE_VERDICT` (~521) makes a `KIND_GATE` node; `TYPE_UNIT_STARTED` (~545) and
`TYPE_UNIT_INTEGRATED` (~583) make `KIND_UNIT` nodes; and `TYPE_DECISION_MADE` (~454) /
`TYPE_REVIEW_FINDING` (~668) each `ensure_node(actor, KIND_AGENT)` for the persona that acted. De-noise
each arm at the node/edge level while preserving everything else it does:

- **Drop the machinery nodes and edges.** No fold produces a `KIND_AGENT`, `KIND_UNIT`, or `KIND_GATE`
  node, no `REL_TOUCHES` edge, and no agent-attribution edge (the `RAISED` agent-to-finding edge). The
  `TYPE_FILE_TOUCHED` and `TYPE_GATE_VERDICT` arms become graph no-ops (their events stay in the log,
  read by metrics and the run-tree).
- **Keep the content.** The `TYPE_DECISION_MADE` / `TYPE_REVIEW_FINDING` / `TYPE_LESSON_LEARNED` arms
  still create the `KIND_DECISION` / `KIND_FINDING` / `KIND_LESSON` content node and its
  `GOVERNS` / `ABOUT` / `explains` edges to the code and design it concerns. Only the agent attribution
  (the `KIND_AGENT` actor node and the `RAISED` edge) drops. The content is the target project's design
  memory and feeds the rationale overlay.
- **Keep the lifecycle.** The `TYPE_UNIT_INTEGRATED` arm no longer creates a `KIND_UNIT` node, but it
  STILL performs disposition-expiry (spec 25): invalidating the upheld findings that the integrating
  unit owns. That finding-invalidation reads the finding's `$.unit` attribute (a string token, not a
  `KIND_UNIT` node) and must be unaffected. Any other content or lifecycle side-effect an arm performs
  is preserved; only its machinery node/edge creation is removed.
- **Re-point the run-tree seed.** `graph_seeds` (`src/dash.rs` ~1663) enumerates unit/decision/finding
  ids to seed the dash's run-scoped pre-fetch, and the run-tree's click-to-seed seeds the graph with a
  unit id. With unit nodes gone a unit seed lands nowhere, so re-point the seed to the decisions and
  files that unit produced (which remain in the graph) - clicking a unit in the run-tree still lands on
  a real, non-empty neighborhood.

The change is safe because no functional consumer reads the dropped nodes: peer-decision grounding reads
the sidecar, not the graph; metrics folds the event log (`metrics.rs::project(&[Event])`); run pruning
and run-attribution read events; blast-radius reads code edges. A full rebuild from the log re-derives
the de-noised graph, and because the change is deterministic and additive-only to the fold logic, the
existing `.rigger/graph.db` collapses to the de-noised form on its next rebuild.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The event log stays the source of truth; the graph is a rebuildable projection. This spec changes
  what the fold PROJECTS - it never mutates or drops a log event; a rebuild re-derives the de-noised
  graph.
- Determinism by construction: the fold stays deterministic; any serialized set uses ordered structures.
- Safe-superset preserved for KNOWLEDGE: dropping machinery removes only harness nodes/edges; every
  code, design, decision, finding, and lesson node and edge a consumer relies on remains.
- Project- and run-scoped: the de-noise applies to the fold uniformly; it never crosses a project.

## Done when

- [ ] a test proves the MACHINERY IS GONE: after folding a run's `FileTouched`, `UnitStarted`,
  `UnitIntegrated`, `GateVerdict`, `DecisionMade`, and `ReviewFinding` events, the projection contains
  NO `KIND_AGENT`, `KIND_UNIT`, or `KIND_GATE` node, NO `REL_TOUCHES` edge, and NO agent-to-finding
  `RAISED` edge. This criterion OWNS the machinery drop.
- [ ] a test proves the CONTENT SURVIVES: the same fold still produces the `KIND_DECISION` /
  `KIND_FINDING` / `KIND_LESSON` content nodes and their `GOVERNS` / `ABOUT` / `explains` edges to code
  and design; only the agent attribution is absent. This criterion OWNS content preservation; it does
  NOT own the machinery drop (criterion 1).
- [ ] a test proves the LIFECYCLE SURVIVES: disposition-expiry still fires - an upheld finding owned by
  a unit is invalidated when that unit's `UnitIntegrated` folds, even though no `KIND_UNIT` node is
  created. This criterion OWNS the lifecycle-preservation guarantee.
- [ ] a test proves CONSUMERS ARE UNAFFECTED: metrics, run pruning, and blast-radius produce the same
  result over a run's events before and after the de-noise (none reads the dropped nodes). This
  criterion OWNS the safe-consumer guarantee; it does NOT own content (criterion 2).
- [ ] a test proves the RUN-TREE SEED still lands: seeding the graph from a unit through the re-pointed
  `graph_seeds` returns the neighborhood of that unit's decisions and files (a non-empty, real
  neighborhood), not an empty result. This criterion OWNS the click-to-seed re-point.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
