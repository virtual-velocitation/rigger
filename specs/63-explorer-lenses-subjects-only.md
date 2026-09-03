# 63 - Explorer lenses render subjects only; files, concepts, and memory become metadata

**Goal:** the graph explorer stops mixing taxonomies on one canvas. Today the code view
renders storage buckets as nodes (a `code-entity (4280)` hub, sibling `file`/`decision`/
`finding`/`design-doc` buckets) and files as peers of their own entities - the user sees the
storage schema, not the knowledge. The accepted design (mockups:
`docs/design/kg-lens-mockups.html`, approved 2026-08-11): EACH LENS OWNS EXACTLY ONE SUBJECT
TAXONOMY; every other taxonomy is metadata on the inspected node - hover card and docked
rail, never nodes or grouping keys.

## Design

The mockup file is the visual contract for all of the below; it renders standalone.

- **Code lens** (`src/dash.html`, the `data-lens` code tab): nodes are code entities ONLY,
  shaped by kind (functions circles, types rounded squares, traits diamonds), labeled with
  the bare name. Edges typed and directed: solid `calls`, dashed structural (`implements`,
  `contains`, `creates`, `reads`), each labeled. Coupling communities are low-contrast
  tinted hulls BEHIND members with an uppercase label - a region, never a hub node. No file
  node, no bucket, no schema type name at any zoom. `view=calls` keeps its own layout.
- **Files lens**: nodes are files, sized by contained-entity count, labeled with the
  repo-relative path; edges are `uses` weighted by coupling (label carries weight);
  directories are the same hull treatment. Entities never render as nodes here.
- **Concepts lens**: nodes are concepts, sized by evidence weight; edges join concepts
  sharing evidence, labeled with the count. Entities and files stay in the card.
- **The metadata card** (all lenses): one hover-card anatomy everywhere: title row (kind dot
  + name), provenance row (file:line, degree, community), chip rows - FILE / CONCEPTS /
  MEMORY for a code subject, TOP ENTITIES for a file, TOP EVIDENCE for a concept. Hover
  highlights direct neighbors, dims the rest. Chips are the LENS HANDOFF: a chip opens its
  own taxonomy's lens with the clicked thing as subject. The card never occludes the node.
- **Subject view** (all lenses, on click): clicking focuses the node as subject - canvas
  re-seeds to its one-hop neighborhood (typed edges preserved), a breadcrumb names lens and
  subject with an escape route, and a MEMORY RAIL docks right listing the governing
  decisions/findings/concepts as scrollable cards. The rail is a panel: it never inserts
  nodes and the layout never re-flows for it.
- **Overview zoom** (code lens zoomed out): hulls collapse to COMMUNITY nodes sized by
  member count, labeled by community name, drill-down on click. Storage type names are
  never a grouping key at any zoom.
- **Data plumbing** (`src/dash.rs`, only as needed): payloads carry what the views consume -
  entity kind, typed edges, community id + label, per-file entity counts and coupling
  weights, concept evidence weights, the subject's governs-linked memory rows. Gaps close by
  extending the EXISTING view DTOs (dash stays a thin read-only adapter; projection modules
  untouched per the dash charter).

## Notes (non-criteria)

- Palette and shapes as mocked (exact values in the mockup's token block).
- Test seams, decided here: Rust view DTOs pinned by cargo tests; in-page JS behavior (lens
  purity, card anatomy, handoff, overview collapse) pinned by the node-gated runtime
  harnesses this dashboard already uses (spec-42/55/59 precedent) - node absent skips them.
- Degrade, decided here: a lens whose grain the projection lacks (no concepts, no
  communities, empty graph) renders a labeled empty state naming the populating command -
  never an error, never a blank canvas.
- Mockups use this repo's entities; the implementation derives everything from the live
  graph of any target project.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The dash charter holds: no external assets, all JS inline in `dash.html`, zero new
  dependencies, dash read-only over existing projections.
- Lens purity is total: no lens renders another taxonomy's node, and no storage schema name
  is ever user-visible as a node, hub, or group label.

## Done when

- [ ] a test proves CODE-LENS PURITY: the code-lens payload/render yields only code-entity
  nodes with kind and typed directed edges - no file nodes and no per-type bucket nodes at
  any zoom. This criterion OWNS the subjects-only rule.
- [ ] a test proves the METADATA CARD: a code subject's card carries file:line, concept
  chips, and memory counts, and a card chip resolves to a lens handoff target of the chip's
  own taxonomy carrying the chosen subject.
- [ ] a test proves the FILES LENS: file nodes sized by entity count with weighted `uses`
  edges, directory hulls, and a card listing the file's top entities - handoff MECHANICS are
  criterion 2's, NOT this one's.
- [ ] a test proves the CONCEPTS LENS: concept nodes sized by evidence weight with
  shared-evidence edges, and a card listing top evidence - handoff mechanics again
  criterion 2's, NOT this one's.
- [ ] a test proves the SUBJECT VIEW: clicking a node re-seeds to its one-hop neighborhood
  and the docked memory rail lists the subject's governing decisions/findings/concepts
  without adding nodes to the layout.
- [ ] a test proves OVERVIEW COLLAPSE: zoomed out, communities render as labeled community
  nodes sized by member count, and no storage type name appears as a node or group label.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
