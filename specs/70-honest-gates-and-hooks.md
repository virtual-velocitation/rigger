# 70 - Honest gates and hooks: no silent rewrites, evidence that names the failure, isolated gate stores

**Goal:** close three loop-infrastructure defects, measured in production runs, that launder
infrastructure failures into the unit under test:

1. **The managed pre-commit hook silently rewrites rendered docs.** It re-renders with
   whatever binary is first on PATH; a binary older than the tree re-renders committed docs
   to the OLD text and silently stages them, stripping the branch's rendered changes from
   every later commit. (Cost: three rejected attempts on one unit.)
2. **Gate evidence cannot name a failing test.** The compactor keeps the first five
   failure-keyword lines; passing test names containing the keyword consume every slot, so
   the recorded evidence never names the test that failed. (Cost: every consumer re-runs the
   suite to learn what the gate already knew.)
3. **A gate's test process can bind the LIVE project store.** Gate cwd is inside the unit
   worktree, whose committed `.rigger/` has no `events.db`, so store resolution walks up to
   the running project's store. (Cost: a gate red irreproducible in four clean environments.)

## Design

- **The hook fails loudly, never rewrites silently** (`src/main.rs`, the managed pre-commit
  hook install and its script): a re-render that DIFFERS from the staged content fails the
  commit, naming the drifted files, the rendering binary (path and build provenance), and
  the two remedies - re-render with the tree-built binary, or reinstall. It never stages its
  own render. A matching render passes silently, exactly as today.
- **Gate evidence prefers real failure markers** (`src/gate.rs::compact`): genuine failure
  syntax (`test <name> ... FAILED`, `error[`, `panicked at`, the failures summary block)
  ranks above mere keyword hits, and a passing `... ok` line never consumes a slot. The
  five-line bound and the compactor's role are unchanged.
- **The gate's store resolution is fenced** (`src/gate.rs` env seam, same seam as the
  target-dir override): every gate process runs with store resolution pinned to an isolated
  scratch store, so a walk-up finds a fenced empty store, never the live run's. The fence is
  the gate runner's job, not each test's.

## Notes (non-criteria)

- Shared principle: infrastructure fails loudly in its own name, never laundered into the
  unit under test.
- Hook scope: the managed hook only; a consumer's own hooks are theirs.
- If the store-resolution authority lacks an env override, the fence adds one - additive,
  defaulted off, honored only when set by the gate runner.
- No new event type is introduced anywhere in this spec.
- Two non-blocking gaps surfaced during review, named here so they are not lost when this
  spec closes: `cmd_setup`'s hook narration (`src/main.rs`, the `install_precommit_hook` call
  comment and the `InstallOutcome::Installed` println under `match hook`) still describes the
  pre-unit-1 stage-and-commit contract even though the hook now refuses instead of staging on
  drift, and no test pins the corrected string; and no meta-test enforces the
  `tests/common::rigger_courier()` store-fence-clearing convention on future test files, unlike
  the existing precedent for the product-binary authority. Neither affects a Done-when
  criterion of this spec; a future pass should close both.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Fail-safe directions: the hook may only ever REFUSE more (never stage more); the evidence
  compactor may only ever inform more (same bound, better lines); the store fence may only
  ever isolate more (a fenced gate sees strictly less ambient state).
- The docs-drift gate itself is untouched: it keeps failing when committed renders drift;
  this spec changes who gets told and when, not the invariant.

## Done when

- [ ] a test proves the HOOK REFUSES INSTEAD OF REWRITING: with a staged render differing from
  the hook binary's render, the commit fails naming the files and the rendering binary's
  provenance, and nothing is re-staged; with a matching render the commit passes untouched.
  This criterion OWNS the hook behavior.
- [ ] a test proves EVIDENCE NAMES THE FAILURE: gate evidence for a cargo test output whose
  early lines contain passing tests with failure-keyword names carries the actual `FAILED`
  test line and the failures summary, and no `... ok` line occupies an evidence slot. This
  criterion OWNS the compactor ranking.
- [ ] a test proves the GATE STORE FENCE: a gate-spawned process whose cwd is a unit worktree
  under `.rigger/tmp` resolves its store to the fenced scratch location, not the repo's live
  store - pinned at the gate runner's env seam, with the live store byte-identical before and
  after the gate. This criterion OWNS the fence.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
