# 64 - Worktree lifecycle is phase-aware: a parked stage keeps its worktree

**Goal:** stop the loop deleting the unit worktree out from under its own out-of-process
agents. The unit-stage driver creates the worktree, runs the stage, and removes the worktree
UNCONDITIONALLY when the stage call returns (`src/conductor.rs` around line 3010, `w.remove()`
after `run_single_stage`). That teardown is correct for the BLOCKING drivers, whose agents run
in-process while the worktree exists, and wrong for the parked step driver, where
`run_single_stage` returns every time it PARKS a spawn for the courier: the worktree is removed
at return, the parked agent then runs BETWEEN conductor processes and finds its assigned
worktree absent (dir gone, `.git/worktrees` admin entry gone, durable branch intact), and the
next step recreates it just in time to remove it again. Run 40064f99 recorded five-plus
restorations per unit, performed by the review agents themselves as unprompted forensic
self-repair; the consumer project reports the same failure constantly. The same premature
remove also reclaims the `cargo-target-<slug>` sibling build cache every inter-step gap, so
every attempt cold-rebuilds - a large share of per-invocation cost - and it is what pushed
agents into inventing private CARGO_TARGET_DIR workarounds against the shared-target clobber.

## Design

- **Phase-aware teardown** (`src/conductor.rs`, the single-stage driver): the stage's return
  is split into TERMINAL (the stage completed in this process: integrated, failed, escalated,
  or otherwise done) and PARKED (the stage handed one or more spawns to the courier and will
  resume in a later process). A TERMINAL return removes the worktree and reclaims its cache
  sibling exactly as today. A PARKED return keeps BOTH the worktree and the cache on disk, so
  the out-of-process agents run in the tree they were assigned and the next attempt reuses a
  warm build cache. The blocking drivers' behavior is unchanged by construction - their
  returns are always terminal.
- **Ensure-on-park** (defense in depth, same driver): at the moment a spawn is parked with an
  assigned worktree, the conductor guarantees the worktree exists on the unit branch at the
  tip it is handing out (the existing deterministic adopt-or-create machinery,
  `worktree_on_branch` / `Worktree::create`), so even an out-of-band deletion between park and
  spawn self-heals in the binary rather than in an agent's opening minutes. Agents stop
  needing their improvised `git worktree add` recovery; the persona guidance that grew around
  the failure can retire once this lands.
- **Crash reclamation learns liveness** (`src/worktree.rs::sweep_terminal` + its caller): the
  step-start sweep remains the reaper for worktrees a crashed process leaked, but its
  merged-only ancestry rule is NOT sufficient alone: a parked unit whose attempt produced an
  EMPTY diff has a branch tip that IS an ancestor of the run branch while the unit is live in
  review (a recorded, recurring case - review-only and docs-scoped units), and the ancestry
  rule would sweep its worktree mid-review. The sweep therefore skips any worktree whose unit
  is LIVE in the current run - liveness derived from the CURRENT run's slice of the event log
  (the same run-scoped fold the conductor already reads), never from a process-memory list -
  and reclaims only worktrees that are both merged-or-dead AND not live. The premature remover
  was the stage driver; the sweep's new liveness conjunct closes the empty-diff corner the
  ancestry test cannot see.
- **Lifecycle invariant restated** (comments at the remove site and on `sweep_terminal`): the
  conductor owns the worktree's whole lifecycle, where the lifetime is the UNIT'S LIFECYCLE
  (create at first need, remove at terminal), not the conductor process's. The durable-branch
  checkpoint semantics are untouched.

## Notes (non-criteria)

- The per-unit build cache surviving across attempts is a deliberate consequence, not a leak:
  it is bounded by the unit's lifetime and reclaimed at terminal teardown and by the sweep's
  crash path, exactly as before.
- Review-only worktrees (`rigger-review-*`) keep their throwaway create/discard lifecycle;
  they are stage-scoped by design and their agents run while the conductor is live... unless
  the parked path also parks standalone-review spawns, in which case the same
  terminal-vs-parked split applies to them identically.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The durable unit branch remains the resume checkpoint in every path: no change may delete
  or rewrite a unit branch that today survives.
- Blocking-driver behavior is bit-for-bit today's: create at stage entry, remove at stage
  return.

## Done when

- [ ] a test proves a PARKED STAGE KEEPS ITS WORKTREE: in the parked driver, a stage that
  parks a spawn returns with the unit worktree still on disk, registered, and checked out at
  the handed-out tip, and the `cargo-target-<slug>` sibling still present. This criterion OWNS
  the terminal-vs-parked split.
- [ ] a test proves TERMINAL TEARDOWN IS UNCHANGED: a stage that completes (integrates or goes
  terminal) in-process removes the worktree, reclaims the cache sibling, and leaves the
  durable branch - identical to today's behavior, in both drivers.
- [ ] a test proves ENSURE-ON-PARK: with the assigned worktree deleted out-of-band after a
  park, the conductor's next hand-off (or the park itself, whichever the seam makes honest)
  restores it on the unit branch at the recorded tip before the agent consumes it.
- [ ] a test proves the SWEEP STILL RECLAIMS CRASH RESIDUE WITHOUT EATING LIVE UNITS: a
  merged worktree whose unit is NOT live is swept at step start; an unmerged parked unit's is
  not; and a LIVE parked unit whose branch tip equals the run tip (the empty-diff case) is
  NOT swept despite passing the ancestry test - liveness read from the current run's log
  slice.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
