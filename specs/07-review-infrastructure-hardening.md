# 07 - Review infrastructure hardening: close Gaps 17-19

**Goal:** close the three gaps the spec-06 run surfaced: findings/lessons still blow prompts (Gap 17), a degenerate reviewer spawn charges the unit a remediation attempt (Gap 18), and shared build-cache pollution false-fails gates across divergent unit trees (Gap 19).

Authored under handbook rules 6-8: consolidated by blast radius, one owner per mitigation with exclusions stated, no open dispositions.

## Problem

All three recorded with evidence in [design-intent-gaps.md](../docs/design-intent-gaps.md):

- **Gap 17.** Spec 06 unit 5 capped the DECISIONS prompt section; its own measurement showed findings are 4-8x larger on hot files (~95KiB about conductor.rs, ~187KiB about main.rs) and still render uncapped, as do lessons.
- **Gap 18.** Unit-6's final-attempt adjudicator returned empty output; the conductor folded it as a substantive failure and escalated a unit every producing tier had approved. Infrastructure failures must not spend remediation attempts.
- **Gap 19.** Unit-6 burned attempts 4-5 on E0425 false-fails from the shared `CARGO_TARGET_DIR`: concurrent units with divergent trees poison each other's incremental state, and every post-run re-verification needed a dedicated target dir to be trustworthy.

## Design

**Unit 1 - budget every prompt section (Gap 17). OWNS all prompt-slice budgeting.** Extend the exact cap-and-curate mechanism unit-5 built for decisions (`write_capped_decisions`: recent-N verbatim, a visible elision note naming the count and the `rigger peers <file>` recovery, a hard byte budget) to the FINDINGS and LESSONS sections - one shared budgeted-section writer, three call sites, no divergent second mechanism (the existing decisions behavior and its tests must be preserved, not reimplemented). Budgets may differ per section (findings run larger); each is a named constant with the rationale. The store keeps full history; only the prompt slice narrows. Exclusion: WHAT gets injected (grounding/graph selection) is unchanged - this unit only bounds HOW MUCH renders.

**Unit 2 - degenerate reviewer results are infrastructure failures (Gap 18).** A reviewer spawn (lens, adversary, or adjudicator) whose recorded result is empty or whitespace-only is NOT a verdict: the conductor respawns that reviewer with a deterministic retry-suffixed spawn id (replay-safe), bounded at two respawns; only a NON-degenerate result folds into the review outcome. If the respawn budget exhausts with only degenerate results, the unit does not lose the attempt - the run halts loudly naming the dead reviewer (an infrastructure problem for the operator, not a code defect for remediation). Exclusion: what a non-degenerate reject does is unchanged (unit 3 of spec 06 already owns the terminal ordering); worker (implementer) death handling is unchanged - it belongs to the driver's death-courier protocol.

**Unit 3 - per-unit build caches for worktree gates (Gap 19).** Gate commands the conductor runs INSIDE a unit worktree get `CARGO_TARGET_DIR=<scratch-root>/cargo-target-<unit-slug>` - divergent trees never share incremental state, so a compile error in a gate is always the unit's own. The per-unit cache dir is swept with the unit's worktree by the existing terminal sweep (extend the sweep's match, do not build a second sweeper). The courier-level shared cache (`<scratch-root>/cargo-target`) remains for `rigger step`'s inline gates on the integrated tree, where there is exactly one tree. Exclusion: scratch-root resolution and sweep timing are DONE (Gap 14 a/c); worker agents' own cargo invocations already carry the driver's scratch policy.

## Explicitly deferred (unchanged from spec 06)

The housekeeping batch: config-helper consolidation, fresh-repo scaffold-seed alignment, test pins for landed setup behavior and the `--if-absent` no-op path, shadow-store prefer-outermost policy beyond warning.

## Global constraints

- Hyphens, not em dashes, in every file this spec touches.
- NO new event types; respawned reviewer spawns reuse the existing spawn-request/result vocabulary with deterministic retry-suffixed ids.
- Idiomatic Rust; no placeholder/TODO-stub code; every unit leaves the workspace green on both feature lanes (fmt, clippy, build, test, style).
- Replay safety: every new spawn id (unit 2's retries) derives deterministically from unit + role + attempt + retry ordinal, never from wall clock or randomness.

## Done when

- [ ] the findings and lessons prompt sections render through the same budgeted-section mechanism as decisions (recent-N verbatim, visible elision note naming the count and the `rigger peers` recovery, hard per-section byte budgets as named constants), with a test proving a synthetic pile of oversized findings stays under budget with the note present, and the existing decisions cap tests still passing unchanged
- [ ] an empty or whitespace-only reviewer result triggers a deterministic retry-suffixed respawn (bounded at two) without charging the unit an attempt, a non-degenerate result on retry folds normally, and exhausting the respawn bound halts the run loudly naming the dead reviewer - each path pinned by a test, including replay determinism of the retry ids
- [ ] gate commands run inside a unit worktree use a per-unit `CARGO_TARGET_DIR` under the scratch root, the terminal sweep reclaims it with the unit's worktree, the courier's shared cache still serves `rigger step`'s inline gates on the integrated tree, and a test pins that two units' gate environments never share a target dir
