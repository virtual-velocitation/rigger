# Pit of Success - Implementation Plan (Campaign)

> **For agentic workers:** this plan is executed by the rigger loop itself, not by hand
> and not by ad-hoc subagents. Each spec below is run through the native `/rigger`
> workflow (visible in `/workflows` and the dashboard); the conductor decomposes each
> spec into units and takes every unit through implement -> cargo gates -> three-tier
> adversarial review -> integrate, with bounded remediation. TDD is intrinsic to that
> lifecycle; do not add a separate TDD or execution harness. If a spec cannot be built by
> the loop, that is a gap in the loop to fix, not a reason to hand-build.

**Goal:** make rigger's existing guarantees reachable, visible, validated, and
self-documenting, per `docs/architecture-addendum-pit-of-success.md`.

**Architecture:** four loop-ready specs, one per addendum workstream, authored to the
spec-shape rules the campaign itself hardens (one observable behavior per criterion; type
shapes in Notes). Each spec is independently runnable and reviewable; the campaign
ordering respects the cross-spec dependencies below.

**Tech stack:** Rust (the rigger crate), the native `/rigger` Claude Code workflow, the
`.rigger/workflow.yml` self-hosted gate library (cargo fmt / clippy / test on both feature
lanes), a Rust-native compile-time-checked template engine for spec 20.

## Global constraints (inherited by every spec)

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both CI lanes stay green on every unit: `cargo fmt --check`; `cargo clippy --all-targets
  -D warnings`; `cargo test` - on default features AND on `--no-default-features`.
- Determinism by construction: serialized data uses `BTreeMap`/`BTreeSet`/sorted `Vec`.
- The addendum's load-bearing decisions are preserved throughout: the gate reads only the
  result channel; the base default stays `origin/main`; cross-run grounding is not scoped
  to the active run.

## The specs

| Spec | Workstream | Owns (primary files) |
|---|---|---|
| `specs/18-fail-fast-validation.md` | A - fail fast | `config.rs`, `main.rs` (validate/workflow/run/load_criteria), `conductor.rs` (gate, harvest), `spec.rs`, `worktree.rs` |
| `specs/19a-observability-surfaces.md` | B - observability | `main.rs` (status/setup), `dash.rs`, `workflows/rigger.js`, `spawn.rs`, `conductor.rs` (coverage wire) |
| `specs/19b-dashboard-and-lifecycle.md` | B - observability | `main.rs` (run/serve), `dash.rs`, `dash.html`, `shim/shim.mjs` |
| `specs/19c-loud-failure-surfacing.md` | B - observability | `conductor.rs` (done/escalation), `workflows/rigger.js`, `config.rs`, `main.rs` (validate) |
| `specs/21-provenance-and-pruning.md` | D - provenance | `contextgraph/`, `run.rs`, `main.rs` (reset/peers), `conductor.rs` (graph_context) |
| `specs/20-self-documenting-discipline.md` | C - self-documenting | `main.rs` (docs/validate/setup), a new template set |

## Ordering and dependencies

Run in this order:

1. **Spec 18 (Workstream A).** Independent. Produces the `unmatched-proposal` signal and
   the verdict-channel diagnostic that spec 19's blocker line consumes, and the `--base`
   flag / refusal messages that spec 20 documents.
2. **Specs 19a -> 19b -> 19c (Workstream B).** The monolithic spec 19 was SPLIT into three
   small specs after it repeatedly wedged: its 9 criteria led the planner to over-refine a
   criterion into two units with byte-identical criteria (a rule-7 loop the planner cannot
   win). Each split spec is 4-5 atomic criteria the planner keeps whole. Run in order:
   19a (blocker line, setup discoverability, tagline, work-line) - its blocker line consumes
   spec 18's `unmatched-proposal` signal + verdict diagnostic, so 18 precedes it;
   19b (always-on dash, responsive redesign, no-orphaned-process);
   19c (wedged run surfaces as a loud error, no silent hang).
3. **Spec 21 (Workstream D).** Independent of 19 and 20. Adds the `reset` and `peers`
   surface that spec 20 documents.
4. **Spec 20 (Workstream C) - LAST.** Its generated subcommand list and CLI facts must
   reflect the complete surface (the `--base` flag from 18, and the `reset`/`docs`
   subcommands from 21 and itself), so it renders after every other spec has landed. Its
   drift check then locks the docs to the final code.

Rationale for docs-last: spec 20 reads the subcommand registry and CLI facts FROM code;
running it before 18/21 land would generate docs missing the new flags and commands, and
the drift check would immediately fail once they landed. Running it last captures the whole
surface in one render.

## How to run the campaign

For each spec, in the order above, drive it through the native workflow (the blessed,
visible driver):

```
/rigger specs/18-fail-fast-validation.md
```

Watch progress in `/workflows` and the dashboard (`rigger dash`, then the printed
`127.0.0.1` URL). The run lands integrated commits on the `rigger-run` branch (which
already exists, so it is reused, not re-anchored off `origin/main`). Advance to the next
spec only after the prior spec reaches a clean fixpoint (all its units integrated, zero
escalations). A wedge is fixed at the loop level (spec, persona, or config) - never by
hand-implementing or hand-banking a unit.

After all four specs are green, a human turns `rigger-run` into `main` through normal PR
review (the loop lands on the run branch; humans land on main).

## Acceptance - the pit-of-success canary

After the four specs land, prove the acceptance test of the addendum (§8) with a canary
spec + persona set that deliberately trips each guard, asserting the tool fails loud (or
steers the operator away) instead of hanging:

- a gating persona whose only verdict path is `rigger_emit` -> `rigger validate` and run
  start HARD-error naming the fix (spec 18);
- a base ref missing every file the spec's criteria name -> refusal suggesting `--base`
  (spec 18);
- a multi-behavior criterion -> a named advisory at `rigger validate` time (spec 18);
- a paraphrased planner proposal -> matched by id or run-and-flagged, never a silent
  duplicate (spec 18);
- a hand-edited `using-rigger` skill -> `rigger validate` drift failure (spec 20);
- a stale decision from a dead run -> labelled historical, prunable with `rigger reset
  --runs` (spec 21).

The canary lives under `canaries/` (the repo's existing canary home) and is itself a
spec/persona fixture, not production code.

## Self-review (spec coverage vs the addendum)

- Addendum §2 (load-bearing decisions) -> preserved as global constraints in every spec
  and documented by spec 20 Unit 3.
- Addendum §3 (Workstream A) -> spec 18, Units 1-7 (incl. §3.5 version + build-provenance drift diagnostic).
- Addendum §4 (Workstream B) -> spec 19, Units 1-7 (incl. §4.5 no orphaned processes, §4.6 wedged run surfaces as a loud error, §4.7 no silent hang).
- Addendum §5 (Workstream C) -> spec 20, Units 1-3, criteria 1-4.
- Addendum §6 (Workstream D) -> spec 21, Units 1-3, criteria 1-3.
- Addendum §8 (acceptance) -> the canary section above.
