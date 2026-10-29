# Authoring agents

An agent is one Markdown file in `.rigger/agents/<id>.md`: YAML frontmatter that declares its capabilities, and a body that is its system prompt. Adding an agent to the fleet is writing that file - no code, no registration step. `rigger validate` checks the result; any workflow stage can then reference the agent by its `id`.

## The file format

```markdown
---
id: implementer
model: opus
tools: [Read, Edit, Write, Grep, Glob, Bash, Agent]
isolation: worktree
recurse: true
---
You implement ONE fully-specified unit inside your worktree. Write the failing
test first, confirm RED, implement minimally, confirm GREEN, run the named gates,
commit. Report the final line as JSON: {"id","pass","evidence"}.
```

### Frontmatter fields

| Field | Values | Meaning |
|---|---|---|
| `id` | string | The name stages and review panels use to reference this agent. Convention: lowercase, hyphenated, role-shaped (`planner`, `architecture-reviewer`). |
| `model` | `opus` \| `sonnet` \| `haiku` | The capability tier. Always an alias, never a pinned model ID - the alias resolves to the current model of that tier at spawn time, so the fleet upgrades when the driver does. See "Model tiering" below. |
| `tools` | list of tool names | The capability allowlist. The agent physically cannot call anything not listed - this is enforcement, not advice. See [tools-and-context.md](tools-and-context.md). |
| `isolation` | `worktree` (or omit) | `worktree` gives the agent its own git worktree on its own branch; it cannot see or corrupt another agent's files. Mandatory for anything that writes code in a fan-out. |
| `recurse` | `true` \| `false` | Whether the agent keeps the `Agent` tool and may dispatch subagents. `false` strips every spawn tool from the grant, whatever `tools` lists. Rigger's own personas are `recurse: true` so they can fan mechanical work out to the `lookup` and `verify` helpers (see "Fanning out" below); the built-in working discipline bounds that fan-out. |

The body below the frontmatter is the agent's entire system prompt. Everything the agent knows about its job comes from three places: this prompt, the per-task assignment the conductor sends it, and the context slice it retrieves through grounding.

## Model tiering: judgment on opus, mechanics on the helpers

Every persona runs on `opus`, reviewers included: planning, implementing, reviewing and adjudicating are all judgment, and a unit that lands right the first time costs less than a cheap attempt that is sent back. The cheap tiers do the mechanical work, as subagents the persona dispatches:

| Tier | Use for | Rigger examples |
|---|---|---|
| `opus` | Every persona: planning, implementation, each review tier, adjudication | `planner`, `rust-engineer`, `architecture-reviewer`, `sdet`, `adversary`, `adjudicator` |
| `sonnet` | One mechanical run at a time: a build, the gate battery, a test run, a reproduction | the `verify` helper |
| `haiku` | One knowledge-graph question per node, many in parallel | the `lookup` helper |

Never hardcode a model ID (`claude-sonnet-4-6`) in an agent file. The alias is the design: when the harness maps `opus` to a newer Opus, every agent upgrades for free, and nothing rots.

## Fanning out: the lookup and verify helpers

