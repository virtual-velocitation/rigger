# 12 - Wave 3: verify only what changed

**Goal:** stop re-earning verdicts the log already holds: content-addressed gate caching, staleness-driven re-verification, blast-radius gate selection, and compensation for integrated-but-wrong units. Program Wave 3.

## Design

**Unit 1 - content-addressed gate verdicts.** Every `GateVerdict` gains `input_digest = hash(gate command + git tree-SHA of the gate's input paths [default: whole tree])` as metadata. Before spending a gate run, the conductor consults the log: a prior GREEN verdict with the same `(command, input_digest)` answers the gate as a logged cache-hit (an annotated verdict citing the prior event's position - provenance, not silence). Failures are never cache-answered (a red must re-prove). Owns: verdict addressing and hit semantics. Exclusion: which gates run is unit 3's; when cached verdicts die is unit 2's.

**Unit 2 - staleness propagation, after unit 1.** On `UnitIntegrated`, downstream units whose gate inputs (or blast radius) intersect the integrated unit's touched files are marked stale (metadata event on the existing vocabulary): their cached verdicts stop hitting and their next lifecycle step re-gates. Exactly the transitively-affected set re-verifies; everything else's green stands. Owns: cache invalidation. Exclusion: it never re-opens review verdicts - only gate verdicts.

**Unit 3 - blast-radius gate selection, after unit 1.** Gates accept optional `inputs: [globs]`. During implement/remediate iterations, only gates whose inputs intersect the unit's blast radius run; skipped gates are logged with the reason (never silent). The integrate step ALWAYS runs the full gate library - "done" is asserted only against the exhaustive suite (R6 preserved at the point that matters). Owns: inner-loop selection. Exclusion: integrate-time behavior is deliberately untouched.

**Unit 4 - compensation for integrated-but-wrong units, after unit 2.** When a later unit's work proves an integrated unit wrong (the adjudicator names a prior unit's integrating commit as the defect source, or a stale re-gate goes red on an integrated unit), the conductor records `UnitCompensated{commit}` metadata (existing vocabulary), reverts the integrating commit(s) in reverse integration order on the run branch, and re-enters the unit into remediation with the contradiction as feedback. The one-way integrate door gets a principled, evented reverse gear. Owns: post-integration rollback. Exclusion: pre-integration remediation is unchanged.

## Global constraints

- Hyphens, not em dashes. New event types: NONE (digests, staleness marks, and compensation ride as metadata on existing types). Cache hits, skips, staleness, and compensations are ALWAYS logged with provenance - no silent shortcuts. Both lanes green; replay determinism preserved (digest computation is pure over tree state).

## Done when

- [ ] gate verdicts carry an input digest and a prior green verdict with a matching digest answers the gate as a logged cache-hit citing the prior position (failures never cache) - pinned by tests covering hit, miss-on-content-change, and red-never-cached
- [ ] integrating a unit marks exactly the downstream units whose inputs intersect its touched files as stale, their cached verdicts stop hitting, and unaffected units' greens stand - pinned with a three-unit dependency fixture
- [ ] gates with `inputs:` globs are selected by blast-radius intersection during the inner loop with skips logged, while integrate always runs the full library - pinned including the skip-logging and the exhaustive-integrate assertions
- [ ] a contradiction against an integrated unit records compensation metadata, reverts the integrating commits in reverse order on the run branch, and re-enters the unit into remediation with the contradiction as feedback - pinned end-to-end with a seeded two-unit contradiction
