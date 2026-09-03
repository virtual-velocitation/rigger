---
name: rigger-resume-a-run
description: Continue interrupted work after a dead driver (spent quota, a crash, a laptop that slept mid-run) or `rigger status` showing an agent 'in flight' with a stale heartbeat. Read this before relaunching a run or reaching for `--fresh`.
---

# rigger-resume-a-run

## Procedure

Diagnose first: `rigger status` (or the dashboard) shows each in-flight agent's last progress report and heartbeat age. A stale heartbeat with no recent store event means the DRIVER died mid-run (quota ran out, the process crashed, the machine slept) - it does not mean the run itself is broken; the event log already holds every decision and gate verdict the run made before the driver stopped.

Relaunch the same blessed driver on the same spec WITHOUT `--fresh` - `rigger run <spec>`, `rigger serve <spec>` / `rigger workflow <spec>`, or the native `/rigger <spec>` workflow with its `fresh` argument left unset. Because the run lives in the event log, not in the dead process, the conductor's own run-starting step adopts the existing run instead of minting a new one: it replays the log, rebuilds its in-memory state, and continues exactly where the dead driver left off. No unit restarts from zero and no work already recorded is lost.

`--fresh` is for a DIFFERENT situation, not this one: a run wedged in a terminal state (for example a plan-critique escalation) on a spec that is otherwise UNCHANGED. It is a one-shot new-run boundary, never the default way to continue interrupted work.

## Anti-move

Never hand-drive `rigger step` yourself in a shell to "help it along" - the driver owns stepping, and a hand step races it, which can double-spawn a unit or wedge the frontier (see using-rigger). And do not reach for `--fresh` reflexively just because a run looks stuck: on a merely-interrupted run it abandons the adoptable state your relaunch would otherwise have continued from, in exchange for nothing - reserve it for the genuinely wedged-terminal case above.

## See also

rigger-handle-an-escalation for the run-level and unit-level terminal states `--fresh` genuinely exists for.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
