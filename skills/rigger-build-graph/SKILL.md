---
name: rigger-build-graph
description: Cold-build the context graph - empty `rigger graph --around`/`--show` lookups on a repo that already has source, or a first setup before any run exists. Read this before deleting a store file to force a re-ingest.
---

# rigger-build-graph

## Procedure

`rigger graph build` folds the project's source straight into `.rigger/graph.db` - no run, no `RunStarted`, and nothing on the event log, since the graph re-derives each file's batch from the tree. Run at the project root, it CREATES the store when the checkout is cold (`.rigger/` does not exist yet); run from a subdirectory or a linked worktree, it writes the root's one store, and where the root holds none it refuses, naming `rigger setup` in the root as the command that makes it. It REFRESHES an existing store incrementally: an unchanged file re-ingests nothing, and it reuses the exact same walk-and-content-key ingest authority a live run uses, so a standalone build and a run can never fold the same file under two different keys.

Rerun it any time it is convenient - on a schedule, after pulling a large set of changes, or simply because a lookup came back empty and you want to check. It is always safe: nothing is deleted, only appended and incrementally refreshed, so it never asks for a driver to be stopped. When something else does: A hold (`rigger hold --reason <text>`) stops a run and Ctrl-C stops its driver: a held run launches nothing, a driver that ends while the hold stands is relaunched by `rigger release` alone, and an interrupted driver is never relaunched. Ending a driver's process any other way is a death, and the next `rigger status` (not `--line` or `--json`) or session start (`rigger prime`) brings it back by adoption, as `rigger release` and `rigger resume-unit` do, each invoked from the main checkout; no other command relaunches it.

## Anti-move

Never force a rebuild by deleting `.rigger/graph.db` (or `events.db`) and re-running `rigger graph build` on the empty result. Deleting the log throws away truth that no rebuild can get back, and deleting only the graph is unnecessary work `rigger graph build` already does FOR you, incrementally, without erasing anything first. If lookups are empty, just run `rigger graph build`; only reach for rigger-reset-store if you specifically mean to prune, not rebuild.

## See also

rigger-reset-store for pruning `graph.db`'s dead-run accumulation rather than rebuilding it.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
