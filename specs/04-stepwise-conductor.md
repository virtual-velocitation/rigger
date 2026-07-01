# 04 - The stepwise conductor: one loop implementation, two faces

**Goal:** make the Rust conductor drivable one frontier at a time from the event log (`rigger step` + `rigger result`), and rewrite the native workflow driver as a thin client of it - closing design-intent Gaps 1, 2, 3, and 7 in one stroke.

## Problem

The primary driver (`/rigger`, `workflows/rigger.js`) reimplements a thin subset of the conductor: it runs units strictly sequentially despite `strategy: fan-out` (Gap 1), has no spawn-budget breaker or autonomy ratchet (Gap 2), and under-emits the event vocabulary, blinding `rigger stats` (Gap 3). The real machinery - ledger, breaker, ratchet, remediation policy, full event emission - lives in the Rust conductor, which is now the least-traveled path (Gap 7). Every conductor improvement must be manually mirrored into JS or silently diverge.

A Claude Code Workflow script cannot call MCP tools or manage processes (see `shim/README.md`), so the script cannot be a client of `rigger serve`. What it CAN do is spawn agents, and agents can shell out to the `rigger` CLI - the same channel the current driver already uses for `ground` / `emit` / `peers`.

## Design

**The stepwise core: a replay driver.** `conductor::run`'s imperative control flow stays intact. A new `AgentDriver` implementation answers each `spawn` call from the event log:

- If the log already holds the result for this spawn's deterministic id, return it immediately (replay).
- If not, persist the spawn request as an event and park the call. When every in-flight spawn is parked at the unrecorded frontier, the step is over: the conductor's state is entirely in the log, so the process simply ends.

`ledger::RunState::apply` is already a pure fold over events; this driver extends the same principle to the conductor's control flow. Replay must be **idempotent**: a step that re-runs the conductor over recorded history appends no event twice - not unit events, not gate verdicts, not spawn requests. Gate runs recorded as `GateVerdict` events are replayed, never re-executed; the first step that reaches an unrecorded gate runs it inline (so `rigger step` invocations that hit a cargo gate take gate-duration time; callers use a generous timeout).

**Deterministic spawn ids.** Each spawn request's id derives from its position in the run's structure (unit id + stage/role + attempt), never from wall clock or randomness, so replay matches results to calls across processes. Requests carry what the thin driver needs: id, prompt, persona/system prompt, model alias, tools, working dir, and the unit id + stage for display labeling.

**The breaker binds across steps.** The spawn count is derived from recorded spawn-request events, not an in-memory counter, so `defaults.budget` from `workflow.yml` aborts the run with `BudgetExhausted` no matter how many step processes the run spans. `max_retries` already folds from the log.

**The CLI surface.**

- `rigger step [--spec <path>] [--base <ref>]` - advance one frontier; print the newly requested spawn wave plus a `done` flag as JSON on stdout. Disjoint ready units (the conductor's existing blast-radius partition) park their spawns in the same wave, so fan-out falls out of the structure.
- `rigger result <id> [--error] [--meta <json>]` - record a spawn's outcome (stdin or arg) to the log, making the next `step` advance past it.

**The thin native driver.** `workflows/rigger.js` becomes a small loop with no loop logic of its own: a courier agent runs `rigger step` and returns the wave; the script spawns the wave's agents natively in parallel (per-unit `opts.phase` labels built from each request's unit + stage, preserving the per-unit progress groups); each worker finishes by reporting through `rigger result`; if a worker dies without reporting, the script reports the failure on its behalf via a courier. Repeat until `done`. The run branch base is configurable (`--base`, default `origin/main`) so a later run can build on an earlier run's branch. `meta` stays a pure literal (asserted by the existing test, which is updated alongside the embedded-copy assertion).

**The other drivers keep working.** `rigger run` and `rigger workflow` (the shim) continue to drive `conductor::run` with their blocking drivers, unchanged; their existing tests still pass.

## Global constraints

- Hyphens, not em dashes, in every file this spec touches - code, comments, docs, and prompts.
- No new event types beyond the spawn-request/result pair if the existing vocabulary cannot carry them; prefer reusing existing types with metadata.
- Idiomatic Rust; no placeholder or TODO-stub code; every unit leaves the workspace green (fmt, clippy, build, test).

## Done when

- [ ] spawn requests carry deterministic ids derived from unit + stage/role + attempt, plus prompt, persona, model alias, tools, dir, and unit/stage labels, and are persisted as events when a step parks them
- [ ] `rigger result <id>` records a spawn's outcome (output, or error with `--error`, optional `--meta` json) to the event log, and the next step advances past it
- [ ] a replay AgentDriver answers already-recorded spawns from the log and parks unrecorded ones; the step process ends when every in-flight spawn is parked at the frontier
- [ ] replay is idempotent: a step re-running the conductor over recorded history appends no duplicate events, and a recorded `GateVerdict` is replayed without re-running its gate command
- [ ] the spawn-budget breaker binds across step processes: the spawn count folds from recorded spawn-request events and a run exceeding `defaults.budget` aborts with `BudgetExhausted`
- [ ] `rigger step` prints the requested wave and a `done` flag as JSON; two ready units with disjoint blast radii park their spawns in the same wave
- [ ] `workflows/rigger.js` is a thin client: courier fetches the wave via `rigger step`, the script spawns the wave natively in parallel with per-unit `opts.phase` labels, workers self-report via `rigger result`, a worker that dies without reporting has its failure recorded on its behalf (a courier agent running `rigger result --error`), and the loop repeats until done
- [ ] the driver and `rigger step` accept a base ref for the run branch (default `origin/main`)
- [ ] a step-driven run recorded in the event log yields non-empty gate and review-verdict sections in `rigger stats`
- [ ] `rigger run` and `rigger workflow` (the shim) pass their existing test suites unchanged
