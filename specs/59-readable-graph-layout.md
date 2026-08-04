# 59 - A readable graph: collision-free spacing and adaptive labels

**Goal:** the knowledge-graph views must be READABLE at real densities. Today the force layout
collapses into a central clump the moment an overview carries more than a few dozen nodes - at the
Concepts lens's ~56 clusters and the Code lens's ~160, nodes pile onto each other mid-panel and
their labels render on top of one another; the operator's verdict on the live views was "completely
unusable as is." Make the layout answer three requirements directly: nodes are SPACED so the
relationships (edges) can be seen; the TEXT can be read; and there is NO OVERLAP - at every density
the lenses actually produce. The dash charter holds (one self-contained page, no libraries, no
build step), and the layout stays deterministic: the same graph renders identically on every poll.

## Design

### Collision-free placement (the label is part of the node)

- After the force pass, a deterministic SEPARATION pass resolves overlaps: each node's collision
  body is its circle PLUS its label's bounding box (estimated from the label's character count and
  font metrics - the label is part of the node, so text can never sit under a neighbor). The pass
  iterates displacing overlapping pairs apart (stable order, fixed iteration bound) until no two
  bodies intersect; it is pure arithmetic over the position map, deterministic by construction.
- Spacing scales with density: repulsion and the layout extent grow with node count and total
  label area, so a 160-cluster overview spreads to the room its labels need instead of compressing
  into the panel's center. The panel pans and zooms already; a layout larger than the viewport is
  fine - clarity beats fitting.

### Adaptive labels (declutter by importance, reveal on demand)

- At the default zoom, labels render only for the nodes that MATTER at that altitude - the largest
  or highest-degree nodes (a deterministic top-K by size, ties lexicographic); the rest draw as
  unlabeled dots. Zooming IN reveals more labels as room appears (thresholds keyed to the zoom
  scale); hovering any node always shows its label immediately (a title/tooltip surface that needs
  no layout room). No label ever renders overlapping another - the visible-label set is chosen so
  their boxes are disjoint.
- Edges render beneath nodes and labels, and the weighted inter-cluster edges keep their width
  scaling - with real spacing between endpoints they become VISIBLE relationships instead of a
  hairball under a clump.

### Scope

- Applies to every force-laid view: the whole-graph overviews under all three lenses, cluster
  drills, and the seeded neighborhood. The layered call-DAG views keep their own layout (already
  readable by construction) but adopt the same label-collision rule within layers.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The dash charter holds: one self-contained page, no external assets or libraries, all JS inline.
- Determinism: the separation pass and the visible-label selection are deterministic (stable
  ordering, fixed bounds, no randomness) - the same graph and viewport produce the identical
  render on every poll.
- Read-only presentation change: no route, projection, or data shape changes; the server-side
  bodies are untouched except any label-length metadata the client already receives.

## Done when

- [ ] a test proves NO OVERLAP AT DENSITY: running the layout (force + separation) over synthetic
  overviews at the real densities (at least 60 and 160 nodes with realistic label lengths) yields
  positions where NO two collision bodies (circle + label box) intersect, deterministically across
  two runs. This criterion OWNS the separation pass.
- [ ] a test proves DENSITY-SCALED SPACING: the layout extent grows with node count and label area
  (the 160-node overview occupies a strictly larger extent than the 60-node one), and every edge's
  endpoints are separated by at least the sum of their node radii (edges are drawable as visible
  relationships). This criterion OWNS the spacing rule.
- [ ] a test proves ADAPTIVE LABELS: at default zoom only the deterministic top-K labels are
  visible and their boxes are pairwise disjoint; a deeper zoom threshold admits more labels; the
  hover surface carries every node's label regardless of visibility. This criterion OWNS the
  declutter behavior.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
