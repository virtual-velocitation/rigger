# The Agentic SDLC: what agents own, what humans keep

This document maps the software development lifecycle onto Rigger's agent roles. It answers two questions for every stage: *can an agent own this today*, and *if so, which role, checked by what*. The short version: agents own everything whose "done" is machine-verifiable; humans own everything whose "done" is a judgment call about intent.

## The load-bearing principle

**"Done" is a machine-verifiable predicate, never "looks done."** Every stage you hand to an agent must terminate in a check a machine can run: a test that passes, a gate that exits zero, an adjudicator verdict grounded in cited evidence. If you cannot state the check, the stage is not ready to delegate - fix the spec, not the agent.

The corollary is where humans sit: at the points where the check itself is being *decided*. Humans write the acceptance criteria. Humans resolve design forks. Humans receive escalations. Everything between those points is agent territory.

## Stage-by-stage map

```
 SDLC stage            Owner            Rigger role(s)            Verified by
 ─────────────────────────────────────────────────────────────────────────────
 Requirements          HUMAN            (you, writing the spec)   loop-readiness gate
 Design decisions      HUMAN + agent    planner surfaces forks;   human answers;
                                        human decides             decision -> event log
 Planning              AGENT            planner                   coverage gate (every
                                                                  criterion -> a unit)
 Implementation        AGENT            implementer /             red -> green TDD +
                                        rust-engineer             the cargo gates
 Verification          MACHINE          gates (fmt, clippy,       exit codes
                                        build, test)
 Code review           AGENT            lenses -> adversary ->    adjudicator verdict
                                        adjudicator
 Integration           AGENT            the conductor (merge      green gates + approve;
                                        on_pass)                  CI re-verifies
 Documentation         AGENT            scribe                    docs match live code
 Release               HUMAN            (you: PR merge, tags)     CI + your review
 Maintenance /         AGENT + HUMAN    lessons resurface in      LessonLearned events;
 retrospective                          future grounding          escalations to human
```

### Requirements: the spec is yours

Rigger's entry gate (`loop-readiness`) refuses to start a run unless the spec has enumerable acceptance criteria. This is deliberate friction: the single highest-leverage thing a human does in this lifecycle is write "Done when" bullets that a machine or a strict reviewer can check. See [authoring-loops.md](authoring-loops.md#the-spec) for the format.

Do not delegate spec-writing to the loop that will consume the spec. An agent grading its own homework against criteria it wrote is the failure mode this whole architecture exists to prevent.

### Planning: the planner refines a deterministic baseline

The conductor itself creates one baseline unit per acceptance criterion - the deterministic decomposition. The **planner** (an Opus-tier agent) refines that baseline: it splits a criterion into several units, adds a necessary sub-unit, or wires a dependency edge, always by emitting `UnitProposed` events. Two hard rules bind it:

1. **Coverage is provable.** Every unit names the acceptance criterion it covers, the files it touches, and the gate that proves it. The coverage gate blocks the run if any criterion has no unit.
2. **No silent scope.** A need that surfaces mid-plan with no covering criterion is flagged as scope-creep and surfaced - never silently added. Scope is a human decision.

The planner reads code before planning. Most "new" work extends prior art; a plan that reinvents an existing seam is a planning defect.

### Implementation: one unit, one worktree, one writer

The **implementer** owns exactly one fully-specified unit inside an isolated git worktree. Its lifecycle is fixed: write the failing test, confirm RED, implement minimally, confirm GREEN, run the unit's named gates, commit. It cannot recurse (spawn sub-agents) and it cannot see or touch another unit's worktree - isolation is what makes fan-out safe.

What it *can* see is the shared memory: before writing code it grounds itself (`rigger ground`, `rigger graph --around <file>`), and while working it reads peers' live decisions (`rigger peers`) and records its own (`rigger emit DecisionMade ...`). File isolation with decision awareness is the core split: agents never touch each other's work, and never work blind to each other's reasoning.

### Verification: gates are the floor, not the review

Gates are named shell commands (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build`, `cargo test`) declared in the workflow YAML. They run *before* review, every time, and a red gate feeds straight back into the unit's remediation loop - a reviewer's time is never spent on code that does not compile.

Gates carry an autonomy level that ratchets: after enough clean passes a `manual` gate proposes its own promotion to `auto_notify`; any non-manual gate that fails demotes itself back to `manual`. A graduated gate can never become a silent hole.

### Code review: three tiers, adversarial by construction

Review is **per unit**, not a downstream stage, and it is a contest:

- **Tier 1 - the lenses.** Two or more expert reviewers (architecture, technical/SDET) read the unit's diff in parallel. Each has a lane and stays in it.
- **Tier 2 - the adversary.** Reviews *the lenses' findings and the diff*, and tries to prove the lenses wrong: it surfaces the substantive issues they all missed, refutes overreach on narrow factual grounds only, and runs the gates itself rather than trusting anyone's report. Its success is catching real problems, not converging.
- **Tier 3 - the adjudicator.** The neutral final judge. Weighs the lenses against the adversary, is strict on design and architecture adherence, and renders the approve/reject verdict that gates integration.

The economics: every finding is a win for the adversary and a failure for everyone upstream of it - the implementer should have left it nothing to find. That framing pushes quality left; the target state is a strict adversary that consistently *loses* because first-pass work is airtight.

### Integration: merge on green, remediate on red

Only `adjudicator: approve` + green gates integrates a unit (`on_pass: merge`). A reject or a gate failure re-enters that same unit's remediation loop - the implementer is re-run with the feedback and a re-grounding pass - bounded by `max_retries` (default 3). Exhausting the bound **escalates to a human**; it never silently drops the unit and never silently merges it.

### Documentation: the scribe owns accuracy

Code lenses do not review docs. A dedicated **scribe** agent cross-checks documentation claims against the live code after a change lands and fixes staleness in place. Feed it the code delta, not the whole tree.

### Maintenance: lessons are memory, escalations are training data

When a unit escalates, when a reviewer catches a defect class, when an agent hits a wall - that is recorded as a `LessonLearned` event. Lessons attach to the files and decisions they concern, so future grounding *resurfaces them exactly when the next agent is about to touch the same ground*. This is how the fleet stops repeating mistakes: not by a bigger prompt, but by structural memory.

## Where humans stay in the loop, permanently

1. **Acceptance criteria.** You define done.
2. **Design forks.** When two architectures are genuinely valid, an agent surfaces the fork with a recommendation; it does not pick for you.
3. **Escalations.** A unit that exhausts `max_retries`, a budget breaker trip, a spec defect flag - all land on your desk with the evidence attached.
4. **Scope.** Discovered work gets flagged, not silently added. What becomes a unit is your call.
5. **Release.** The loop lands commits on a run branch; merging that branch to main - and anything else outward-facing - goes through your PR review.

## Adoption order: what to hand over first

If you are introducing agents into an existing SDLC, delegate in this order - it tracks how verifiable each stage's "done" is:

1. **Verification** (day one) - gates are just your existing CI commands, run earlier and per-unit.
2. **Implementation of well-specified units** - the strongest gate coverage, the tightest isolation, TDD-enforced. This is where the leverage is.
3. **Code review** - start with the panel *advisory* (autonomy `manual`, you read the verdicts), ratchet to gating once you trust its calibration against a few weeks of your own reviews.
4. **Planning** - once your specs are consistently loop-ready, the planner's coverage gate makes decomposition safe to delegate.
5. **Documentation** - the scribe, fed deltas.

Do not start by delegating design or requirements. Those are where a wrong call is cheapest to make and most expensive to discover.
