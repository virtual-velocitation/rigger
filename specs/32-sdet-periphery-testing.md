# 32 - SDET owns the periphery test layer: no boundary surface lands untested

**Goal:** give the unit lifecycle a distinct SDET test-authoring role that owns the PERIPHERY test
layer - contract, API, and integration tests - so a boundary a unit exposes cannot land untested.
The implementer keeps its inside-out, unit-level TDD (the developer's process for proving the unit
does what they meant); the SDET adds the outside-in layer that unit tests are structurally blind to
(does the unit honor its contract, behave at its API edges, and integrate?). The role is always-on
and self-scoping, its surface accounting is exhaustive and diff-grounded, and the tier-2 adversary is
the independent backstop that makes "no untested surface" a guarantee rather than a hope.

## Design

The unit lifecycle is hardcoded Rust control flow in `RunCtx::run_single_stage` (`src/conductor.rs`):
implementer spawn -> commit worktree -> `run_gates` -> `review_unit` -> integrate, in one loop that
re-enters on remediation. The seam for the SDET is BETWEEN the implementer emitting green (its code +
unit tests pass, in its worktree) and the pre-gate commit - so the SDET's periphery tests land in the
SAME committed tree the gates and reviewers see.

**The SDET test-author is a new role, because today's SDET cannot write.** `.rigger/agents/sdet.md`
is a review LENS with tools `[Read, Grep, Glob, Bash]` - no `Edit`/`Write` - so its prompt's "write
the failing test first" is aspirational; it physically cannot author. This spec adds a NEW agent
`.rigger/agents/sdet-author.md` with `Edit`/`Write` + `isolation: worktree` (shaped like
`rust-engineer.md`), a new role token in `src/spawn.rs` (alongside `ROLE_IMPLEMENTER`), and a spawn
call in `run_single_stage` that parks it at the seam above. It is always spawned (config-driven,
defaulting on) and self-scopes.

**Self-scoping is exhaustive and diff-grounded, never a glance.** The author may not conclude "no
seam here" by inspection. Its first duty is a COMPLETE enumeration of the unit's boundary surface from
the diff, each item grounded in a deterministic signal, and for each: covered-by-a-named-periphery-
test or exempt-with-a-specific-reason. It records that accounting as a `DecisionMade` (no new event
type). "No periphery surface" is valid ONLY when the enumeration is provably empty.

```
surface                         diff signal                          periphery layer / test
-----------------------------   ------------------------------       -------------------------------
new/changed public API          `pub fn|struct|enum|trait` added     API test (tests/cli.rs et al.)
trait impl                      `impl <Trait> for`                   contract suite (assert_contract,
                                                                     src/eventstore/contract.rs)
CLI subcommand / flag           addition to the command registry     tests/cli.rs (drives the binary)
event type / serialized format  new Event / changed serde struct     round-trip + back-compat contract
cross-module seam / fold arm    new call across a module boundary    integration test
```

**Gates need no change:** the `test` gate is unscoped `cargo test` (`config::Gate.inputs` empty), so
any periphery test the author writes - under `src/eventstore/contract.rs`, `tests/cli.rs`,
`tests/ci_lanes.rs`, or a new `tests/*.rs` - runs automatically. A failing periphery test reveals a
boundary bug and drives remediation of the CODE (the implementer), never a weakening of the test.

**The adversary is the independent completeness backstop.** `.rigger/agents/adversary.md` gains a
surface-completeness hunt: independently re-enumerate the diff's boundary surface, confirm the author
accounted for EVERY item, and mutate a periphery-tested behavior to confirm the tests discriminate. A
surface the author missed or wrongly exempted, or a vacuous periphery test, is a BLOCKING finding. Two
independent enumerations must agree - the guarantee never rests on one agent's judgment.

**Independence holds because authorship and review are separated.** The `sdet-author` writes periphery
tests; the `sdet` lens reviews the IMPLEMENTER's code + unit tests (it authored neither); the adversary
vets the author's periphery tests; the adjudicator hard-gates - a unit with any unaccounted surface is
a reject, in the constraints-recheck category. No role grades its own artifact.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: the surface accounting and any folded set use sorted / `BTreeMap` /
  `BTreeSet`; identical input yields an identical accounting.
- No new event type: the surface accounting is recorded as a `DecisionMade`; the emit allowlist is
  unchanged.
- The implementer's inside-out unit-level TDD is UNTOUCHED: the sdet-author adds ONLY the periphery
  layer (contract / API / integration) and never authors or edits the unit tests.
- The role is ALWAYS spawned and self-scopes: a purely-internal unit yields a provably-empty
  accounting and a fast no-op, never a skipped surface.

## Done when

- [ ] a test proves a distinct, write-capable SDET-AUTHOR ROLE exists: a new agent
  `.rigger/agents/sdet-author.md` with `Edit`/`Write` tools + `isolation: worktree` (separate from the
  read-only `sdet` review lens), plus its role token in `src/spawn.rs`. This criterion OWNS the
  sdet-author role definition; it does NOT own the conductor spawn wiring (the spawn-placement criterion
  below).
- [ ] a test proves the conductor SPAWNS the sdet-author at the build seam - after the implementer
  emits green and before the pre-gate commit, in the implementer's worktree - so its authored periphery
  tests land in the committed tree the gates run against. This criterion OWNS the spawn placement in
  `run_single_stage`; it does NOT own the role definition (the role-definition criterion above).
- [ ] a test proves the sdet-author ENUMERATES the unit's boundary surface from the diff: every
  public-API / trait-impl / CLI / event-format / cross-module-seam item is identified, each grounded in
  a deterministic diff signal. This criterion OWNS producing the surface enumeration; it does NOT own
  recording it (the accounting-record criterion below).
- [ ] a test proves the enumeration is RECORDED as a `DecisionMade` and is EXHAUSTIVE: each item is
  marked tested (naming the periphery test) or exempt (with a reason), and an empty accounting is valid
  only when the diff contains no such item. This criterion OWNS the accounting record and its
  exhaustiveness; it does NOT own producing the enumeration (the enumeration criterion above).
- [ ] a fixture proves NO UNTESTED SURFACE LANDS: a unit that adds a boundary surface (a new `pub fn`
  / trait impl / CLI flag) with NO periphery test BLOCKS - the adversary independently re-enumerates,
  flags the uncovered-or-wrongly-exempted surface as a blocking finding, and the adjudicator rejects, so
  the unit cannot integrate green. This criterion OWNS the independent adversary completeness backstop
  and the no-untested-surface guarantee; it does NOT own the sdet-author's own enumeration or accounting
  (the enumeration / accounting criteria) or the spawn.
- [ ] a test proves review INDEPENDENCE: the `sdet-author` (authors periphery tests) is a distinct role
  and agent from the `sdet` lens (reviews the implementer's code + unit tests) and the adversary (vets
  the periphery tests); no role reviews its own authored artifact. This criterion OWNS the independence
  invariant; it does NOT own the spawn, enumeration, accounting, or guarantee (the criteria above).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
