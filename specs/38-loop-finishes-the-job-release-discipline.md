# 38 - The loop finishes the job: branch-GC after integrate + a PR-ready run branch

**Goal:** correct the release-boundary drift. The agentic-SDLC handbook is explicit that "the loop
lands commits on a run branch; merging that branch to main goes through your PR review" - so DONE is
a merged PR, and the loop's job is to leave the run branch clean and ready to open one. Today it does
neither cleanly: after a unit integrates, its per-unit `rigger/u/<unit>` branch is LEFT BEHIND (15
accumulated), never garbage-collected; and nothing surfaces that a completed run is ready to release,
so work silently piled up (332 commits on a run branch that had even drifted into a history disjoint
from `main`, which GitHub refuses to PR). This spec makes the loop finish the job: delete a unit's
per-unit branch once it is integrated, base the run branch on the release target so its diff is
exactly the run's work and a PR always applies, and on run completion surface a clear "ready to open
a PR to <base>" handoff. It does NOT auto-merge to `main` - release stays the human's call (handbook:
humans own release).

## Design

The conductor creates a run branch, and per unit an isolated worktree on a `rigger/u/<unit>` branch;
`on_pass: merge` integrates an approved+green unit onto the run branch (`src/conductor.rs`
integration path; `src/worktree.rs` for worktree/branch creation and teardown). Three additions,
all in the conductor's lifecycle - no new event type beyond what integration already emits:

- **Branch-GC on integrate.** When a unit reaches `integrated`, delete its `rigger/u/<unit>` branch
  (and remove its worktree if the reaper has not) as part of the same integration step, so a
  completed run leaves no per-unit debris. A unit that ESCALATES keeps its branch (the human needs
  it); only an integrated unit's branch is reclaimed. Idempotent on resume-by-replay: re-folding an
  integration whose branch is already gone is a no-op.
- **Run branch based on the release target.** The run branch is created from the configured base
  (the branch a run integrates toward, default `main`), so `base..run-branch` is exactly the run's
  work and a PR always applies - never a disjoint history. Deriving the base is config with a
  sensible default; a run started with no reachable base fails the loop-readiness gate loudly rather
  than branching from nowhere.
- **Ready-to-release handoff.** On `done()` (every criterion covered, every unit integrated, every
  gate green), the conductor surfaces a RELEASE-READY summary - the run branch, the base, the
  integrated unit count, and the exact `gh pr create ... --base <base> --head <run-branch>` (or the
  configured release command) - via the same status/observability surface the dash and `rigger
  status` already read. This is a surfaced handoff, not an action: the loop stops at "ready to open a
  PR", the human opens/merges it.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Event-sourced + resume-safe: branch-GC and the release-ready summary derive from the existing
  integration / done state (a projection over the log), so a resume-by-replay re-reaches the same
  end state; deleting an already-deleted branch and re-surfacing an already-ready run are no-ops.
- Escalation is never reclaimed: an escalated or un-integrated unit keeps its branch and worktree so
  the human has the evidence (handbook: escalations land on the human's desk with evidence attached).
- No auto-merge to the release target: the loop surfaces the PR handoff; it never merges to `main`.

## Done when

- [ ] a test proves BRANCH-GC on integrate: after a unit reaches `integrated`, its `rigger/u/<unit>`
  branch no longer exists, while an ESCALATED unit's branch is retained; re-folding the integration
  (resume-by-replay) with the branch already gone is a no-op, not an error. This criterion OWNS
  per-unit branch reclamation.
- [ ] a test proves the RUN BRANCH is based on the release target: the run branch is created from the
  configured base so `base..run-branch` contains exactly the run's integrated commits (a clean,
  applicable PR diff), and a run with no reachable base fails loop-readiness loudly. This criterion
  OWNS run-branch basing; it does NOT own branch-GC (criterion 1).
- [ ] a test proves the RELEASE-READY handoff: on `done()`, the conductor surfaces a summary naming
  the run branch, the base, the integrated-unit count, and the PR command, on the existing status
  surface; a run that is NOT done surfaces no release-ready signal. This criterion OWNS the
  ready-to-release handoff; it does NOT own basing or GC (criteria 1-2). It never merges to the base.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
