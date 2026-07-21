# 29b - Unified KG: design-intent docs ingested as first-class nodes

**Goal:** fold the design-intent layer into the unified graph - the reference architecture,
`architecture.md`, the addenda, load-bearing decisions, spec-shape / loop-discipline rules, and
inline `# WHY:` rationale - as first-class nodes linked to the code they design, so an agent
grounded on a subsystem reaches the INTENT behind it, not just its code and prior decisions. Second
of the three unified-KG specs (section 6); it lands the design-knowledge half of the one graph. User-
facing usage docs are deliberately OUT of scope.

## Design

A docs/design-intent extraction pass emits `DocConceptExtracted` (one per design-intent node) and
`DocLinkExtracted` (one per link) events, folded by new arms in `Projection::apply` -> `fold`
(`src/contextgraph/sqlite.rs`) into these node kinds (added alongside the existing `KIND_*` consts
in `src/contextgraph/mod.rs`):

- `design-doc` - a reference architecture / `architecture.md` / an addendum (the DESIGN-INTENT
  layer);
- `arch-decision` - a load-bearing decision / ADR / a design-intent-gaps entry;
- `handbook-rule` - a spec-shape or loop-discipline rule;
- `rationale` - a `# WHY:` / `# NOTE:` inline comment attached to a code entity.

and these edges (reusing `REL_GOVERNS`, which already exists; adding `SPECIFIES`, `CONSTRAINS`,
`references`, `explains`):

- `design-doc --SPECIFIES/DESIGNS--> code` (a doc section designs a subsystem);
- `arch-decision --CONSTRAINS--> code` (a load-bearing decision binds this code);
- `handbook-rule --GOVERNS--> code` (a rule governs this file/entity);
- `rationale --explains--> code`;
- `design-doc --references--> doc` (a markdown link / ADR citation).

**Scope boundary (deliberate).** Only design/architecture knowledge is ingested. Docs written for
END USERS (how to drive the tool) produce NO nodes - they describe usage, not design, and would add
noise to code-grounded traversal.

**The recursion is the point.** This context-management reference architecture, once ingested, is
itself a set of `design-doc` nodes `SPECIFIES`-linked to the very code it specifies - so the RA
becomes queryable, and an agent editing a subsystem reaches the RA section that designed it and the
load-bearing decision that constrains it. This directly attacks the design-intent-blind failure
class (the rule-7 / plan-critique escalations were "the agent did not know the governing rule").

Nodes/edges carry the project scope from spec 28. Depends on 28; independent of 29a (different node
kinds) but sequenced after it. Does NOT change `graph_context` (that is 29c).

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: extraction emits in sorted order; folded sets use `BTreeMap`/
  `BTreeSet`; identical docs yield byte-identical nodes/edges.
- The event log stays the source of truth; the design-intent graph is a rebuildable projection.
  Supersede-not-delete: re-ingesting a changed doc sets `valid_to` on stale edges, never deletes.

## Done when

- [ ] a test proves a design-intent extraction pass EMITS events that the fold turns into
  `design-doc` / `arch-decision` / `handbook-rule` / `rationale` nodes: a reference-architecture doc
  becomes design-doc nodes, a load-bearing decision an arch-decision node, a spec-shape rule a
  handbook-rule node, and a `# WHY:` comment a rationale node. This criterion OWNS the design-intent
  node kinds and their fold arms.
- [ ] a test proves the design-intent EDGES land: `design-doc --SPECIFIES--> code`,
  `arch-decision --CONSTRAINS--> code`, `handbook-rule --GOVERNS--> code` (reusing `REL_GOVERNS`),
  `rationale --explains--> code`, and `design-doc --references--> doc`. This criterion OWNS the
  design-intent edge relations; it does NOT own the nodes (criterion 1).
- [ ] a test proves the SCOPE boundary: design/architecture docs (RA, `architecture.md`, addenda,
  load-bearing decisions, inline rationale) are ingested and produce nodes, while a user-facing
  usage doc produces NO nodes. This criterion OWNS the design-intent-only scope; it does NOT own
  node or edge folding (criteria 1-2).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
