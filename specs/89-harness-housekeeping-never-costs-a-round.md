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

A RUNNING TOOL IS LIVENESS, decided: the driver refreshes a worker's activity marker for as
long as one of the worker's tool calls is still executing (the harness knows a Bash call is in
flight), so a worker blocked in one legitimately long command is never classified hung; only a
worker with no tool in flight and a stale marker is. Evidence (2026-09-12, u88c1 implementer#2):
a 59-mutant `cargo mutants` call outlasted `max_wall_clock`, the sweep aborted the worker and
re-ran the same spawn, and the re-run restarted the same sweep - five incarnations in one night
with zero progress, a livelock the operator broke by hand (shard the sweep, report between
shards). The outer wall-clock still bounds the whole spawn; it no longer bounds one command. When the
driver does abort a worker, the worker's own process tree ends with it: the harness abort
reaches the agent, not the `cargo mutants` (or `cargo test`) it started, which today survives
as an orphan burning cores against its dead owner (2026-09-12: a sweep from an aborted u88c1
incarnation was still running two rounds later, reported by a sibling's SDET author as
"apparently-orphaned implementer scratch process"). The driver records every worker's spawned
process group in its marker and, on abort, hands that group to rigger's handle-bound lifecycle
(spec 78) so it is ended by its owner, never by an OS-level kill from a script.

CHECKPOINT BEFORE LONG WORK, decided: the implementer persona commits a checkpoint
(`wip(<unit>): checkpoint before <mutation sweep | lane suite>`) before any full lane suite and,
for the check-in stage spec 91 introduced, before its mutation sweep - the persona text names
the sweep by that phrase, never by the tool's command, because spec 91's persona guard forbids
that literal in any persona file - and squashes it into its round commit when the round is
reported. When a
spawn is halted by the liveness sweep with UNCOMMITTED changes in its worktree, the conductor
commits them as `wip(<unit>): tree of halted spawn <id>` before re-parking - the re-park prompt
names that commit and says "finish and report; do not start over" - so a halt never discards a
tree. The halt still charges no attempt (it is an infrastructure fault today and stays one).
A CHECKPOINT NEVER COMMITS A HALF-MERGE, decided: every conductor commit into a unit worktree
(the halt `wip`, the per-attempt checkpoint, the integration merge) first checks the worktree
for an in-progress merge or cherry-pick (`MERGE_HEAD`, `CHERRY_PICK_HEAD`, unmerged index
entries) and for conflict markers in tracked files; finding either it commits nothing, charges
nothing and fails loud as an infrastructure fault naming the state and the worktree, and the run
branch is never advanced to a commit whose tree carries conflict markers. On 2026-09-12 the
attempt checkpoint ran `git add -A && git commit` over a merge someone had left in progress in
the unit worktree, staged the marker-laden files as resolved, produced a merge commit with 2720
conflict markers and integrated it as the unit's approved final round; the run branch had to be
moved by hand.

THE FAN-OUT IS A DEFINITION KNOB, decided: the number of units a run builds at once is
`defaults.max_parallel_units` in workflow.yml (default 2), replacing the conductor constant
`MAX_CONCURRENCY = 4` (src/conductor.rs:35); a review round costs one SDET author, two lenses,
an adversary and an adjudicator, each an independent cold build in its own target dir, so four
parallel units put ~90 cargo/rustc processes on a 32-core workstation at once (load average
125, 2026-09-11) and ~200G of embedded worktree targets on disk. The operator sizes the run to
the machine in the definition, never in code.

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
appears only for a path that is genuinely outside the root. The same lexical rule governs the
process side: a process whose `/proc/<pid>/cwd` resolves to a DELETED path is matched by the
path text (the ` (deleted)` suffix stripped), so a runaway whose scratch was removed under it is
still "rooted under" the scratch root and is reaped. Evidence: a spec-80 mutant test binary
(u80c1, 2026-09-03) hung in a busy loop inside its pid namespace after `cargo-mutants` removed
its tree; the cwd-rooted reaper never matched the deleted path, and it ran for eight days at
roughly seventeen cores before the operator killed it by hand.

AN AGENT NEVER MUTATES OUTSIDE ITS WORKTREE, decided: `rigger setup` installs a PreToolUse
hook (beside the kill hook) that refuses a `git commit`, `git add`, `git reset`, `git merge` or
`git cherry-pick` whose repository, resolved from the command's effective directory, is not
the spawn's assigned worktree - the spawn env carries the assigned dir - with a message naming
both. Evidence (2026-09-11): a unit-4 implementer's `cd` chain failed silently in a scratch
git experiment, its shell fell back to the main checkout, and `git add -A && git commit`
created a real commit on `rigger-run` carrying two gigabytes of untracked store backups; the
operator reset it before any sibling merged it. The persona's "every command starts with
`cd <worktree> &&`" rule is text; this is the tool-path guard that makes the text unnecessary.

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
  names that commit, and the implementer persona text carries the checkpoint rule; and every
  conductor commit into a unit worktree (halt `wip`, attempt checkpoint, integration) refuses
  a worktree with an in-progress merge or cherry-pick or with conflict markers in tracked
  files - commits nothing, charges nothing, fails loud naming the state - and the run branch
  never advances to a tree carrying conflict markers. This criterion OWNS the halt path, the
  checkpoint guard and the persona rule; scratch placement is criterion 2's, NOT this one's.
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
