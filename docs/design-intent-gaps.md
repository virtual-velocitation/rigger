# Design-intent gaps

Status: assessed 2026-07-01 against [architecture.md](architecture.md) after the three-gap dogfood run (PR #7); updated through 2026-07-03 across the stepwise-conductor campaign's four loop runs (specs 04-07). ALL RECORDED GAPS (1-19) ARE CLOSED. Open work: Gap 21 (below; spec-12 companion), and Gaps 23-25 (memory retrieval-boundary gaps from the 2026-07-07 context-vs-memory eval - placement, projection maintenance, trust weighting; all fold into the existing injection-ranking machinery). The improvement program in docs/research/ drives specs 10-13.

This document records where the implementation currently falls short of the design intent, with the evidence that surfaced each gap and the shape of the fix. It is the feed for the next loop runs: each gap is written so it can be lifted into a spec's "Done when" criteria with little editing. Remove entries as they close.

## How these were found

Dogfooding. Rigger ran on its own spec; the run's telemetry (`rigger stats`, `rigger peers`), the `/workflows` display, and independent verification of the run's output are the evidence base. The through-line: the memory layer and the review economics are delivering as designed (78.6% first-pass yield, 0% escalations, decisions demonstrably inherited across agents); the gaps concentrate in the **native workflow driver**, which implements the loop's shape but not all of the conductor's safeguards.

---

## Gap 21: integrate asserts "done" on a merged tree the gates never saw

**Intent.** R6: a unit is done only when machine-verified - and the thing that must be verified is what actually LANDS, the post-merge tree.

**Reality.** Gates run in the unit's worktree (pre-merge); integration merges into the run branch WITHOUT re-running gates on the merged result. Two units that are each green in isolation can merge into a broken tree: spec-10 unit-1 (stale-based, one-arg `agent_model` calls) textually auto-merged over unit-4's two-arg signature change and `rigger-run` stopped compiling - while both units' events read Integrated. The breakage surfaced only at the next `cargo install`, hours later.

**Evidence.** 2026-07-03: commits 57baf35 (unit-4) + 07f9f44/95ba133 (unit-1) produced a rigger-run tree failing `cargo build` with E0061/E0063; operator hand-weave repaired it. PR-level CI would have caught it eventually; the run branch was broken and "integrated" in the meantime.

**Fix shape.** The integrate step re-runs the gate suite against the MERGED tree before emitting `UnitIntegrated` (spec 12's content-addressed verdicts make this cheap - an unchanged-input gate is a cache hit, so the post-merge re-gate costs only what the merge actually changed); a red post-merge re-gate blocks integration and feeds remediation with the semantic-conflict evidence.

## Gap 22: the plan-critique gate escalates on a resumed run over baseline-vs-replan duplication

**Intent.** The plan-critique gate (spec 10 unit 1) rejects a decomposition that violates handbook rules 6-8 (duplicate ownership, shared blast radius) BEFORE fan-out, and a reject sends the planner back to fix it.

**Reality.** On a resumed run the planner re-proposes units under FRESH slugs (`u-plancritique`, `u-modelladder`) while the run still holds the earlier waves' baseline/integrated units (`unit-1-a-plan-critique-...`, `unit-4-...`) - so the gate correctly sees TWO units owning one mitigation and rejects, but the planner cannot resolve it: `UnitProposed` only ADDS, and the supersede-the-baseline path needs a verbatim-criterion match the re-slug breaks. The planner itself diagnosed "the plan-critique REJECT is UNRESOLVABLE by any UnitProposed" and the gate escalated after 6 attempts, wedging the fan-out for work that was already done. Introducing the gate MID-RUN (the binary gained it between waves) is what first exposed this.

**Evidence.** 2026-07-07: spec-10 run wf_3db89015 escalated `plan-critique` at event 5522 after 6 replan cycles; findings 5496-5499 name the baseline-vs-replan duplicate pairs. Operator disposition: removed the gate from rigger's OWN workflow.yml (kept in the scaffold, fully tested) so rigger self-hosts; unit-2/3 finished by hand.

**Fix shape.** Two parts: (a) the gate critiques only units that will actually FAN OUT, excluding already-integrated and terminal units, treating them as settled not duplicate candidates; (b) definition pinning (spec 13 unit 1) prevents a gate being introduced mid-run at all.

**Status: CLOSED (root-cause) 2026-07-07.** Part (a) landed by hand: `dag_unit_blast_radii` excludes integrated+terminal units, pinned by `an_already_integrated_unit_is_not_a_duplicate_the_gate_can_flag`. plan-critique is RE-WIRED into rigger's own workflow.yml - the gate is now safe even when introduced mid-run. Part (b) (definition pinning, spec 13) remains as defense-in-depth against mid-run definition drift generally, but is no longer required for plan-critique.

## Gap 23: the budgeted prompt injection controls amount but not placement

**Intent.** Retrieved memory that is critical to the current reasoning should sit where the model attends most - near the generation point (the "lost in the middle" effect).

**Reality.** Spec 07 budgets the AMOUNT of each injected section (decisions/findings/lessons) but assembles them in a fixed section order, most-recent-first within a section - not positioned by relevance to the current task. The most load-bearing decision can land in the low-attention middle.

**Evidence.** The context-vs-memory-engineering evaluation (docs/research/2026-07-07-context-vs-memory-eval.md, Failure Mode #2), corroborated by the review-calibration research's position-bias finding.

**Fix shape.** Rank the budgeted items by relevance-to-this-task and place the highest-relevance last (nearest the prompt tail); keep the byte budgets. Composes with Gap 24's ranking.

## Gap 24: the injected projection has no maintenance (decay, dedup, TTL)

**Intent.** Retrieval quality holds as a project accumulates history: stale facts do not crowd out current ones.

**Reality.** The event log correctly never forgets (R2), but the PROJECTION into a prompt is recency-N with no confidence decay on volatile decisions, no semantic dedup of near-duplicate findings, and no TTL - so signal-to-noise of the injection drops over a long project (the article's "memory degradation"). Spec 13's playbook distillation dedups lessons only.

**Evidence.** docs/research/2026-07-07-context-vs-memory-eval.md, "memory maintenance."

**Fix shape.** A maintenance pass over the injected projection (NOT the log): decay confidence on volatile decisions, dedup semantically-near findings before budgeting, rank by importance x recency x finding-survival-rate rather than pure recency. Folds the review-calibration finding-survival telemetry into the ranking.

## Gap 25: memory retrieval is not weighted by source trust

**Intent.** A ratified adjudicator decision and a refuted adversary finding should not carry equal weight when injected.

**Reality.** Events record WHO emitted them (META_ACTOR) but retrieval/injection does not WEIGHT by source trust (internal vs user vs external). Low impact while the fleet is all-internal; real once external content (imported fleets, web-grounded facts) enters the graph.

**Evidence.** docs/research/2026-07-07-context-vs-memory-eval.md, trust-level weighting (article: 1.0 internal / 0.5 user / 0.0 external).

**Fix shape.** A trust field on the emit vocabulary, folded into Gap 24's injection ranking. Lower priority than 23/24.

---

## Closed

Move entries here when they land, with the closing PR.

- **Fresh `cargo install` required `--locked`** - closed by PR #7 (u1): `ort-sys` pinned exact, `install-nolock` CI job guards the fresh-resolve path.
- **`/workflows` phase display implied a false global stage order** - closed by PR #7 (u2): per-unit `opts.phase` progress groups; `meta` stays a pure literal, asserted by test.
- **EventStore contract too narrow to trust SQLite as a KurrentDB proxy** - narrowed by PR #7 (u3): four checks added (exact-revision concurrency, nonzero-revision subscription resume, never-appended reads, distinct-stream concurrent appends), enforced against both adapters in CI. Breadth remains a judgment call; revisit when a real KurrentDB behavior diverges.

- **Gap 1 (driver ran units sequentially)** - closed by spec 04: the stepwise conductor + thin driver run the full pending frontier as parallel waves; Run B's first step fanned 12 implementers at once.
- **Gap 2 (primary driver lacked the breaker and ratchet)** - closed structurally by spec 04: the conductor drives every run, and the cross-process spawn budget tripped live at both 60 and 200 spawns (its silent-halt UX is Gap 13).
- **Gap 3 (driver under-emitted the event vocabulary)** - closed structurally: gate and verdict events are load-bearing for replay, so they are always recorded; `rigger stats` showed 57 real gate runs by campaign close.
- **Gap 4 (constraints outside the gate suite)** - closed: the `style` gate (unit-1) and the adjudicator's named Constraints Recheck (unit-2), already exercised by every post-run adjudication in this campaign.
- **Gap 5 (installed workflow drift)** - closed: drift-aware re-runnable setup with refresh reporting (unit-4) plus `rigger validate` warnings for installed-vs-embedded drift and uncommitted tracked `.rigger/` changes (unit-6).
- **Gap 6 (resolved model unrecorded)** - closed by unit-3: spawn events carry the requested alias and the resolved model id.
- **Gap 7 (primary/fallback drivers inverted)** - closed by spec 04: `/rigger` is a thin client of the Rust conductor (`rigger step`/`rigger result`/`rigger prompt`, spawn-by-reference waves); one loop implementation drove Run B end-to-end.
- **Gap 8 (uncommitted agent configs)** - closed by the campaign pre-run commit (e12c083).
- **Gap 9 (setup dirties git status)** - closed: referenced-agent scaffold skip + gitignore writes (landed via f6d7222), silent-no-op re-runnable setup (unit-4), `--agents` import with the starter-fleet pointer instead of blind scaffolds (unit-8), and the four stray duplicates removed (unit-7 disposition).
- **Gap 10 (stale unknown-provenance branches)** - manual half closed: the three legacy branches inspected and pruned 2026-07-01, and every unit/improvised branch pruned at campaign close (branch signal restored: main + rigger-run only). The systemic residue check joins the spec 06 pool.
- **Gap 11 (unscoped run stream)** - closed by spec 06 unit 1: `RunStarted` + run-id metadata on every conductor event; ready work folds only from `runscope::current_run` (pre-run-id history can never become live work); `rigger stats` defaults to the latest run with `--all` for the aggregate.
- **Gap 12 (worktree idempotence)** - closed: adopt-or-prune landed by hand mid-campaign (ce68575), deterministic unit-derived worktree paths by spec 06 unit 4 (adoption is now a path computation).
- **Gap 13 (silent breaker halts)** - closed by spec 06 unit 2: a trip records `BudgetExhausted`, `rigger step` prints halted-vs-converged, and the thin driver stops loudly on a halt.
- **Gap 14 (scratch storage)** - closed: configurable scratch root + overrides and terminal sweeps by hand (e986abd, 64d58c4), gate-lane shared build cache via the courier (ebc93dd; its pollution hazard is Gap 19), and validate residue surfacing by spec 06 unit 6.
- **Gap 15 (unbounded prompt growth)** - closed for the decisions section by spec 06 unit 5 (recent-N verbatim, visible elision note, 24KiB budget); the measured larger half - findings/lessons - is Gap 17.
- **Gap 16 (approval loses to the retry cap)** - closed by spec 06 unit 3: the verdict folds before the attempt counter, pinned by a regression test of the approve-on-final-attempt scenario.
- **Gap 17 (findings/lessons uncapped in prompts)** - closed by spec 07 unit 1: one shared budgeted-section writer renders decisions, findings, and lessons alike (recent-N verbatim, visible elision note with the `rigger peers` recovery, per-section byte budgets as named constants).
- **Gap 18 (degenerate reviewer charges an attempt)** - closed by spec 07 unit 2: an empty/whitespace reviewer result triggers a bounded, deterministic, replay-safe respawn instead of folding a failure; exhausting the respawn bound halts the run loudly naming the dead reviewer (tier-aware and recoverable after the retry round).
- **Gap 19 (shared build-cache pollution)** - closed by spec 07 unit 3: worktree gates get per-unit `CARGO_TARGET_DIR`s reclaimed by the terminal sweep; the shared cache serves only the courier's inline gates on the integrated tree.
- **Gap 20 (volatile project identity)** - closed by spec 09: identity is the tracked `.rigger/project.id` (minted from the normalized origin URL, random without a remote; clones inherit), with legacy-basename fallback, a one-time recorded stream migration, loud refusal on ambiguous double-namespace stores, and a validate nudge. History survives a directory rename, pinned end-to-end.
