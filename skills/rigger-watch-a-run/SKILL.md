---
name: rigger-watch-a-run
description: Monitor a run you just launched, or one that has driven unattended a while, for the five signals a run can be failing on even while every other view still looks healthy. Read this before walking away from a launched run.
---

# rigger-watch-a-run

## Procedure

On EVERY look, check all FIVE signals below, not just the one you already suspect - a run can read healthy on any single signal while another one is quietly failing, which is why liveness reads healthy in a stalled run (signal 5 exists for exactly that case):

1. **escalated blockers** - a unit `rigger status` (or the dashboard) marks `escalated (awaiting a human)`. Respond with `rigger-handle-an-escalation`.
2. **heartbeat staleness** VS LIVE AGENT PROCESSES - an in-flight agent's last heartbeat is stale but its worker process is actually gone, not merely slow (the driver quit, crashed, or the machine slept). Respond with `rigger-resume-a-run`.
3. **dash liveness** - the dashboard URL does not answer, `rigger status` says it is not serving, or a browser just spins. Respond with `rigger-restore-the-dash`.
4. **reject-recurrence trend** - a unit keeps failing the SAME finding rather than converging (reject-recurrence at or past the diagnose threshold). Respond with `rigger-diagnose-churn`.
5. **frontier progress** - is the run actually consuming what it spawns? A spawn id surviving consecutive looks, an hours-old last run event under "working" agents, or a repeating wave is a STALL even though every signal above reads clean - this is why progress is its own signal, not a restatement of liveness. Respond: stop the driver and diagnose before another round spends.

FIRST instruction, every time: on launch, ARM `rigger watch` under the harness's background monitor - it polls store and status on its own (default every 180s, `--interval <s>` to change it) and folds these same five signals, plus a sixth store-integrity check of its own, into one printed line per anomaly. The manual look above is the FALLBACK for when nothing is armed, exercised at least once per remediation cycle even while `rigger watch` is running.

## Anti-move

Do not make polling `git log` or `ps` by hand the PRIMARY view - a shell only shows what a shell can see, and misses the signals the store and status already resolve for you (escalation, reject-recurrence, frontier progress). And do not hand-intervene in a run that is merely SLOW, not stuck: a long-running unit with fresh heartbeats and advancing store events is working, not stalled, and hand-driving it only races the loop (see rigger-resume-a-run's own anti-move).

## See also

rigger-handle-an-escalation, rigger-resume-a-run, rigger-restore-the-dash, and rigger-diagnose-churn - the four response skills this protocol routes to by name; never invent a response beyond them.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
