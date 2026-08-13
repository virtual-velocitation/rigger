# 70 - Honest gates and hooks: no silent rewrites, evidence that names the failure, isolated gate stores

**Goal:** close three loop-infrastructure defects that a production unit measured and root-caused
while burning most of its remediation budget on them (recorded across the u5 compaction attempts;
none of the three is covered by any queued spec):

1. **The managed pre-commit hook silently rewrites rendered docs.** The hook runs `rigger docs`
   with whatever binary is first on PATH; when that binary predates the tree (the operator's
   installed build vs a branch that changes the render), the hook re-renders the committed
   operator docs to the OLD text and SILENTLY STAGES them - stripping a branch's rendered
   changes on every later commit by any agent, including test-only commits whose blast radius
   never mentions docs. Recorded cost: three rejected attempts on one unit before an agent
   root-caused it.
2. **Gate evidence cannot name a failing test.** The gate-output compactor keeps the first five
   lines containing a failure keyword; over cargo test output those slots are consumed by
   PASSING test names that merely contain the word (`..._counts_from_the_prior_FAILURE ... ok`),
   so the evidence a rejected unit receives never names the test that actually failed, and
   every consumer of that evidence re-runs the suite to learn what the gate already knew.
3. **A gate's test process can bind the LIVE project store.** The gate runs with cwd inside the
   unit worktree under `<repo>/.rigger/tmp/`; the worktree carries a committed `.rigger/` with
   no `events.db`, so store resolution walks UP and binds the running project's store - a test
   that touches ambient store paths can then collide with the live run (measured as a gate red
   nobody could reproduce in four independent clean environments).

## Design

- **The hook fails loudly, never rewrites silently** (`src/main.rs`, the managed pre-commit
  hook install and its script): when the hook's re-render DIFFERS from what is staged, the
  commit FAILS with a message naming the drifted files, the binary that rendered (path and
  build provenance), and the two ways forward - re-render with the tree-built binary, or
  reinstall. It never stages its own render over the author's. A hook whose render MATCHES
  stays silent and passes exactly as today. (Silent mutation of a commit's content is the
  defect; loud disagreement is the fix.)
- **Gate evidence prefers real failure markers** (`src/gate.rs::compact`): the compactor ranks
  genuine failure lines - the harness's own failure syntax (`test <name> ... FAILED`, `error[`,
  `panicked at`, the failures summary block) - ABOVE mere keyword hits, and a passing line
  (`... ok`) never consumes an evidence slot. The five-line bound and the compactor's role are
  unchanged; what fills the slots becomes the five most diagnostic lines the output holds.
- **The gate's store resolution is fenced** (`src/gate.rs` env seam, same seam as the
  target-dir override): every gate process runs with the store-resolution environment pinned
  to an ISOLATED scratch store location (the env override the store-resolution authority
  already honors), so a test that walks up finds a fenced empty store, never the live run's.
  The fence is the gate runner's job, not each test's: shipped tests already isolate, and the
  fence makes the NEXT unisolated test a self-contained red instead of a live-store collision.

## Notes (non-criteria)

- The three fixes share one principle: infrastructure must fail loudly in its own name, not
  launder its failures into the unit under test.
- Hook scope: the managed hook only; a consumer's own hooks are theirs.
- If the store-resolution authority lacks an env override to pin, the fence adds one -
  additive, defaulted off, honored only when set by the gate runner.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to prior
  harnesses or to projects unrelated to the mechanism.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Fail-safe directions: the hook may only ever REFUSE more (never stage more); the evidence
  compactor may only ever inform more (same bound, better lines); the store fence may only
  ever isolate more (a fenced gate sees strictly less ambient state).
- The docs-drift gate itself is untouched: it keeps failing when committed renders drift; this
  spec changes who gets told and when, not the invariant.

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
