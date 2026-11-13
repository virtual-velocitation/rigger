# Evaluation

After every landed run of this repository's own workflow, an evaluation measures the run, groups its failures, its time and its grep fallbacks into classes and proposes at most three changes to how the next run is driven. Each run leaves one file here. Together the files record what each change was predicted to move, what it moved, and whether it was kept.

The evaluation is wired to no stage and applies nothing by itself, so it can never block or undo a landing.

## Procedure

Run these from the operator checkout, after the check-in has merged and before the next `rigger run`:

1. `sh .rigger/gates/evaluate.sh run` measures the landed run into a new file here. It then runs the evaluator (`.rigger/agents/evaluator.md`) through `$EVALUATE_AGENT` and checks the result.
   - With `EVALUATE_AGENT` unset, it stops after measuring. Run the evaluator on the file by hand, then run `sh .rigger/gates/evaluate.sh check <file>`.
2. Adopt at most one proposal:
   - Set `"adopted"` in the proposals block to its id, or to `"none"`.
   - Make the change. A `binary` or `spec-skill` change is the spec its `path` names, run like any other spec.
   - When the adopted change differs from the proposal, rewrite its `change`, `predicted_move` and `hours_saved` to match what was made, and list under `also` each other metric the change is predicted to move, such as the cost of a gate it adds. The `change` states one decision, never an either-or, because `emit` records it as the decision.
   - The next run's file scores it.
3. Commit the file and the change on the run branch.
4. Run `sh .rigger/gates/evaluate.sh emit <file>`. It records the adopted proposal as one decision, `eval-<run8>-adopted`, governing the file its change touches, so the next run reaches it from that file. Re-emitting updates the same decision, and with no proposal adopted it records nothing. It records no finding, so an evaluation never appears as a reviewer in the run it evaluates; the proposals not adopted stay in the committed run file, which the graph holds as a document.

`measure` and `evidence` also read a run that has not landed; the scorecard's `labels.run` then reads `interim`. While a run is live, point `EVALUATE_DB` and `EVALUATE_PROGRESS` at a copy of the store files (each database with its `-wal` and `-shm`, copied together), never at the live stores.

## Evidence sources

`sh .rigger/gates/evaluate.sh evidence <run>` prints, as JSON, the three sources the evaluator classes and the run's scheduling, which a `cost` class draws on:

| Source | What it holds | A class's evidence |
|---|---|---|
| `failures` | every reject, gate failure, other failure, escalation and deferred gate failure, each with its `rework_hours`: its unit's hours from the failure to the unit's next failure, next merge attempt or integration | stream positions |
| `cost` | the run's unit hours by label, by gate, by unit and by battery; the repeats the log shows (a post-merge battery over the same tree as the unit's pre-merge battery, a battery re-run after a failure, a gate re-run over an input it already judged); relaunched sessions; and `not_shown`, what the log cannot show | stream positions |
| `scheduling` | the run's live wall hours by slot state, the units in flight, the derived co-schedulable share; per started unit its blast radius beside the files it changed, the units it conflicts with and the units it ran beside; the files that put the most unit pairs in conflict, by radius and by change; and `not_shown` | stream positions (`UnitStarted`, `BlastRadiusComputed`) |
| `grep_fallbacks` | the lines agents recorded when they searched the sources by text, grouped by what they searched for: a caller, a text, a symbol, a file, or unclassified. The grouping is by keyword and the lines are self-reported. | `progress:<position>` |

A grep-fallbacks class is a habit gap when the graph could answer and the agent did not ask, and targets a persona; it is a capability gap when the graph cannot answer that kind of query, and targets the lookup surface as a `binary` spec.

## Scheduling

The scheduling figures are live wall hours, not unit hours: the run's first `RunStarted` to its last landing, outages removed by the outage rule in baseline.md. `measure` writes the run-level figures to the scorecard and a summary, a row per started unit and the top five files of each overlap table to the run file's Scheduling section; `evidence` holds the rest.

