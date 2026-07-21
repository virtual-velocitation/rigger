# 28 - Project-scoping enforcement on the context graph

**Goal:** make project isolation an ENFORCED invariant of the context graph itself, not an
incidental consequence of separate `graph.db` files - so a single shared graph backend can hold
many projects without their nodes/edges ever mixing. This is the section 2.2 prerequisite of the
context-management addendum: it must land BEFORE the unified knowledge graph (specs 29a-c) moves
the graph off per-project-local storage onto a shared backend.

## Design

Today project isolation on the graph is purely physical: the `Namespaced` decorator
(`src/eventstore/namespace.rs`) prefixes event STREAMS with `proj-<id>-`, but the graph nodes and
edges carry NO project field - the schema is `nodes(id, kind, attrs)` and `edges(from_id, to_id,
rel, valid_from, valid_to, source)` (`src/contextgraph/sqlite.rs`). Two projects stay separate only
because each has its own `graph.db`. A shared backend would mix them. This spec adds the missing
in-graph state.

- **Tag on write.** Every node and edge carries a `project` scope derived from the SAME identity
  `Namespaced::new` uses to build the `proj-<id>-` prefix (a plain project string). The fold path
  (`Projection::apply` -> `fold`, `src/contextgraph/sqlite.rs`) stamps it on insert.
- **Filter on read.** `subgraph(seed, depth)` and every read the conductor uses (`graph_context`,
  `src/conductor.rs`) filter to the current project, so a seed id that exists in two projects
  returns only the current project's neighborhood. This mirrors, for the graph, what
  `Namespaced::scope_filter` does for streams.
- **Scope the prune.** `Projector::prune` (the `reset --runs` authority) deletes only the current
  project's nodes; another project's nodes are never touched.
- **Rebuildable under scope.** Because the graph is a projection (section 2.1), rebuilding from a log that
  carries two projects' events re-derives two correctly-scoped subgraphs - the project tag is
  re-derived on every fold, not stored as a mutable side fact.
- **Re-key on identity adoption.** A project may run first under its directory basename and later
  adopt a durable identity (`.rigger/project.id`): its event streams are renamed from the basename
  namespace to the minted one. The graph folds INCREMENTALLY, so the renamed streams are never
  re-folded and its pre-adoption nodes/edges keep the basename scope. So that the read filter above
  keeps returning that history (and the deployment reads exactly as before adoption), the identity
  migration re-scopes the graph rows the SAME way it renames the streams - the graph analog of the
  stream-prefix rename, keyed on the SAME identity, so it stays one source of truth for project
  scope, never a second. The read filter and this re-key are one concern: neither is correct
  without the other, so they land together.

This is net-new state: there is nothing to "tighten," so the work is additive and its whole value
is proven by a TWO-PROJECT fixture against ONE shared backend.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: any project-keyed collection uses `BTreeMap`/`BTreeSet`/sorted
  `Vec`.
- The event log stays the source of truth; the graph is a rebuildable projection. The project tag
  is DERIVED on fold from the same identity that namespaces the streams - never a second source of
  truth for project identity.
- Backward-compatible: a single-project deployment (one `graph.db`) behaves exactly as before; the
  new scoping is transparent until a shared backend holds more than one project.

## Done when

- [ ] a test proves every node and edge carries a PROJECT scope on fold, derived from the same
  project identity `Namespaced` uses for `proj-<id>-` stream prefixing (a fold of the same events
  under project P tags all resulting nodes/edges with P). This criterion OWNS the project tag as
  net-new node/edge state written on fold.
- [ ] a test proves READ isolation on a SHARED backend: one graph store holding two projects' folds
  returns, via `subgraph`/`graph_context`, ONLY the current project's nodes - even when both
  projects contain a node with the same seed id. This criterion OWNS read/traversal project-
  isolation; it does NOT own the write tag (criterion 1). Because the read filter is what makes an
  incrementally-folded pre-adoption row unreachable after an identity adoption re-keys its streams,
  this criterion ALSO owns the graph re-key that keeps the read backward-compatible (a test proves a
  single-project deployment that adopts a durable identity still reads its pre-adoption history):
  the read filter and its re-key are one concern and land together.
- [ ] a test proves `reset --runs` / `Projector::prune` is PROJECT-SCOPED on the shared backend:
  pruning project P's dead-run nodes leaves project Q's nodes fully intact. This criterion OWNS
  prune project-scoping; it does NOT own read-isolation or the write tag (criteria 1-2).
- [ ] a test proves the projection stays REBUILDABLE under scope: rebuilding the graph from a
  two-project log re-derives two correctly-scoped subgraphs (each project sees only its own nodes),
  with no manual backfill. This criterion OWNS rebuild-under-scope; it does NOT own the write tag,
  read-isolation, or prune (criteria 1-3).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
