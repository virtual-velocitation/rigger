# Planning a loop run: the field guide

The handbook's [authoring-loops](authoring-loops.md) rules say what a loop-ready spec IS. This
guide is the other half: how to PRODUCE one, distilled from this repository's own event store -
every escalation, plan-critique rejection, and multi-attempt review churn it has recorded. The
one-sentence summary of all of it: **almost every expensive run failure was decided before the
run started, at planning time, and each failure class below has a mechanical countermeasure.**

## The failure catalog

Each class below appeared in the recorded history at least once; the recurring ones are marked.
Read this before writing a spec; the authoring recipe that follows exists to make each class
structurally impossible.

### F1 - Duplicated or ambiguously-owned units (the #1 recurring killer)

Six separate plan-critique escalations trace to one shape: the planner turns one criterion into
TWO units - byte-identical criterion text, identical blast radius, no exclusion naming which
twin owns what ("mechanism" and "proof" twins, "scaffold" and "seam" twins, or whole DAGs
duplicated as parallel chains). The reviewers then enforce each twin against the other and
neither converges.

**Countermeasure:** the OWNS sentence lives INSIDE the criterion checkbox ("This criterion OWNS
the selection surface"), and every neighbor that could plausibly claim the same concern carries
the exclusion by name ("the orphan-id advisory is unit-9's, NOT this unit's"). A criterion whose
ownership sentence sits outside the checkbox gets truncated away when the planner copies
criteria verbatim into units - that truncation is a recorded failure, not a hypothesis.

### F2 - Bundled criteria

One checkbox demanding two mitigations ("a drift monitor AND distilled playbooks") forces the
planner to either split it (creating F1 twins) or build an unreviewably wide unit. The
plan-critique gate correctly escalated a spec for exactly this.

**Countermeasure:** one observable behavior per checkbox. If the sentence contains "and" between
two verifiable outcomes, it is two criteria wearing one checkbox - split it yourself rather
than letting the planner guess.

### F3 - The self-contradictory spec (the most expensive single failure)

A spec prescribed a mechanism (dedup against every key ever recorded), asserted a property of it
("a changed file hashes to fresh keys"), and demanded a constraint (the folded graph equals the
current tree) that the prescribed mechanism violates in a corner case the author never walked
(a file REVERTED to earlier content). No implementation can satisfy a contradiction; the review
panel spent SIX attempts correctly rejecting partial resolutions before the spec was amended.
The panel even ruled `cause: spec-ambiguity` - the system explicitly billing the spec author.

**Countermeasure:** the constraints walk. Take every Global constraint and every criterion and
walk them against the standard corner-case list: empty input, repeated input, REVERT/rollback to
a prior state, concurrent actors, crash-and-resume, cold start (fresh process, empty caches). A
constraint you have not walked against a corner case is a rejection you have scheduled for
attempt 5. When Design prescribes a mechanism, the walk applies to the mechanism too - or drop
the prescription and let the criteria state observables the implementer must find a mechanism
for.

### F4 - Open dispositions

"Removed" and "ignored" are different verdicts on the same files; if the spec has not picked
one, the implementer picks one and a reviewer picks the other (a recorded rejection loop). Any
question a reviewer could reasonably re-litigate - backend scope, what happens on the degraded
path, whether a doc updates - is a disposition the spec must close.

**Countermeasure:** grep your draft for every "or", "either", "could", and "worth considering" -
each is either a decision to make now or a Notes line explicitly deferring it OUT of scope.
Recent specs close these with explicit "BACKEND SCOPE, decided here so no unit has to" blocks;
that pattern generalizes: decide it where you noticed it.

### F5 - State that lives in the wrong place

A guard against CROSS-PROCESS duplication implemented as an in-process seen-set defends nothing:
every driver step and every cold rebuild is its own process, so the set starts empty exactly
when it matters. The class generalizes: any criterion about persistence, dedup, recovery, or
budgets must say WHERE the authoritative state lives (the log, a file, a lock) and the spec must
reject in-memory stand-ins by name, or an implementer will reach for the easy one and a reviewer
will (correctly) reject it late.

### F6 - Criteria that cannot survive verbatim copying

The planner copies criteria into units verbatim and the conductor reconciles proposals against a
baseline match. Over-long criteria, sub-bullets-as-units, and multi-sentence checkboxes get
paraphrased or truncated in that copy, and the mismatch fails the reconcile. The spec-shape lint
flags these; heed it before launch rather than after the plan escalates.

**Countermeasure:** a criterion is ONE self-contained sentence-or-two, copyable as a unit's
whole contract. Type shapes, tables, and long detail go in a non-criteria Notes section.

### F7 - Unpinned environment

Agent models resolve through aliases, and an alias can silently re-point between runs (a
recorded re-point preceded the churniest run in this repo's history and was flagged only by
`rigger validate`, which nobody ran). Gates, corpus, and binary are part of the same
environment.

**Countermeasure:** `rigger validate` is a MANDATORY preflight, not a linter you run when
curious. On a model-drift warning, run the canary (`rigger canary --if-model-changed`) before
trusting a big run to the new resolution.

### F8 - Infra noise misread as semantic failure

Attempt counts inflate from harness defects (assigned worktrees deleted between spawns, shared
build caches thrashed by concurrent lanes, agents killed by quota exhaustion). A run that "took
6 attempts" may have burned half of them on infrastructure. Reacting to the raw count - blaming
the spec, the model, or the panel - misdiagnoses it.

**Countermeasure:** before reacting to churn, audit the blocking findings against the diffs
(they cite checkable facts) and separate infra findings from semantic ones. Fix infra in the
binary via its own spec; never let it masquerade as review strictness.

### F9 - As-built prose that keeps earning new rejections

A unit whose CODE was ratified burned four consecutive attempts on its DOCUMENTATION: each
remedial rewrite of an as-built narrative added a fresh universal claim ("no suppression
decision is involved", "it can leave the graph no further behind", "cases sit outside the
contract") that the next adversarial round falsified against the code. The adjudicator's own
post-mortem: the enumeration is where every falsehood came from, and a shorter true section
clears the bar a longer one has to earn universal-by-universal.

**Countermeasure, at spec time:** when a criterion demands documentation, ask for the RULE
stated short and pinned by an accuracy test, never an exhaustive enumeration of cases or
guarantees the section does not owe. Write the bound into the criterion ("re-render the
paragraph so it states the rule; no new enumeration"). At remediation time the same discipline
is "prefer deletion to replacement": most falsified prose is a claim nobody asked for.

## Amending a spec mid-run

Sometimes the panel proves the spec wrong while the run is live (F3 was caught exactly this
way). The protocol, validated in production:

1. Amend Design and Global constraints ONLY. The criteria checkboxes are the RUN'S IDENTITY -
   the conductor adopts a run by matching criteria, so editing a checkbox mid-run orphans the
   live run. Criteria changes wait for a fresh run.
2. Commit the amendment to the run branch when no step is mid-flight.
3. Emit the clarification as a decision (`rigger emit DecisionMade ...` naming the spec file) -
   in-flight reviewers ground through the knowledge graph and see it IMMEDIATELY, ahead of
   their worktrees picking up the text.
4. If the run still escalates, restart FRESH under the amended spec: durable unit branches
   carry the work forward, the budget resets, and the graph's findings steer the new attempts.

## What good looks like, measured

Runs whose specs followed all of the above have recorded 85-100% first-pass yields, zero
escalations, and flawless 6-wave convergences. The disasters (40 rejections over three
criteria; six attempts against a contradiction; four plan-critique rounds) each map to a
catalog class above. The delta is not model quality or reviewer mood - it is whether the spec
closed these holes before `rigger run` ever started. Use the `planning-a-spec` skill to apply
this guide as a procedure.
