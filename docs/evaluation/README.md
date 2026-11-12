# Evaluation

After every landed run of this repository's own workflow, an evaluation measures the run, groups its failures, its time and its grep fallbacks into classes and proposes at most three changes to how the next run is driven. Each run leaves one file here. Together the files record what each change was predicted to move, what it moved, and whether it was kept.

The evaluation is wired to no stage and applies nothing by itself, so it can never block or undo a landing.

## Procedure

Run these from the operator checkout, after the check-in has merged and before the next `rigger run`:

1. `sh .rigger/gates/evaluate.sh run` measures the landed run into a new file here. It then runs the evaluator (`.rigger/agents/evaluator.md`) through `$EVALUATE_AGENT` and checks the result.
   - With `EVALUATE_AGENT` unset, it stops after measuring. Run the evaluator on the file by hand, then run `sh .rigger/gates/evaluate.sh check <file>`.
2. Commit the file on the run branch.
3. Run `sh .rigger/gates/evaluate.sh emit <file>`. It records each proposal as a finding about the file its change touches, so the next run reaches it from that file.
4. Adopt at most one proposal:
   - Set `"adopted"` in the proposals block to its id, or to `"none"`.
   - Make the change and commit both. A `binary` or `spec-skill` change is the spec its `path` names, run like any other spec.
   - The next run's file scores it.

`measure` and `evidence` also read a run that has not landed; the scorecard's `labels.run` then reads `interim`. While a run is live, point `EVALUATE_DB` and `EVALUATE_PROGRESS` at a copy of the store files (each database with its `-wal` and `-shm`, copied together), never at the live stores.

## Evidence sources

`sh .rigger/gates/evaluate.sh evidence <run>` prints the three sources the evaluator classes, as JSON:

| Source | What it holds | A class's evidence |
|---|---|---|
| `failures` | every reject, gate failure, other failure, escalation and deferred gate failure, each with its `rework_hours`: its unit's hours from the failure to the unit's next failure, next merge attempt or integration | stream positions |
| `cost` | the run's unit hours by label, by gate, by unit and by battery; the repeats the log shows (a post-merge battery over the same tree as the unit's pre-merge battery, a battery re-run after a failure, a gate re-run over an input it already judged); relaunched sessions; and `not_shown`, what the log cannot show | stream positions |
| `grep_fallbacks` | the lines agents recorded when they searched the sources by text, grouped by what they searched for: a caller, a text, a symbol, a file, or unclassified. The grouping is by keyword and the lines are self-reported. | `progress:<position>` |

A grep-fallbacks class is a habit gap when the graph could answer and the agent did not ask, and targets a persona; it is a capability gap when the graph cannot answer that kind of query, and targets the lookup surface as a `binary` spec.

## File format

**Name:** `YYYYMMDD-HHMM-<run8>.md`. The first part is the run's start in UTC and `<run8>` is the first eight characters of its run id, so the names sort in run order. TEMPLATE.md is the layout.

**Sections:**

| Section | Written by | Content |
|---|---|---|
| Scorecard | measure | `json scorecard`: the run's numbers (definitions in baseline.md) |
| Comparison | measure | the headline metrics for the baseline, the last five run files and this run |
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
| `change` | the change, in one sentence |
| `evidence` | the records that show the class |
| `surface` | `persona`, `gate`, `workflow`, `spec-skill` or `binary`. A binary change becomes an ordinary spec. So does a spec-skill change: the spec-writing skills ship to every project that uses rigger, so a change to them is reviewed like a change to the binary. A binary change is to rigger's own code and tests: rigger is language and process agnostic, so the binary never gains a gate, a mandatory step or a language rule. |
| `path` | the file to change, or the spec to write |
| `form` | `check`, `gate` or `instruction`. A `gate` is a workflow gate, so its surface is `gate` or `workflow`. |
| `metric` | a measured scorecard key, or `<map>.<key>` of a by-map. A cost class's metric is a time measure (`hours_by_label`, `gate_hours_by_gate`, `repeated_hours`, `unit_elapsed_hours`, `time_shares`, `gates_share` or `post_merge_gates_share`); a grep-fallbacks class's is `grep_fallbacks` or `grep_fallbacks_by_target`. |
| `predicted_move` | the signed move in the metric's units. A cost proposal lowers its metric by at most the value it holds. |
| `hours_saved` | the hours per run the change removes, the ranking key; null for a grep-fallbacks class, which ranks last |

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
- **No class from what the log cannot show.** The evidence lists under `not_shown` what no event records, such as when a session made its first edit or whether it ended by running out of context; a repeat whose hours are null or 0 is no class either.
- **Unmeasured and self-reported figures stay labelled.** `grounding_share` is always null: nothing in the log marks the end of an agent's orientation, so it is never estimated. `grep_fallbacks` counts only the fallbacks agents reported, and `grep_fallbacks_by_target` groups them by keyword. `relaunched_sessions` counts relaunches the log records, not their cause. Time shares follow the method only when `labels.time_shares` says so; otherwise they are approximate.

## Known departure

The script reads the event store and the progress store by opening their SQLite files directly, in read-only mode with `query_only` set, rather than through a rigger verb. That ties it to the stores' table layout (`events`: `position`, `stream`, `type`, `data`, `meta`, `recorded_at`), which no contract fixes. The clean fix is a read-only export verb in the binary that prints a run's events, which the script would read in place of the tables.
