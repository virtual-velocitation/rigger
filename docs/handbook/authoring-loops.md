# Authoring loops

A loop is the machine that turns a spec into integrated code. You author one by writing two artifacts - a **spec** (what "done" means) and a **workflow** (who does the work, checked by what) - and then driving them with one of four drivers. This document covers all three parts.

## The spec

A spec is a Markdown file whose load-bearing content is its acceptance criteria: enumerable, machine-checkable "Done when" bullets. Everything else in the spec is context for the agents; the criteria are the contract.

```markdown
# Spec: close three dogfood-surfaced gaps

## Item 1 - a fresh `cargo install` fails without `--locked`

<context: what is broken, why it matters, what was already tried>

### Done when
- [ ] `cargo install --path . --force` WITHOUT `--locked` resolves and compiles
      cleanly to a working binary ... Verify by ACTUALLY running the install
      into a temp `--root` and executing the resulting binary.

## Global constraints
- Idiomatic Rust; no placeholders.
- Both CI lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets
  -D warnings`; `cargo test`.
```

Rules that make a spec loop-ready:

1. **One criterion, one observable behavior.** A criterion an agent can partially satisfy is two criteria wearing one checkbox.
2. **Name the verification, not just the state.** "Verify by actually running the install into a temp `--root`" beats "install works" - it tells the implementer what evidence to produce and the adjudicator what evidence to demand.
3. **State the fallback if the ideal is impossible.** "If a clean fresh resolve is genuinely impossible, the fallback is BOTH: document it AND add a CI job that catches the regression." Without this, an agent that hits the wall either stalls or silently ships less.
4. **Flag what the gates cannot see.** If a criterion's proof lies outside the gate set (a JS file `cargo test` never touches, an install flow the lock file hides), say so in the spec and instruct the adjudicator to demand explicit evidence. The gate suite verifies what it verifies - a green gate on an unverifiable criterion is a false positive factory.
5. **Global constraints ride along.** Style rules, CI invariants, attribution rules - state them once in the spec; every unit inherits them.

The entry gate is real: `rigger run <spec>` refuses to start unless every acceptance criterion is covered by a stage. A spec with no enumerable criteria does not run - fix the spec.

## The workflow: `.rigger/workflow.yml`

The workflow is a GitHub-Actions-style DAG declaring defaults, a gate library, and stages. The repo's own `.rigger/workflow.yml` is the canonical example - Rigger produces itself with it.

```yaml
name: rigger-self-hosted

defaults:
  autonomy: auto_notify     # manual | auto_notify | silent
  grounder: turbovec        # turbovec | grep | nop
  budget: 60                # spawn-cap circuit breaker (see below)
  max_retries: 6            # remediation depth before escalation
  review:                   # the three-tier panel every unit inherits
    lenses: [architecture-reviewer, sdet]
    adversary: adversary
    adjudicator: adjudicator

gates:                      # the verification library, referenced by name
  fmt:    { run: "cargo fmt --check",                         kind: core }
  clippy: { run: "cargo clippy --all-targets -- -D warnings", kind: core }
  build:  { run: "cargo build",                               kind: core }
  test:   { run: "cargo test",                                kind: core }

stages:
  plan:
    agent: planner
    produces: dag           # may extend the unit DAG at runtime (UnitProposed)

  implement:
    needs: [plan]
    agent: rust-engineer
    strategy: fan-out       # one agent per ready unit
    partition: by-blast-radius
    gates: [fmt, clippy, build, test]
    on_pass: merge
    coverage: "each unit is implemented, reviews itself, and integrates green"
