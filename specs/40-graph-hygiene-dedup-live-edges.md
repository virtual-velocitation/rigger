# 40 - Graph hygiene: at most one live edge per relationship (fold dedup)

**Goal:** stop the context graph from accumulating duplicate live edges. `add_edge`
(`src/contextgraph/sqlite.rs`) is a bare `INSERT ... valid_to = NULL`, so every fold that
re-asserts a relationship appends another identical live row. Measured on the live `.rigger/graph.db`:
**60% of the live graph is redundant** - 39,340 of 65,415 live edges are exact duplicates, e.g.
`rust-engineer --TOUCHES--> src/conductor.rs` holds **45 identical live rows** because every
`FileTouched` event folds another. The duplication bloats the graph, the grounding slice injected
into every prompt, and the dash. Make the fold IDEMPOTENT for live edges: at most one live edge per
`(from, rel, to, tier)` in a project; a re-assertion updates the existing live edge's provenance
instead of adding a row. The event log is untouched (every `FileTouched` stays recorded); only the
graph PROJECTION collapses to the current truth, and a rebuild from the log cleans the existing
duplicates for free.

## Design

`add_edge` (`src/contextgraph/sqlite.rs`, ~line 1141) unconditionally inserts a new
`valid_to = NULL` row. Every fold arm that re-asserts a relationship over time therefore accumulates
duplicates: `TYPE_FILE_TOUCHED` (line 452) folds `agent --TOUCHES--> file` on EVERY touch with no
supersession - the worst case; decision `GOVERNS` / `ABOUT` and any relationship re-added across runs
likewise. The structural code edges (`REFERENCES` / `CALLS` / `CONTAINS`) carry the `fresh`-batch
supersession, but it only covers a re-extracted file's own edges and does not stop non-structural or
cross-source duplication.

Make `add_edge` **UPSERT-LIVE**: before inserting, look for an existing edge with the same
`(from_id, rel, to_id, tier, project)` and `valid_to IS NULL`; if one exists, UPDATE it to record the
latest assertion (bump `source`, keep the earliest `valid_from`) and do NOT insert a second row;
otherwise INSERT as today. This is ONE localized change that dedups every fold arm at once -
`TOUCHES`, `GOVERNS`, `ABOUT`, and the structural edges - without touching a single fold-arm call
site.

Supersession is unaffected: an edge whose `valid_to` is set is NOT live, so a later re-assertion of
the same relationship correctly inserts a NEW live edge (the dedup keys on live edges only, so it
never suppresses a legitimate re-assert after invalidation). The existing 39,340 duplicates are
collapsed by a rebuild: because the projection is rebuildable from the log (spec 29a) and `apply` is
idempotent per position, re-folding the log into a fresh projection with the upsert-live `add_edge`
yields at most one live edge per relationship - the operational cleanup is a fresh graph rebuild.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: the upsert-live lookup is deterministic (a single live edge per key
  by construction); any serialized set uses `BTreeMap`/`BTreeSet`/sorted `Vec`.
- The event log stays the source of truth; the graph is a rebuildable projection. This spec dedups
  the PROJECTION only - it never mutates or drops a log event; a rebuild re-derives the deduped graph.
- Safe-superset preserved: dedup removes only EXACT duplicates (identical `from`/`rel`/`to`/`tier`);
  it never collapses two DISTINCT edges, so no safety consumer loses a reference it needs.

## Done when

- [ ] a test proves the FOLD DEDUP: folding the same `agent --TOUCHES--> file` assertion N times
  (N `FileTouched` events) yields exactly ONE live `TOUCHES` edge, not N, with its provenance
  reflecting the latest assertion; a DIFFERENT agent or a DIFFERENT file still folds its own distinct
  live edge. This criterion OWNS the upsert-live fold for the re-assert/TOUCHES case.
- [ ] a test proves the dedup keys on LIVE edges only: an edge that has been INVALIDATED (`valid_to`
  set - a superseded `GOVERNS`, or a re-extracted structural edge) and is then re-asserted folds a
  NEW live edge; the dedup never suppresses a legitimate re-assertion after invalidation. This
  criterion OWNS the live-only scoping; it does NOT own the TOUCHES fold (criterion 1).
- [ ] a test proves a REBUILD collapses existing duplicates: re-folding into a fresh projection a log
  that under the old bare-insert produced K identical live edges yields exactly ONE live edge per
  `(from, rel, to, tier)`. This criterion OWNS the rebuild-dedup / idempotency of the projection; it
  does NOT own the fold arm (criterion 1).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
