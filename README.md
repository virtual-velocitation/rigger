# Rigger

Point Rigger at a spec and it produces working, integrated code by running a team of AI agents. What makes it different is memory. Every decision an agent makes is recorded, and the next agent can see it. No agent works blind.

The name is a climbing reference. A rigger is the person who sets up your harness and checks your gear before you leave the ground. That is the job here: set up the agent fleet, hold the rope, keep everyone clipped into the same system.

> Status: Rigger is written in Rust and CI-green. The event store (embedded SQLite behind a KurrentDB-shaped trait, with optimistic concurrency and a contract suite), the bi-temporal context graph (supersession invalidates, never deletes), the config loader (agent files + workflow YAML), the gate / autonomy-ratchet / safety rails, the event-sourced ledger, the conductor, both agent drivers (the standalone CLI driver and the in-Claude-Code workflow driver over a stdio MCP bridge - `rigger serve`), and the `rigger` binary are implemented and tested. A run lands code: each unit's worktree is committed and merged into the repo on a green gate. Independent stages run concurrently in isolated worktrees; each agent is fed the subgraph of decisions governing the files it touches plus the lessons learned about them; the side-car surfaces peers' decisions; escalations are recorded as lessons that resurface later; `rigger run <spec>` refuses to start unless every acceptance criterion is covered by a stage; a planning stage can extend the DAG at runtime; and gates ratchet their own autonomy. Semantic grounding is the real turbovec engine - `turbovec` + `fastembed` as native Rust crates, no FFI - behind the opt-in `turbovec` feature; the default build uses a grep grounder so a plain `cargo install` stays light. The full blueprint is [docs/architecture.md](docs/architecture.md).

## The problem

When you fan out a bunch of AI agents across a codebase, they cannot see each other. Each one starts from a clean slate, does its work in isolation, and reports back. That sounds fine until you run it at scale, and then the same three failures show up over and over:

1. They repeat mistakes. One agent learns something the hard way. The next agent, with no knowledge of it, walks straight into the same wall.
2. They re-litigate settled decisions. You decide something, an agent two steps later has no idea it was decided, and quietly undoes it.
3. They work off stale ground. Agent A changes a file while Agent B is still editing around the old version of it. Now you have a conflict, or worse, a silent regression nobody catches.

I ran into all three building a game engine with an agent fleet. The usual fixes do not actually fix it. Dumping more context into every agent just buries the signal. Vector search over the codebase gives you text that looks similar to your query, but "looks similar" is not the same as "is related," and it tells you nothing about what another agent decided thirty seconds ago. What the agents are missing is not more text. It is shared, current, structured memory.

## What Rigger does

Rigger gives the whole fleet one shared memory and treats it as the source of truth.

Every meaningful thing an agent does, whether that is a decision it makes, a file it touches, or a gate it passes or fails, gets written to an append-only event log. You never edit or delete an event. You only add to it. The log is the truth, and everything else is built from it.

From that log, Rigger projects a context graph: a map of decisions, files, lessons, and the relationships between them. The graph is bi-temporal, which just means it tracks not only what is true but when it was true. When a decision gets overruled, the old one is not deleted. It is marked as no longer valid as of a point in time, and the replacement takes over. So you can ask "what do we believe right now" and also "what did we believe yesterday, and when did that change." Stale beliefs never resurface with false confidence, because the graph knows they expired.

When an agent picks up a job, it is not handed the whole codebase or the whole history. It gets the slice of the graph connected to the files it is about to touch: the decisions that govern them, the lessons that apply, and what other agents have already decided about them. All the context it needs, and only the context it needs.

And because the log is shared and live, an agent working right now can watch for decisions other agents are making in parallel. The isolation that stops two agents from corrupting each other's files stays exactly where it is. Decisions travel on a separate channel. That split is the whole idea: agents stay aware of each other without ever touching each other's work.

Put those together and the three failures stop happening by construction. An agent cannot repeat a mistake whose lesson is wired to the file it just opened. It cannot re-litigate a decision the graph hands it as current. It cannot work off a stale base when it can see, live, that a peer just moved the ground under it.

