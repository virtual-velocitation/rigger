# Spec 77: every byte rigger writes has a lifecycle

## Problem

Rigger's disk footprint reached hundreds of gigabytes: mutation-testing tree copies leak
into the user cache with no owner (47G observed - killed runs never clean up, and the
reclamation authority only manages `.rigger/tmp`), per-unit cargo target caches (~19G
each) persist past their unit, and the shared gate build cache (39G observed) grows
without bound or even a visible total. Cleanup today is operator hygiene, which is not a
mechanism.

## Design

- ONE BUILD LOCATION PER UNIT, the biggest leak's root cause: agents running cargo
  inside their worktree without the shared cache env embed a full ~19G target tree in
  EVERY worktree (~35G worktrees observed, five at once). The conductor exports
  `CARGO_TARGET_DIR=<the unit's cache>` in every spawned agent's environment, so a
  worktree stays source-sized, all of a unit's builds land in the one per-unit cache,
  and teardown reaps one place.
- ONE REAPER, extended not duplicated: the spec-34 per-spawn reclamation authority
  (`spawn_scratch_path` / `cmd_result`'s reclaim) gains registered SCRATCH ROOTS beyond
  `.rigger/tmp` - first registrant the mutation scratch root
  `${XDG_CACHE_HOME:-$HOME/.cache}/rigger-mutants/<spawn>` - keyed by the FULL injectively-encoded spawn id, never the bare unit, because speculation lanes run concurrent spawns of one unit and a shared dir lets one lane's reap kill its sibling's live mutation run - so a spawn's mutation copy is
  deleted the moment its result records, exactly like its agent scratch. The seeded
  persona invocation moves to its own spawn-scoped subdir and pre-deletes it before running
  (bounding a killed run's leak to one tree, reclaimed on retry or result).
- UNIT-TERMINAL REAP: when a unit reaches a terminal state (integrated or abandoned at a
  fresh run boundary), its per-unit cargo target cache and any registered scratch of its
  spawns are deleted by the same teardown that already removes its worktree. A LIVE
  unit's assets are never touched - the existing sweep-liveness guard is the authority.
- BOUNDED SHARED CACHE: `rigger reset` gains `--build-cache`, deleting the shared gate
  build cache (`.rigger/tmp/cargo-target`) - a pure cache, always safe to cold-rebuild -
  and reporting bytes reclaimed like the other reset modes. EXCLUSION, decided here
  (three rounds proved any in-cache lock cannot close the race - flock is advisory to
  lock-takers and never gates unlink, so a queued build resumes into a deleted dir): a
  reader-writer flock on a guard file BESIDE the cache, never inside it. Every
  rigger-launched shared-cache build holds it SHARED for the whole cargo invocation;
  reset takes it EXCLUSIVE and NON-BLOCKING, refusing loudly with a retry-when-idle
  message when contended - never waiting, so no build can queue behind the delete.
  While holding it, reset renames the cache to a tombstone (atomic) and deletes the
  tombstone after release. Builds rigger does not launch are outside the guarantee, by
  scope.
- INJECTIVE SCRATCH NAMING, decided here (closing the sanitization-alias class three
  rounds patched one placeholder at a time: "", ".", ".." each aliased onto a nameable
  real unit, letting a malformed spawn id delete another unit's live scratch): the
  id-to-directory-name map used by BOTH creation and reaping is one shared INJECTIVE
  encoding - every byte outside `[A-Za-z0-9-]` becomes `_` plus two lowercase hex
  digits, and `_` itself is so escaped - so distinct ids can never share a directory
  name and no placeholder drawn from the output alphabet exists at all. The empty id
  encodes to the empty string, which names nothing: both sides refuse it (create
  nothing, reap nothing) - deletion of a path a degenerate input cannot name is
  fail-safe by construction, not by guard.
- FOOTPRINT ACCOUNTING: `rigger validate` reports rigger's total on-disk footprint by
  category (store, backups, shared build cache, per-unit caches, worktrees, registered
  scratch roots) and flags any category whose dead share exceeds an advisory threshold,
  naming the reclaiming command. Advisory tone; never a hard failure.

## Done when

- [ ] A test proves ONE BUILD LOCATION: every spawned agent's environment carries
  `CARGO_TARGET_DIR` naming its unit's cache, pinned at the spawn seam, and a worktree
  whose agent ran a real cargo build holds no embedded `target/` dir - pinned at the
  gate/driver seam with a real subprocess. This criterion OWNS the spawn-environment
  export.
- [ ] A test proves MUTATION SCRATCH IS REAPED: a spawn with a populated registered
  mutation scratch dir has it deleted the moment its result is recorded, for every
  outcome, while a sibling spawn with no recorded result keeps its dir - pinned at the
  same seam as the existing per-spawn reclamation tests. This criterion OWNS scratch-root
  registration and the persona invocation's spawn-scoped subdir text.
- [ ] A test proves UNIT-TERMINAL REAP: a terminal unit's per-unit cargo target cache is
  deleted by teardown while a live sibling's survives, gated on the existing
  sweep-liveness authority. This criterion OWNS the teardown extension; scratch-root
  registration is criterion 1's, NOT this one's.
- [ ] A test proves the BOUNDED SHARED CACHE: `rigger reset --build-cache` deletes the
  shared gate build cache, reports bytes reclaimed, composes with the existing reset
  modes, and appears in the usage registry. This criterion OWNS the reset mode.
- [ ] A test proves FOOTPRINT ACCOUNTING: `rigger validate` on a fixture tree with seeded
  category sizes reports each category's total and flags a dead-share threshold breach
  naming the reclaiming command, exit 0. This criterion OWNS the accounting surface.
- [ ] Both feature lanes green: `cargo fmt --check`; `cargo clippy --all-targets -D
  warnings`; `cargo test` on default features AND `--no-default-features`. This criterion
  OWNS the whole-diff gates-green audit and claims no lifecycle concept of its own.

## Global constraints

- Hyphens, not em dashes, anywhere the diff touches.
- No new event type; reap facts ride the existing decision/result surfaces.
- Fail-safe deletion only: a reaper deletes exactly what a registered root or teardown
  names, never walks upward, and skips anything a liveness guard claims.
- The store and its backups are NEVER auto-deleted; accounting reports them, only the
  operator removes them.

## Notes

- Constraints walk: crash between result and reap -> the next unit-terminal or fresh-run
  teardown covers the residue (registered roots are enumerable); concurrent units and speculation lanes ->
  spawn-scoped injective paths never collide; cold start -> registration is code, not state;
  repeated reset --build-cache -> idempotent zero-report; REVERT/re-run of a reaped unit
  -> caches are pure, cold rebuild is the cost.
- Persona edit lands with this spec's unit (definition-hash change accepted at this
  run boundary, not mid-run).
