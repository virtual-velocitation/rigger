# 12 - Wave 3: verify only what changed

**Goal:** stop re-earning verdicts the log already holds: content-addressed gate caching, staleness-driven re-verification, blast-radius gate selection, and compensation for integrated-but-wrong units. Program Wave 3.

## Sequencing (required): a strict linear chain

All four units edit `src/conductor.rs` (and adjacent conductor seams). Under handbook rule 6, criteria that share a blast radius must be **one unit OR an explicitly sequenced chain** - never a concurrent split. So these units form a STRICT LINEAR CHAIN and the planner MUST propose them with the corresponding `needs` edges:

> unit 1 -> unit 2 -> unit 3 -> unit 4  (each `needs` the one before it)

Each unit grounds on the PRIOR unit's already-integrated tree, so no two ever run concurrently on `conductor.rs`, every reviewer sees a whole tree (never a half-landed sibling), and each integrate is conflict-free by construction (it builds on the prior integration, not a stale base). The plan-critique gate approves a sequenced chain sharing a blast radius; it rejects only a CONCURRENT overlap. The logical independence of the units is preserved - the sequencing is purely to serialize edits to the shared file.

## Design

**Unit 1 - content-addressed gate verdicts (chain head).** Every `GateVerdict` gains `input_digest = hash(gate command + git tree-SHA of the gate's input paths [default: whole tree])` as metadata. Before spending a gate run, the conductor consults the log: a prior GREEN verdict with the same `(command, input_digest)` answers the gate as a logged cache-hit (an annotated verdict citing the prior event's position - provenance, not silence). Failures are never cache-answered (a red must re-prove). Owns: verdict addressing and hit semantics. Exclusion: which gates run is unit 3's; when cached verdicts die is unit 2's.

**Unit 2 - staleness propagation, sequenced after unit 1.** On `UnitIntegrated`, downstream units whose gate inputs (or blast radius) intersect the integrated unit's touched files are marked stale (metadata event on the existing vocabulary): their cached verdicts stop hitting and their next lifecycle step re-gates. Exactly the transitively-affected set re-verifies; everything else's green stands. Consumes unit 1's content-addressed verdicts (invalidates them). Owns: cache invalidation. Exclusion: it never re-opens review verdicts - only gate verdicts.

**Unit 3 - blast-radius gate selection, sequenced after unit 2.** Gates accept optional `inputs: [globs]`. During implement/remediate iterations, only gates whose inputs intersect the unit's blast radius run; skipped gates are logged with the reason (never silent). The integrate step ALWAYS runs the full gate library - "done" is asserted only against the exhaustive suite (R6 preserved at the point that matters). Owns: inner-loop selection. Exclusion: integrate-time behavior is deliberately untouched.

**Unit 4 - compensation for integrated-but-wrong units, sequenced after unit 3.** When a later unit's work proves an integrated unit wrong (the adjudicator names a prior unit's integrating commit as the defect source, or a stale re-gate goes red on an integrated unit - via unit 2's staleness), the conductor records `UnitCompensated{commit}` metadata (existing vocabulary), reverts the integrating commit(s) in reverse integration order on the run branch, and re-enters the unit into remediation with the contradiction as feedback. The one-way integrate door gets a principled, evented reverse gear. Owns: post-integration rollback. Exclusion: pre-integration remediation is unchanged.

## Global constraints

- Hyphens, not em dashes. New event types: NONE (digests, staleness marks, and compensation ride as metadata on existing types). Cache hits, skips, staleness, and compensations are ALWAYS logged with provenance - no silent shortcuts. Both lanes green; replay determinism preserved (digest computation is pure over tree state).
- The four units form a strict `needs` chain (Sequencing above); a concurrent decomposition of this spec is a defect the plan-critique gate must reject.

## Done when

- [ ] as the chain head, gate verdicts carry an input digest and a prior green verdict with a matching digest answers the gate as a logged cache-hit citing the prior position (failures never cache) - pinned by tests covering hit, miss-on-content-change, and red-never-cached
- [ ] sequenced after the digest unit, integrating a unit marks exactly the downstream units whose inputs intersect its touched files as stale, their cached verdicts stop hitting, and unaffected units' greens stand - pinned with a three-unit dependency fixture
- [ ] sequenced after the staleness unit, gates with `inputs:` globs are selected by blast-radius intersection during the inner loop with skips logged, while integrate always runs the full library - pinned including the skip-logging and the exhaustive-integrate assertions
- [ ] sequenced after the selection unit, a contradiction against an integrated unit records compensation metadata, reverts the integrating commits in reverse order on the run branch, and re-enters the unit into remediation with the contradiction as feedback - pinned end-to-end with a seeded two-unit contradiction
