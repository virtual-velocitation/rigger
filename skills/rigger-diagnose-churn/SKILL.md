---
name: rigger-diagnose-churn
description: Act on a unit whose blocker line shows `reject-recurrence #n/max (remediating)` past roughly 3 attempts, or whose diffs are oscillating rather than converging. Read this before blaming the model or the panel, or reaching for `max_retries`.
---

# rigger-diagnose-churn

## Procedure

By the time reject-recurrence reaches the diagnose threshold (3, the same bound `rigger watch`'s reject-recurrence-trend signal alerts on), do the FINDING AUDIT before reacting to the raw attempt count: read every blocking finding against the diffs it cites - each finding names a checkable fact, and the audit is comparing that fact against what the diff actually does, not trusting the finding's prose. A high attempt count on its own proves nothing about what actually went wrong.

SEPARATE infra-caused attempts before judging the rest: a finding about a deleted worktree, a thrashed shared build cache, or a quota-killed agent is an INFRA failure, not a semantic one - it inflates the attempt count without saying anything about whether the unit's actual approach is wrong. Fix infra in the binary via its own spec; never let it count toward, or be blamed as, review strictness.

Once the infra noise is set aside, look at what remains: if the SURVIVING, factually-correct findings keep citing the SAME constraint against different, otherwise-reasonable diffs, the spec itself is self-contradictory - no implementation can satisfy a contradiction, so every attempt is individually correct to reject and the run will churn forever without a spec change. Fix it with the amendment protocol (`planning-a-spec`: amend Design and Global constraints only, commit when no step is mid-flight, then `rigger emit DecisionMade` naming the spec file so in-flight reviewers see it through the graph immediately).

For any OTHER recurring pattern, match the SIGNATURE you found against `planning-a-spec`'s own "Quick reference: churn signature -> planning defect" table - it maps what a rejection loop looks like (twinned units, a bundled checkbox, an unresolved either/or, findings blaming process-local state, a paraphrased criterion) to the specific catalog class and its fix at spec time, so the diagnosis names the actual defect class rather than just "it keeps failing".

## Anti-move

Never blame the model or the panel without having run the finding audit first - a reviewer that is factually correct every single round is not the problem, even when it rejects the same unit five times in a row. And do not reflexively raise `defaults.max_retries` to buy another attempt: a bigger budget spent against the SAME unaudited failure reproduces exactly the failure the audit exists to catch, just more expensively.

## See also

planning-a-spec owns the churn-signature table and the spec-amendment protocol this procedure applies; rigger-watch-a-run names reject-recurrence as one of its five signals, routing here by name; rigger-handle-an-escalation for when a unit exhausts its remediation budget rather than merely churning.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
