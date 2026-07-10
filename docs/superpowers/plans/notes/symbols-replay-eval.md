# Symbols-vs-turbovec evaluation: methodology, stated margin, recorded status

Spec 15, unit 4, done-when criterion 4: the `symbols` grounder is selectable via
`defaults.grounder: symbols`, and its adoption as the shipped default is gated on a
quantified stats comparison against the **turbovec baseline** (grep is only a floor) that
`symbols` must exceed **by a stated margin**. This note is that gate: it fixes the
methodology and the numeric margin, and records the measurement status. Per the standing
decision `d15-loud-selection-default-unchanged`, **the shipped `defaults.grounder` stays
turbovec until this gate is cleared** - nothing in spec 15 flips it.

## What `rigger replay --against` measures, and what it does not

`rigger replay <run|latest> --against <config-rev>` re-drives a completed run's recorded
trajectory under a candidate config in an isolated scratch namespace and prints the stats
diff versus the recorded baseline. Critically, the re-drive **runs no agent and no gate
command**: every spawn is answered from the baseline's recorded `SpawnResult`s and every
gate from its recorded `GateVerdict`s. It re-derives only the run's *shape* - which stages,
which review tier, which budget, which gates the candidate config dictates.

Consequence for a grounder-only config change: swapping `defaults.grounder` from turbovec to
symbols does **not** alter the stages, tiers, budgets, or gate set, so an offline replay of a
grounder-only diff is **shape-neutral by construction** - its stats diff is expected to be
zero. Offline replay therefore serves as the **shape-neutrality check** (confirming the
config edit does not perturb the trajectory), *not* as a measurement of grounding quality:
replay never re-grounds, so it cannot observe whether symbols surfaces better context than
turbovec.

Measuring grounding *quality* (does symbols raise first-pass yield?) requires a **live A/B**:
two real campaigns over the same task corpus, one per grounder, because only a live run
actually invokes the grounder. That live A/B is the operator gate below; it is out of scope
for an automated unit and is deliberately not run here.

## Procedure

### Step A - offline shape-neutrality check (deterministic, cheap)

1. On a branch, set `defaults.grounder: symbols` in `.rigger/workflow.yml` and commit it;
   note its rev as `<symbols-rev>`.
2. For each recorded baseline run `<run>` (recorded under turbovec):

   ```bash
   rigger replay <run> --against <symbols-rev>
   ```

   Expect a **zero diff** on every column (first-pass yield, gate runs, escalations): a
   grounder-only change does not change the trajectory shape. A non-zero diff here means the
   candidate config changed something *other* than the grounder and must be investigated
   before any quality claim.

### Step B - live A/B yield measurement (the real quality gate; operator-run)

1. Pick a fixed task corpus (a set of comparable units/specs) and a fixed model panel.
2. Run the corpus **twice**, identically except for `defaults.grounder`:
   - baseline: `defaults.grounder: turbovec`
   - candidate: `defaults.grounder: symbols`
3. From each run's `rigger stats`, record:
   - **first-pass yield** = units accepted by review on attempt 1 / total units
   - **escalation rate** = units that exhausted remediation and escalated / total units
   - **review-reject rate** = review rejections / total review verdicts
4. `grep` is the floor: also run the corpus under `defaults.grounder: grep` so the
   turbovec-vs-symbols delta is read against how far each sits above the literal floor.

## Stated margin (the objective pass/fail rule)

`symbols` replaces turbovec as the shipped default **only if**, over a live A/B (Step B)
covering **at least 3 runs totaling at least 20 units**, ALL of the following hold:

1. **first-pass yield**: symbols >= turbovec **+ 5 percentage points** (the stated margin);
2. **escalation rate**: symbols <= turbovec (no regression); and
3. **review-reject rate**: symbols <= turbovec (no regression);

and the offline shape-neutrality check (Step A) is a zero diff on every run. `grep` never
counts toward clearing the gate - it is only the floor. If any clause fails, the default
stays turbovec.

The +5pp margin is the concrete number `pc-margin-not-open` asked for: a raw tie or a sub-5pp
edge is **not** enough, so noise cannot flip a load-bearing default.

## Recorded status (2026-07-09)

- **Selectability**: DONE and pinned. `defaults.grounder: symbols` selects the real
  `Symbols` grounder (feature on) via `main::select_grounder`, and is a **loud** error when
  the binary is built without the `symbols` feature (`grounder::symbols_feature_missing_error`,
  mirroring turbovec) - never a silent grep degrade. Pinned by
  `tests/cli.rs::ground_via_symbols_grounder_ranks_a_definition_first` (end-to-end selection +
  ranking) and `grounder::tests::symbols_without_the_feature_is_a_loud_error_not_a_grep_fallback`.
- **Ranking fixture**: DONE and pinned. Definition > reference > incidental-prose ranking is
  pinned by `symbols::grounder::tests::ranks_a_definition_above_an_incidental_prose_mention`,
  `a_reference_ranks_below_a_definition_of_the_same_name`, and the CLI test above.
- **Offline shape-neutrality check (Step A)**: not yet recorded here - it requires a recorded
  baseline run to replay and a committed `<symbols-rev>`; it is deterministic and adds no
  agent cost, so it is the operator's first step when a baseline corpus exists. Expected
  result: zero diff (a grounder-only change is shape-neutral).
- **Live A/B yield (Step B)**: NOT RUN. It is a multi-run live campaign, out of scope for an
  automated unit. Until it is run and clears the stated margin above, **the shipped
  `defaults.grounder` remains turbovec** (the hard freeze this spec keeps).

**Gate outcome: UNCLEARED -> default unchanged (turbovec).** This is the required spec-15
disposition, not a deferral of code: the code path to flip the default is deliberately absent
from this spec (`defaults.grounder` is untouched everywhere), so clearing the gate later is a
one-line config change an operator makes on the evidence above, never a code change here.