## How it is built

Rigger is a single Rust binary, installable with `cargo install`, and it is built to be configured rather than modified. You do not fork it to use it. You point it at config.

There are two kinds of config, and both live in your repository, not in Rigger:

- Agent definition files. One file per agent, declaring its model, its tools, and its instructions. If you have used agent files in other tools, these will look familiar.
- A workflow file. A YAML file shaped like a GitHub Actions workflow: a set of stages, the dependencies between them, which agent runs each one, which gates must pass, and how much autonomy each gate gets. This file is the loop. The shape of your process lives in YAML, not in Rust, so changing it is an edit, not a pull request against Rigger.

Gates are config too. A gate is just a command that has to exit clean, plus a label for how much it is trusted. `go test`, `cargo test`, `pytest`, a custom script, all of it is your YAML. Rigger ships zero gates of its own, because it has no opinion about your project.

Everything underneath is pluggable through interfaces, each with a sensible default and an optional upgrade:

- Storage. The event log and the context graph live in embedded SQLite (bundled - one file, no server, nothing to install). The store sits behind a KurrentDB-shaped trait, so a KurrentDB adapter drops in without touching the rest of the system; the local store is modeled on KurrentDB's own shape on purpose, so it is a faithful stand-in.
- Agent driver. By default Rigger runs agents by shelling out to the `claude` command line tool, so it works anywhere that tool is installed, with no dependency on a particular editor or runtime. The in-Claude-Code workflow driver (`rigger serve`, over a stdio MCP bridge) is the richer alternative, again behind the same trait.
- Grounding. The default is a self-contained grep grounder. Built with `--features turbovec`, the real turbovec engine (semantic vector search via `turbovec` + `fastembed`) plugs in behind the same trait.

The loop itself is the part that has already been proven. It came out of a real harness that drove a large engine refactor: take a spec, break it into a dependency graph of units, refuse to call anything done until every requirement is covered and every gate is green, run independent units in parallel in isolated worktrees, review each one adversarially before it lands, and escalate or retry on failure instead of silently dropping it or spinning forever. Rigger is that machinery with everything specific to the project it grew up in stripped out.

## Quick start

```
cargo install --git https://github.com/virtual-velocitation/rigger

cd your-project
rigger setup                   # scaffold config + hook, install the native /rigger
                               #   workflow, and provision the JS driver (npm install)
```

Then drive the loop from inside Claude Code:

```
/rigger specs/feature.md       # run the loop on a spec - no hand-editing, it just works
rigger graph --around path/to/file
```

`rigger setup` installs the native Claude Code workflow at `.claude/workflows/rigger.js`, which Claude Code auto-discovers - so `/rigger <spec>` is runnable immediately, with nothing else to wire up. That workflow is the primary driver: it decomposes the spec into a unit DAG and, per unit, implements -> runs the cargo gates -> three-tier adversarial review -> integrates, with bounded remediation. Its agents ground themselves with `rigger ground` and read/write the shared context graph through `rigger emit` and `rigger peers`, so no agent works blind.

`rigger setup` also provisions the standalone JS driver in `.rigger/shim/`, so `rigger workflow specs/feature.md` runs the same loop from the command line as a fallback - useful outside an interactive Claude Code session. The lower-level pieces are still there too: `rigger init` does config-only setup, `rigger run specs/feature.md` is the standalone CLI driver, and `rigger serve` runs the MCP bridge.

By default this uses the local SQLite store, the grep grounder, and the `claude` CLI agent driver, so there is nothing else to stand up. For semantic grounding, install OpenBLAS and build with `--features turbovec` (it pulls the ONNX runtime and downloads an embedding model).

## Where this is going

The full design, including the data model, the schemas, the failure-mode handling, and the phased build plan, is in [docs/architecture.md](docs/architecture.md). Read that if you want to understand exactly how it works or you intend to build it. This README is the why. That document is the how.

To *use* Rigger - which SDLC stages to hand to agents, how to author agents and loops, how to wire tools into their hands, and the best practices learned running Rigger on itself - read [the handbook](docs/handbook/README.md).
