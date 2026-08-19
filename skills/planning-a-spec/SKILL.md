---
name: planning-a-spec
description: Use when writing, splitting, or amending a spec for a rigger loop run - before launching /rigger on new work, when a plan-critique gate rejects a decomposition, when a run churns in review and the spec is suspect, or when turning bug reports or design discussions into Done-when criteria.
---

# Planning a spec

## Overview

A loop run's outcome is mostly decided at spec time. This skill is the authoring procedure for
the failure catalog in `docs/handbook/planning-field-guide.md`; the shape rules live in
`docs/handbook/authoring-loops.md` (rules 1-8). Follow the recipe in order - each step exists
because skipping it has a recorded escalation attached.

## The recipe

**1. Ground the Goal in evidence.** State the problem with measured numbers and real anchors
(`file.rs:line`, event counts, durations) - look them up via `rigger graph --show/--around` and
`rigger peers`, not memory. A goal an implementer can re-verify is a goal an adjudicator can
hold the line on.

**2. Close every disposition.** Scan the draft for "or", "either", "could", "worth
considering". Each becomes a decision recorded in Design ("BACKEND SCOPE, decided here so no
unit has to: ...") or an explicit Notes deferral OUT of scope. A disposition left open is a
rejection loop: implementer picks one reading, reviewer picks the other.

**3. Run the constraints walk.** For every Global constraint x every criterion (and every
mechanism Design prescribes), walk the corner-case list: empty, repeated, REVERT/rollback,
concurrent actors, crash-resume, cold start (fresh process, empty memory). Write what must
happen into the spec. If a prescribed mechanism fails a corner under a constraint, the spec is
self-contradictory - fix it now; the panel will otherwise find it around attempt 5.

**4. Place state explicitly.** Any criterion about dedup, persistence, recovery, budgets, or
caches names WHERE the authoritative state lives (the log, a file, a flock) and names the
inadequate stand-in ("an in-memory seen-set is NOT an implementation of this guard") so the
easy-but-wrong implementation is rejected by the text, not by attempt 4's adversary.

**5. Write criteria to the criterion contract.** Each checkbox is:
- ONE observable behavior, self-contained in one-to-two sentences, copyable verbatim as a
  unit's whole contract (the planner copies it; the conductor baseline-matches the copy);
- named verification ("a test proves X ... pinned at the Y seam"), not just a state;
- ownership INSIDE the checkbox ("This criterion OWNS the selection surface") with exclusions
  on every neighbor that could claim the concern ("the advisory is criterion 3's, NOT this
  one's").
Type shapes, tables, long detail: a non-criteria Notes section. Two behaviors joined by "and":
two checkboxes.

**6. Carry the house constraints.** Hyphens not em dashes (U+2014 fails the diff gate); both
feature lanes green; no new event type unless the spec's whole point is one; fallback stated
for any criterion that might be impossible; anything the gates cannot see flagged for the
adjudicator to demand evidence on.

**7. Preflight, then launch.** `rigger validate` is mandatory (it catches model-alias drift -
run `rigger canary --if-model-changed` on a warning); `rigger reset --runs` before a large run;
anchor `base=` on the ref the work must land on. Launch via the /rigger workflow only.

## Amending mid-run

Design and Global constraints only - criteria checkboxes are the run's identity (editing one
orphans the live run). Commit when no step is mid-flight, then `rigger emit DecisionMade` naming
the spec file so in-flight reviewers see the change through the graph immediately. Still
escalates? Restart fresh: durable branches carry the work, the budget resets.

## Quick reference: churn signature -> planning defect

| Signature in the run | Catalog class | Fix at spec time |
|---|---|---|
| Plan-critique rejects twins with identical criteria | F1 ownership | OWNS inside checkbox + neighbor exclusions |
| Plan-critique names one criterion, two mitigations | F2 bundling | Split the checkbox |
| Panel rejects every mechanism variant; `cause: spec-ambiguity` | F3 contradiction | Constraints walk (esp. revert) |
| Implementer and reviewer disagree on a reading | F4 disposition | Decide it in Design |
| Guard/dedup rejected as process-local | F5 state placement | Name where state lives + the banned stand-in |
| Plan baseline-match fails, paraphrased units | F6 copyability | One-sentence criteria, detail to Notes |
| First run after a while churns everywhere | F7 environment | validate preflight + canary on drift |
| High attempt counts, findings about worktrees/caches/quota | F8 infra noise | Audit findings; fix infra separately |

## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
