# 92 - The graph is the lookup surface: fresh on every integration, covering the whole product, ranked by intent, and in every session's hand

**Goal:** the knowledge graph is rigger's lookup surface, yet the operator's own session
reached for `grep` in 22 of 52 code lookups over four days (docs/audit/2026-09-graph-vs-grep.md).
Measured against the same tree, the graph answered fast (every `ground` under 0.7 s at load
125) but wrong in ways that erode trust: three of four found entities carried a stale line
pointer and a function integrated two specs earlier was absent (the index lags integrations),
the driver script and workflow definition are not indexed at all, and `ground` ranks a bare
identifier match (`run`, `new`, `tests`) above intent. The operator session also has no graph
tool: the loop's agents get `rigger_peers` through the MCP shim, the operator gets a shell.
A lookup surface nobody reaches for by default is not doing its job; the fix is in the tool
path, not in another instruction.

## Design

FRESH ON EVERY INTEGRATION, decided: the integration step reindexes every file the merge
touched (the existing `rigger reindex <files>` path, run by the conductor inside the step,
never left to an operator), so the graph never lags the run branch by more than the step in
flight. `rigger graph --show <name>` resolves the entity in the CURRENT tree: it locates the
definition by name (module path, then bare name with the ambiguity list it already prints),
prints the live line, and marks the recorded position only when the two differ ("recorded
line N, now M") instead of refusing. `rigger validate` reports index lag (files changed on the
run branch since their last index) as an advisory, so staleness is visible before it is felt.

THE WHOLE PRODUCT IS COVERED, decided: the symbols grounder indexes `workflows/*.js` and
`shim/*.mjs` (tree-sitter JavaScript, the grammar the crate already bundles for the design
pass if present, else added as a bundled grammar under the same feature), and the workflow
definition's stages, gates and agents become graph entities (`stage:implement`,
`gate:mutation`, `agent:rust-engineer`) with `needs`, `runs`, and `reviews` relations, so
"how is a stage's needs satisfied" has both the definition node and the conductor function on
one page.

RANKED BY INTENT, decided: `ground` scores a hit by exact-name match first (a query token that
equals an entity's name beats a token that merely occurs in it), then by inverse document
frequency of the token across the tree (a token in hundreds of files - `run`, `new`, `tests`
- carries near-zero weight), then by the existing lexical score; the top page is deduplicated
by entity, so six call sites of one function occupy one row with its degree. A query with no
strong token returns the honest "no entity matches strongly" line instead of noise.

IN EVERY SESSION'S HAND, decided: `rigger setup` registers the shim's MCP server with the
operator's Claude Code session (the same `rigger_peers` the loop agents get, plus
`rigger_ground` and `rigger_graph` tools mapped to `ground` and `graph --show/--around`), and
installs a PreToolUse hook beside the kill hook that intercepts a Grep or a `grep` over
`src/`, `tests/`, `workflows/` in a rigger project with the message "use rigger_ground /
rigger_graph for code lookups; grep is for literal text - add `--literal` to proceed", so the
graph is the path of least resistance and a grep is a deliberate act. The shipped skill's
lookup section states the same rule for a human reader.

CONSTRAINTS WALK: a name defined in two files - `--show` prints the ambiguity list (as
today) and `ground` returns both entities as separate rows. A file deleted by the integration
- its entities are retired through the existing supersession, not left dangling. An
integration that touches hundreds of files (a refactor-wave unit) - the reindex is bounded by
the merge's file list and runs under the build budget, and validate reports its duration. A
JavaScript file with syntax the grammar cannot parse - indexed as far as the parse reaches,
with a `partial` marker on the file node, never dropped silently. The hook on a project without
rigger - inert (it checks for `.rigger/`). A literal-text lookup - `--literal` passes the hook
and is what grep is for.

## Notes (non-criteria)

Operator ordering (2026-09-11): this spec lands after the resource-consumption specs 91 and
89. The audit document names the twelve questions this spec is judged against; its criterion
1 re-runs them.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No new event type. A bundled JavaScript grammar is the only permitted new dependency, under
  the existing `symbols` feature.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves FRESH ON EVERY INTEGRATION: after a fixture integration touching a file,
  its entities resolve at their live lines, `graph --show` on a moved function prints the live
  line with the recorded one noted, validate reports zero index lag, and the twelve questions
  in docs/audit/2026-09-graph-vs-grep.md re-run against the real tree with no stale pointer.
  This criterion OWNS reindex-on-integration, name resolution and the lag advisory; coverage
  is criterion 2's and ranking criterion 3's, NOT this one's.
- [ ] a test proves THE WHOLE PRODUCT IS COVERED: `workflows/rigger.js` constants and
  functions and the workflow definition's stages, gates and agents are graph entities with
  their relations, and questions 5 and 8 of the audit answer from the graph. This criterion
  OWNS the JavaScript and definition indexers only.
- [ ] a test proves RANKED BY INTENT: `ground` returns the exact-name entity first, one row
  per entity with its degree, near-zero weight for tree-wide tokens, and the honest no-match
  line for a weak query; questions 3, 9, 11 and 12 answer on the first row. This criterion
  OWNS the scorer and the page shape only.
- [ ] a test proves IN EVERY SESSION'S HAND: `rigger setup` registers the MCP tools
  `rigger_ground` and `rigger_graph` for the operator session and installs the lookup hook,
  the hook bounces a source grep with the stated message and passes `--literal`, and the
  shipped skill's lookup section states the graph-first rule. This criterion OWNS setup, the
  hook and the skill text.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
