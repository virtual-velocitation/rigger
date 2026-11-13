# Evaluation baseline

The reference point every run file's comparison table starts from: the loop's accuracy and velocity over 14 completed runs (82 integrated units) of this repository's own workflow, measured by the method below. `sh .rigger/gates/evaluate.sh measure` reads the `json baseline` block; a metric the block leaves null has no baseline figure. The baseline's `escalations` is per run, as one run file's is.

```json baseline
{
 "runs": 14,
 "units": 82,
 "first_pass_rate": 0.34,
 "attempts_per_unit": 2.78,
 "rejects_per_unit": 1.44,
 "gate_fails_per_unit": 0.37,
 "escalations": 0.5,
 "agent_busy_hours_per_unit": 3.31,
 "wall_hours_per_criterion_unit": null,
 "rework_share": null,
 "units_in_flight_mean": 1.367,
 "units_in_flight_max": 9,
 "serial_share": 0.0,
 "queued_share": 0.0,
 "chain_share": 0.315,
 "co_schedulable_share": 0.0
}
```

Every figure in the block but the scheduling ones is the script's own measure of those 14 runs, totalled over their units, so a run file compares like with like. The scheduling figures are pooled the same way and are approximate (see Scheduling below). The figures first recorded for the same runs agree with the script but one:

| Metric | Recorded | Script | Difference | Reason |
|---|---|---|---|---|
| units | 82 | 82 | 0 | |
| first_pass_rate | 0.34 | 0.341 | 0 | |
| attempts_per_unit | 2.78 | 2.780 | 0 | |
| rejects_per_unit | 1.52 | 1.439 | -0.08 | The recorded figure also counts the 7 rejects of two runs stopped before any unit integrated (61a52a6d, b4556cd0); with them the script gives 1.524. A run file measures one run, so the block holds 1.44. The recorded escalations figure leaves those two runs out (with them, 0.57 per run). |
| gate_fails_per_unit | 0.37 | 0.366 | 0 | |
| escalations | 0.5 | 0.5 | 0 | |
| agent_busy_hours_per_unit | 3.31 | 3.311 | 0 | |

The baseline is exactly these 14 runs, by run id and first `RunStarted` in Central time, with their integrated units:

| Run | Started | Units |
|---|---|---|
| cf4ceb24 | 2026-09-07 04:49 | 5 |
| ceb43bf1 | 2026-09-09 02:00 | 4 |
| a56448fd | 2026-09-10 13:56 | 4 |
| 4dabfe2c | 2026-09-11 07:50 | 5 |
| 8220e91a | 2026-09-13 01:36 | 4 |
| c6ccaf8e | 2026-09-13 18:47 | 7 |
| 0f85a1f5 | 2026-09-15 00:18 | 4 |
| a1c45db7 | 2026-09-15 08:46 | 7 |
| 14e2b32d | 2026-09-17 07:52 | 7 |
| cf12df79 | 2026-09-18 03:54 | 5 |
| f5021a11 | 2026-09-19 17:43 | 7 |
| eb8f55d6 | 2026-09-21 14:18 | 10 |
| ec9ab8f5 | 2026-09-22 18:56 | 5 |
| 34c25db1 | 2026-09-23 10:38 | 8 |

Two runs of the same period stopped before any unit integrated, 61a52a6d (2026-09-08 06:55) and b4556cd0 (2026-09-19 16:56), and are excluded: rejects_per_unit is 1.44 without them and 1.52 with them. Every later run file compares against this one set.

1 + rejects_per_unit + gate_fails_per_unit is not attempts_per_unit, by definition: attempts count the failures of units that integrated, while rejects and gate fails count every failure in the run, a unit that never integrated included.

**Scheduling.** The shares are the 14 runs' live seconds in each slot state over their 467.134 live wall hours: serial 0.218 h, queued 0.059 h, chain 146.93 h. The in-flight mean is weighted by live wall hours, and the maximum is the highest of the runs'. They are approximate, not the method's figures:

