# 63 - Explorer lenses render subjects only; files, concepts, and memory become metadata

**Goal:** the dashboard's graph explorer stops mixing taxonomies on one canvas. Today the code
view renders per-type storage buckets as nodes (a `code-entity (4280)` hub with every file spoked
into it, sibling buckets named `file`, `decision`, `finding`, `design-doc`) and renders files as
peers of the entities they contain - the user sees the storage schema, not the knowledge. The
accepted design (mockups: `docs/design/kg-lens-mockups.html`, approved 2026-08-11) is a
discipline applied uniformly: EACH LENS OWNS EXACTLY ONE SUBJECT TAXONOMY, and every other
taxonomy is metadata on the inspected node - in a hover card and a docked subject rail, never as
nodes or grouping keys in the layout.

## Design

The mockup file is the visual contract for all of the below; it renders standalone in a browser.

- **Code lens** (`src/dash.html`, the `data-lens` code tab): nodes are code entities ONLY,
  shaped by kind - functions/methods as circles, types as rounded squares, traits/contracts as
  diamonds - labeled with the bare entity name. Edges are typed and directed: solid for `calls`,
  dashed for structural (`implements`, `contains`, `creates`, `reads`), each with its label.
  Coupling communities render as low-contrast tinted hulls BEHIND their member nodes with an
  uppercase community label; a community is a region, never a hub node. No file node, no
  storage-type bucket, and no schema type name appears on the canvas at any zoom. The layered
  `view=calls` keeps its own existing layout.
- **Files lens** (the files tab): nodes are files, sized by contained-entity count, labeled with
  the repo-relative path; edges are `uses` weighted by coupling strength (label carries the
  weight); directories render as the same quiet hull treatment. Entities never render as nodes
  here - the file's top entities live in its metadata card.
- **Concepts lens** (the concepts tab): nodes are concepts, sized by evidence weight; edges join
  concepts sharing evidence, labeled with the shared-evidence count. Entities and files stay in
  the card.
- **The metadata card** (all lenses): hovering a node raises one card anatomy everywhere: title
  row (kind dot + name), provenance row (defining file:line, degree, community), then chip rows
  - FILE, CONCEPTS, MEMORY counts for a code subject; TOP ENTITIES for a file; TOP EVIDENCE for
  a concept. Hovering also highlights the node's direct neighbors and dims the rest. Card chips
  are the LENS HANDOFF: an entity chip opens the code lens focused on that entity, a file chip
  the files lens, a concept chip the concepts lens - the clicked thing rides along as the new
  subject. The card is positioned clear of the hovered node and never occludes it.
- **Subject view** (all lenses, on click): clicking a node focuses it as the subject - the
  canvas re-seeds to the subject's one-hop neighborhood (typed edges preserved), a breadcrumb
  names the lens and subject with an escape route back, and a MEMORY RAIL docks on the right
  listing the decisions, findings, and concepts that govern the subject (the same governs
  linkage the graph already holds), each as a card, scrollable. The rail is a panel: it never
  inserts nodes into the layout and the layout never re-flows for it.
- **Overview zoom** (code lens zoomed out): hulls collapse to COMMUNITY nodes sized by member
  count and labeled with the community name - a knowledge grouping that drills back down on
  click. Storage type names are never a grouping key at any zoom level.
- **Data plumbing** (`src/dash.rs`, only as needed): the graph payloads carry what the views
  above consume - entity kind, typed edges, community id + label, per-file entity counts and
  coupling weights, concept evidence weights, and the subject's governs-linked memory rows. Any
  gap is closed by extending the EXISTING view DTOs in `src/dash.rs` (the dash stays a thin
  read-only adapter; projection modules stay untouched per the standing dash charter).

## Notes (non-criteria)

- Node palette and shapes as mocked: functions blue, types teal, traits violet, concepts amber,
  files slate, memory pink; exact values in the mockup file's token block.
- The mockups use real entities from this repo; the implementation derives everything from the
  live graph, of any target project.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The dash charter holds: no external assets, all JS inline in `dash.html`, zero new
  dependencies, dash stays read-only over existing projections.
- Lens purity is total: no lens renders a node from another lens's taxonomy, and no storage
  schema name is ever user-visible as a node, hub, or group label.

## Done when

- [ ] a test proves CODE-LENS PURITY: the code-lens payload/render yields only code-entity
  nodes with kind and typed directed edges - no file nodes and no per-type bucket nodes at any
  zoom. This criterion OWNS the subjects-only rule.
- [ ] a test proves the METADATA CARD: a code subject's card carries file:line, concept chips,
  and memory counts, and a card chip resolves to a lens handoff target of the chip's own
  taxonomy carrying the chosen subject.
- [ ] a test proves the FILES LENS: file nodes sized by entity count with weighted `uses`
  edges, directory hulls, and a card listing the file's top entities as code-lens handoffs.
- [ ] a test proves the CONCEPTS LENS: concept nodes sized by evidence weight with
  shared-evidence edges, and a card listing top evidence as handoffs.
- [ ] a test proves the SUBJECT VIEW: clicking a node re-seeds to its one-hop neighborhood and
  the docked memory rail lists the subject's governing decisions/findings/concepts without
  adding nodes to the layout.
- [ ] a test proves OVERVIEW COLLAPSE: zoomed out, communities render as labeled community
  nodes sized by member count, and no storage type name appears as a node or group label.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
