# Experiment protocol: is the review panel's quality regression caused by the lens model?

Pre-registered 2026-08-11, before any arm is run. This document is the decision rule; results
must be judged against it as written, not reinterpreted after the fact.

## Background and observed evidence

The review panel's agents resolve their model through the UNPINNED alias `opus`. The event log's
per-spawn `model_resolved` stamps show the alias silently re-pointed between campaigns:

- Through 2026-08-04 (specs 43-59, ~1,000 stamped spawns): `claude-opus-4-8[1m]`.
  First-pass yields across those campaigns were routinely 85-100%, escalations rare.
- From 2026-08-10 (spec 60, the first run after the re-point): `claude-opus-5[1m]`.
  Both in-flight units hit reject-recurrence #5/6 - unprecedented in this project's history.

`rigger validate` warns about the re-point and prescribes `rigger canary --if-model-changed`.
This protocol is that measurement, done as a controlled comparison.

## Hypotheses

- H1 (the suspicion): the Opus 5 lens tier over-rejects - it raises blocking findings against
  correct work at a materially higher rate than alternates, and this drives the remediation
  churn.
- H0: rejection rates reflect the work under review, not the lens model; arms differ within
  noise.

The panel's suspected failure mode is FALSE POSITIVES (rejecting correct work), so the primary
metric is the control false-positive rate, not the catch rate. A canary run before the fix for
issue #24 already showed BOTH known-good controls rejected; attribution was broken, so which
tier drove it is unknown. That is the gap this experiment closes.

## Step 0 - deterministic finding audit (no model calls)

Before any arm runs: audit the FINAL-round rejection findings of spec-60's u1 and u4 against
their diffs, classifying each finding as FACTUALLY CORRECT (the defect is real and present),
FACTUALLY WRONG (the claimed defect is not in the diff), or JUDGMENT CALL. This is fully
deterministic - each finding names a checkable fact. If a majority of blocking findings are
factually wrong, H1 gains strong direct support independent of the arms; if a majority are
factually correct, the spec-60 churn is substantive regardless of model, and the arms measure
only the panel's general tendency.

## Instrument

`rigger canary` over the committed 6-item corpus (4 planted defects across the defect classes +
2 known-good controls), with the spec-61 fixes landed first - they are prerequisites, not
niceties:

- tolerant per-tier attribution (issue #24) - otherwise per-tier rates read 0 regardless of arm;
- first-class control/false-positive line (issue #24);
- honest n/a over fake zeros (issue #24);
- parallel tiers and sharded items (issue #22) - otherwise each arm costs 2.5h serial;
- per-tier model override pinning (spec 61 amendment, below).

## Arms

Everything identical - binary build, corpus, personas, git rev, config - except the LENS tier's
pinned model id. Adversary and adjudicator stay pinned to the current `claude-opus-5` resolution
in ALL arms, isolating the lens variable.

- Arm O5: lenses pinned to the Opus 5 resolution (the current, post-re-point state).
- Arm S5: lenses pinned to Sonnet 5.
- Arm O48: lenses pinned to the Opus 4.8 resolution (the historical baseline; include if the
  API still serves it - this arm directly tests "regression since 5.0" rather than merely
  "difference from Sonnet").

n = 3 full-corpus repetitions per arm, interleaved (O5, S5, O48, O5, S5, O48, ...) so drift in
anything ambient spreads across arms. Total ~9 corpus runs, ~50 spawns each.

## Metrics (per arm, aggregated over repetitions)

1. PRIMARY - control false-positive rate: known-good control items rejected / control items
   scored (6 per arm).
2. Catch rate per tier on planted defects (guards the trivial degenerate: a lens that approves
   everything has a perfect FP rate).
3. Verdict correctness overall.
4. Stability: adjudicator order-shuffle flip rate (already probed per item).
5. Findings volume per item per tier (over-flagging tendency).
6. Per-spawn duration (from the spec-61 timing work) - cost context, not a quality metric.

## Pre-registered decision rule

- SUPPORTS H1: Arm O5's control false-positive count exceeds Arm S5's by >= 2 (out of 6
  control scorings) AND Arm O5's planted-defect catch rate is not more than 1 catch better.
  The same comparison against Arm O48, if run, carries the same rule.
- REFUTES H1: control false-positive counts within 1 of each other across arms with comparable
  catch rates.
- Anything between is INCONCLUSIVE: extend to n=5 repetitions before judging.
- Regardless of arms: if Step 0 finds the spec-60 blocking findings factually correct in the
  majority, the spec-60 churn is not evidence of model fault, whatever the arms show about
  general tendency.

## Determinism and provenance

LLM sampling is not bitwise-deterministic; this design gets "fairly deterministic" the honest
way: fixed ground truth with planted expected outcomes, pinned model ids (never aliases),
identical inputs per arm, interleaved repetitions, a pre-registered threshold, and full
provenance - every spawn's `model_resolved` stamp rides the log, and each arm's scorecard must
record the binary build, corpus content hash, and the per-tier pinned ids, so any reading can be
audited from the store after the fact.

## Sequencing and consequence

Runs after spec 60 completes and spec 61 lands (the instrument fixes). The result gates the
rest of the campaign: if H1 is supported, the lens tier's config pins to the winning model
BEFORE specs 62/63 run, and the pin-vs-alias policy gets recorded as a decision; if refuted,
the alias re-point stands and the spec-60 churn is charged to the work's difficulty, not the
model. Either way `rigger validate`'s drift warning has proven its keep and the pre-launch
check ("validate before launching a run") should become part of the run driver's own preflight
rather than operator discipline.
