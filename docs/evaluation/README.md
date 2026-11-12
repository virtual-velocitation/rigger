# Evaluation

After every landed run of this repository's own workflow, an evaluation measures the run, groups its failures by cause class and proposes at most three changes to how the next run is driven. Each run leaves one file here. Together the files record what each change was predicted to move, what it moved, and whether it was kept.

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

## File format

**Name:** `YYYYMMDD-HHMM-<run8>.md`. The first part is the run's start in UTC and `<run8>` is the first eight characters of its run id, so the names sort in run order. TEMPLATE.md is the layout.

**Sections:**

| Section | Written by | Content |
|---|---|---|
| Scorecard | measure | `json scorecard`: the run's numbers (definitions in baseline.md) |
| Comparison | measure | the headline metrics for the baseline, the last five run files and this run |
| Previous proposal | measure | `json previous`: the previous file's adopted proposal, scored |
| Diagnosis | evaluator | one paragraph per cause class; `json classes`: `[{class, summary, evidence[]}]` |
| Proposals | evaluator | `json proposals`: `{adopted, proposals[]}` |

**Fields:**

- **Class:** a stable kebab-case slug. It names the cause, never the incident, and is reused for the same cause in later files.
- **Evidence:** stream positions from `sh .rigger/gates/evaluate.sh evidence`.
- **Proposal fields:**

| Field | Meaning |
|---|---|
| `id` | `P1` to `P3`, in rank order |
| `class` | one of the file's classes |
| `change` | the change, in one sentence |
| `evidence` | the records that show the class |
| `surface` | `persona`, `gate`, `workflow`, `spec-skill` or `binary`. A binary change becomes an ordinary spec. So does a spec-skill change: the spec-writing skills ship to every project that uses rigger, so a change to them is reviewed like a change to the binary. |
| `path` | the file to change, or the spec to write |
| `form` | `check`, `gate` or `instruction` |
| `metric` | a measured scorecard key, or `<map>.<key>` of a by-map |
| `predicted_move` | the signed move in the metric's units |
| `hours_saved` | estimated hours saved per run, the ranking key |

## Rules

- **At most three proposals**, ranked by hours saved. A proposal with no metric or no non-zero predicted move fails `check`, as does one missing any other field.
- **One adopted proposal per run.** The operator chooses it, and nothing is applied automatically.
- **Recurrence:** when a class appears in two consecutive files, the second file's proposal for it must be a `check` or a `gate`, not instruction text. Persona and spec-skill changes are instruction text.
- **Scoring:**
  - The actual move is this run's metric minus the previous run's.
  - The verdict is `kept` when the metric moved in the predicted direction, `reverted` otherwise.
  - The operator undoes a reverted change.
- **Numbers come only from the script.** The evaluator writes no figure into a measured section.
- **Unmeasured and self-reported figures stay labelled.** `grounding_share` is always null: nothing in the log marks the end of an agent's orientation, so it is never estimated. `grep_fallbacks` counts only the fallbacks agents reported. Time shares follow the method only when `labels.time_shares` says so; otherwise they are approximate.

## Known departure

The script reads the event store and the progress store by opening their SQLite files directly, in read-only mode with `query_only` set, rather than through a rigger verb. That ties it to the stores' table layout (`events`: `position`, `stream`, `type`, `data`, `meta`, `recorded_at`), which no contract fixes. The clean fix is a read-only export verb in the binary that prints a run's events, which the script would read in place of the tables.
