---
name: rigger-restore-the-dash
description: Get the run dashboard serving again when its URL does not answer, `rigger watch` reports it not serving, or a browser just spins. Read this before restarting the dash or touching its marker file by hand.
---

# rigger-restore-the-dash

## Procedure

The dash is a SINGLETON per project: at most one `rigger dash` serves a given project's fixed address at a time. A second `rigger dash` against an address a real rigger dash already answers on reports that address and exits 0 rather than binding a second one - so a not-serving dash is never "already running somewhere else", it is genuinely down.

`rigger status`'s dashboard line is NOT yet a liveness check - it always prints whatever URL was last recorded, even when nothing answers there, so do not trust that line alone. `rigger watch --once` IS the accurate check: it verifies the recorded marker by actually probing its port, and prints a `dash liveness` line naming the dead PID when nothing answers there - trust that over a bare status line.

Restart with a plain `rigger dash` (no flags needed for the default address). The singleton bind then does the right thing either way: if the address is genuinely free it binds and serves; if a live rigger dash is already there after all, it reports that address and exits cleanly instead of fighting it.

The HUNG-HOLDER case is the one that actually hangs a client instead of failing cleanly: the marker records a port whose process died, froze, or was suspended without releasing it, so a fresh probe against that port neither serves nor cleanly refuses - it just hangs, and so does anything waiting on it. The marker's own PID, not a fresh diagnosis, is what names the culprit: `rigger watch` reads it and prints that exact PID on its dash-liveness line. RESUME that process if it is merely stopped (a suspended terminal, a paused container), or KILL it if it is dead weight - THAT pid, the one the marker and `rigger watch`'s own line actually name - then restart with `rigger dash`.

## Anti-move

Never hand-edit the dash marker file to "fix" it - it is a breadcrumb the step path itself writes and overwrites, and a hand-edited value only makes the next real dash's own self-heal harder to trust. And never kill a process by PORT-ADJACENT GUESSWORK ("kill whatever's near the dash port") - resume or kill the EXACT pid the marker and `rigger watch`'s own line name, never a guess.

## See also

rigger-watch-a-run names dash liveness as one of its five signals, routing here by name; rigger-diagnose-churn for the DIFFERENT case of a unit that keeps failing review, not a dead dashboard.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