- None of the 14 runs recorded a heartbeat or a hold, so a driver outage counts as live time and a slot stays open across it.
- The units in flight are computed for 9 of the 14 runs. cf4ceb24, ceb43bf1, a56448fd, 4dabfe2c and 8220e91a recorded no gate verdict, so their units in flight would count agent sessions only, and the block leaves them out. On the 9 runs alone the shares are serial 0.001, queued 0.0 and chain 0.208.
- The width bound differed between runs: the per-run maxima of units in flight are 2, 2, 2, 2, 3, 3, 4, 5 and 9.

co_schedulable_share does not depend on time and is exact for all 14 runs. None of the 96 grounded units is co-schedulable: all 303 grounded pairs conflict, and 72 of them ran together. No run recorded a radius before its unit started, so the radius rule set no schedule.

## Method

Every figure is read one run at a time from the event store's run stream (`proj-<project>-run`), except the grep fallbacks and the driver's heartbeat, which the progress store holds, and a tested commit's tree, which the repository resolves. The script opens both stores in SQLite's read-only mode with `query_only` set, so it cannot write to either. A run's slice runs from its first `RunStarted` to the first `RunStarted` of another run, so a relaunch of the same run stays in its slice.

| Metric | Definition |
|---|---|
| units_integrated | `UnitIntegrated` events whose `commit` is not the review-only marker, one per unit id. The plan and the plan critique integrate with the marker and are not counted; the check-in is counted. |
| criterion_units | integrated units whose `UnitStarted` names a `spec_criterion` and that do not join a stage. A unit joins a stage when its `needs` names no unit started in the run, as the check-in waits on the fan-out stage as a whole. |
| units_not_integrated | units started in the slice that did not integrate, review-only included |
| attempts_per_unit | integrated units plus their `UnitFailed` events of any cause, over integrated units |
| rejects_per_unit, gate_fails_per_unit | every `UnitFailed` in the slice with `cause` `reject`, or `gate:<name>`, over integrated units |
| first_pass_rate | integrated units with no `UnitFailed` of any cause, over integrated units |
| escalations | distinct units with a `UnitEscalated` event |
| agent_busy_hours_per_unit | the union of `SpawnRequested` to `SpawnResult` intervals, paired by spawn id, each capped at 65 minutes, over integrated units. Null when the slice holds no spawn record. |
| wall_hours_per_criterion_unit | first `RunStarted` to last landing, over criterion units |
| outage_hours | the part of that wall time the driver was not running, by the outage rule below. Null when the run recorded neither a heartbeat nor a hold. |
| live_wall_hours_per_criterion_unit | wall time minus outage time, over criterion units |
| rework_share | capped spawn time of attempts 1 and later (the `#<n>` of the spawn id) over all capped spawn time, summed rather than unioned |
| reject_items_by_class | each reject's `required` items by the class their `finding` opens with (wrong code, unmet criterion, missing test); a reject with none counts as `no-items` |
| rejects_by_cause | each reject by the `cause` of the adjudicator's reject verdict that preceded it |
| gate_fails_by_gate | `UnitFailed` with `cause` `gate:<name>`, by name; `DeferredGateFailed` and red `GateVerdict` runs are counted beside it |
| unit_elapsed_hours | over integrated units: each unit's first `UnitStarted` or `SpawnRequested` to its last `UnitIntegrated`, outage seconds removed, summed. A unit's events are those its id, spawn id or replay key opens with. |
| time_shares | each second of unit elapsed time takes one label, the first that covers it in this order: `spawn` and `merge_spawn` (a `SpawnRequested` to its `SpawnResult`, uncapped; `merge_spawn` when it opens at or after the unit's first `integrate-merge-attempt` status), `gates` and `post_merge_gates` (a battery, from a writer's `SpawnResult`, an implementer's or an SDET author's, to the last fresh `GateVerdict` before the unit's next `SpawnRequested`; a cache-hit verdict is not fresh; `post_merge_gates` when the last of its verdicts that ran a gate, not a skipped one or the driver's closing settle marker, has a post-merge replay key), `integration` (the unit's last `reviewed` status to its landing). A second with no label is `waiting`. Each label's seconds over unit elapsed time. An outage second takes no label and is not unit elapsed time. Null when the slice holds no spawn record. |
| gates_share, post_merge_gates_share | `time_shares.gates` and `time_shares.post_merge_gates` |
| grep_fallbacks | `AgentProgress` lines on the run's own stream of the progress store (`.rigger/progress.db`) whose `activity` opens with `grep-fallback:`. Self-reported: agents record these lines themselves, so a fallback no agent reported is not counted. |
| grep_fallbacks_by_target | the same lines grouped by what they searched for, by the first keyword rule a line matches, in the order `caller`, `text`, `symbol`, `file`; a line matching none is `unclassified`. Self-reported, as grep_fallbacks is. |
| hours_by_label | each time-shares label's seconds of unit elapsed time, in hours, so the labels sum to unit_elapsed_hours. Units run in parallel, so these hours add up past the run's wall time. Null when the slice holds no spawn record. |
| gate_hours_by_gate | over every battery of the integrated units, pre- and post-merge: each fresh `GateVerdict`'s time since the previous verdict of its battery, or since the battery opened, outage seconds removed, summed by gate. The driver runs a battery's gates one at a time, so a gate's time is its run plus the driver's step before it. |
| repeated_hours | the hours of three repeats the log can show. `post_merge_same_tree`: post-merge batteries whose last fresh verdict tested a commit with the same tree as the unit's last pre-merge battery's commit (the commit is the one an `input_digest` names), so the merge changed nothing they gate; null when a tree does not resolve in the repository. `retry_batteries`: pre-merge batteries opened by a writer whose spawn id is attempt 1 or later, re-running the battery after a failure. `same_input_reruns`: fresh verdicts of a gate over an `input_digest` an earlier fresh verdict of that gate already judged. A battery's hours run from its opening to its last fresh verdict, outage seconds removed. |
| relaunched_sessions | `UnitStatus` spawn-session records whose `resumed_from` names a session other than their own `session_id`: launches that continued an earlier session. It counts relaunches, not what stopped the earlier session. |
| failure_rework_hours | per `UnitFailed`, by position: its unit's hours from the failure to the unit's next `UnitFailed`, its next `integrate-merge-attempt` status or its `UnitIntegrated`, whichever comes first, outage seconds removed |
| units_in_flight_mean, units_in_flight_max | over the live wall time (first `RunStarted` to last landing, outage seconds removed): the started units with a `SpawnRequested` open to its `SpawnResult` or a gate battery open, as a time-weighted mean and a maximum. Null when the slice holds no spawn record; agent sessions only, and labelled so, when it holds no `GateVerdict`. |
| slot_hours | the live wall time by slot state, in hours. A unit holds a slot from its first `UnitStarted` to its `UnitIntegrated` or `UnitEscalated`. A unit is ready at the latest first `UnitIntegrated` of the needs its `UnitStarted` names: at the run's start with none, and never while a need has not integrated. A need naming no started unit is a stage, met when every unit that joins no stage has integrated. Each second takes the first state that applies: `idle`; with one slot held, `solo_ready_held` or `solo_ready_waiting` (a unit not started is ready, inside or outside a `RunHeld` to `RunReleased` span), `solo_needs_waiting` (a unit not started, none ready), `solo_none_left`; with two or more, `multi_ready_waiting` (a unit not started is ready) or `multi`. |
| serial_hours, queued_hours, chain_hours | `slot_hours.solo_ready_waiting`, `slot_hours.multi_ready_waiting` and `slot_hours.solo_needs_waiting` |
| serial_share, queued_share, chain_share | each of those hours over the live wall time |
| co_schedulable_share | derived, since no event carries it: from each unit's latest `BlastRadiusComputed` (`safe` files), two units conflict when their radii share a file or either is empty; in unit id order each unit joins the first batch it conflicts with no member of; the units in batches of two or more over the grounded units, those with a radius. It reproduces the share the driver's closing line states for spec 121's landed run: 0.0%, 24 grounded units, 1 with an empty radius. |
| grounded_units, empty_radii, conflicting_pairs, conflicting_pairs_ran_together | the units with a `BlastRadiusComputed`, those whose radius is empty, the grounded pairs that conflict, and those of them whose slots overlapped in live time |
| radii_recorded_before_start | the units whose first `BlastRadiusComputed` precedes their first `UnitStarted` in the stream. A wave that applies the radius rule records a ready unit's radius before admitting it. |
| grounding_share | unmeasured, always null: no event marks the end of an agent's orientation, and `FileTouched` is recorded from the diff after the gate battery, so no event times a session's first edit |
| context restarts | unmeasured, no key: `SpawnResult` records no end reason, so a session that ran out of context is not told from one that finished; relaunched_sessions counts relaunches whatever stopped them |
| labels | `run`: `landed`, or `interim` with what is still open; `time_shares`: whether the shares follow the outage rule or are approximate; `grep_fallbacks`: self-reported; one line each for hours_by_label, gate_hours_by_gate, repeated_hours (and which trees did not resolve), failure_rework_hours and grep_fallbacks_by_target; `scheduling`: the scheduling definitions, and whether the figures are approximate (no gate verdict, or no heartbeat or hold) |

**The outage rule.** While it runs, the driver records a `SupervisorBeat` on the run's progress stream every 30 seconds. An outage is:

- a gap of more than 120 seconds between consecutive beats: the driver was not running;
- a hold a host raised (`RunHeld` whose `by` is not `operator`: a rate limit, a billing or a login fault), from the hold to its `RunReleased`: the session it stopped waits for the release;
- a hold the operator raised, only while no unit has an agent session or a gate battery open: running work finishes under an operator hold, and only the next launch waits.

A hold still standing at the end of the slice runs to the slice's last event. A run with no heartbeat counts its holds only. A run with neither has `outage_source` `none recorded`: its wall time and time shares include any outage, its `outage_hours` and live wall time are null, and its time-shares label reads approximate.

The rule reproduces a hand measurement: on a copy of the store taken at 2026-11-10 10:51 CST, the first 16 units of spec 121's run give 51.18 hours of unit elapsed time, split as agent sessions 62.7%, gate batteries 19.1%, post-merge batteries 14.3%, merge sessions 3.3%, integration 0.3% and waiting 0.2%, the split found by removing the same run's outages by hand.

## Reference points

Later runs, measured by the script. The runs differ in conditions and are few, so they show direction, not magnitude. Busy hours need spawn records on the run stream; live wall time needs a heartbeat or a hold.

| Spec | Run | Units (criterion) | First pass | Attempts/unit | Rejects/unit | Gate fails/unit | Escalations | Busy h/unit | Wall h/criterion unit | Live wall h/criterion unit |
|---|---|---|---|---|---|---|---|---|---|---|
| 101 | 1e8f9a61 | 7 (6) | 0% | 7.14 | 5.14 | 0.86 | 2 | 8.37 | 13.57 | - |
| 112 | e0ceb29b | 7 (6) | 29% | 2.29 | 1.43 | 0.14 | 0 | 2.18 | 2.83 | - |
| 113 | efafd1a6 | 10 (9) | 40% | 1.80 | 0.70 | 0.10 | 0 | 1.02 | 5.12 | - |
| 107 | dc9cca7d | 21 (20) | 10% | 2.62 | 1.29 | 0.33 | 0 | - | 3.62 | - |
| 116 | 98df67ad | 15 (14) | 47% | 1.53 | 0.47 | 0.07 | 0 | - | 1.54 | - |
| 119 | 5482845d | 17 (16) | 12% | 2.47 | 1.47 | 0.00 | 0 | - | 3.44 | - |
| 123 | 2f782f38 | 5 (4) | 80% | 1.20 | 0.40 | 0.00 | 0 | 1.77 | 2.99 | 2.99 |
| 121, first 16 units | 050ed244 | 16 (16) | 81% | 1.19 | 0.19 | 0.00 | 0 | 1.44 | 6.95 | 1.70 |

Their scheduling, for the whole of each run (spec 121's landed run, all 25 units). Live wall hours are wall hours for the runs with no heartbeat or hold, and units in flight need spawn records:

| Spec | Run | Live wall h | In flight mean / max | Serial h | Queued h | Chain h | Co-schedulable | Conflicting pairs (ran together) |
|---|---|---|---|---|---|---|---|---|
| 101 | 1e8f9a61 | 81.413 | 1.065 / 2 | 1.19 | 0.0 | 28.731 | 0.0 | 28 of 28 (3) |
| 112 | e0ceb29b | 16.986 | 1.457 / 2 | 0.705 | 6.902 | 4.89 | 0.0 | 28 of 28 (4) |
| 113 | efafd1a6 | 46.113 | 1.103 / 2 | 35.09 | 3.729 | 3.733 | 0.0 | 55 of 55 (6) |
| 107 | dc9cca7d | 72.441 | - | 0.0 | 0.0 | 72.059 | 0.0 | 231 of 231 (0) |
| 116 | 98df67ad | 21.604 | - | 0.0 | 0.0 | 21.337 | 0.0 | 120 of 120 (0) |
| 119 | 5482845d | 55.093 | - | 0.0 | 0.0 | 54.833 | 0.0 | 153 of 153 (0) |
| 123 | 2f782f38 | 11.951 | 0.995 / 1 | 0.0 | 0.0 | 9.256 | 0.0 | 15 of 15 (0) |
| 121 | 050ed244 | 41.092 | 1.591 / 2 | 0.025 | 23.038 | 13.565 | 0.0 | 276 of 276 (15) |

- **107, 116, 119 and 123** each planned their criterion units as one chain, every unit needing the one before it, so most of their live time is chain hours.
- **113's** 35.09 serial hours include the 33.8-hour silence noted below, from 0.46 to 34.30 hours into the run. Its unit c2 was ready at 0.30 hours and started at 35.38.
- **121** ran under a width bound of two. Ready units waited 23.038 hours behind two running ones, and only 0.025 hours behind one.

Where these differ from the figures first recorded for the same runs:

| Spec | Recorded | Script | Reason |
|---|---|---|---|
| 101, 112 | as above | as above | none |
| 113 | as above; most of the 46 wall hours were a usage-limit outage | wall 46.1 h, outage not measured | The log shows a 33.8-hour silence with no heartbeat and no hold, so the script cannot subtract it, and its wall time includes it. |
| 107 | units 1-5: 2.80 attempts; units 7-19: 2.77 attempts, 1.23 rejects, 0.38 gate fails, per unit; hours as mean per-unit wall | the whole run: as above | On the same unit subsets the script gives 2.80 and 2.77 attempts, and 1.31 rejects and 0.46 gate fails per unit. The recorded rejects and gate fails sum to two failures fewer than the recorded attempts imply (1 + 1.23 + 0.38 = 2.61, against 2.77). The run recorded no spawn, so it has no busy hours. |
| 116 | 14 units, 43% first pass, 1.57 attempts, 1.51 h | 15 units, 47%, 1.53, 1.54 h | The recorded figures divide by the 14 criterion units and leave the check-in out (6/14, 22/14); the script counts it, as the baseline does (7/15, 23/15). The recorded wall time starts about half an hour after `RunStarted`. |
| 119 | 15 units, 1.44 attempts, 3.44 h | 17 units (16 criterion), 2.47 attempts, 3.44 h | Every criterion unit but one has at least one reject, 25 in all. 1.44 is 23/16, which is the 16 criterion units plus one for each of the 7 units rejected more than once; it does not follow from the log's counts. |
| 121 | 16 units, 3.2 h of unit elapsed time per unit | 51.18 h over 16 units, 3.20 | none |
