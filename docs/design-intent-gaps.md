# Design-intent gaps

Status: assessed 2026-07-01 against [architecture.md](architecture.md) after the three-gap dogfood run (PR #7); updated through 2026-07-03 across the stepwise-conductor campaign's four loop runs (specs 04-07). ALL RECORDED GAPS (1-19) ARE CLOSED. Open work: only the deferred housekeeping list in specs 06/07 (config-helper consolidation, scaffold-seed alignment, small test pins, shadow-store prefer-outermost policy). New gaps discovered by future runs land here as before.

This document records where the implementation currently falls short of the design intent, with the evidence that surfaced each gap and the shape of the fix. It is the feed for the next loop runs: each gap is written so it can be lifted into a spec's "Done when" criteria with little editing. Remove entries as they close.

## How these were found

Dogfooding. Rigger ran on its own spec; the run's telemetry (`rigger stats`, `rigger peers`), the `/workflows` display, and independent verification of the run's output are the evidence base. The through-line: the memory layer and the review economics are delivering as designed (78.6% first-pass yield, 0% escalations, decisions demonstrably inherited across agents); the gaps concentrate in the **native workflow driver**, which implements the loop's shape but not all of the conductor's safeguards.

---

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
