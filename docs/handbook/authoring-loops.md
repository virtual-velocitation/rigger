# Authoring loops

A loop is the machine that turns a spec into integrated code. You author one by writing two artifacts - a **spec** (what "done" means) and a **workflow** (who does the work, checked by what) - and then driving them with one of four drivers. This document covers all three parts.

## The spec

A spec is a Markdown file whose load-bearing content is its acceptance criteria: enumerable, machine-checkable "Done when" bullets. Everything else in the spec is context for the agents; the criteria are the contract.

The excerpt below is from a real spec the Rigger repo ran on itself - Rigger is a Rust project, so its criteria name cargo commands. The *shape* is what to copy; substitute your own toolchain throughout.

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
4. **Flag what the gates cannot see.** If a criterion's proof lies outside the gate set (an artifact in a language your test suite never compiles, an install flow the dependency lock file hides), say so in the spec and instruct the adjudicator to demand explicit evidence. The gate suite verifies what it verifies - a green gate on an unverifiable criterion is a false positive factory.
5. **Global constraints ride along.** Style rules, CI invariants, attribution rules - state them once in the spec; every unit inherits them.
6. **A shared blast radius is fine - the loop serializes it; ambiguous ownership is what hurts.** Two units touching the same file are NOT a defect: `partition: by-blast-radius` runs them in separate sequential batches (each branches off the prior batch's integrated tree) and per-unit worktree isolation keeps every reviewer on its own diff, so overlap integrates cleanly and reviews independently. The spec-05 disaster (three setup-path criteria, 40 rejections, five escalations) was long blamed on "reviewers see siblings' half-landed changes" - but worktree isolation makes that impossible. The real cause was cross-unit OWNERSHIP confusion through the shared context graph (rule 7) and open dispositions (rule 8): a reviewer grounding on a sibling's decisions enforced the sibling's criterion, so the unit could not converge. So split coupled criteria freely - just give each unit clean ownership (rule 7) and no open dispositions (rule 8). Only merge into one unit when the concerns are genuinely inseparable, not merely because they share a file.
7. **Assign every demanded mitigation to exactly one criterion, and say so.** When a review disposition spawns follow-up criteria, name which unit OWNS each mitigation and write the exclusion into the neighbors ("the orphan-id advisory is unit-9's, NOT this unit's") - otherwise a reviewer enforces a sibling's criterion and the implementer cannot converge on its own.
8. **A criterion must not leave a disposition open.** "Removed" and "ignored" are different verdicts on the same files; if the spec has not picked one, the implementer will pick one and a reviewer will pick the other. Decide in the spec; a criterion a reviewer can reasonably re-litigate is a rejection loop waiting to run.

These rules say what a loop-ready spec IS. The [planning field guide](planning-field-guide.md) is the other half - how to PRODUCE one: the failure catalog distilled from this repository's own run history, with a mechanical countermeasure for each recorded class. Read it before writing a spec.

The entry gate is real: `rigger run <spec>` refuses to start unless every acceptance criterion is covered by a stage. A spec with no enumerable criteria does not run - fix the spec.

## The workflow: `.rigger/workflow.yml`

The workflow is a GitHub-Actions-style DAG declaring defaults, a gate library, and stages. The example below is the Rigger repo's own `.rigger/workflow.yml` - Rigger produces itself with it, so the gates are cargo commands and the engineer is a Rust engineer. Nothing about the structure is Rust-specific: your gate library is whatever your CI runs (`npm test`, `pytest`, `go vet ./...`), and your engineer agent is whatever your stack needs.

```yaml
defaults:
  autonomy: auto_notify     # manual | auto_notify | silent
  grounder: symbols         # symbols | grep | nop
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

Upgrading an older workflow: the top-level `name:` key is retired, and `rigger validate` refuses a workflow.yml that still carries it - delete that line.

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

### The check-in mutation sweep: bounds, scope and budget

The `checkin` stage runs the `mutation` gate once, after every implement unit has integrated: `cargo mutants` over the whole spec diff, one remediation round for its survivors, then the sweep again. Its logic lives in `.rigger/gates/mutation.sh` (`rigger init` writes the same script into your project; point your `mutation` gate at `sh .rigger/gates/mutation.sh` for a Rust workspace). What it guarantees:

- **Scope.** Mutants come from every package the diff touches (`--workspace` with `--in-diff`), and each mutant runs only the tests of those packages plus the root package's, never the whole workspace's (`--test-package`, never `--test-workspace`). The package of a file is the nearest `Cargo.toml` that declares a `[package]`. A survivor this exposes is closed with a test in the right crate, never by widening the test scope.
- **Memory bound.** The sweep runs in its own transient systemd scope with `MemoryMax` at half of `MemAvailable` when it starts, so a runaway mutant can exhaust only the sweep's own memory - never the step that launched the gate or the operator's session. The scope carries `OOMPolicy=continue`: when the kernel's out-of-memory reaper ends a process inside it, that ends one mutant's build or test, not the whole sweep (systemd's default policy stops the entire scope). With no systemd user manager (a CI container) the sweep runs unbounded and the gate prints an advisory saying so. Separately, every test process runs under a 4 GiB address-space cap (`.cargo/pidns-runner.sh`).
- **Memory shape.** A healthy mutant copy is small: measured on a 32-core machine, a cold 8-job test build of the root package peaks at 1.3 GiB of anonymous memory (the rest of its footprint is page cache for the artifacts it writes, which the kernel reclaims at the bound), and its nextest run peaks at 3.0 GiB at 32 test threads and 1.5 GiB at 10. What reaches the bound is a runaway mutant: every test process in flight runs the same mutated code and can grow to the 4 GiB cap, so the exposure scales with the number of test processes in flight across all copies, not with the number of copies. The gate therefore holds that total at the core count: each copy runs cores / `-j` nextest test threads (passed after `--`, which cargo-mutants hands to the test phase only). `-j` comes from the bound at 5 GiB per job, at least 1 and at most 3: 5 GiB covers one build per copy plus the one core-wide test fan-out all copies share, for any `-j`.
- **Verdict on a reaper-ended mutant.** cargo-mutants exits 0 even when a mutant's phase was ended by a signal, so the gate reads each mutant's log. A test phase ended by a signal counts as a detection, like a timeout: the mutant made its tests grow until the reaper ended them. A build phase ended by a signal, or a compiler under it, means the mutant was never tested: the gate fails with an environment failure naming each such mutant and phase (the full list is `mutants.out/environment.tsv`), and it does not advance its incremental anchor, so the next run examines those mutants again.
- **Budget.** A typical unit diff sweeps in minutes, not hours: the baseline tests only the mutated packages (seconds), each job's first mutant pays one build of the root package in its copy, and each caught mutant stops at its first failing test (nextest). Each mutant's test run is bounded at 300 s, and nextest ends any single hung test at 240 s; a timeout counts as a detection. Re-sweeps are incremental: only mutants in code changed since the last swept tree, earlier survivors, and catches whose catching test changed are examined again.
- **The instrument is the gate's.** A unit that adds an exclusion or examine key to `.cargo/mutants.toml`, or a skip attribute in the code, fails the gate before any sweep runs.

`rigger reset --build-cache` reclaims every class of dead scratch `rigger validate`'s footprint names with that verb - dead per-unit caches, dead spawns' registered scratch, unowned agent scratch, and the shared gate build cache - and leaves any entry a live process still holds (its working directory or an open file) where it is.

## The four drivers

Same loop, four entry points - pick by where you are sitting:

| Driver | Command | When |
|---|---|---|
| Native Claude Code workflow | `/rigger specs/feature.md` | The primary driver. Installed by `rigger setup` at `.claude/workflows/rigger.js`; runs inside your Claude Code session, progress visible in `/workflows`. |
| Standalone JS driver | `rigger workflow specs/feature.md` | Same loop from a plain terminal - no interactive session needed. Provisioned in `.rigger/shim/` by `rigger setup`. |
| Standalone CLI driver | `rigger run specs/feature.md` | The lower-level conductor binary driving the `claude` CLI directly. |
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
