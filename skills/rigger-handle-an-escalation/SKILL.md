---
name: rigger-handle-an-escalation
description: Act on a unit the loop handed back - `rigger status` (or the dashboard) names it `escalated (awaiting a human)` after it exhausted its remediation attempts. Read this before touching the unit's branch or relaunching the run.
---

# rigger-handle-an-escalation

## Procedure

An escalated unit gave up at its remediation bound - its stage's or fan-out template's own `max_retries`, else `defaults.max_retries` (3 when neither is set), the bound its `reject-recurrence #n/max` blocker line showed. The loop will not retry it on its own; it is waiting on a human decision. Read the recorded lesson for the CONCRETE final failure - via `rigger peers` scoped to the unit's files, or the dashboard - rather than guessing: the escalation lesson carries the actual failing gate or review reason, not a placeholder, and that reason is the bounded remedy you are about to apply.

Apply EXACTLY that remedy on the unit's durable branch (`rigger/u/<unit-id>`, the branch rigger itself created and kept for this unit's committed work across every attempt) - nothing more, nothing less. Then relaunch the run fresh - `rigger run <spec> --base <ref> --fresh` - against the same, otherwise-unchanged spec: the conductor mints a new run boundary, and the loop picks the escalated unit back up with a clean remediation budget. Stop the old driver first if it still runs, so the run you relaunch is the only one driving: A hold (`rigger hold --reason <text>`) stops a run and Ctrl-C stops its driver: a held run launches nothing, a driver that ends while the hold stands is relaunched by `rigger release` alone, and an interrupted driver is never relaunched. Ending a driver's process any other way is a death, and the next `rigger status` (not `--line` or `--json`) or session start (`rigger prime`) brings it back by adoption, as `rigger release` and `rigger resume-unit` do, each invoked from the main checkout, and as the next login does for a run started with `--relaunch-at-login`; no other command relaunches it.

## Anti-move

Never hand-merge the unit's durable branch onto the run branch yourself - that bypasses review and integration and forks the merged code away from what the event log says happened. And never re-implement more than the remedy names: scope creep here is work the next review has no record of and did not ask for. If the remedy genuinely needs more than a bounded fix, that is a reason to amend the spec (see planning-a-spec), not to freelance on the branch.

## See also

rigger-resume-a-run for the DIFFERENT case of a merely-interrupted run, where `--fresh` is the wrong move.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
