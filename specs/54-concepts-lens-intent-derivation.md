# 54 - The Concepts lens: intent-layer concept derivation

**Goal:** the inspector's top altitude - the CONCEPTS lens - shows what the project is ABOUT: ideas
like "the knowledge graph" or "review adjudication" that span many files and many documents, with the
code and docs that realize each idea grouped under it. Concepts cannot be derived from ids or
directories; they live in the INTENT layer - the design docs, handbook rules, and specs that govern
and specify code - and in how that layer attaches to the code. This spec adds the deterministic,
event-sourced concept derivation over the intent layer, the `REALIZES` membership it records, and the
`lens=concepts` view. It follows the Code lens's exact disciplines (offline pass, recorded events,
superseding re-runs, pluggable bucket key); what differs is the INPUT LAYER (intent edges, not call
coupling) and the membership relation (`REALIZES`). The derivation is deterministic end to end; a
model-assisted refinement (splitting a multi-concept document, sharpening a name) can LATER enrich
the same recorded shapes through the existing emit surface, but nothing in this spec waits on a
model.

## Design

### The derivation: `rigger graph concepts` (offline pass)

- **Input layer:** the live INTENT edges - `GOVERNS`, `SPECIFIES`, `CONSTRAINS`, the rationale
  `explains`, and doc-to-doc references - among design docs, handbook rules, spec docs, rationale
  nodes, AND the code entities/files those edges attach to. Project-scoped, undirected for grouping.
- **Grouping:** the same deterministic community detection the Code lens ships (sorted visit order,
  lexicographic tie-breaks, connected communities, `--resolution` knob) run over THIS layer - so a
  concept is a connected region of intent: the docs that describe an idea plus the code they govern,
  regardless of directory. Code-only nodes with no intent edge are NOT forced into a concept (a
  concept is an idea, not a bucket of leftovers).
- **Recording:** one `ConceptDerived` event per concept (id `concept/<resolution>/<n>`, its label,
  the pass content hash) and one `ConceptRealized` event per member, folded as the concept node plus
  `<member> --REALIZES--> <concept>` membership edges (per the inspector addendum's membership
  direction: the concept is realized BY its members; the fold may record the edge in whichever
  direction the projection queries best, but ONE direction, consistently). Re-runs supersede per
  resolution exactly as the Code lens does.
- **Labels, deterministically:** a concept's label is the TITLE of its most-central document member
  (highest intent-degree, ties lexicographic) - a doc title is the closest thing the graph has to a
  human name for an idea. A concept with no doc member (possible at high resolution) falls back to
  its most-central member's label.

### The lens: `lens=concepts` (`src/dash.rs` + `src/dash.html`)

- The pluggable bucket key gains `concepts`: nodes carrying live `REALIZES` membership bucket by
  their concept; the overview shows concept super-nodes (sized by member count, weighted
  inter-concept edges - shared members and cross-concept intent edges); drill shows a concept's
  members (docs and code together - the idea's whole footprint). Members of MULTIPLE concepts appear
  under their primary bucket (largest membership, ties lexicographic) and carry a `shared` marker -
  flagged, never silently duplicated.
- Nodes in NO concept (unattached code, runtime knowledge) group under their kind buckets alongside,
  so the view remains whole-graph; the subject-by-lens rule for them is the documented "not part of
  any concept" state, not an invented assignment.
- `resolution=` selects the derived grain; an underived graph renders the documented "concepts not
  derived yet - run `rigger graph concepts`" empty state, never an error. `lens` absent stays
  byte-identical to today. Drill, select-to-seed, and the call views compose unchanged beneath the
  lens.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The event log stays the source of truth: the derivation is event-sourced (`ConceptDerived` /
  `ConceptRealized`), the graph rows are a rebuildable fold, and a rebuild reproduces identical
  membership without re-running the pass.
- Determinism by construction: the grouping inherits the Code lens's deterministic detection; labels
  and primary-bucket selection tie-break lexicographically; byte-identical across runs and machines.
- Honest membership: no node is forced into a concept; multi-concept members are flagged; the lens
  never invents an assignment the derivation did not record.
- Read-only and additive at the view layer; the pass is the only writer and writes only its own
  events; `lens` absent keeps every existing view byte-identical. Project-scoped throughout.

## Done when

- [ ] a test proves CONCEPT DERIVATION: on a fixture intent layer (two docs governing disjoint code
  regions, one shared rationale), the pass derives the expected concepts - each a connected intent
  region grouping docs WITH the code they govern across directory lines - with deterministic,
  byte-identical assignments across two runs and a rebuild from the recorded events. This criterion
  OWNS the derivation.
- [ ] a test proves LABELS + NO FORCED MEMBERSHIP: each derived concept carries its most-central
  document's title as its label (lexicographic tie-break), and a code node with no intent edge
  belongs to NO concept. This criterion OWNS labeling and membership honesty.
- [ ] a test proves the CONCEPTS LENS VIEW: `lens=concepts` buckets members by concept through the
  existing overview/drill folds, a multi-concept member appears once under its primary bucket with
  the shared marker, unattached nodes keep kind buckets, and an underived graph returns the
  documented empty state while `lens` absent stays byte-identical. This criterion OWNS the lens
  plumbing.
- [ ] a test proves RE-RUN SUPERSESSION: re-deriving at the same resolution supersedes only that
  resolution's prior concept membership (old edges retired, new live), leaving other resolutions
  untouched. This criterion OWNS the lifecycle.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
