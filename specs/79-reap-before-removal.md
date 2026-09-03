# 79 - Reap before removal: every dir-removal path reaps what is rooted inside it first

**Goal:** spec 78 made the reaper's SIGNALLING safe (handle-bound or the sanctioned internal
call, an authorization boundary, TOCTOU re-checks) but left reap COVERAGE as it was: several
paths remove a worktree or scratch directory WITHOUT first reaping the processes rooted inside
it, so a build, test binary, or dash whose cwd is inside the removed dir survives the removal
as an orphan holding dead paths (and, for a cargo target dir, holding gigabytes open). The
spec-78 run's adversary proved the class live rather than by inspection. Inventory as found
(re-ground each site before implementing; line numbers drift): `sweep_terminal`
(src/worktree.rs ~864, `git worktree remove --force` with no reap), `clear_worktree_dir` (three
call sites, `fs::remove_dir_all` with no reap), `reclaim_worktree_on_branch` (no reap),
`reclaim_cache_sibling` (src/worktree.rs ~810-818, removes the per-unit `cargo-target-<slug>`
dir on both its normal and review-fence branches with no reap - empirically shown to leave a
live process rooted in the removed tree), and `Worktree::discard` (leaks a review-fence sibling
process; live-confirmed during the spec-78 run). `Worktree::remove` is the exemplar: it already
reaps (git-identity authorized, `reap::reap_authorized`) before removing.

## Design

One rule, one seam: a directory that can have processes rooted inside it is removed ONLY
through a reap-then-remove path. Route every inventoried site through the existing authorities
- `reap::reap_authorized` where the caller owns a git-identity or authorized-root context
(exactly as `Worktree::remove` does), or the `reap_then_remove_dir` /
`reap_then_remove_worktree` helpers (src/main.rs) - never a bare `fs::remove_dir_all` /
`git worktree remove` on a dir that can hold rooted processes. No new signalling sites: spec
78's two sanctioned functions stay the only signal callers, and the `no-os-kill` gate plus
`tests/no_os_kill_audit.rs` continue to enforce that form. A removal path whose dir provably
cannot host a rooted process (created and removed within one function, never handed to a
spawn) may stay bare, but the exemption is claimed in a code comment at the site and the
audit in criterion 3 lists it.

## Notes (non-criteria)

Spec 78's Notes name this spec as the owner of reap-coverage completeness; nothing in spec 78
enumerates sites. The pid-namespace test runner already contains TEST-spawned orphans; this
spec is about the OPERATOR-side runtime paths, which run in no namespace.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green; the `no-os-kill` gate green on every
  unit's diff (no new signalling sites, comments included).
- No new event type. No new dependency.
- The operator's installed `rigger` binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves EVERY INVENTORIED REMOVAL PATH REAPS FIRST: each site in the Goal's
  inventory (re-grounded at implementation time) routes through `reap::reap_authorized` or a
  reap-then-remove helper before its directory is removed, proven for at least
  `reclaim_cache_sibling` and `Worktree::discard` by a periphery test that roots a live
  process in the dir and observes it reaped before the removal - the two sites the spec-78
  run proved leaking live. This criterion OWNS all removal-site rewiring.
- [ ] a test proves NO BARE REMOVAL REMAINS: an audit test walks `src/` for
  `fs::remove_dir_all` and `git worktree remove` call sites on process-hostable dirs and
  fails on any site that neither routes through a reap-then-remove path nor carries the
  claimed-exemption comment it also verifies. This criterion OWNS the audit only; rewiring
  is criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
