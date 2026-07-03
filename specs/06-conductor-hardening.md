# 06 - Conductor hardening: close the campaign-discovered gaps

**Goal:** close design-intent Gaps 11-16 - the defects the stepwise campaign itself surfaced: unscoped run history, non-deterministic worktree paths, silent breaker halts, unsurfaced residue, unbounded prompt growth, and the approval-loses-to-retry-cap race.

This spec is authored under handbook rules 6-8 (authoring-loops.md): units are consolidated by blast radius, every mitigation has exactly one owner with exclusions stated, and no criterion leaves a disposition open.

## Problem

Six gaps, all recorded with evidence in [design-intent-gaps.md](../docs/design-intent-gaps.md):

- **Gap 11.** The conductor folds the whole per-project stream as one run; the first stepwise fold resurrected two zombie units from aborted historical runs, and `rigger stats` reports all-time aggregates as one run.
- **Gap 12 (remainder).** Worktree paths are UUID-per-process; the adopt-or-prune hand-fix works, but deterministic paths would make reuse trivial and delete a whole failure class.
- **Gap 13.** A breaker halt prints as a clean fixpoint: no `BudgetExhausted` event, `{"wave":[],"done":true}`, and the driver reports success on a starved run.
- **Gap 14 (remainder d).** Residue is invisible: nothing reports leftover worktrees, orphaned build caches, or shadow stores until a disk fills.
- **Gap 15.** Prompt assembly concatenates every prior verdict verbatim; by review round 4 prompts hit 280KB and an 11-item wave hit 2.2MB.
- **Gap 16.** An adjudicator APPROVE on the final permitted attempt is recorded as `UnitFailed`/`UnitEscalated`: the retry cap fires before the verdict folds, so finished work reads as failure.

## Design

**Unit 1 - run-scoped stream (Gap 11). OWNS all run-scoping.** A run begins with a `RunStarted` event carrying a fresh run id; every unit/spawn/gate/verdict event the conductor emits carries that id in its metadata. The fold that produces ready work considers ONLY the current run's slice; prior runs remain visible as memory (decisions, findings, `rigger peers`) but can never become live work - a fresh step over a store holding stale non-terminal units from older runs parks nothing for them. `rigger stats` reports the LATEST run by default, with `--all` for the historical aggregate. Exclusions: halt semantics belong to unit 2; terminal-ordering belongs to unit 3.

**Unit 2 - loud halts (Gap 13), after unit 1.** When the spawn budget trips, the conductor records `BudgetExhausted` and `rigger step` prints a halt reason: `done` splits into converged-vs-halted (e.g. `{"wave":[],"done":true,"halted":"budget exhausted: 200/200 spawns"}`). The thin driver treats a halt as a LOUD stop (workflow failure with the reason), never a clean completion. Exclusion: what counts as terminal for a unit is unit 3's.

**Unit 3 - approval beats the retry cap (Gap 16), after unit 1.** The terminal check folds the verdict BEFORE the attempt counter: a unit whose final attempt is adjudicator-approved integrates; `max_retries` gates only STARTING another attempt. Regression test pins unit-2-the-adjudicator-persona's exact scenario (approve on attempt == cap; expect integrate, not escalate).

**Unit 4 - deterministic worktree paths (Gap 12 remainder).** A unit's worktree dir derives purely from the scratch root and the unit id (`<scratch-root>/rigger-wt-<unit-slug>`, no UUID), so every process computes the same path and adoption is a lookup, not a porcelain parse; review worktrees derive from stage + attempt. The adopt-or-prune logic remains as the fallback for dirs deleted out from under git. Exclusion: scratch-root placement and sweeping are DONE (Gap 14 a/c) - do not rework them.

**Unit 5 - prompt budget (Gap 15).** The decisions-that-govern injection is capped and curated: the most recent N verdicts per governed file verbatim, older ones as a one-line elision note naming the count and the recovery command (`rigger peers <file>`), under a hard per-prompt byte budget; the trim is visible in the prompt itself, never silent. The store keeps full history - only the prompt slice narrows. Test: a synthetic pile of K rejection rounds yields a prompt under the budget with the elision note present.

**Unit 6 - residue surfacing (Gap 14d, Gap 10 systemic, shadow stores).** `rigger validate` gains a residue section reporting, with sizes: scratch-root worktrees whose unit is not live in the CURRENT run (needs unit 1's run scoping - sequence after it), orphaned build caches under the scratch root, shadow stores (an `events.db` anywhere under the scratch root or a worktree - the adversary-proven misfiling hazard), and local `rigger/u/*` branches with no live unit. Warnings, never failures. Exclusion: cleanup actions stay with the step-start sweep (done); validate only surfaces.

## Explicitly deferred (not this spec)

Housekeeping carry-forwards from the campaign's adjudications, batched for a later run: the unit-8 config-helper consolidation, the fresh-repo scaffold-seed alignment (dropped unit-7 branch content), test pins for the landed setup behavior and the `--if-absent` no-op path, and the shadow-store prefer-outermost policy beyond warning.

## Global constraints

- Hyphens, not em dashes, in every file this spec touches.
- New event types are permitted ONLY as named here: `RunStarted` and `BudgetExhausted`. Everything else reuses the existing vocabulary with metadata.
- Idiomatic Rust; no placeholder/TODO-stub code; every unit leaves the workspace green on both feature lanes (fmt, clippy, build, test, style).
- Backward compatibility: a store holding pre-run-id history must still load; pre-existing events without a run id fold as "before the first RunStarted" and never become live work.

## Done when

- [ ] a run begins with a `RunStarted` event carrying a fresh run id, every conductor-emitted event carries it, ready work folds ONLY from the current run's slice (stale non-terminal units from prior runs park nothing), and `rigger stats` defaults to the latest run with `--all` for the aggregate
- [ ] a breaker trip records `BudgetExhausted`, `rigger step` prints a halt reason distinct from convergence, and the thin driver stops loudly on a halt instead of reporting success
- [ ] a unit whose final permitted attempt is adjudicator-approved integrates (the cap gates only starting another attempt), pinned by a regression test of the approve-on-attempt-equals-cap scenario
- [ ] unit worktree paths derive deterministically from the scratch root and unit id (no per-process UUID), adoption is a path lookup, and the stale-registration fallback still passes its existing tests
- [ ] the decisions-that-govern prompt injection is capped (recent-N verbatim, elision note naming the count and `rigger peers` recovery, hard byte budget), with a test proving a K-round rejection pile stays under budget with the note present
- [ ] `rigger validate` reports residue with sizes - scratch worktrees with no live unit in the current run, orphaned build caches, shadow stores under scratch/worktrees, and `rigger/u/*` branches with no live unit - as warnings that never fail validation