Two helper subagents live in `.claude/agents/`, where Claude Code discovers subagents; `rigger init` installs them in every project, and an existing copy is kept. The headless host also hands both to every spawn it launches (see [How a headless spawn is configured](tools-and-context.md#how-a-headless-spawn-is-configured)), so a unit worktree needs no copy:

- `lookup` (`haiku`) answers ONE question about ONE knowledge-graph node through the rigger graph tools. A persona dispatches one instance per graph node, in parallel, and never one per file: the graph is the lookup surface, and a per-file fan-out goes around it. A literal `Grep` is its last resort and is reported as a `GAP:` line so the graph gap gets fixed.
- `verify` (`sonnet`) runs a build, the gate battery, a test run or a reproduction and reports the exact result. A persona runs one at a time, because builds compete for the machine's CPU, memory and disk; the machine-wide build slots belong to gate commands alone. A reviewer never sends it the gate battery: its prompt carries the gate evidence for the tree it judges.

A persona needs `Agent` in `tools` and `recurse: true` to dispatch them. How to use them - which work fans out, how many run at once, what stays in the persona's own turn - is the built-in working discipline (below), so a persona file never restates it.

## Prompt-writing practice

The prompts that work share a shape. Study `.rigger/agents/adversary.md` for the strongest worked example.

**State the role and its lane - including what is out of lane.** "You review the reviews - you are NOT a parallel lens, and you do NOT render the final verdict" prevents the two most common review-panel failures (duplicated lens work, verdicts from the wrong tier).

**Define success in terms that resist gaming.** The adversary's prompt says success is *catching real problems, not converging*. A reviewer told to "approve when it looks good" converges early; a reviewer told its wins are the issues everyone else missed keeps digging.

**Demand evidence, not opinion.** "Cite file:line for every finding. Verify behavioral claims by running them, not by reading." Findings without citations get refuted in tier 2; make the evidence rule explicit in tier 1 and the panel converges faster.

**Enumerate the specific defect classes to hunt.** Generic "find bugs" produces generic findings. The adversary's prompt names its quarry: concurrency races, lock-upgrade deadlocks, optimistic-concurrency edges, absent-value-sentinel inversions, the live-emit boundary, resource leaks. Every defect class your project has actually been bitten by belongs in a reviewer prompt - that is how a lesson becomes prevention.

**Wire the memory verbs in.** Any agent that decides something must be told to record it: "Record each planning decision with the rigger_emit tool (type DecisionMade) so the implementers inherit your reasoning." An unrecorded decision does not exist to the rest of the fleet.

**Fix the output contract.** Workers report machine-parseable results: `Report the final line as JSON: {"id","pass","evidence"}`. The conductor parses that line; prose around it is tolerated, absence of it is a failure.

**Forbid the known cheats by name.** Prompts should expressly forbid: deferring scope ("follow-up", TODO comments), weakening or skipping a test to go green, suppressing warnings instead of fixing them, and any masking of a quality problem so a reviewer cannot find it. An agent that cannot finish must surface the hole, never hide it. If the work moves files, require the agent to confirm the deletion is actually in the commit (`git show --stat HEAD`) - orphaned sources pass every compile gate while failing every file-scanning one.

## Reviewer agents specifically

A review panel needs *diverse lenses*, not redundant ones. Give each lens one lane (architecture adherence; correctness and test rigor; domain-specific invariants) and keep panel prompts scoped to defects, not taste: a finding must be a contradiction, a factual error, a spec deviation, or a demonstrable bug - "I would have named this differently" is not a finding. Which findings block is the built-in review rule's call, never a persona's: wrong code, an unmet Done-when criterion and a missing test for a behaviour a Done-when criterion names block, and every other finding is a note that never fails the unit. Where the workflow has a check-in stage - a stage that needs the fan-out implement template - that stage fixes the run's notes once, before its gates; without one, the notes stay in the run's log for the operator.

The adversary must stay strict to stay useful. Calibrate it in one direction only: it may refute a lens finding as overreach solely when the finding is out of lane, describes an unreachable state, or is factually wrong on cited evidence. "Minor" or "inconvenient" never qualify. A softened adversary is worse than none - it launders defects as reviewed.

## Injecting instructions

Some rules apply to every agent regardless of its role: how code is shaped, how it is tested, what counts as done. Copying them into each persona drifts the moment one copy is edited, and an imported agent arrives without them. Rigger therefore composes every spawned agent's system prompt in layers:

```text
persona  +  built-in instructions (communication among them)  +  operator instructions
```

**The built-in layer** ships inside the binary and reaches every agent Rigger spawns, including an agent with an empty body, unless an operator file replaces or switches an entry off. It has three entries, injected in this order:

1. `engineering-principles` - the engineering law every change is held to: Clean Architecture, SOLID, DRY, KISS, YAGNI, TDD, and BDD where a behavior is operator-facing. Source: `crates/rigger-domain/src/instructions/engineering-principles.md`.
2. `working-discipline` - how every agent works: one pass by excellence; a turn's end being the session's end - a headless spawn's process exits with its turn and nothing re-invokes it, so every job it starts (a command, one the host moved to the background on its timeout, a watch) is waited on inside the turn and its result read before the report, and a denied or blocked command is an error that names its alternative rather than the end of the turn; the hand-over an author's tree meets before it reports, each outcome with the check that proves it and no check owing a record (the project's gates green in its worktree, each run to completion inside the turn; generated files committed with their inputs; tests table-driven - the cases of one behaviour over different values rows of one table run by one body, never copies of a test function that differ only in their values - and each table, fixture, helper, builder or oracle the author adds existing once, looked up where the project's test helpers live and moved to the shared home when a second test home needs it, duplication the author did not write named in its hand-over as the tree's debt and never lifted, checked by the project's duplication gate; every sentence about the changed behaviour true at head, checked by reading the changed files' module docs and the docs of the changed items and of their callers, read from the graph at depth 1, against head, a renamed or retired name's old spelling counted to zero; nothing added that nothing reads; the red test's body untouched by the green commit; every fact the production delta states with one home; a whole-tree audit a rejection's pattern item requires fixing every site and recorded as each search with its hit count and each site fixed, never every hit listed; a probe that edits the tree run to completion and restored before the turn ends, never as a background job); the review method and its rule (never re-running a gate the evidence shows green, one targeted reproduction in a scratch worktree, and a finding blocking only as wrong code, an unmet Done-when criterion or a missing test for a behaviour a Done-when criterion names, every other finding - docs wording, naming, a fact with two homes, test shape, comment density, a record - a note that never fails the unit, fixed once before its gates by the workflow's check-in stage where it has one and otherwise left in the run's log for the operator); and fanning mechanical work out to the `lookup` and `verify` helpers. Source: `crates/rigger-domain/src/instructions/working-discipline.md`.
3. `communication` - how every agent communicates through the shared event log: record each decision and each review finding the moment it is made, check peers between actions, and supersede a peer's decision explicitly rather than contradict it silently. Source: `crates/rigger-domain/src/instructions/communication.md`.

Only an operator file named as an entry replaces or switches it off (see the operator layer below). Because the entries in force reach every spawn, a persona file carries only role-specific text: its role, its lane, its defect classes, its output contract and the project's anchors for a built-in rule (the hand-over's gate commands and test homes, say). A rule that applies to every role belongs in the instruction layer, never copied into each persona, where the copies drift the moment one is edited.

**The operator layer** is every `*.md` file in `.rigger/instructions/`, appended in filename order after the built-ins, each under a `## <file stem>` heading, except a file whose stem equals a built-in entry's name byte for byte (`communication.md`, say): with a body that is not blank it replaces that built-in's body in the built-in's own place, and with an empty or whitespace-only body it switches that built-in off, so neither the built-in nor the file is injected; either way the file is not appended. A near name (`Communication.md`, or a stem with a trailing space) replaces nothing and appends. Use a numeric prefix (`10-house.md`, `20-team.md`) to control the order. `rigger init` scaffolds the directory with a `README.md` that explains it; the README itself is never injected. Put project-wide house rules here instead of into personas, and keep each persona about its role.

**Reading the composed layers.** `rigger instructions` prints exactly what your agents are held to: the three built-in entries first, then each operator file by name, or `(none)` when the directory holds no instruction files. `rigger prime` opens every session with a one-line summary of the layers in force.

**The definition pin.** Operator instruction files are part of a run's definition, alongside `workflow.yml` and the agent files. Editing one while a run is live changes the definition hash, and the next step halts as a definition drift. Edit instructions between runs, or continue deliberately with `--rebase-definition`.

## Starting from an existing collection

You do not have to write a fleet from scratch. [agency-agents](https://github.com/msitarzewski/agency-agents) (MIT) is a collection of 200+ specialized agent definitions - engineering, testing, security, design, and a dozen other divisions - in the same Markdown-with-YAML-frontmatter shape Rigger reads. To adopt one:

1. Copy the `.md` file into `.rigger/agents/` and rename its identity field to Rigger's `id:` (lowercase, hyphenated, role-shaped).
2. Decide the five frontmatter fields deliberately for YOUR loop: pick the `model` tier by the judgment the role needs (see "Model tiering" above), narrow `tools` to the minimum the role requires (reviewers get read-only), add `isolation: worktree` to anything that writes code in a fan-out, and grant `Agent` with `recurse: true` so the agent can dispatch the helpers.
3. Keep the body's role and workflow; strip platform-specific instructions that assume a particular editor or chat UI, and any rule the built-in layer already carries.
4. `rigger validate` confirms the fleet loads; a workflow stage can then reference the agent by `id`.

Treat imported prompts as first drafts: the checklist below applies to them the same as to agents you write yourself.

## Checklist for a new agent

1. Write `.rigger/agents/<id>.md` with the five frontmatter fields decided deliberately (tier? tools? isolation? recursion?).
2. Prompt: role + lane + out-of-lane, success definition, evidence rule, defect classes, memory verbs, output contract, forbidden cheats.
3. `rigger validate` - catches schema errors and dangling references.
4. Reference the `id` from a workflow stage or review panel ([authoring-loops.md](authoring-loops.md)).
5. First runs at autonomy `manual` or `auto_notify`: read what it actually does before you let it act silently.
6. When it fails in a new way, encode the lesson - into its prompt if it is agent-specific, into a gate if it is machine-checkable, into a reviewer's hunt-list if it is a defect class.
