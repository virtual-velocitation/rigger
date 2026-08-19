# Spec 72: replan supersede stamps the criterion id

## Problem

Run 57373714-b52 (spec 68) escalated at plan-critique after exhausting all six rounds
on a defect no replan could fix: every round's re-proposed units were ADDED alongside
the units they were meant to replace, so the DAG accumulated duplicate owners per
criterion (u68c1 and u68c1r3 both live for criterion 1; three live owners of
criterion 2; u68c3/u68c3r3; u68c4/u68c4r3) and the critique gate correctly rejected
each round on rule 7 (one-live-unit-per-criterion).

Root cause, verified by direct read: `harvest_proposed`'s supersede filter
(src/conductor.rs:7487-7495) finds prior owners by `st.criterion_id == criterion_id`,
but the ADD-path `Stage` insert (src/conductor.rs:7539-7547) fills only
name/agent/needs/coverage/gates and leaves `criterion_id` at `..Default::default()`
(empty). The conductor-synthesized baselines DO stamp it
(src/conductor.rs:9078), so the FIRST harvest supersedes the baseline correctly -
but the planner-added replacement is inserted without the id, making it invisible
to every later round's supersede filter. Each replan round therefore adds a
duplicate instead of replacing the prior owner: an unwinnable reject loop.

## Design

- THE STAMP, decided here so no unit has to: every stage the ADD path inserts for a
  proposal that resolves to a criterion carries that criterion's stable id in its
  `criterion_id` field. This is an OUTCOME requirement on the inserted stage, not a
  prescription of where the value is computed - a pre-pass, a threaded local, or an
  insert-site expression are all conforming so long as the stored stage carries the id.
- Unmatched proposals (a proposal `resolve_served_criterion` maps to no criterion -
  the genuinely-new sub-unit path that records `unmatched-proposal`) keep an EMPTY
  `criterion_id`, exactly as today, and an empty id NEVER participates in supersession
  on either side: an empty-id stage is never removed by any supersede, and no proposal's
  supersede ever matches on emptiness. A genuinely-new unit owns no criterion and stays
  outside supersession entirely.
- The same-id fold path (a re-emit under an id that already exists) still folds
  needs only and never touches `criterion_id`; this spec changes the ADD path alone.
- THE SUPERSEDE RULE, decided here (round-5 disposition; supersedes the round-4 rule,
  which the sdet proved internally contradictory against the spec-31 real-split
  guarantee). A call's proposals form ONE WAVE. A proposal resolving to criterion X
  removes every live (not-integrated, not-terminal) stage serving X that EXISTED BEFORE
  THIS CALL - the conductor-synthesized baseline and any prior-call planner unit - and
  NEVER a stage this same call inserted, in any order. ALL of one call's inserted stages
  survive together: a deliberate same-call split of one criterion into sibling units is
  preserved (spec 31 criterion 2 - `planner_refinement_split_is_still_harvested` and the
  same-call refine-plus-split tests stay green as written). Empty ids never participate
  in supersession on either side. The surviving set is therefore a pure function of
  (pre-call stages, the call's proposal set) - order-independent within the call by
  construction, and deterministic under replay.
- MECHANISM BOUNDARY, decided here (splitting what round 3 proved from what round 4
  over-generalized): per-call tracking of which stages THIS call inserted (an
  added-this-call set or equivalent) is REQUIRED by the rule above and is a conforming
  mechanism, not a violation. What stays FORBIDDEN is conditioning supersession on
  plan-critique rounds, gate presence, or any advancing counter - the adjudicator's
  round-3 probe stands: a workflow with no critique gate keeps `round == 0` forever, and
  the rule must behave identically there. No `gate.is_none()` branch, no round
  comparison; the same one rule runs in every workflow.
- State placement: the authoritative record is the event log's `UnitProposed`, which
  already carries `criterion_id` on the wire (src/conductor.rs:1310); `stages` is the
  per-step in-memory fold of those events, so stamping at the fold point applies
  identically on replay and resume. No migration, no new persistence, no wire change.

## Done when

- [ ] A planner re-proposal serving a criterion already owned by a prior
  planner-added unit leaves exactly one live owner: a test pinned at the
  `harvest_proposed` seam drives two harvest rounds for the same criterion (round 1
  adds a planner unit superseding the baseline, round 2 re-proposes under a fresh id)
  and proves the sole surviving stage is the round-2 unit whose stored
  `criterion_id` equals that criterion's stable id.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- Both feature lanes stay green: `cargo test` with default features AND with
  `--no-default-features`.
- No new event type; `UnitProposed`'s serialized shape is unchanged.

## Notes

- OUT of scope, deferred deliberately (recorded so they are not silently dropped):
  - Critique-prompt visibility: `build_dag_critique_prompt`
    (src/conductor.rs:5589) surfaces neither the spec path nor its non-criteria
    sections to the gate. The gate holds Read/Grep and demonstrably reads the spec
    file today (round-5 adversary cited exact spec line numbers), so this is an
    ergonomics gap, not a correctness gap.
  - `ready_stages` criterion-level dedup as defense in depth against an
    already-polluted DAG: with the stamp in place a new run cannot pollute, and the
    one polluted run (57373714-b52) is abandoned in place; its log survives.
- The guard on the supersede filter (never remove an integrated or terminal stage)
  is existing behavior this spec relies on and does not modify.
