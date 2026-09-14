# 91 - Mutation testing is a check-in stage of the DAG, run once, remediated once

**Goal:** spec 73 put the mutation sweep inside the implementer's every round
(`.rigger/agents/rust-engineer.md:41-70`, gated by `build.mutation: "on"` in
`.rigger/workflow.yml:111`), so a unit that takes four review rounds pays four full
`cargo mutants --in-diff` sweeps, each about an hour on many cores and about 47G of scratch,
and reviewers re-derive or re-run the same sweep cold. On the spec-88 run (four units in
parallel) that was the dominant cost: load average 125 on 32 cores and `/home` at 97%.
Operator rule (2026-09-11): cargo-mutants runs ONLY after the normal dev cycle has completed
and the code is about to be checked in, and remediates once - and that belongs in the DAG
flow, not in application code. Today the DAG cannot say it: a stage's `needs` are satisfied by
integrated STAGE names (`ready_stages`, src/conductor.rs), and the fan-out `implement`
template never integrates as itself (only its units do), so a stage after it never becomes
ready; and `max_retries` is run-wide (`defaults.max_retries`, src/config.rs:374), so no stage
can bound its own remediation.

## Design

TWO GENERIC RULES IN THE CONDUCTOR, decided - the whole code change: (1) a `needs` entry that
names a fan-out template stage is satisfied when EVERY unit expanded from that template has
integrated (units escalated or failed terminal leave it unsatisfied, so the run's escalated
fixpoint stays loud); (2) a stage may set `max_retries` and it overrides
`defaults.max_retries` for that stage's units. Nothing mutation-specific enters the
conductor: no sweep, no accounting writer, no cargo-mutants path.

THE CHECK-IN STAGE IN THE DEFINITION, decided - shipped in the rigger workflow template and
in this repository's `.rigger/workflow.yml`:

```yaml
gates:
  mutation:
    run: >-
      test -n "$RIGGER_RUN_BASE" &&
      git diff "$RIGGER_RUN_BASE" -- '*.rs' > unit.diff &&
      rm -rf "$MUTANTS" && mkdir -p "$MUTANTS" &&
      TMPDIR="$MUTANTS" cargo mutants --in-diff unit.diff --timeout-multiplier 1.5 -j 2
    kind: core
stages:
  checkin:
    needs: [implement]
    agent: rust-engineer
    max_retries: 2
    gates: [fmt, clippy, build, test, mutation]
    on_pass: merge
    coverage: "mutation efficacy of the whole spec diff"
```

where `$MUTANTS` is the registered mutants root keyed by the stage's unit
(`<mutants-root>/<unit>`, exported by the conductor to every gate command like
`CARGO_TARGET_DIR` already is, reaped at unit terminus), and `$RIGGER_RUN_BASE` is the
run's base commit: the run branch's tip at the moment the run started, recorded on
`RunStarted` as `base_tip` (a field on an existing event type, not a new type) and exported
to every gate command. The whole-spec diff is `git diff "$RIGGER_RUN_BASE"`, never a
merge-base with the run branch: the check-in stage's worktree branches from the run branch
AFTER every implement unit has integrated, so `git merge-base rigger-run HEAD` is HEAD there
and the diff would be empty - the sweep would certify nothing. A run whose `RunStarted`
predates this field has no base to diff against; the gate's `test -n` fails loud rather than
sweeping an empty diff. The stage's task text (config, not
code) is the kill-or-justify protocol spec 73 wrote for the implementer: read
`mutants.out/outcomes.json`, kill each missed mutant with a strengthened test or justify it by
an `exclude_re` entry in `.cargo/mutants.toml` with a one-line reason, commit, and record
`<unit>-mutation-accounting` (spec 73's deterministic per-mutant shape) as a DecisionMade.
Its gate fails while a missed mutant is neither killed nor justified, so the loop is: sweep
(gate) -> one remediation round -> sweep again -> integrate, else escalate. `max_retries` is an
ATTEMPT bound with the same meaning as `defaults.max_retries` (a value of 1 escalates on the
first failed gate, as the conductor's own tests state), so the stage declares `max_retries: 2`:
the sweep, one remediation round, the sweep again; the first live run (spec 89) escalated on
its first miss under a value of 1 and was resumed by the operator with one attempt
to the operator with the accounting on record. `build.mutation` is retired from the schema
(an explicit value is a validation error naming this spec) and the enabled-but-absent refusal
(`resolve_mutation_layer`) moves to the `mutation` gate: a workflow that lists it refuses to
start when `cargo-mutants` is not on the path.

NO SWEEP IN THE LOOP, decided: the implementer persona's mutation block is removed together
with its `unit.diff`/TMPDIR choreography; the reviewer personas (adjudicator, architecture
reviewer, SDET) never invoke cargo-mutants; the SDET author is unchanged. Spec 73's persona
pins are superseded by pins on the template's `checkin` stage and `mutation` gate.

CONSTRAINTS WALK: cargo-mutants reports zero mutants - the gate passes, accounting records
`mutants: 0`. The sweep itself fails to build (copied tree, disk) - the failure taxonomy's
existing `infra` classification reruns it and never charges the stage. A workflow without a
`checkin` stage - no sweep, exactly `build.mutation: off` today. A unit of `implement` still
open when the rest have integrated - `checkin` waits; an escalated unit keeps it waiting and
the run stops at the escalated fixpoint as today. Speculative implement candidates - the
template need counts integrated units only, the losing candidates never integrate. Crash
resume - readiness is recomputed from the log on every step; the stage's gate outputs are
scratch.

## Notes (non-criteria)

The per-round sweeps also drove the scratch-placement and reaper incidents recorded for
spec 89; this spec removes their trigger, spec 89 keeps the guards. This spec's own run is
launched with the shipped definition minus any sweep (there is no `checkin` stage until it
lands); the first run to use the stage is the one after it.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No new event type; no new dependency. `cargo-mutants` stays operator-installed locally and
  install-action in CI.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves A STAGE MAY NEED THE TEMPLATE: a stage whose `needs` names the fan-out
  implement template becomes ready exactly when every unit expanded from it has integrated,
  stays unready while any unit is open, failed or escalated, and a stage-level `max_retries`
  overrides the run default for that stage's units. This criterion OWNS the two conductor
  rules; the definition is criterion 2's, NOT this one's.
- [ ] a test proves THE CHECK-IN STAGE IS DEFINITION: the shipped template and this
  repository's workflow define the `mutation` gate and the `checkin` stage as above, the gate
  command receives the unit-keyed mutants root and reaps it at terminus, `build.mutation` is
  rejected by validation with a message naming this spec, and a workflow listing the gate
  refuses to start without `cargo-mutants`, and a timed-out mutant's test process dies with
  the `cargo` that launched it (the shipped test runner marks itself `--pdeathsig=KILL`; a
  launcher-exits fixture proves no process survives, so a sweep can never wait on an orphan's
  pipe). This criterion OWNS the template, the gate environment, the runner guarantee and the
  schema retirement; the persona text is criterion 3's, NOT this one's.
- [ ] a test proves NO SWEEP IN THE LOOP: no persona under `.rigger/agents/` invokes
  `cargo mutants`, an implementer round on a fixture creates no mutants directory, and the
  `checkin` stage's task text carries the kill-or-justify protocol and the accounting shape.
  This criterion OWNS the persona and task text only.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
