---
name: rigger-reset-store
description: Store hygiene for rigger's own state - growing .rigger/ disk usage, the bloat advisory from `rigger validate`, or `rigger step`/replay running slow. Read this before running `rigger reset` or touching any store file by hand.
---

# rigger-reset-store

rigger keeps three stores under `.rigger/`, and only one of them holds anything durable:

- `events.db` - the event log. This IS the truth: every decision, finding, gate verdict, and run milestone rigger has ever recorded, in the order it happened. Nothing else derives it; it derives everything else.
- `graph.db` - the context graph. A REBUILDABLE projection folded from the event log: rigger-build-graph regenerates it from `events.db` alone, so losing it loses time, never truth.
- `progress.db` - live per-agent progress telemetry. Never replayed into a run's state; it is a side channel `rigger status` and the dashboard read to show what an agent is doing right now, not a record anything else depends on.

## Procedure

`rigger reset` with no flags is the MENU, not an error: it exits 0 and prints one line per prunable accumulation, each with a measured count and the flag that acts on it. It is read-only - safe to run any time just to look.

- `rigger reset --runs` prunes dead-run rows and superseded edges out of `graph.db`. It works over ANY event-store backend (the graph is always a local file); rerun it any time, especially before a large run.
- `rigger reset --derived` compacts `events.db`: it keeps the LATEST event per replay key of each derived project-ingest type, deletes the superseded duplicates, and vacuums so the file shrinks on disk. Every other event - every decision, finding, lesson, gate verdict, the whole run history - survives byte-for-byte. Only the embedded sqlite backend can compact this way, and it refuses (unless overridden with `--force-live`) while a run is live against the store.
- The two flags compose: `rigger reset --runs --derived` sheds both accumulations in one pass.

## Anti-move

Never touch `events.db`, `graph.db`, or `progress.db` with raw SQL, `rm`, or any tool outside `rigger reset`. The event log is append-only truth: a hand-edit or a hand-deleted row can desync the graph from the log in ways `rigger reset --derived`'s own key-preserving compaction is specifically built to avoid. A store file that is genuinely corrupt is an incident to fix at its root, never a reason to reach for a database client.

## See also

rigger-build-graph if `graph.db` needs regenerating rather than pruning; rigger-reindex if only the symbols index is stale.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
