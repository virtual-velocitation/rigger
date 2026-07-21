# 25 - Disposition-expiry: invalidate resolved findings so grounding stays live

**Goal:** activate the unused bi-temporal invalidation machinery for review findings. When a
finding is RESOLVED - discarded by the adjudicator, or upheld and then addressed by the unit
integrating - fold that as an invalidation (set `valid_to` on the finding's graph edges), so the
existing `subgraph` live-filter (`valid_to IS NULL`) prunes resolved findings for free and agents
ground on LIVE findings only. This is the highest measured-ROI item in the context-management
addendum (Workstream A, section 3): the `valid_to` column exists and is used by 0% of edges today, while
the injected findings pool (48 KiB budget) is the largest grounding section and truncation is
load-bearing.

## Design

The projection already invalidates on decision supersession: the `TYPE_DECISION_MADE` fold arm,
when `supersedes` is set, runs `UPDATE edges SET valid_to = ?1 WHERE from_id = ?2 AND rel =
'GOVERNS' AND valid_to IS NULL` (`src/contextgraph/sqlite.rs`, the supersession arm). Mirror that
mechanism for findings.

A finding becomes a `KIND_FINDING` node with a `REL_RAISED` edge (from the raiser) and one
`REL_ABOUT` edge per touched file, created in the `TYPE_REVIEW_FINDING` fold arm
(`src/contextgraph/sqlite.rs`). There is NO dedicated disposition event; a finding's disposition
is the join of its attribution (`by`) with the adjudicator's result, exactly as `src/metrics.rs`
(`ReviewQuality`, `survival()`, `upheld_unattributed`) computes it:

- **Discarded** = raised in a review whose adjudicator `SpawnResult` (`TYPE_SPAWN_RESULT`, the
  adjudicator's `upheld: Vec<finding-id>`) does NOT list the finding -> invalidate when that
  adjudicator result folds.
- **Upheld-and-addressed** = listed in `upheld` AND the unit later integrates
  (`TYPE_UNIT_INTEGRATED`) -> invalidate on integration.
- **Still-open** = raised, review not yet adjudicated -> stays live (`valid_to IS NULL`).

`TYPE_SPAWN_RESULT` is not folded today (it projects to nothing); this spec adds the fold arm that
reads the adjudicator disposition and sets `valid_to` on the resolved findings' `REL_RAISED` /
`REL_ABOUT` edges, attributed to the disposing run's provenance (the `RunStarted`-boundary
attribution that `reset --runs` and the LIVE/HISTORICAL peer labels already use, spec 21).

Grounding observes the effect for free: `graph_context` (`src/conductor.rs`) builds its injected
slice from a single `graph.subgraph(seed, 2)` whose traversal already filters `valid_to IS NULL`,
then renders `write_capped_findings` under `FINDINGS_BUDGET_BYTES` (48 KiB). Resolved findings
simply stop appearing.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: any folded/serialized set uses `BTreeMap`/`BTreeSet`/sorted `Vec`.
- The event log stays the source of truth; the graph is a rebuildable projection. Supersede-not-
  delete: invalidation ONLY sets `valid_to`; it never deletes a node/edge (deletion stays the
  `reset --runs` prune authority, `Projector::prune`). A rebuild from the log re-derives the same
  invalidations.
- Load-bearing decision preserved - expiry is by DISPOSITION, not by run age: a still-open finding
  from a PRIOR run stays live and cross-run-visible. This spec must NOT scope grounding to the
  active run; it removes only RESOLVED findings.

## Done when

- [ ] a test proves that when a finding is DISCARDED (raised in a review whose adjudicator
  `SpawnResult` omits it from `upheld`), folding that adjudicator result sets `valid_to` on the
  finding's graph edges so `subgraph` no longer returns it, while a still-open finding (no
  adjudicator result yet) remains live (`valid_to IS NULL`). This criterion OWNS the discard->
  invalidation fold arm (modeled on the decision-supersession `valid_to` update).
- [ ] a test proves that an UPHELD finding is invalidated when its unit is ADDRESSED: after the
  finding is listed in `upheld` and the unit emits `TYPE_UNIT_INTEGRATED`, its edges get `valid_to`
  set and `subgraph` drops it; an upheld-but-not-yet-integrated finding stays live. This criterion
  OWNS the upheld-and-addressed invalidation trigger; it does NOT own the discard trigger
  (criterion 1).
- [ ] a test proves the invalidation is RUN-SCOPED: a disposition recorded under run A sets
  `valid_to` attributed to A, and the SAME finding re-raised under a later run B is returned LIVE by
  `subgraph` (A's disposition never suppresses a B re-raise). This criterion OWNS run-scoping of the
  invalidation; it does NOT own either invalidation trigger (criteria 1-2).
- [ ] a test proves `graph_context` reflects it end to end: for a unit whose blast-radius seed has
  both open and resolved findings, the rendered findings section (via `subgraph` + `write_capped_
  findings`) contains the open findings and omits the resolved ones. This criterion OWNS the
  grounding-slice observable effect; it does NOT own the fold or run-scoping (criteria 1-3).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
