---
name: rigger-handle-an-escalation
description: Act on a unit the loop handed back - `rigger status` (or the dashboard) names it `escalated (awaiting a human)` after it exhausted its remediation attempts. Read this before touching the unit's branch or relaunching the run.
---

# rigger-handle-an-escalation

## Procedure

An escalated unit gave up at the remediation bound (`defaults.max_retries`, 3 by default) - the loop will not retry it on its own; it is waiting on a human decision. Read the recorded lesson for the CONCRETE final failure - via `rigger peers` scoped to the unit's files, or the dashboard - rather than guessing: the escalation lesson carries the actual failing gate or review reason, not a placeholder, and that reason is the bounded remedy you are about to apply.

Apply EXACTLY that remedy on the unit's durable branch (`rigger/u/<unit-id>`, the branch rigger itself created and kept for this unit's committed work across every attempt) - nothing more, nothing less. Then relaunch the run fresh - `rigger run --fresh <spec>` (or `rigger serve --fresh <spec>` / the native `/rigger <spec>` workflow with `fresh` set) - against the same, otherwise-unchanged spec: the conductor mints a new run boundary, and the loop picks the escalated unit back up with a clean remediation budget.

## Anti-move

Never hand-merge the unit's durable branch onto the run branch yourself - that bypasses review and integration and forks the merged code away from what the event log says happened. And never re-implement more than the remedy names: scope creep here is work the next review has no record of and did not ask for. If the remedy genuinely needs more than a bounded fix, that is a reason to amend the spec (see planning-a-spec), not to freelance on the branch.

## See also

rigger-resume-a-run for the DIFFERENT case of a merely-interrupted run, where `--fresh` is the wrong move.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
