# 89 - Harness housekeeping never costs a round: checkpoints, scratch placement, reclaim guard, step cwd, per-unit pipelining

**Goal:** five harness behaviours spent rounds or hours on the spec-86 run without any defect in
the unit under review. (1) The driver's outer wall-clock (`OUTER_WALL_CLOCK_SEC`, 4h,
workflows/rigger.js:100) halted an implementer whose round was finished and verified but
uncommitted after a one-hour `cargo mutants` sweep; the operator re-drove it by hand. (2)
Per-spawn scratch (`rigger scratch <id>`) puts `TMPDIR` and `CARGO_TARGET_DIR` under
`<repo>/.rigger/tmp/agent-scratch`, inside the live store tree, so the twelve store-walk unit
tests (`find_store_dir_from` and kin) climb into the real `.rigger` and fail - every reviewer
re-reproduces and rules that out every round. (3) The reclaim guard (src/reap.rs:302) refuses
`~/.cache/rigger-mutants/<spawn>` as "not strictly under" `~/.cache/rigger-mutants` once the
target is gone, so every `rigger result` logs a false refusal. (4) `rigger step` trusts the
shell's cwd: run from inside a linked worktree it fails with "'rigger-run' is already used by
worktree", and the driver stopped after 28 waves for that reason. (5) The driver awaits a whole
parallel wave before the next step (`await parallel(wave.map(...))`, workflows/rigger.js:793), so
one unit's finished round waits on a sibling's hour-long sweep before its review starts.

## Design

CHECKPOINT BEFORE LONG WORK, decided: the implementer persona commits a checkpoint
(`wip(<unit>): checkpoint before <mutation sweep | lane suite>`) before `cargo mutants` and before
any full lane suite, and squashes it into its round commit when the round is reported. When a
spawn is halted by the liveness sweep with UNCOMMITTED changes in its worktree, the conductor
commits them as `wip(<unit>): tree of halted spawn <id>` before re-parking - the re-park prompt
names that commit and says "finish and report; do not start over" - so a halt never discards a
tree. The halt still charges no attempt (it is an infrastructure fault today and stays one).

SCRATCH LIVES OUTSIDE THE STORE TREE, decided: the per-spawn scratch container, `TMPDIR`, and
`CARGO_TARGET_DIR` defaults move to `$XDG_CACHE_HOME/rigger/<project-id>/<run>/<spawn>` (on the
large mount, never under `/tmp`, never under any `.rigger`); the registered scratch roots and the
reaper (spec 77's lifecycle) follow the relocation, `rigger validate` reports the footprint at
the new root, `rigger reset --build-cache` reclaims there, and the persona line that pinned
`TMPDIR` to repo scratch (spec 73) is rewritten. The store-walk tests then pass by construction
under a spawn's environment, with no per-test workaround.

THE RECLAIM GUARD COMPARES PATHS, decided: `reap.rs` normalizes the joined path lexically
(`.`/`..` segments resolved, no filesystem canonicalization) before the "strictly under" check,
and treats a target that no longer exists as already reclaimed (silent), so the refusal message
appears only for a path that is genuinely outside the root.

STEP RESOLVES THE MAIN WORKTREE, AND EXACTLY ONE ROOT, decided: `rigger step`, `rigger run` and
`rigger workflow` derive the repository from `git rev-parse --git-common-dir` and operate on the
MAIN worktree; invoked from inside a linked worktree they refuse with a message naming the main
tree and the linked one (never a git error about a branch being "already used"), and the
driver's courier command carries the main tree as an absolute path. The step then requires ONE
root: the store it opens must live at `<repo>/.rigger` for that same repo, and the scratch root
it sweeps must be that repo's; when the git toplevel, the store's parent and the scratch root's
parent are not the same directory the step refuses before any sweep or add, naming all three.
Evidence (2026-09-11, u87c3): a spawn's full-suite `cargo test` ran a fixture nested under
`.rigger/tmp/agent-scratch`; the fixture had its own store but no `.git`, so the step it drove
resolved the REAL repo and REAL scratch root with the FIXTURE's unit-less events, and
`sweep_terminal` removed every live unit worktree of the running spec-87 run. The runner-level
`TMPDIR` relocation (spec 90 pattern, landed as operator config the same day) makes such nesting
impossible for test processes; this rule makes the sweep safe even when it happens.

PER-UNIT PIPELINING, decided: the driver treats each wave item as its own pipeline stage: when
any worker records its result, the driver couriers a step immediately (steps stay serialized by
the step lock) and spawns only the NEW items the step parks, keeping an in-flight set so an item
already running is never spawned twice; a unit whose round is reported enters review while its
siblings are still building. The fixpoint rule is unchanged: `done` with nothing in flight.

CONSTRAINTS WALK: a halted spawn whose tree is clean - nothing to commit, the re-park proceeds as
today. A checkpoint commit that fails fmt - the checkpoint is `wip`, gates run on the round
commit only. Relocated scratch on a machine without `XDG_CACHE_HOME` - `~/.cache` is the
fallback, as spec 77 already does for mutants. A courier step started while a sibling's step
holds the lock - the existing "another step is already running" back-off applies. A re-parked
item for a running worker (result not yet recorded when the step ran) - filtered by the
in-flight set; when its result lands the next step parks its successor.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features); no-os-kill and reap audits green.
- No new event type; no new dependency. The driver script change ships via `rigger setup`.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves A HALT NEVER DISCARDS A TREE: a spawn halted with uncommitted worktree
  changes has them committed as a `wip` commit on its branch before the re-park, whose prompt
  names that commit, and the implementer persona text carries the checkpoint rule. This
  criterion OWNS the halt path and the persona rule; scratch placement is criterion 2's, NOT
  this one's.
- [ ] a test proves SCRATCH IS OUTSIDE THE STORE TREE: a spawn's `rigger scratch`, `TMPDIR` and
  `CARGO_TARGET_DIR` resolve under the cache root, never under any `.rigger`, the reaper and
  `validate` account for the new root, and the store-walk unit tests pass under a spawn's
  environment. This criterion OWNS scratch placement and its lifecycle; the reclaim guard is
  criterion 3's, NOT this one's.
- [ ] a test proves THE RECLAIM GUARD COMPARES PATHS: a target under the root is reclaimed
  whether or not it still exists, with no refusal logged, and a target outside the root is still
  refused by name. This criterion OWNS `reap.rs`'s containment check only.
- [ ] a test proves STEP RESOLVES THE MAIN WORKTREE AND ONE ROOT: `rigger step` invoked from a
  linked worktree operates on the main tree or refuses naming both trees, the driver's courier
  command is absolute, and a step whose git toplevel, store parent and scratch-root parent
  differ refuses before any sweep with all three named - a fixture store nested inside a real
  repository can no longer sweep that repository's worktrees. This criterion OWNS repository
  and root resolution in the three commands.
- [ ] a test proves PER-UNIT PIPELINING: with two units in one wave, the unit whose result lands
  first is reviewed while the other still builds, no running item is spawned twice, and the
  run reaches the same fixpoint. This criterion OWNS the driver loop; the conductor's parking is
  unchanged.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
