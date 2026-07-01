# Authoring agents

An agent is one Markdown file in `.rigger/agents/<id>.md`: YAML frontmatter that declares its capabilities, and a body that is its system prompt. Adding an agent to the fleet is writing that file - no code, no registration step. `rigger validate` checks the result; any workflow stage can then reference the agent by its `id`.

## The file format

```markdown
---
id: implementer
model: sonnet
tools: [Read, Edit, Write, Grep, Glob, Bash]
isolation: worktree
recurse: false
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
| `recurse` | `true` \| `false` | Whether the agent may spawn sub-agents. Default deny for workers. A recursing implementer is how one runaway loop burns a five-hour budget in fifteen minutes - constrain capability, do not rely on prompt instructions. |

The body below the frontmatter is the agent's entire system prompt. Everything the agent knows about its job comes from three places: this prompt, the per-task assignment the conductor sends it, and the context slice it retrieves through grounding.

## Model tiering: pay for judgment, not for typing

The tier assignment is an economic decision with a quality floor. The pattern proven across Rigger's own runs:

| Tier | Use for | Rigger examples |
|---|---|---|
| `opus` | Judgment: planning, adversarial review, adjudication, genuinely novel implementation | `planner`, `adversary`, `adjudicator`, the senior-engineer role |
| `sonnet` | Execution against an explicit spec: mechanical implementation, lens review with a rubric, integration | `implementer`, `reviewer.technical`, `scribe` |
| `haiku` | Cheap single-purpose calls: escalation formatting, summaries | escalation notices |

Two rules make the cheap tiers work:

1. **Plan with the expensive model, execute with the cheap one.** An Opus plan pass that produces an explicit, unambiguous unit spec lets a Sonnet implementer execute it at a fraction of the cost with no quality loss. The intelligence is in the spec; the execution is mechanical.
2. **Cheap models need explicit prompts.** Sonnet does not infer your conventions the way Opus does. Spell out the who / what / when / where / why, enumerate the checks, name the files, and state the architectural rules it must review against. Vague prompt + cheap model = generic output.

Never hardcode a model ID (`claude-sonnet-4-6`) in an agent file. The alias is the design: when the harness maps `sonnet` to a newer Sonnet, every agent upgrades for free, and nothing rots.

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

A review panel needs *diverse lenses*, not redundant ones. Give each lens one lane (architecture adherence; correctness and test rigor; domain-specific invariants) and keep panel prompts scoped to defects, not taste: a finding must be a contradiction, a factual error, a spec deviation, or a demonstrable bug - "I would have named this differently" is not a finding.

The adversary must stay strict to stay useful. Calibrate it in one direction only: it may refute a lens finding as overreach solely when the finding is out of lane, describes an unreachable state, or is factually wrong on cited evidence. "Minor" or "inconvenient" never qualify. A softened adversary is worse than none - it launders defects as reviewed.

## Checklist for a new agent

1. Write `.rigger/agents/<id>.md` with the five frontmatter fields decided deliberately (tier? tools? isolation? recursion?).
2. Prompt: role + lane + out-of-lane, success definition, evidence rule, defect classes, memory verbs, output contract, forbidden cheats.
3. `rigger validate` - catches schema errors and dangling references.
4. Reference the `id` from a workflow stage or review panel ([authoring-loops.md](authoring-loops.md)).
5. First runs at autonomy `manual` or `auto_notify`: read what it actually does before you let it act silently.
6. When it fails in a new way, encode the lesson - into its prompt if it is agent-specific, into a gate if it is machine-checkable, into a reviewer's hunt-list if it is a defect class.
