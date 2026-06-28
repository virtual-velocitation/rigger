# 01 - Make the loop observable

**Goal:** project the metrics needed to tune the loop from the existing event log and surface them with a `rigger stats` command.

## Problem

Every signal needed to tune the loop is already in the event log, but nothing aggregates it.

- `run_single_stage` emits `UnitStarted`, `UnitFailed`, `UnitEscalated`, and `UnitIntegrated` (constants in `ledger.rs`), plus `UnitStatus` transitions (`green` / `verified` / `reviewed`).
- `run_gates` emits a `GateVerdict` (`contextgraph::TYPE_GATE_VERDICT`) per gate run, carrying `{gate, pass}`.
- The adjudicator's outcome is decided by `verdict_approves` and reflected in the unit reaching `reviewed` + `Integrated` (approve) versus looping back through `UnitFailed` (reject); the adversary and lenses emit `DecisionMade` / `LessonLearned` findings.

`ledger::project` folds these into per-unit status, but there is no aggregate view: no way to read first-pass yield, gate noise (how often a gate fails before it passes), escalation rate, or whether the review tiers earn their cost. An operator tuning the workflow has to eyeball the raw log.

The projection pattern already exists twice - `ledger::project` (folds run events into `RunState`) and the context-graph projector (`contextgraph::Projection::apply`, folding events into a read model). A metrics projection is the same fold over the same `&[Event]` slice, read from the same `.rigger/events.db`. No new event types are required; this is read-only over facts already recorded.

## Design

Add a `metrics` projection and a `rigger stats` CLI command that prints it.

- **New module `src/metrics.rs`** (registered in `lib.rs` next to `ledger`). It defines a `Metrics` struct and a `pub fn project(events: &[Event]) -> Metrics` that folds an ordered event slice, mirroring `ledger::project`. It reuses the event-type constants from `ledger` and `contextgraph` (e.g. `ledger::TYPE_UNIT_STARTED`, `contextgraph::TYPE_GATE_VERDICT`); it does not re-declare them.
- **Metrics computed** (all derivable from the named events):
  - **First-pass yield** - the count and percentage of units that reached `Integrated` with zero `UnitFailed` events for that unit id (a clean first pass), over total units started.
  - **Per-gate remediation counts** - per `gate` id from `GateVerdict`, the number of `pass:true` and `pass:false` verdicts, so a gate that fails repeatedly before passing is visible (gate noise). The artifact-tagged `GateVerdict`s emitted at integrate time (those carrying an `artifact` field) are excluded so the count reflects real gate runs, not the GATED_BY bookkeeping.
  - **Escalation rate** - `UnitEscalated` count over units started.
  - **Review outcomes** - approve versus reject counts. A unit reaching `reviewed` then `Integrated` is an approve; a unit that emitted `reviewed`/review activity but looped back into `UnitFailed` and never integrated on that attempt is a reject. Where the actor metadata on the emitted finding/verdict makes the flagging tier derivable (`contextgraph::META_ACTOR` on adversary/adjudicator events), split reject counts by tier; otherwise report the aggregate.
- **New `rigger stats` command** in `main.rs`: add a `"stats"` arm to the `match args[1]` dispatch and a `cmd_stats`, listed in `usage()`. It opens the per-project store the same way `cmd_prime` does (`Store::open(db_path("events.db"))`, wrapped in `Namespaced::new(.., &project_identity())` so it reads only this project's `run` stream), reads `read_stream(conductor::STREAM, 0, Direction::Forward)`, calls `metrics::project`, and prints a compact human-readable report.
- When the events database does not exist or holds no run units, `cmd_stats` prints a clear "no runs yet" line (the shape `cmd_prime` already uses for its empty case) and exits success.

## Done when

- [ ] a `metrics` module projects first-pass yield, per-gate remediation counts, escalation rate, and review approve/reject counts from a `&[Event]` slice
- [ ] `rigger stats` prints first-pass yield, per-gate remediation counts, escalation rate, and review approve/reject counts for the current project's run
- [ ] the metrics projection is covered by a unit test that folds a synthetic event slice and asserts each metric value
- [ ] `rigger stats` reads an existing run's `.rigger/events.db` through the per-project namespace and prints a clear "no runs yet" message when the log is empty or absent
