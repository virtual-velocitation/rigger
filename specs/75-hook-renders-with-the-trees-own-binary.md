# Spec 75: the pre-commit hook renders with the tree's own binary

## Problem

The managed pre-commit hook renders docs with the PATH `rigger` binary. A worktree whose
code legitimately changes a rendered fact (a new command, a new skill) then deadlocks: its
correctly regenerated docs read as drift against the stale installed render, and the
refusal blocks the very commit that would land the change. Hand-patching the installed
hook does not survive `rigger setup`, which any agent may lawfully run - the fix must live
in the template `install_precommit_hook` composes (src/main.rs:9209).

## Design

- BINARY SELECTION IN THE TEMPLATE, decided here: before rendering, the managed block
  resolves the first executable of: `$CARGO_TARGET_DIR/release/rigger`,
  `$CARGO_TARGET_DIR/debug/rigger`, `./target/release/rigger`, `./target/debug/rigger`,
  `<git-common-dir>/../.rigger/tmp/cargo-target-<unit>/{release,debug}/rigger` where
  `<unit>` derives from the worktree directory name (`rigger-wt-<unit>`), then the shared
  step cache `<git-common-dir>/../.rigger/tmp/cargo-target/debug/rigger`, and finally PATH
  `rigger`. Debug candidates matter: unit gates build the debug profile only.
- SAFE-CLOSED: a wrong candidate renders drift and the existing refusal fires; candidate
  selection can only convert a false refusal into a pass when the render actually matches,
  never the reverse. The refusal message keeps naming the rendering binary and provenance.
- The composer stays byte-level, chaining, idempotent, and non-destructive exactly as
  today; only the managed block's render invocation changes.

## Done when

- [ ] The installed hook prefers a tree-built rigger over PATH: a test at the
  `compose_precommit_bytes` seam proves the managed block resolves the candidate order
  (env target dir release then debug, local target, unit-derived cargo-target, shared step
  cache, PATH last) and invokes the resolved binary for both the render and the provenance
  line. This criterion OWNS the candidate order and its rendering in the template.
- [ ] A commit in a worktree whose code adds a rendered fact passes when that worktree's
  own built binary is present and renders docs matching the staged ones, while the same
  commit with only a stale PATH binary still refuses - proven at the hook-execution seam
  with a fixture repo, a fixture binary, and both outcomes asserted. This criterion OWNS
  the end-to-end hook behavior; the template text is criterion 1's, NOT this one's.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; no new gate; no new conductor stage.
- `rigger setup` on an up-to-date repo stays a byte-identical no-op after this change
  lands and is installed.

## Notes

- Constraints walk: no candidate exists -> PATH fallback, today's behavior; a stale unit
  binary -> drift renders, refusal fires (safe-closed); non-UTF-8 pre-existing hook ->
  byte-level chaining untouched; concurrent worktrees -> each resolves its own unit
  candidate; main-repo commits -> no rigger-wt- prefix, unit candidates skip, shared cache
  then PATH apply.
- Related recorded debts this closes: the hook-reversion-by-setup recurrence and the
  step-killing refusal's immediate trigger. The separate blast-radius question of a
  refusal parking the unit rather than failing the step stays open, deliberately.
