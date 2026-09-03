# Spec 72: replan supersede is grounded in the log, not the call

## Problem

Run 57373714-b52 (spec 68) escalated at plan-critique on an unwinnable defect: every replan
round's re-proposed units were ADDED alongside the units they should replace, because
`harvest_proposed`'s ADD path inserts stages without the resolved `criterion_id` the
supersede filter matches on (the baselines are stamped at synthesis; planner-added stages
were not). The DAG accumulated duplicate owners per criterion and the critique gate
correctly rejected every round on rule 7.

A first repair run (10c69e11-3f9) exhausted its full remediation budget and escalated, and
its five review rounds are the evidence this rewrite is built on. Each round's panel
empirically disproved one mechanism family:

- Round 2 (adversary probe): supersession that removes broadly is order-dependent within
  one call - a refine plus a new split sibling must both survive in either event order.
- Round 3 (adjudicator probe): supersession conditioned on plan-critique rounds or gate
  presence never fires in a workflow with no critique gate wired (`round == 0` forever).
- Round 4 (sdet probe): latest-wins-within-a-call contradicts the spec-31 real-split
  guarantee - a planner may legitimately split one criterion into sibling units in one
  planning pass, and `planner_refinement_split_is_still_harvested` plus both same-call
  refine-plus-split tests encode that.
- Round 5 (sdet + adjudicator, independently reproduced): ANY per-call or per-process
  tracking (`added_this_call`, `no_gate_harvest_seen` on `RunCtx`) breaks on resume:
  every `rigger step` is a fresh process whose pre-wave catch-up folds the ENTIRE run
  history through ONE `harvest_proposed` call, so call structure cannot distinguish
  same-wave siblings from cross-wave replans - two historical proposals for one criterion
  read as siblings on every resume and both survive.

The conclusion those five rounds force: the sibling-vs-supersede distinction must be
carried by THE LOG, not reconstructed from call or process structure.

## Design

- PLAN-EPISODE IDENTITY, the load-bearing decision: every `UnitProposed` carries the
  identity of the PLANNING EPISODE that emitted it - one planner pass (an initial plan or
  a replan after a critique reject) is one episode. Two proposals with the SAME episode
  identity are siblings; a proposal whose episode is LATER (by log order of the episodes'
  first events) supersedes earlier episodes' owners of the same criterion. The identity is
  an additive, serde-defaulted field on `UnitProposed` (permitted by the constraints
  below); a conforming value is anything log-unique per planner pass - the planner spawn's
  id is the natural choice. Where it is computed is free; that it is PERSISTED ON THE
  EVENT is not.
- STATE PLACEMENT, and the banned stand-in: the authoritative sibling-vs-replan state
  lives in the event log (the episode field on each `UnitProposed`). A per-process
  `RunCtx` field, an added-this-call set, a harvest-call counter, or any assumption about
  how many events one `harvest_proposed` call folds is NOT an implementation of this
  requirement - round 5 proved every such stand-in wrong on resume.
- THE SUPERSEDE RULE: a proposal resolving to criterion X removes every live
  (not-integrated, not-terminal) stage serving X from an EARLIER episode - the
  conductor-synthesized baseline counts as earlier than every episode - and never a stage
  from its OWN episode, in any event order. All of one episode's stages survive together
  (the spec-31 real-split guarantee stays green as written). Empty `criterion_id` never
  participates in supersession on either side: an empty-id stage is never removed by any
  supersede and no supersede matches on emptiness. The surviving set is a pure function of
  the logged events - identical under live incremental folding, crash-resume catch-up
  (whole history in one call), and cold-start replay.
- NO ROUND OR GATE CONDITIONING (round-3 probe stands): no `gate.is_none()` branch, no
  round comparison; the same rule runs in every workflow, critique gate wired or not.
- BACK-COMPAT, decided here so no unit has to: a logged `UnitProposed` WITHOUT the episode
  field (every pre-existing event) belongs to one implicit LEGACY episode that orders
  after the baseline and before every identified episode. Legacy events are therefore
  mutual siblings (never cannibalize each other retroactively), and any new identified
  episode supersedes their owners - the exact recovery a wedged historical run needs.
- THE STAMP (unchanged from the first authoring, outcome-level): every stage the ADD path
  inserts for a proposal that resolves to a criterion carries that criterion's stable id
  in its `criterion_id` field, however the value is computed. Unmatched proposals (the
  genuinely-new sub-unit path recording `unmatched-proposal`) keep an empty id. The
  same-id fold path still folds needs only.

## Done when

- [ ] A proposal from a later planning episode serving a criterion owned by an earlier
  episode's planner unit leaves exactly one live owner - the later unit, stamped with the
  criterion's stable id - proven by a test at the `harvest_proposed` seam driving two
  episodes over one criterion. This criterion OWNS cross-episode supersession; same-episode
  behavior is criterion 2's, NOT this one's.
- [ ] All proposals carrying one episode identity survive one harvest together in any event
  order - including two serving the same criterion (a real split) and a refine beside a
  new empty-id split sibling - proven at the `harvest_proposed` seam, with the existing
  spec-31 tests (`planner_refinement_split_is_still_harvested` and both same-call
  refine-plus-split tests) green unmodified. This criterion OWNS same-episode behavior;
  cross-episode supersession is criterion 1's, NOT this one's.
- [ ] A fresh conductor process whose pre-wave catch-up folds a pre-populated run history in
  ONE `harvest_proposed` call yields the same surviving stage set as the live incremental
  fold of that history - proven by a test at the resume seam whose history holds both a
  two-episode supersession and a one-episode split, and by a companion case where the
  history's proposals carry NO episode field (the legacy episode: mutual siblings, then
  superseded by a new identified episode). This criterion OWNS the resume/catch-up and
  back-compat surface.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type. One additive, `#[serde(default)]` field on `UnitProposed` is
  permitted and expected; every existing field's serialization is unchanged, and every
  pre-existing logged event must deserialize and fold under the back-compat rule above.
- No new gate, no new conductor stage; the change lives in the fold and the emit path.

## Notes

- Constraints walk record (why each corner is closed): empty proposal set - no fold
  change; repeated same-id re-emit - fold-needs-only path untouched; same-episode repeat
  for one criterion - siblings by rule (criterion 2); crash-resume and cold start -
  criterion 3 pins both; concurrent actors - single conductor writer, n/a; revert - the
  log is append-only, a superseded stage is removed from the fold, never from history.
- The five prior review rounds' probes are the acceptance intuition: every one of them
  (round-2 order reversal, round-3 no-gate workflow, round-4 spec-31 split, round-5 fresh
  RunCtx catch-up) must pass under this design; criteria 1-3 encode them.
- Critique-prompt visibility and `ready_stages` dedup remain OUT of scope, deferred
  deliberately (unchanged from the first authoring). The prior unit branch
  `rigger/u/u-c1-stamp-criterion-id` holds five attempts of exploratory work; the fresh
  run re-plans from this rewritten spec and owes it nothing.
