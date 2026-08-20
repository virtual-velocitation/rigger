# Spec 73: mutation testing - the implementer measures its tests' efficacy

## Problem

Nothing measures whether a unit's tests can actually FAIL: the gates prove the suite
passes, the canary measures the review panel, but a test layer that kills no mutants
passes everything. cargo-mutants runs on an unmodified tree and `--in-diff` scopes a
unit's small diff to tens of mutants.

## Design

- The IMPLEMENTER runs the instrument as part of implementation: after its unit-level TDD
  is green, in its worktree, before the periphery author and pre-gate commit. A
  deterministic external tool grading the work is not self-grading; only the judgment
  residue (equivalence justifications) is review material, reaching reviewers through the
  recorded accounting like any other evidence - no re-run duty for any review role.
- CONFIG VS CODE, decided here: the implementer persona
  (`.rigger/agents/rust-engineer.md`) is OPERATOR CONFIGURATION and its mutation-step
  content is seeded by the operator, not authored by any unit - the grounder cannot
  ground non-code files, so no unit can own a Markdown blast radius (a named capability
  gap, out of scope here). The loop owns the CODE (config key, validation) and the
  DRIFT-GUARD TESTS that pin the seeded persona content; the seeded text pre-exists this
  run.
- The step: `cargo mutants --in-diff <diff vs the unit's merge-base with the run branch>`
  on the DEFAULT feature lane only. A missed (surviving) mutant is either KILLED by a
  strengthened test or JUSTIFIED with a concrete equivalence reason; an unjustified missed
  mutant means the unit is not done. Timeouts and unviable mutants are recorded, never
  dropped.
- The accounting: a `DecisionMade` (no new event type), deterministically ordered, one
  entry per mutant with status caught | missed-killed (naming the killing test) |
  missed-justified (with reason) | unviable | timeout, plus the diff base and total. A
  diff touching no `.rs` file records a provably-empty accounting - never a skipped step.
- Enabled-but-absent fails at run start: a `build.mutation` config key (`on`/`off`); `on`
  with no `cargo-mutants` binary on PATH fails `Config::validate()` naming the binary and
  the key; `off` skips the step (no accounting owed). `rigger validate` reports
  `build mutation: on|off`.
- Budget: the mutation run stays inside the unit's existing build-budget slot;
  `--jobs` never exceeds the build budget's per-unit allowance.

## Done when

- [ ] A test pins the implementer persona's seeded mutation step - after unit-green, a
  diff-scoped cargo-mutants run against the unit's merge-base on the default feature
  lane, kill-or-justify as the response to a missed mutant - so drift from that contract
  fails the suite. This criterion OWNS the step drift-guard test; the accounting contract
  is criterion 2's, NOT this one's.
- [ ] A test pins the persona's seeded accounting contract - a deterministically ordered
  `DecisionMade` with one entry per mutant (caught, missed-killed naming the test,
  missed-justified with reason, unviable, timeout), the diff base, the total, and a
  provably-empty accounting for a diff touching no Rust file - so drift fails the suite.
  This criterion OWNS the accounting drift-guard test; step placement is criterion 1's,
  NOT this one's.
- [ ] A test proves enabled-but-absent fails at run start: `build.mutation: on` with no
  cargo-mutants binary resolvable fails `Config::validate()` naming the binary and the
  key, `rigger validate` reports the setting, and `off` validates without probing PATH.
  This criterion OWNS the config key and its resolution.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; the accounting rides `DecisionMade`.
- No new gate and no new conductor stage: agent discipline (persona config) plus config
  validation only.

## Notes

- Suggested invocation (persona-level detail): `git diff <BASE> -- '*.rs' > <tmp>.diff`;
  `cargo mutants --in-diff <tmp>.diff --timeout-multiplier 1.5 -j <share>`; read
  `mutants.out/outcomes.json` for the accounting, not stdout.
- Whole-tree audit scorecard: OUT of scope, deferred; an operator can run bare
  `cargo mutants`.
- cargo-mutants is installed (27.1.0); installing it elsewhere is a setup concern
  (`cargo install cargo-mutants`) - the run-start check makes absence loud; no agent
  installs tooling into the operator's toolchain.
- Periphery author and review personas untouched; reviewers meet the accounting through
  the graph as ordinary evidence.
