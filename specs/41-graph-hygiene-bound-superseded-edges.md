# 41 - Graph hygiene: bound the superseded-edge accumulation

**Goal:** stop the context graph's edges table from growing without bound as runs re-extract. Every
re-extraction of a file supersedes its prior structural edges by setting `valid_to` (the `fresh`
batch boundary, spec 29a), and decision/finding supersession does the same - so superseded
(historical) rows pile up permanently and are never reclaimed. Measured on the live `.rigger/graph.db`
after the loop had run the corrective campaign: **301,156 superseded rows** vs ~32k live - the
historical cruft was ~90% of the 333k-row table and 70MB of an 83MB graph, and it made
`ingest_project_into_graph` (which folds at every step) slow enough to stall a loop run. Grounding
only ever reads the LIVE slice (`valid_to IS NULL`), so the unbounded historical tail is pure weight
on every fold and traversal. Bound it: reclaim superseded edges beyond a retention window so the
edges table tracks the LIVE structural state plus recent history, not cumulative re-extraction. The
event log is untouched (a rebuild re-derives any needed history); only the projection is pruned.

## Design

The projection accumulates superseded edges from three folds, none of which reclaims the rows it
invalidates: the `fresh`-batch structural supersession (`src/contextgraph/sqlite.rs`, spec 29a - the
dominant source, re-firing for every changed file every run), decision supersession (`GOVERNS`
invalidation on `REL_SUPERSEDES`), and disposition-expiry (finding-edge invalidation, spec 25). Today
`reset --runs` (`Projector::prune`) reclaims dead-run DECISIONS and FINDINGS but not the superseded
STRUCTURAL edges, which are the bulk.

Extend the prune authority to reclaim superseded edges (`valid_to IS NOT NULL`) beyond a retention
boundary - the run-boundary attribution `reset --runs` already uses (spec 21). A superseded edge from
a run older than the retention window is dead cruft: no live query reads it, and the log can
re-derive it. The prune:

- reclaims ONLY superseded rows (`valid_to IS NOT NULL`) older than the retention boundary; it NEVER
  touches a live edge (`valid_to IS NULL`), so grounding, blast-radius, and the two-view safe superset
  are unaffected;
- keeps recent history inside the retention window, so a still-useful "what did the last run see"
  query still resolves; the window is a bounded, not cumulative, tail;
- runs through the existing prune surface (`reset --runs` / `Projector::prune`), so it is one
  authority, not a new mutation path, and stays a projection operation that a full rebuild re-derives.

Because grounding reads live-only, the observable win is that `ingest_project_into_graph`'s fold and
`subgraph`'s traversal scan a bounded table (live + recent) instead of one that grows with every
re-extraction - keeping steps fast as the project's run history grows.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: the prune is a deterministic set operation keyed on `valid_to` + the
  retention boundary; any serialized set uses `BTreeMap`/`BTreeSet`/sorted `Vec`.
- The event log stays the source of truth; the graph is a rebuildable projection. This prunes the
  PROJECTION only - it never mutates or drops a log event; a full rebuild re-derives the history.
- LIVE is sacrosanct: the prune reclaims ONLY `valid_to IS NOT NULL` rows; a live edge is never
  removed, so no grounding or safety consumer loses a reference it needs (the safe-superset invariant).
- Project- and run-scoped (context-management addendum §2.2/§2.3): the prune reclaims only the current
  project's superseded edges, attributed by the run boundary; it never crosses a project.

## Done when

- [ ] a test proves SUPERSEDED-EDGE RECLAMATION: after a file is re-extracted across multiple runs
  (leaving superseded structural edges), a prune removes the superseded rows older than the retention
  boundary while EVERY live edge (`valid_to IS NULL`) remains. This criterion OWNS the superseded-edge
  prune.
- [ ] a test proves LIVE is untouched and grounding is unaffected: after the prune, `subgraph` over
  the live slice returns exactly the same nodes/edges it returned before (the prune removed only
  historical rows). This criterion OWNS the live-invariant guarantee; it does NOT own the prune
  mechanism (criterion 1).
- [ ] a test proves the table is BOUNDED across re-extraction: re-extracting the same file N times
  and pruning leaves the superseded-row count bounded by the retention window, NOT growing O(N) with
  re-extractions. This criterion OWNS the bounded-growth regression; it does NOT own the live
  invariant (criterion 2).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
