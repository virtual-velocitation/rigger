# Design: the design-intent-gaps campaign

Date: 2026-07-01. Closes every open gap in [design-intent-gaps.md](../../design-intent-gaps.md) as one PR, delivered through two rigger loop runs plus a small set of manual dispositions.

## Decision record

**Scope:** all open gaps (1-10). Gap 7 subsumes 1-3; Gaps 5 and 9 merge into one setup-hygiene unit; Gaps 8 and 10 are manual dispositions, not loop work.

**Vehicle:** hybrid. Dispositions happen directly in-session; everything else ships through spec-driven loop runs, keeping the dogfood telemetry as the evidence base.

**Gap 7 shape: stepwise conductor via a replay driver.** Three options were considered:

1. *CLI bridge to `rigger serve`* - add a socket transport and `rigger next --wait` / `rigger result` subcommands; the workflow script polls through courier agents while a serve daemon runs in the background. Rejected: daemon lifecycle (launch, cleanup on abort, stale sockets, concurrent runs) is fragile in an environment where the script itself cannot manage processes, and the done-flag polling race the shim already hit returns in a second form.
2. *Stepwise conductor* (chosen) - `rigger step` advances the run synchronously from the event log and exits; no daemon at all. Feasible because the foundation already exists: `ledger::RunState::apply` is already a pure fold over events. The imperative control flow of `conductor::run` is kept intact by a **replay driver** (the Temporal / Durable-Functions pattern): each `spawn` call is answered from the log when its result is already recorded, and parked as a persisted spawn request when it is not; the step ends when every in-flight call is parked at the unrecorded frontier. Replay makes two things load-bearing that were previously optional: deterministic spawn ids, and gate outcomes recorded as events (which is exactly Gap 3's fix, now forced by construction rather than asked of prompt discipline).
3. *Fold `/rigger` into the shim* - one workflow agent runs `rigger workflow <spec>` end to end. Rejected: collapses the run into a single opaque agent; loses per-unit progress groups and native agent spawning.

**Delivery: two runs, one PR.** A running workflow cannot hot-swap its own driver, so fan-out cannot benefit the same run that builds it. Instead:

1. **Pre-run (direct):** commit the two modified agent configs (Gap 8) plus this design doc and both specs as the first commit on the run branch.
2. **Run A** on the current sequential driver: spec 04 (stepwise conductor + thin native driver). Integrates onto the run branch; no PR yet.
3. **Seam:** re-run `rigger setup` to refresh the installed workflow from the just-built source. This is the one manual moment; making it automatic is part of spec 05 (Gap 5).
4. **Run B** on the new driver, based on Run A's branch: spec 05 (review hardening, model stamping, setup hygiene). Its three-plus disjoint units are the first live dogfood of the new fan-out, with the budget breaker and full event emission active.
5. **One PR** carrying the whole branch.

**Gap 10 (manual):** `wf-run` and `work-limit-resume` carry zero commits over main; `wf/metrics-project` holds a superseded 2026-06-30 draft of `src/metrics.rs`, replaced the same day by PR #1. All three are residue; disposition is deletion (pending Byran's confirmation). The systemic fix (flagging unit-less local branches) is spec 05 territory via `rigger validate`.

## Known risks, stated up front

- **Replay idempotence is the heart of spec 04.** A step that re-runs the conductor over recorded history must append nothing twice - events, gates, or spawns. This is pinned as its own acceptance criterion and needs first-class tests.
- **The breaker must count spawns from the log.** The in-memory spawn counter dies with each step process; deriving it from recorded spawn events is what makes `defaults.budget` bind across steps.
- **Run A itself runs on the old driver:** sequential, no breaker, `maxRetries` as the only bound. Accepted for one final run; it is the last such run.
- **Run A's ~8 criteria produce ~8 sequential units.** Larger than PR #7's three, but each criterion is independently green-able and the review economics (78.6% first-pass yield) support it.

## Success criteria for the campaign

- Every open gap in design-intent-gaps.md moves to its Closed section, citing this PR.
- `rigger stats` after Run B shows gate runs and review verdicts (Gap 3's blindness gone) and Run B's telemetry shows overlapping unit execution (Gap 1's fan-out live).
- `git status` is clean after a fresh `rigger setup` on a configured repo (Gap 9).
