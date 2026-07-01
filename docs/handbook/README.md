# The Rigger Handbook

This is the operator's manual for replacing portions of your software development lifecycle with agents. [architecture.md](../architecture.md) explains how Rigger is built; this handbook explains how to *use* it - what to hand to agents, how to author the agents and loops that do the work, and how to wire tools into their hands.

## How to read this

Read in order if you are new. Jump to the file you need if you are not.

| Doc | Answers |
|---|---|
| [agentic-sdlc.md](agentic-sdlc.md) | **What** - the stages of the SDLC, which of them agents can own today, the roles that own them, and where humans stay in the loop |
| [authoring-agents.md](authoring-agents.md) | **How** - defining a new agent: the `.rigger/agents/<id>.md` file format, model tiering, prompt-writing practice |
| [authoring-loops.md](authoring-loops.md) | **How** - creating a loop: spec format, the workflow YAML, the per-unit lifecycle, the four ways to drive a run |
| [tools-and-context.md](tools-and-context.md) | **How** - making tools available to agents: the `tools:` allowlist, the `rigger` CLI surface, the MCP bridge, gates |
| [best-practices.md](best-practices.md) | The distilled rules - what we learned running Rigger on itself, stated as practice you can adopt directly |

## The one-paragraph model

Rigger turns a spec into integrated code by running a team of agents over shared, structured memory. A **spec** enumerates machine-checkable acceptance criteria. A **loop** decomposes it into a DAG of small units, and each unit runs its own lifecycle: ground -> implement (red/green TDD in an isolated worktree) -> gates -> three-tier adversarial review -> integrate on approval. Every meaningful act - a decision, a file touched, a lesson learned - is appended to a shared event log, projected into a bi-temporal context graph, and fed to the next agent as *the slice it needs*. Agents are Markdown files; loops are YAML; gates are shell commands. You reconfigure the fleet by editing text, not code.

## Vocabulary

- **Spec** - a Markdown file with enumerable "Done when" acceptance criteria. The loop refuses to start unless every criterion is covered by a stage.
- **Unit** - the smallest independently implementable, testable, reviewable piece of work. One unit maps to one acceptance criterion (or a planner-refined slice of one).
- **Stage** - a node in the workflow DAG (`plan`, `implement`, ...). A `fan-out` stage spawns one agent per ready unit.
- **Gate** - a named shell command whose exit code is the verdict (`cargo test`, `cargo clippy ...`). Gates are config, not code.
- **Review panel** - the three-tier check on every unit: expert lenses in parallel, then an adversary who reviews *the lenses*, then an adjudicator whose verdict gates integration.
- **Ledger** - the run's durable state, itself a projection of the event log. A resumed run re-reads it; state never lives in a conversation.
- **Context graph** - the bi-temporal projection of the event log: decisions, files, lessons, and their relationships, queryable by "what governs the files I am about to touch."
- **Grounding** - the pre-work retrieval step: semantic search over the code (`rigger ground`) plus the graph slice around the target files (`rigger graph --around`).
- **Autonomy** - per-gate policy: `manual` (a human decides), `auto_notify` (runs, passes silently, logged), `silent` (invisible). Gates ratchet their own autonomy up on clean passes and demote themselves on failure.
