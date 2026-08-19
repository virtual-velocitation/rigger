# Spec 73: mutation testing - the implementer measures its tests' efficacy

## Problem

Nothing in the loop measures whether a unit's tests can actually FAIL. The gates prove the
suite passes; the canary measures the review panel's efficacy (judge-the-judges); but a test
layer that kills no mutants passes every gate we have. The instrument for this exists and
fits the loop's shape: cargo-mutants runs on an unmodified tree and scopes to a diff
(`--in-diff`), so a unit's small diff yields tens of mutants, not the crate's thousands.

Placement was decided in design conversation (operator decision, 2026-08-19): the
IMPLEMENTER runs the instrument as part of implementation - the tightest loop, in a worktree
with a warm target dir, killing survivors while the code is fresh. Running a deterministic
external tool over your own work is not self-grading: the tool grades. Only the JUDGMENT
residue (a claim that a surviving mutant is semantically equivalent) is review material, and
it reaches the reviewers through the recorded accounting like any other evidence - no
special re-run duty is created for any review role.

## Design

- TOOL, decided here: cargo-mutants (subcommand `cargo mutants`), chosen over mutest-rs
  (requires source annotation and a custom driver) and mutagen (unmaintained, nightly-only,
  requires annotation). It needs no tree modification and emits machine-readable outcomes
  (`mutants.out/outcomes.json`).
- WHERE IT RUNS: in the implementer's worktree, AFTER its unit-level TDD is green and BEFORE
  the periphery author and pre-gate commit. Diff base is the unit's merge-base with the run
  branch - the same base the periphery author's surface probes use.
- THE STEP: `cargo mutants --in-diff <diff file>` where the diff file is
  `git diff <BASE>` over `*.rs`; mutation runs use the DEFAULT feature lane only (the
  container-backed lane under mutation is prohibitively slow; decided here, not per unit).
  A missed (surviving) mutant is either KILLED by a strengthened test or JUSTIFIED in the
  accounting with a concrete equivalence reason; an unjustified missed mutant means the unit
  is not done. Timeouts and unviable mutants are recorded as their own statuses, never
  dropped.
- THE ACCOUNTING: recorded as a `DecisionMade` (no new event type, the sdet-author
  accounting precedent), deterministically ordered, one entry per mutant with status
  caught | missed-killed (naming the killing test) | missed-justified (with reason) |
  unviable | timeout, plus the diff base and mutant total. A unit whose diff touches no
  `.rs` file records a provably-empty accounting and fast no-op - never a skipped step.
- ENABLED-BUT-ABSENT FAILS AT RUN START (the spec-65 wrapper precedent, no silent degrade):
  a `build.mutation` config key (`on`/`off`) governs the step; `on` with no `cargo-mutants`
  binary on PATH fails `Config::validate()` at run start naming the binary and the key.
  `off` skips the step loudly in the persona contract (the accounting is then not owed).
  `rigger validate` reports `build mutation: on|off` alongside the wrapper line.
- BUDGET: the mutation run stays inside the unit's existing build-budget slot; cargo-mutants
  `--jobs` is not raised above the build budget's per-unit allowance. Flag detail in Notes.

## Done when

- [ ] The implementer persona instructs the mutation step: after unit-green, a diff-scoped
  cargo-mutants run against the unit's merge-base on the default feature lane, with
  kill-or-justify as the response to a missed mutant; a test pins the persona content so
  drift from this contract fails the suite. This criterion OWNS the persona's mutation-step
  text; the accounting record shape is criterion 2's, NOT this one's.
- [ ] The implementer persona specifies the mutation accounting: a deterministically ordered
  `DecisionMade` with one entry per mutant (caught, missed-killed naming the test,
  missed-justified with reason, unviable, timeout), the diff base, and the total, with a
  provably-empty accounting for a diff touching no Rust file; a test pins this content. This
  criterion OWNS the accounting record shape; the step's placement and response contract are
  criterion 1's, NOT this one's.
- [ ] A test proves ENABLED-BUT-ABSENT FAILS AT RUN START: with `build.mutation: on` and no
  cargo-mutants binary resolvable, `Config::validate()` fails naming the binary and the
  config key, and `rigger validate` reports the mutation setting; with `off` validation
  passes without probing PATH. This criterion OWNS the config key and its resolution.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` on default features AND `--no-default-features`.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; the accounting rides `DecisionMade`.
- The mutation step itself is agent discipline (persona config) plus config validation - it
  adds NO new gate to the gate library and NO new conductor stage.

## Notes

- Suggested invocation detail (persona-level, not criteria): write the diff with
  `git diff <BASE> -- '*.rs' > /tmp/unit.diff`; run
  `cargo mutants --in-diff /tmp/unit.diff --timeout-multiplier 1.5 -j <build-budget-share>`;
  read `mutants.out/outcomes.json` for the accounting rather than parsing stdout.
- The whole-tree audit (canary-style scorecard over the full crate, sharded, hours-long) is
  OUT of scope here, deliberately: an operator can run bare `cargo mutants` directly; a
  scorecard surface can be specced later if survivors-at-scale become a workflow.
- Installing cargo-mutants is an operator/setup concern (`cargo install cargo-mutants`);
  this spec's run-start check makes its absence loud, and setup docs gain the install line
  when docs are next regenerated - no agent installs tooling into the operator's toolchain.
- The periphery author and review personas are untouched: reviewers meet the accounting
  through the context graph as ordinary evidence.