| Term | Definition |
|---|---|
| slot | a unit holds one from its first `UnitStarted` to its `UnitIntegrated` or `UnitEscalated`. The width bound, `max_parallel_units` in .rigger/workflow.yml, counts slots. |
| ready | derived, since no event records it: a unit is ready once every need its `UnitStarted` names has integrated, and at the run's start when it names none. A need that names no started unit is a stage, as the check-in's `implement` is, and is met once every other unit that joins no stage has integrated. A need that never integrated leaves the unit never ready. |
| `slot_hours` | each live second takes the first state that applies: `idle` (no slot held); with one slot held, `solo_ready_held` (another unit is ready and not started, inside a hold), `solo_ready_waiting` (the same, outside a hold), `solo_needs_waiting` (another unit is not started and none is ready) or `solo_none_left`; with two or more, `multi_ready_waiting` (another unit is ready and not started) or `multi`. The states sum to the live wall hours. |
| `serial_hours` | `slot_hours.solo_ready_waiting`: one unit held a slot while another was ready and not started, and the run was not held. Slots, not units in flight, decide it, as the width bound does. |
| `queued_hours` | `slot_hours.multi_ready_waiting`: two or more units held slots while another was ready and not started. With a width bound of two, this is time the bound held a ready unit back. |
| `chain_hours` | `slot_hours.solo_needs_waiting`: one unit held a slot and every unit not started waited on its needs. It includes the plan and the plan critique ahead of the fan-out, and the last unit's time while only the check-in waits. |
| `serial_share`, `queued_share`, `chain_share` | each of the three over the live wall hours |
| `units_in_flight_mean`, `units_in_flight_max` | the started units with an open agent session (`SpawnRequested` to `SpawnResult`) or gate battery, as a time-weighted mean over the live wall hours and a maximum. A unit can hold a slot with nothing in flight, between a session and its battery or the next session. Null when the slice holds no spawn record. |
| `co_schedulable_share` | derived, since no event carries it: recomputed from each unit's latest `BlastRadiusComputed` by the driver's own rule. Two units conflict when their safe radii share a file or either radius is empty; in unit id order, each unit joins the first batch it conflicts with no member of. The share is the units in batches of two or more over the grounded units, the units with a radius. |
| `conflicting_pairs`, `conflicting_pairs_ran_together` | the grounded unit pairs that conflict, and those of them whose slots overlapped |
| `radii_recorded_before_start` | the units whose first radius was recorded before their first `UnitStarted`. A wave that applies the radius rule records a ready unit's radius before admitting it, so 0 means no wave applied the rule and only the width bound and the needs set the schedule. |
| overlap tables | per file, the units whose radius names it and the units that changed it, with the unit pairs each puts in conflict (k units make k(k-1)/2 pairs), ranked once by radius and once by change. The evidence lists 15 files of each, the run file 5. The evidence also gives the fewest files two non-empty radii share and how many files tie for the most units: when every pair shares many files, no single file is the cause. |
| changed files | `FileTouched` names no unit, so each is attributed to the next `UnitIntegrated` in log order, which the landing records right after its files. The files include what a regeneration rewrote, in a unit's commits or its merges, so the log does not tell a unit's edit from a regenerated file: a generated file that every unit's tree regenerates tops the by-change table with no unit's radius naming it. |

The scheduling label says when the figures are approximate: with no gate verdict in the slice, units in flight count agent sessions only, and with neither a heartbeat nor a hold, a driver outage counts as live time and a slot stays open across it. The evidence's `not_shown` lists what no event records: when a unit became ready, that a ready unit was held back and by what, whether a wave applied the radius rule, the width bound a wave ran under, and which unit changed a file.

## File format

**Name:** `YYYYMMDD-HHMM-<run8>.md`. The first part is the run's start in UTC and `<run8>` is the first eight characters of its run id, so the names sort in run order. TEMPLATE.md is the layout.

**Sections:**

| Section | Written by | Content |
|---|---|---|
| Scorecard | measure | `json scorecard`: the run's numbers (definitions in baseline.md) |
| Comparison | measure | the headline metrics for the baseline, the last five run files and this run |
| Scheduling | measure | the scheduling summary, a row per started unit and the top five files of each overlap table (see Scheduling) |
| Previous proposal | measure | `json previous`: the previous file's adopted proposal, scored |
| Diagnosis | evaluator | one paragraph per class; `json classes`: `[{class, source, summary, evidence[]}]` |
| Proposals | evaluator | `json proposals`: `{adopted, proposals[]}` |

**Fields:**

- **Class:** a stable kebab-case slug. It names the cause, never the incident, and is reused for the same cause in later files.
- **Source:** `failures`, `cost` or `grep-fallbacks`, the evidence source the class is drawn from.
- **Evidence:** the ids the class's source gives in `sh .rigger/gates/evaluate.sh evidence`: stream positions, or `progress:<position>` for a grep-fallbacks class.
- **Proposal fields:**

