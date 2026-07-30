# 53 - The Code lens: coupling communities as an event-sourced derivation

**Goal:** the inspector's middle altitude - the CODE lens - groups the graph by how the code actually
WORKS TOGETHER, not where it sits on disk. A coupling community is a set of code entities and files
that call and reference each other densely (a conductor function, the grounder modules it drives, the
projection fold they feed) regardless of directory; it is the "subsystem" a maintainer holds in their
head. Today no such grouping exists: the only clustering is the directory fold, which restates the
folder tree. This spec adds the OFFLINE, DETERMINISTIC community-detection derivation over the code
coupling layer - recorded as events, folded as membership edges, per the knowledge-graph disciplines -
and the `lens=code` view that buckets the existing overview/drill machinery by it. Communities are
DERIVED, never computed at request time (request-time clustering would jitter across polls and break
the rebuildable-projection invariant).

## Design

### The derivation: `rigger graph communities` (offline pass)

A new subcommand runs community detection over the projection's CODE COUPLING layer and records the
result as events:

- **Input layer:** the live `CALLS` / `REFERENCES` / `CONTAINS` edges among code entities and files
  (the structure layer), project-scoped, resolvable tiers only. Undirected, weighted by edge
  multiplicity between the same pair.
- **Algorithm:** modularity-based community detection (local moving with a refinement pass so every
  community is internally CONNECTED), parameterized by a `--resolution <r>` grain knob (default 1.0:
  higher resolution, more and smaller communities). DETERMINISM is a hard requirement, not a wish:
  node visit order is the sorted node-id order, ties in modularity gain break to the
  lexicographically-smallest target community, and no randomness is used anywhere - the same graph at
  the same resolution yields byte-identical assignments on every run and every machine.
- **Recording:** the pass emits one `CommunityAssigned` event per member (node id, community id
  `community/<resolution>/<n>`, resolution, the pass's content hash) and the fold projects each as a
  `<node> --IN_COMMUNITY--> community/<resolution>/<n>` membership edge plus the community node
  itself. A RE-RUN at the same resolution supersedes the prior assignments (the existing
  supersession discipline: old membership edges get `valid_to`, new ones fold live), so the lens
  always reads one live assignment set per resolution. Different resolutions coexist (distinct
  community ids), so the grain knob does not destroy other grains.
- **Labels:** a community node's display label is its highest-degree member's label, ties broken
  lexicographically (the dominant-kind tie-break discipline the overview already uses) - a
  deterministic label always exists; nothing waits on a model.

### The lens: `lens=code` (`src/dash.rs` + `src/dash.html`)

- The overview/drill bucket key becomes PLUGGABLE: `lens=files` (the default today's directory fold,
  unchanged and byte-identical when `lens` is absent) or `lens=code`, which buckets every node
  carrying a live `IN_COMMUNITY` membership by its community, renders community super-nodes sized by
  member count and weighted inter-community edges - the SAME `clustered_overview`/`cluster_detail`
  folds, a different key. Nodes with NO membership (design docs, decisions, knowledge nodes - the
  detection runs over code) keep their kind buckets alongside, so the view stays whole-graph.
- `resolution=<r>` selects which derived grain to read (default the default-resolution assignment);
  a resolution with no derived assignments - or a graph where the pass has never run - renders the
  documented "code lens not derived yet - run `rigger graph communities`" empty state, never an
  error.
- Drill and select-to-seed work identically under the lens (a community drills to its members; a
  member click seeds the neighborhood / call views), and the toolbar carries the lens selector with
  the resolution control visible under `lens=code`.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The event log stays the source of truth: the derivation is EVENT-SOURCED (`CommunityAssigned`
  events; the graph rows are a rebuildable fold of them); a full rebuild re-derives identical
  membership edges from the log without re-running detection.
- Determinism by construction: sorted visit order, lexicographic tie-breaks, no randomness; the same
  tree and resolution reproduce byte-identical assignments. Serialized sets use ordered structures.
- The lens is read-only and additive: `lens` absent keeps every existing view byte-identical; the
  detection pass is the only writer and writes only its own events.
- Project-scoped: detection, membership, and the lens never cross a project.

## Done when

- [ ] a test proves DETERMINISTIC DETECTION: on a fixture coupling graph, the pass yields the
  expected communities (densely-coupled entities grouped across directory lines), every community is
  internally connected, and two runs (and a rebuild from the recorded events) produce byte-identical
  assignments. This criterion OWNS the algorithm's determinism and connectedness.
- [ ] a test proves the RESOLUTION KNOB: a higher resolution yields at least as many communities on
  the fixture, distinct resolutions coexist as distinct live assignment sets, and a re-run at one
  resolution supersedes only that resolution's prior assignments. This criterion OWNS the grain and
  supersession; it does NOT own detection (criterion 1).
- [ ] a test proves the EVENT-SOURCED FOLD: each assignment is recorded as a `CommunityAssigned`
  event and folded as a live `IN_COMMUNITY` edge plus the community node with its deterministic
  label; a rebuild from the log reproduces the same rows. This criterion OWNS the recording
  discipline.
- [ ] a test proves the CODE LENS VIEW: `lens=code` buckets member nodes by community through the
  existing overview/drill folds (sized super-nodes, weighted cross-edges, drill to members),
  membership-less nodes keep kind buckets, `lens` absent is byte-identical to today, and an underived
  graph returns the documented empty state. This criterion OWNS the lens plumbing.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
