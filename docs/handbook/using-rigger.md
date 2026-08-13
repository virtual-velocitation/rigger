# Using rigger: the operating discipline

This chapter is the operating discipline for a rigger run: when the loop is the right tool, the one blessed driver, and the rails that keep a run consistent. Its facts are generated from the code the binary runs on, so the chapter cannot silently disagree with how rigger actually behaves.

## When to reach for rigger

Reach for rigger when you have a written spec whose "Done when" section enumerates machine-checkable criteria and you want it built, tested, reviewed, and integrated without hand-holding each step. Do NOT reach for it for a one-line edit, an exploratory spike, or work that has no spec to anchor acceptance on - the loop's value is the disciplined lifecycle around a checkable spec, and without one there is nothing for it to hold to.

## The one blessed driver

Drive every run through the native /rigger workflow (visible in /workflows and on the dashboard at 127.0.0.1:7420). It launches the loop and keeps the event log, the ledger, and the context graph consistent with one another. These anti-patterns split the run's state away from that shared record and must be avoided:

- Polling git or ps by hand to guess progress. Read the dashboard or `rigger status`; the by-hand view misses the ledger and the graph.
- Hand-driving `rigger step` in a shell. The driver owns stepping; a hand step races the driver and can double-spawn or wedge the frontier.
- Hand-implementing a unit the loop parked. That leaves the loop still stuck for the next unit and forks the code from the log - fix the loop instead (see below).

## Looking things up

The knowledge graph is the lookup surface - reach for it before grepping the project's sources. Three verbs answer the three questions you have about the code: `rigger graph --around <file|entity>` (structure: who calls X, and the caller/callee neighborhood), `rigger graph --show <entity>` (text: an entity's definition site and its body), and `rigger peers <file>...` (memory: the prior decisions, findings, and lessons about the files). Grep over the project's sources is a fallback worth reporting, not a habit: if the graph could not answer and you fall back to grep, record it with `rigger progress <id> 'grep-fallback: <what the graph did not answer>'` - one line before moving on - so the gap lands in the event log where it can be measured and closed. Filtering your own build or gate output is not a fallback and is not reported.

## Graph hygiene before a large run

The context graph the loop reasons over is a persistent projection rigger maintains incrementally: each run's decisions and findings are folded in one event at a time as they are emitted, and superseded rows are retired in place rather than re-derived from scratch, so a step never re-folds the whole history. Across many runs graph.db therefore ACCUMULATES the dead-run rows and retired edges that no live query reads, so the file grows on disk without bound even though the live graph the loop grounds on does not. Keep it lean before a large run with `rigger reset --runs`, which prunes that dead-run accumulation and reclaims the disk it held; a very stale graph should be pruned this way first. This is PRE-RUN hygiene through a real command, NOT a hand-driven `rigger step`: hand-stepping races the driver (see the one-blessed-driver anti-patterns above), whereas `rigger reset --runs` is a one-shot prune you run BEFORE launching the loop.

The EVENT LOG accumulates separately, and has its own prune: `rigger reset --derived`. Each run's project-ingest pass records the project's derived index - the code entities, inferred edges, and design links folded from your sources - and a log written before that pass deduplicated across runs holds the WHOLE index once per run, which is re-derivable duplication rather than history. `rigger reset --derived` keeps the LATEST event per replay key of each derived index type, deletes the superseded re-recordings, and vacuums so events.db shrinks on disk. Every other event survives byte-for-byte - lessons, decisions, findings, gate verdicts, and the whole run history `rigger stats` and replay read - and the graph is unaffected, because all recordings of one key fold to the same rows. The two flags COMPOSE and each prunes its own accumulation: `rigger reset --runs --derived` sheds the dead-run graph rows and the duplicated index in one pass.

## Spec shape

One observable behavior per criterion; the atomic unit is one checkbox; put type shapes and structural detail in a non-criteria Notes section. The loop's spec-shape lint flags these shapes because a planner paraphrases or truncates them when told to copy a criterion verbatim, which then fails the baseline match the conductor reconciles proposals against: multi-behavior, sub-bullet-as-unit, over-long. Recommendation: one observable behavior per criterion; put type shapes and detail in a non-criteria Notes section.

## Base anchoring

A run anchors its branch on the working ref (default origin/main) and reuses that branch once it exists. Anchor on the ref you actually want the work to land on, not a stale default: the anchor is what every unit worktree branches from and every approved unit merges back into, so an anchor on the wrong ref lands the run in the wrong place.

## When it wedges, fix the loop

If a unit will not pass, the fix belongs in the loop - the spec, the gate, the agent, or the config - never a manual edit that sidesteps it. A by-hand fix leaves the loop broken for the next unit and splits the code from the log, so the run can no longer be trusted to replay. Correct the underlying cause and let the loop re-run the unit.

## Auto-integration on approve

An approved unit integrates itself onto the run branch. A human reviews the whole run by opening a pull request FROM the run branch, never by cherry-picking approved units by hand - cherry-picking drops the run's accumulated context and its ordering. A failing unit is retried under a bounded budget (up to 3 attempts) and then escalated to a human rather than spinning forever.

## The verdict line

Every gating agent ends its output with its verdict line: a JSON line carrying {"verdict":"approve"} to approve (or the rejecting value to send the unit back). The integration gate reads that result line, not events recorded through any side channel, so an agent that records its verdict only out-of-band returns no verdict the gate can see and stalls the run. Anyone authoring or porting a gating persona must keep this line.

## Self-serve

Run `rigger version` to see the exact binary and its build provenance and to diagnose drift between the installed /rigger workflow and the binary that would run it. This repo keeps its specs in specs/. The full command surface is: run, step, reported, prompt, serve, workflow, graph, stats, canary, playbooks, replay, status, dash, ground, reindex, symbols-index, emit, progress, result, peers, reset, validate, init, setup, docs, prime, version, help.

## The load-bearing decisions

The discipline explains its own constraints:

- One source of truth: every drift-prone fact in this document is read from the code the binary runs on, so the document cannot silently disagree with behavior. A drift check re-renders and diffs it, so it stays accurate rather than merely starting accurate.
- Blast-radius isolation: each unit does its work in its own worktree, so concurrent units never clobber one another and every unit's change is reviewed on its own diff.
- Fail-closed review: only an explicit approve verdict integrates a unit; a missing, unparseable, or rejecting verdict routes the unit back to remediation rather than passing it silently.
