# 33 - Wire the SDET-author spawn into the build lifecycle (code only)

**Goal:** the CODE half of the SDET periphery-testing feature (spec 32): wire the conductor to
spawn the operator-provided `sdet-author` agent at the build seam, so on every unit the SDET
authors its periphery tests into the same committed tree the gates and reviewers judge. This
spec touches ONLY code (`src/conductor.rs`, `src/spawn.rs`); the personas (`sdet-author.md`,
the `adversary.md` surface-completeness hunt) are operator-authored config, deliberately NOT
built here - a run pins its definition, so a spec that edited the personas would drift its own
definition and (for the adversary) self-apply its new review rule to its own units. Splitting
config from code is the self-hosting-safe way to build this: the loop builds code under the
current personas; the operator authors the personas.

## Design

The unit lifecycle is hardcoded in `RunCtx::run_single_stage` (`src/conductor.rs`): implementer
spawn -> commit worktree -> `run_gates` -> `review_unit` -> integrate, one loop that re-enters
on remediation. The implementer spawn is parked at `~L3358-3401`; it emits green around
`~L3419-3427` and the pre-gate commit is around `~L3479-3481`. Insert the sdet-author spawn
BETWEEN the green emit and the commit, in the SAME worktree (`req.dir`), so its authored
periphery tests are committed with the unit and seen by the gates.

- **Role token.** Add `ROLE_SDET_AUTHOR` (or the `lens_role`-style equivalent) in `src/spawn.rs`
  alongside `ROLE_IMPLEMENTER`, for the deterministic spawn id.
- **Spawn call.** In `run_single_stage`, after the implementer's green status and before the
  commit, spawn the agent whose id is `sdet-author` (resolved from `.rigger/agents/` like every
  other role, via the existing `AgentDef` / `build_system_prompt` path) into the implementer's
  worktree. The agent id is a fixed convention; no `workflow.yml` change (which would drift the
  pinned definition).
- **Result handling.** The conductor AWAITS the sdet-author's self-reported result, then proceeds
  to the commit + gates. A normal result (it authored periphery tests, or recorded a provably-empty
  accounting for a purely-internal unit) advances the lifecycle. Its authored files are already in
  the worktree, so the existing pre-gate commit sweeps them in and the unscoped `cargo test` gate
  runs them - a periphery test that fails reddens the gates and drives the EXISTING remediation
  loop (the implementer fixes the code), no new remediation path.
- **Absent-agent tolerance.** If no `sdet-author` agent is configured, the step is a clean no-op
  (the build proceeds exactly as today) - an operator who has not installed the persona is never
  blocked.
- **Both code-building lifecycles.** The conductor builds a unit through ONE of two lifecycles:
  the single-lane `run_single_stage` above, and first-green-wins `run_speculation`
  (`src/conductor.rs`), which a unit class enters when its effective `speculation_width` is `> 1`.
  Speculation races K parallel implementer CANDIDATES in K isolated worktrees, commits each
  candidate, then in a second phase gates + reviews the candidates in lane order and integrates the
  FIRST gate-green adjudicator-approved one (the winner), cancelling the rest. Because the GOAL is
  unqualified - on EVERY unit the SDET authors its periphery tests into the tree the gates judge -
  the seam must be wired into BOTH lifecycles, not only the single lane; otherwise every
  speculation-built unit would ship its boundary surface untested. So the spawn placement is
  factored into ONE shared seam authority that BOTH lifecycles call (never a second parallel
  construction): the single lane calls it before its pre-gate commit, and speculation calls it
  PER CANDIDATE, after that candidate's implementer produced its diff and BEFORE that candidate's
  commit - so whichever candidate wins integrates with its periphery tests in the committed tree
  its gates judged. It composes with speculation's discipline: each lane reserves its OWN sdet
  spawn id (keyed on the lane), so the budget breaker counts every candidate's sdet exactly as it
  counts every candidate's implementer; the sdet emits no routing status, so a candidate stays
  DEFERRED and the unit stays `Fresh` for a deterministic resume; and a PARKED sdet is collected
  into the group's park (that candidate is not committed) so all candidates park together and a
  later step replays the sdet and commits the candidate WITH its periphery.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: the spawn id is deterministic (`spawn_id(stage, ROLE_SDET_AUTHOR,
  attempt)`); no wall-clock or ordering nondeterminism is introduced.
- No new event type; no change to `.rigger/agents/` or `workflow.yml` (code only - the personas are
  operator config, kept OUT of this spec so the run never drifts its own pinned definition).

## Done when

- [ ] a test proves the conductor SPAWNS the `sdet-author` agent at the build seam - after the
  implementer emits green and before the pre-gate commit, in the implementer's worktree - using the
  fixed agent id resolved through the existing agent-loading path, so its authored files land in the
  committed tree. Because the goal is unqualified (on EVERY unit), the placement covers BOTH
  code-building lifecycles through the one shared seam authority: a `speculation_width > 1`
  regression test proves the sdet-author is spawned per candidate and its periphery file is ADDED in
  the SAME committed tree the candidate's gates judge (so dropping the speculation wiring reddens
  it), exactly as the single-lane test proves it for the single-lane commit. This criterion OWNS the
  spawn placement (in both lifecycles) and its role token.
- [ ] a test proves the lifecycle CONTINUES on the sdet-author's result: after it self-reports, the
  conductor proceeds to the pre-gate commit and gates (a normal result and an empty-accounting no-op
  both advance); and if no `sdet-author` agent is configured the step is a clean no-op that never
  blocks the build. This criterion OWNS the result handling and absent-agent tolerance; it does NOT
  own the spawn placement (the criterion above).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