| Field | Meaning |
|---|---|
| `id` | `P1` to `P3`, in rank order |
| `class` | one of the file's classes |
| `change` | the change, in one sentence stating one decision, never an either-or |
| `evidence` | the records that show the class |
| `surface` | `persona`, `gate`, `workflow`, `spec-skill` or `binary`. A binary change becomes an ordinary spec. So does a spec-skill change: the spec-writing skills ship to every project that uses rigger, so a change to them is reviewed like a change to the binary. A binary change is to rigger's own code and tests: rigger is language and process agnostic, so the binary never gains a gate, a mandatory step or a language rule. |
| `path` | the file to change, or the spec to write |
| `form` | `check`, `gate` or `instruction`. A `gate` is a workflow gate, so its surface is `gate` or `workflow`. |
| `metric` | a measured scorecard key, or `<map>.<key>` of a by-map. A cost class's metric is a time measure (`hours_by_label`, `gate_hours_by_gate`, `repeated_hours`, `unit_elapsed_hours`, `serial_hours`, `time_shares`, `gates_share` or `post_merge_gates_share`); a grep-fallbacks class's is `grep_fallbacks` or `grep_fallbacks_by_target`. |
| `predicted_move` | the signed move in the metric's units. A cost proposal lowers its metric by at most the value it holds. |
| `hours_saved` | the hours per run the change removes, the ranking key; null for a grep-fallbacks class, which ranks last |
| `also` | optional: `[{metric, predicted_move}]`, the other metrics the change is predicted to move, each named and signed as `metric` and `predicted_move` are. The next file scores each beside the proposal's own metric; the verdict stays the proposal's own. |

## Rules

- **At most three proposals** across all three sources, ranked by hours saved. A proposal with no metric or no non-zero predicted move fails `check`, as does one missing any other field.
- **One adopted proposal per run.** The operator chooses it, and nothing is applied automatically.
- **Recurrence:** when a class appears in two consecutive files, the second file's proposal for it must be a `check` or a `gate`, not instruction text. Persona and spec-skill changes are instruction text.
- **Scoring:**
  - The actual move is this run's metric minus the previous run's.
  - The verdict is `kept` when the metric moved in the predicted direction, `reverted` otherwise.
  - The operator undoes a reverted change.
- **Numbers come only from the script.** The evaluator writes no figure into a measured section and estimates no duration.
- **Hours are bounded by what the log measured.** A proposal's `hours_saved` is at most its class's measured hours: for a failure class, the sum of its records' `rework_hours`; for a cost class, the hours its metric holds, or for a share, that share of `unit_elapsed_hours`. A grep fallback carries no duration in the log, so a grep-fallbacks proposal's `hours_saved` is null. `check` refuses a proposal over its bound.
- **Scheduling hours are serial hours.** A class drawn from the scheduling names `serial_hours`, and `check` bounds its `hours_saved` by the value it holds, as for any cost class. `queued_hours` and `chain_hours` are measured and compared but never claimed: what a wider bound or a shorter needs chain would save is not measured. A spec whose units all change one shared file, or whose units' radii all name one, is a planning or decomposition class, on the `persona` surface (.rigger/agents/planner.md) or the `spec-skill` surface. A radius that names far more files than its unit changed is a capability class, on the `binary` surface. Either claims serial hours only when `radii_recorded_before_start` is above 0: otherwise no wave applied the radius rule, no radius held a unit back, and the overlap tables alone are no class.
- **No class from what the log cannot show.** The evidence lists under `not_shown` what no event records, such as when a session made its first edit or whether it ended by running out of context; a repeat whose hours are null or 0 is no class either.
- **Unmeasured and self-reported figures stay labelled.** `grounding_share` is always null: nothing in the log marks the end of an agent's orientation, so it is never estimated. `grep_fallbacks` counts only the fallbacks agents reported, and `grep_fallbacks_by_target` groups them by keyword. `relaunched_sessions` counts relaunches the log records, not their cause. Time shares follow the method only when `labels.time_shares` says so; otherwise they are approximate.
- **The scorecard and `rigger stats` count differently.** The scorecard's first-pass figure counts criterion units, while `rigger stats` also counts plan units. The script times only fresh gate runs, while `rigger stats` counts cache hits too.

## Known departure

The script reads the event store and the progress store by opening their SQLite files directly, in read-only mode with `query_only` set, rather than through a rigger verb. That ties it to the stores' table layout (`events`: `position`, `stream`, `type`, `data`, `meta`, `recorded_at`), which no contract fixes. The clean fix is a read-only export verb in the binary that prints a run's events, which the script would read in place of the tables.
