# Design-intent gaps

Status: open work list, assessed 2026-07-01 against [architecture.md](architecture.md) after the three-gap dogfood run (PR #7), and updated 2026-07-02 at the close of the stepwise-conductor campaign: Gaps 1-10 are CLOSED (see below); Gaps 11-16 are the open conductor-hardening family, the natural spec 06.

This document records where the implementation currently falls short of the design intent, with the evidence that surfaced each gap and the shape of the fix. It is the feed for the next loop runs: each gap is written so it can be lifted into a spec's "Done when" criteria with little editing. Remove entries as they close.

## How these were found

Dogfooding. Rigger ran on its own spec; the run's telemetry (`rigger stats`, `rigger peers`), the `/workflows` display, and independent verification of the run's output are the evidence base. The through-line: the memory layer and the review economics are delivering as designed (78.6% first-pass yield, 0% escalations, decisions demonstrably inherited across agents); the gaps concentrate in the **native workflow driver**, which implements the loop's shape but not all of the conductor's safeguards.

---

## Gap 11: the run stream is not run-scoped, so a new run resurrects history's zombies

**Intent.** A run folds ITS OWN events into state; prior runs' history informs memory (decisions, findings) but never becomes live work.

**Reality.** The conductor folds the entire per-project `run` stream as one continuous run. The first stepwise run over the accumulated stream (spec 05, run `wf_a27a741f-767`) parked implementers for `u-autoresume` and `u-metrics-mod` - non-terminal residue of aborted pre-stepwise runs - alongside the spec's real units, and `rigger stats` reports all-time aggregates (35 units) as if they were one run.

**Evidence.** Run B wave 1 contained 12 spawns: 10 spec-05 units plus the two zombies (one at attempt #2, inherited from its original run). Operator disposition: `UnitEscalated` + `rigger result --error` for both (positions 575-579).

**Fix shape.** Scope the fold: a `RunStarted` event carrying a run id, unit events stamped with it, and the conductor folding only the current run's slice (prior-run units visible as history, never as ready work). `rigger stats` gains a per-run view. The workaround until then: terminal-escalate stray units by hand at run start - exactly what should never require a human.

## Gap 12: step replay is event-idempotent but not worktree-idempotent

**Intent.** Any `rigger step` process resumes a run from the log alone; spec 04's idempotency criterion ("a step re-running the conductor over recorded history appends no duplicate events") was meant to make step processes disposable.

**Reality.** Worktree side-effects escape that criterion. The conductor derives a fresh UUID-suffixed worktree dir per step process, so a later step's `Worktree::create` hits git's one-checkout-per-branch rule against the previous process's still-registered worktree and the step dies (`fatal: '<branch>' is already used by worktree at '/tmp/rigger-wt-...'`). The branch-is-the-checkpoint design is right (`Worktree::create` reuses an existing branch without reset); only the stale registration handling is missing.

**Evidence.** Run `wf_74918c04-514` step 1 died on exactly this against wave 1's twelve worktrees. Operator workaround: `git worktree remove --force` the stale dirs (branches and their commits preserved), relaunch.

**Fix shape.** On `Worktree::create`, if the branch is already checked out in a registered worktree, adopt that dir when it still exists (same process or not) or prune the stale registration and re-create. Deterministic (non-UUID) worktree paths would make the reuse trivial.

**Status:** the adopt-or-prune fix landed by hand mid-campaign (ce68575, with the terminal-worktree sweep in e986abd); the deterministic-paths simplification remains for spec 06.

## Gap 13: a breaker halt is indistinguishable from convergence in the step output

**Intent.** "When spawns reach it the breaker records `BudgetExhausted` and aborts the run" (`workflow.yml`) - a halted run is loudly halted.

**Reality.** When the spawn count reaches `defaults.budget`, the step process parks nothing new and exits with every existing spawn answered - so `rigger step` prints `{"wave":[],"done":true}` and the thin driver (correctly, per its contract) reports a CLEAN COMPLETION. No `BudgetExhausted` event lands in the log. The spec-05 run halted at exactly 60/60 spawns with zero units integrated and the workflow said success; only `git log` and the spawn count revealed the truth.

**Evidence.** Run `wf_7e202e7e-927`: `{"waves":8}` success result, ten unit branches unintegrated, `SELECT COUNT(*) ... type='SpawnRequested'` = 60 = `defaults.budget`.

**Fix shape.** The breaker records `BudgetExhausted` (as documented) and `Step` gains a halt reason (`done` splits into `converged` vs `halted:<why>`); the thin driver stops loudly on a halt. Conductor-hardening family (Gaps 11-13).

## Gap 14: worktree storage has no budget, no shared cache, and no lifecycle cleanup

**Intent.** Worktrees are transient isolation; the branch is the checkpoint. Nothing durable or expensive should accumulate in them.

**Reality.** Each worktree builds its own multi-gigabyte cargo `target/` (~5G for this crate) under `std::env::temp_dir()` - the OS partition - and three compounding leaks filled a 69G root disk to 97% mid-run: per-worktree targets for concurrent units, scratch repos (with their own `target/`s) leaked inside a worktree by the setup unit's own tests, and stale worktrees from runs weeks old that no lifecycle ever pruned (the conductor removes a worktree on integrate, but crashed processes, replaced duplicates, and abandoned runs leak theirs forever).

**Evidence.** 2026-07-02: 15G across eleven `/tmp/rigger-wt-*` dirs, five of them from pre-campaign runs; operator cleanup by hand mid-run (the run had to be paused).

**Fix shape** (Byran, 2026-07-02: a small OS partition with most disk on /home is a common layout; the scratch location must be configurable and MAINTAINED, not just relocated). Four parts:

- (a) **Configurable scratch root, sane default.** DONE out-of-band (operator, 2026-07-02, mid-campaign): worktrees live under `<repo>/.rigger/tmp` by default (gitignored, repo partition, same-filesystem adds), `defaults.workdir` / `RIGGER_TMPDIR` override, `~/` expansion; `std::env::temp_dir()` is no longer placement policy (`worktree::scratch_root`).
- (b) **Shared build cache.** One `CARGO_TARGET_DIR` under the scratch root shared across worktrees (cargo's own locking makes concurrent builds safe), so targets stop multiplying per worktree. REMAINS for the loop.
- (c) **Lifecycle: the loop cleans up after itself.** DONE out-of-band, same patch: every `rigger step` starts with `worktree::sweep_terminal` - prune stale registrations, remove scratch-root worktrees whose branch is an ancestor of the run branch (integrated units, review scaffolding); in-flight checkpoints untouched. Bounding unit test scratch to the worktree REMAINS for the loop.
- (d) **Residue is surfaced.** `rigger validate` reports scratch-root residue (worktrees with no live unit, orphaned build caches) with sizes, so accumulation is a warning, never a full disk. REMAINS for the loop.

## Gap 15: prompt assembly accumulates every prior verdict verbatim, without bound

**Intent.** Grounding scopes each agent to "exactly the code it needs" plus the peer decisions that govern its blast radius - a SLICE, by design.

**Reality.** The decisions-that-govern injection concatenates every prior adjudicator/adversary verdict verbatim into every subsequent prompt for the same files. By review round 4 of the spec-05 run, single prompts reached 280KB and an 11-item wave totaled 2.2MB - degrading agent focus, inflating cost, and (before waves went by-reference) exceeding what any relay could carry.

**Evidence.** Run `wf_b9651f0b-dec`: the step courier measured the wave at 2,256,482 bytes, prompts 103-281KB each, and traced the growth to verbatim-quoted historical verdicts.

**Fix shape.** Cap and curate the injection: most-recent-N verdicts per file plus a one-line summary of older ones (the store keeps the full history; the prompt does not need it), and a hard per-prompt context budget with the trim reported in the prompt itself ("k older decisions elided - `rigger peers <file>` for the rest"). The spawn-by-reference wave (operator fix, same day) removed the RELAY bottleneck; this gap is the GROWTH itself. Conductor-hardening family (Gaps 11-15).

## Gap 16: an approval on the final attempt loses to the retry cap

**Intent.** `max_retries` bounds how many attempts a unit gets; an adjudicator APPROVE integrates the unit.

**Reality.** When the approval arrives on the last permitted attempt, the conductor escalates anyway: unit-2's attempt 6 was approved by all three tiers (ReviewVerdict at position 3176) and the unit was still marked `UnitFailed attempts:6` / `UnitEscalated` (positions 3206/3208), with the LessonLearned quoting the APPROVE text under a "review rejected:" header. Finished work read as failure; the operator merged it by hand.

**Fix shape.** Order the terminal check after the verdict fold: an approved unit integrates regardless of the attempt counter; the cap only gates STARTING another attempt. Conductor-hardening family (Gaps 11-16).

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