```

### The knobs that matter

**`budget`** is the hard cap on agent spawns for one unattended run. When spawns reach it, the breaker records `BudgetExhausted` and aborts. Keep it non-zero always: `0` means unlimited, and unlimited is how a unit a reviewer keeps rejecting churns for five hours. Raise it for a big spec; never disable it for an unattended run.

**`max_retries`** is remediation *depth*, not review rigor. It bounds how many attempts a failed unit gets before escalating to a human. Raise it when a unit's defects are genuine but diminishing across iterations - it buys convergence room under the full-strength review; it never loosens the review bar. It is itself bounded by `budget`, so it can never spin unboundedly.

**`autonomy`** sets the default gate policy. Start new workflows at `manual` or `auto_notify`; let the ratchet earn `silent` per gate through clean passes. A gate that fails while non-manual demotes itself back to `manual` automatically.

**`partition: by-blast-radius`** keeps fan-out waves disjoint: units whose file footprints could collide never run concurrently. This plus worktree isolation is why parallel implementation is safe.

### The per-unit lifecycle

Review is per unit, not a downstream stage. Each unit runs its own complete cycle, and a failure anywhere feeds back into *that unit's* remediation, never forward into integration:

```
 ground -> implement (RED -> GREEN, in a worktree) -> gates
   -> tier 1: lenses (parallel)  -> tier 2: adversary -> tier 3: adjudicator
   -> approve + green  =>  integrate (merge to the run branch)
   -> reject or red    =>  remediate (re-ground, re-implement with feedback)
                           ... up to max_retries, then escalate to a human
```

Consequence worth knowing: units run as overlapping pipelines, so an earlier unit's review can complete while a later unit is still building. Progress displays group by per-unit phase labels (`u3:Build`, `u3:Review`) precisely so this does not read as stages running out of order.

## The four drivers

Same loop, four entry points - pick by where you are sitting:

| Driver | Command | When |
|---|---|---|
| Native Claude Code workflow | `/rigger specs/feature.md` | The primary driver. Installed by `rigger setup` at `.claude/workflows/rigger.js`; runs inside your Claude Code session, progress visible in `/workflows`. |
| Standalone JS driver | `rigger workflow specs/feature.md` | Same loop from a plain terminal - no interactive session needed. Provisioned in `.rigger/shim/` by `rigger setup`. |
| Standalone CLI driver | `rigger run specs/feature.md` | The lower-level Rust conductor driving the `claude` CLI directly. |
| MCP bridge | `rigger serve` | The conductor as an MCP server: an external harness pulls assignments and reports results over stdio. See [tools-and-context.md](tools-and-context.md#the-mcp-bridge). |

Setup is two commands in any repo: `rigger init` (config-only: writes `.rigger/` with starter agents and workflow) or `rigger setup` (init plus installing the Claude Code workflow and the shim driver). `rigger validate` checks the whole configuration - agents referenced by stages exist, gates referenced by name are declared, the DAG is acyclic.

## Running Rigger on a new project: the checklist

1. `rigger setup` in the repo root.
2. Edit `.rigger/workflow.yml`: replace the gate library with *your* CI commands - the gates must be the same checks CI runs, or the loop green-lights what CI rejects.
3. Adapt the starter agents: your language's engineer instead of `rust-engineer`, your project's defect classes in the reviewer prompts.
4. Write a small spec (one or two criteria) and run it end-to-end at `autonomy: auto_notify` with a low `budget`.
5. Read the run: the decisions emitted, the review verdicts, what integrated. Calibrate prompts and gates against what you see.
6. Scale up: bigger specs, ratcheted autonomy, raised budget.

## Remediation, escalation, and what the loop never does

The failure policy is uniform everywhere: **escalate or bounded-retry - never silently drop, never infinitely spin.** A failed gate re-enters remediation with the failure attached. A review reject re-enters remediation with the findings attached. Exhausted retries escalate to a human with the full evidence trail. A budget trip aborts the run and says so. A spec defect discovered mid-run is flagged against the spec (the authority gets amended; the agent does not quietly deviate from it).

The loop lands code on a **run branch** (worktree commits merge into it on green). Getting the run branch into `main` is your PR, your review, your merge - the loop does not push to main.
