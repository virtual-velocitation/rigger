---
name: rigger-reindex
description: Refresh the symbols grounding index - a `rigger graph`/`rigger ground` lookup that names an entity the current tree no longer holds, or the index-staleness advisory from `rigger validate`. Read this before rebuilding the whole graph over an index-freshness problem.
---

# rigger-reindex

## Procedure

`rigger reindex <file>...` re-parses ONLY the named files and persists the delta to the project's symbols grounding index at `.rigger/symbols/` - the fast, targeted fix for an index that has drifted from files you just changed (a unit's own commit, a rebase, a branch switch). It is scoped strictly to the symbols index, a DIFFERENT store from the structural context graph, so it costs only the named files, never a walk of the whole tree.

Name every file whose content changed since the index was last built; an unnamed file's stale entry is left exactly as it was.

## Anti-move

Do not reach for a whole-graph rebuild (see rigger-build-graph) or a store wipe to fix a lookup that is really an index-freshness problem: naming the stale files and reindexing exactly them is both cheaper and more targeted than rebuilding the whole structural graph over a handful of drifted entries. Reserve a whole-graph rebuild for when the graph itself is missing or empty, not for a symbols lookup that just needs the files it names re-parsed.

## See also

rigger-build-graph for the whole-project structural graph; rigger-reset-store for the stores' own hygiene.


## Operator binary boundary

An agent never installs, replaces, or modifies the operator's installed `rigger` binary - that binary is operator-only. A tree checkout's own `rigger` build is invoked only by explicit path, and only to render (spec/docs output) - never to overwrite what is on PATH.
