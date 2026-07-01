# Tools and context: what agents can reach, and how

An agent's power is exactly the union of three surfaces: the **harness tools** its allowlist grants, the **`rigger` CLI** it drives through `Bash`, and the **context** the memory layer feeds it. This document covers granting each one.

## 1. The `tools:` allowlist

Every agent file declares the tools it may call:

```yaml
tools: [Read, Edit, Write, Grep, Glob, Bash]
```

This is a *capability boundary*, not a suggestion - the driver only exposes the listed tools, so an agent physically cannot exceed its grant. Design allowlists by role:

| Role | Grant | Withhold and why |
|---|---|---|
| Reviewer / adversary / adjudicator | `Read, Grep, Glob, Bash` | `Edit`/`Write` - a reviewer that can modify code can "fix" its way past a finding instead of reporting it. Read-only reviewers keep the review adversarial. |
| Planner | `Read, Grep, Glob, Bash` | Write access - plans are emitted as events (`UnitProposed`, `DecisionMade`), not files. |
| Implementer | `Read, Edit, Write, Grep, Glob, Bash` + `isolation: worktree` | The `Agent` tool (`recurse: false`) - workers must not spawn workers. Capability constraint beats prompt instruction: an agent that *cannot* recurse never burns your budget recursing. |
| Scribe | `Read, Edit, Write, Grep, Glob, Bash` | Nothing extra - but its prompt confines edits to docs. |

Note that `Bash` is the widest grant on the list: it transitively includes every CLI on the box, including `rigger` itself and your whole toolchain. If a role should not run arbitrary commands, do not grant `Bash` - there is no half-grant.

### External tools (MCP)

Under the Claude Code driver, agents can also reach any MCP server configured in the session (searchable and loaded on demand), so wiring a new external tool to the fleet is: configure the MCP server once, then name the capability in the prompts of the agents that should use it. Two caveats: interactively-authenticated MCP servers may be absent in headless runs, and a tool an agent is not *told about* is a tool it will rarely reach for - the allowlist grants, the prompt directs.

## 2. The `rigger` CLI: the fleet's shared verbs

Any agent with `Bash` can drive the shared memory and grounding through the `rigger` binary. Run these from the repo root - the shared store lives in `.rigger/` at the project root, and per-project segregation keeps every project's data separate no matter which backend holds it.

### Grounding (read the code the smart way)

```
rigger ground "<query>"            # semantic search over the codebase
rigger graph --around <path>       # the context-graph slice governing a file:
                                   # decisions, lessons, peers' touches
rigger reindex                     # rebuild the semantic index (it also
                                   # auto-freshens before every ground)
```

`ground` is the entry point for "where does X live / what couples to Y" questions - use it *before* grep-spelunking. It scopes discovery; exhaustiveness claims ("all call sites") still require an exact grep, and dependency claims require the compiler. The grounder is selected in the workflow (`turbovec` semantic by default; `grep` substring fallback; `nop`), and a missing semantic backend degrades loudly, never silently.

### Memory (write and read the shared log)

```
rigger emit <Type> '<json-object>'      # append an event to the shared log
rigger peers                            # what other agents decided, live
rigger stats                            # store/graph counters
```

The event vocabulary agents use directly:

| Type | Emitted when | Example |
|---|---|---|
| `DecisionMade` | You chose an approach, an interpretation, a tradeoff | `rigger emit DecisionMade '{"id":"nonexistent-stream-empty","summary":"read_stream on a never-appended stream returns Ok(empty), normalized across backends"}'` |
| `LessonLearned` | You hit a wall others will hit | the fix class, the trigger, how to avoid it |
| `FileTouched` | You modified a file (drivers largely automate this) | powers blast-radius partitioning and peer awareness |

The conductor emits the orchestration events (`UnitProposed`, `UnitIntegrated`, `BudgetExhausted`, gate results); agents read their effects through `peers` and `graph` rather than parsing the log raw.

The discipline that makes this layer worth having: **emit at decision time, not at report time.** A decision recorded live is visible to a concurrently running peer *before* it collides with you; a decision batched into a final report reaches the log after every agent who needed it already guessed. The prompts enforce this ("record each planning decision so the implementers inherit your reasoning") - keep that language in any agent you author.

Supersession, not deletion: the graph is bi-temporal, so overruling a decision emits a superseding event and the old belief is marked invalid-as-of, never erased. Agents therefore never need to "clean up" the log - append the correction and the projection handles the rest.

## 3. Gates: the tools that judge

A gate is a shell command whose exit code is the verdict, declared once in the workflow's `gates:` library and referenced by name from stages:

```yaml
gates:
  test: { run: "npm test", kind: core }     # or cargo test, pytest, go test ./...
```

Anything scriptable can gate: compilers, linters, test suites, contract suites against a real server, an install smoke into a throwaway prefix, a syntax check on a script artifact - in any language, or several at once. Two rules:

1. **Gates mirror CI exactly** - same commands, same versions. A gate/CI mismatch means the loop green-lights what CI rejects, and agents "fix" the wrong failure.
2. **Know what the gates cannot see.** Every gate suite has a blind spot (a lock file masking fresh-resolve skew, non-compiled artifacts, behavior only a live server exercises). Name the blind spots in the spec and route their verification to the adjudicator as demanded evidence.

## 4. The MCP bridge: Rigger's tools inside another harness

`rigger serve` runs the conductor as an MCP server over stdio. An external harness (Claude Code or anything MCP-speaking) connects and gets four tools:

| Tool | Direction | Purpose |
|---|---|---|
| `rigger_next` | pull | Fetch the next ready assignment (unit + context slice) |
| `rigger_result` | push | Report a completed assignment's outcome |
| `rigger_emit` | push | Append an event (same contract as `rigger emit`) |
| `rigger_peers` | pull | Live peer decisions (same contract as `rigger peers`) |

This is the integration seam for embedding Rigger's orchestration and memory into a harness you do not control: the harness runs the agents; Rigger assigns the work, holds the memory, and keeps the score. The CLI (`rigger emit` / `rigger peers`) and the MCP tools hit the same store, so a fleet can mix both - in-session workflow agents on the CLI verbs, external agents on the bridge - and still share one memory.

## 5. Context: what an agent is handed without asking

Granting tools is half the surface; the other half is what the driver feeds the agent up front. Per assignment, an agent receives its agent-file prompt, the unit spec (files, criterion, gates), and the **context slice**: the subgraph of decisions governing the files it is about to touch, the lessons attached to them, and what peers have already decided. Not the whole codebase, not the whole history - all the context it needs and only the context it needs.

The side-car keeps that slice *live* during long work: peer decisions land as they are emitted, so an agent mid-unit learns that a concurrent agent just made a governing decision without polling for it.

What this means for you as an author: you rarely need to stuff context into prompts. Put durable knowledge in events (lessons, decisions), keep prompts about *role and method*, and let grounding deliver the knowledge at the moment it is relevant. A prompt that hardcodes facts about the codebase goes stale; a graph that stores them gets superseded properly.
