# 02 - Targeted remediation feedback

**Goal:** inject the exact rejection reason and failing-gate evidence into the retry so the next attempt addresses the specific failure, not a re-grounded restart.

## Problem

On an adjudicator reject or a gate failure, `run_single_stage` remediates by RE-GROUNDING: it loops and calls `build_prompt(st)` again, which rebuilds the prompt from the grounder's seed plus the graph subgraph. The specific reason the last attempt failed reaches the next attempt only indirectly - if it happened to be emitted as a `DecisionMade` / `LessonLearned` that the graph subgraph later surfaces - and the immediate, concrete evidence is otherwise discarded.

The conductor already holds both pieces of evidence at the moment it decides to remediate:

- `run_gates` builds each `gate::GateResult` whose `evidence` field is the compact PASS/FAIL summary (`gate::compact`: the verdict plus up to five failure-signal lines). Today `run_gates` returns only a `bool`; the per-gate evidence is dropped.
- `run_adjudicator` receives the agent's `AgentResult.output` and passes it to `verdict_approves`, which returns only a `bool`. The rejection reasoning in that output is dropped after the approve/reject decision.

So the architecture's stated intent ("re-ground, re-implement **with the feedback**", §4 lifecycle) is not actually realized: the feedback is thrown away before the retry.

## Design

Carry the last failure's specifics into the next attempt's prompt.

- **Capture the failing gate evidence.** Change `run_gates` to return the failing gates' compact summaries alongside the pass/fail result (e.g. return a small struct or `(bool, Vec<String>)` where the strings are the `GateResult.evidence` of the gates that failed). `run_single_stage` keeps these in a `last_failure` accumulator for the current unit.
- **Capture the adjudicator's rejection reasoning.** Change `run_adjudicator` (and `review_unit`, which calls it) to surface the adjudicator's `AgentResult.output` (or the reject reason parsed from it) on a reject, not just the `bool`. `run_single_stage` records it into the same `last_failure` accumulator when review does not approve.
- **Thread it into the next prompt.** `run_single_stage` passes the accumulated `last_failure` (empty on the first attempt) into prompt assembly. `build_prompt` gains an optional prior-failure argument (or a sibling `build_retry_prompt`) that, when the failure is non-empty, prepends a first-class, clearly delimited block: a heading such as "Your previous attempt was rejected / failed for the following - address exactly these:" followed by the failing-gate summaries and the adjudicator's reject reasoning. On the first attempt the block is absent and the prompt is byte-identical to today's.
- **Persist for the workflow driver.** Emit the captured specifics as an event (a remediation/feedback event, or reuse the lesson mechanism) so the in-Claude-Code workflow driver's shim can surface them to the agent at the tool boundary too, not only via the regenerated prompt string. The escalation lesson emitted in `emit_lesson` on the final `UnitEscalated` includes these final specifics (the concrete reason), replacing the current generic "its gates or review would not pass".

## Done when

- [ ] a failed gate's compact summary is captured and threaded into the next attempt's prompt for that unit
- [ ] an adjudicator rejection's reasoning is captured and threaded into the next attempt's prompt for that unit
- [ ] the retry prompt contains an explicit "previous attempt rejected/failed for X" block, asserted by a test using a stub driver/gate that fails the first attempt and checks the second prompt contains the prior failure detail
- [ ] the first attempt's prompt is unchanged (no prior-failure block) when there is no prior failure
- [ ] the escalation lesson emitted on `UnitEscalated` includes the final failure specifics rather than the generic placeholder
- [ ] the existing remediation and escalation tests still pass
