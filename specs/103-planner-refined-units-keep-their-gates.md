# 103 - The loop's own seams hold: unit gates, worktree heal, checkpoints, post-merge gating

**Goal:** a fan-out unit's gates come from the proposal that created it, not from the
template it replaced. `harvest_proposed` builds the stage a planner proposal supersedes a
baseline with as `gates: u.gates` (`src/conductor.rs:10739`), while the baseline it removes
carried `template.gates` (`src/conductor.rs:12667`) and `PLAN_PROTOCOL`'s proposal shape names
`id`, `agent`, `criterion`, `criterion_id` and `needs` - never `gates`. So a proposal that
omits `gates` yields a unit with none: `verified_evidence` is `{}`, no `GateVerdict` is
recorded, and the unit goes green to verified within a minute of its SDET result. Spec 93's run
recorded 844 gate verdicts because its planner volunteered the list; spec 84's run recorded 38,
every one from the static `checkin` stage or the post-merge re-gate, and u84c1-u84c4 and
u94c5 verified with empty evidence. The same defect family hides in integration: the
already-landed resume path sets `(commit, pre_merge) = (unit_tip, run_tip)`
(`src/conductor.rs:8435`), and the post-merge re-gate addresses the tree of `commit`
(`src/conductor.rs:8671`), so a landing that was a real merge (a686558 = 1293e23 plus the run
branch's newer tree) replayed the unit's own pre-merge green and emitted `UnitIntegrated`
(position 3401888) without a gate run.

Four more seams of the loop's own machinery fail under load. `Worktree::create` heals
"corrupt" worktree admin entries before every add (`src/worktree.rs:2203`), and
`worktree_admin_is_corrupt` (`src/worktree.rs:2231`) flags an entry whose `commondir` or
`gitdir` is missing; git writes an entry as mkdir, `locked`, `gitdir`, `HEAD`, `commondir`, so
a batch-mate's add on a concurrent thread (`run_batch`, `src/conductor.rs:3924`) is deleted
mid-write and fails with `failed to read .git/worktrees/<name>/commondir`. `run_single_stage`
commits whatever the unit worktree holds as `wip(<unit>): tree of halted spawn` on every
window entry (`src/conductor.rs:4637`), even when the named spawn already has a result and
reviewers are live in the shared worktree; it committed a reviewer's temporary red-repro edit
above an approved tip and the unit was rejected and escalated. The post-merge re-gate runs in
the operator's working directory (`src/conductor.rs:8671`), so an untracked operator file
failed a whole-tree test, the landing was reset and charged as `integrate-conflict`, and an
approved unit escalated. And a landing git refuses for local changes records the refusal
text but neither the blocking paths nor where their content came from.

## Design

GATE INHERITANCE, decided: the gates of a fan-out unit are the fan-out template's, always. A
proposal that supersedes a criterion's baseline, refines a unit by id, or adds an unmatched
sub-unit gets the template's gate list; a `gates` field on a proposal is accepted for
compatibility and unioned in, so a proposal can add a gate but never remove one. The list is
re-derived from the pinned definition and the logged proposals on every window, so a resumed
run reaches the same list a live one did.

THE PROTOCOL STAYS SILENT ON GATES, decided: gates are the workflow author's, never the
planner's, so `PLAN_PROTOCOL` keeps its shape and states that every proposed unit runs the
template's gates.

NO UNGATED FAN-OUT UNIT, decided: a fan-out unit whose gate list is empty while its template
declares gates is a conductor invariant violation - the step fails before the unit can spawn,
naming the unit and the template - and `rigger validate` warns on a fan-out template that
declares no gates at all. Unreachable once inheritance holds; kept so the failure is loud if a
future path forgets it.

RE-GATE WHAT LANDED, decided: the integrate-landed record already carries the landed sha; the
already-landed resume path resolves `commit` to that sha, so the post-merge re-gate content-
addresses the landed tree. A landing whose tree equals an already-gated tree replays the cache
hit as today; a landing that was a real merge runs its gates for real; a resumed window whose
re-gate never completed runs it before `UnitIntegrated`.

HEAL NEVER TOUCHES A LIVE ADD, decided: an admin entry carrying git's `locked` file is
never healed - git's own prune honors the same marker - and an entry is healed only when its
directory is older than 60 s. `Worktree::create` also holds a per-repository in-process lock
across the heal and `git worktree add`. The authority is the marker git itself writes into
the entry; the in-process lock is NOT an implementation of this guard on its own, because a
second process adds worktrees in the same repository.

THE HALTED-SPAWN CHECKPOINT, decided: it fires only when the spawn it names has a
`SpawnRequested` and no real `SpawnResult` in the log (a liveness fault is not one), and no
spawn of the unit has a liveness marker fresher than its wall-clock bound. Otherwise the
tree and the branch are left alone.

A REVIEW ROUND LEAVES THE TREE IT REVIEWED, decided: reviewers never write the unit worktree;
the review protocol tells them to reproduce a failure in a scratch worktree. The `reviewed`
status records the sha the round reviewed. When the round's last result is recorded with the
unit worktree dirty or its tip moved, the conductor records a lesson naming the residue,
restores the worktree and the branch to the reviewed sha and charges no attempt; integration
merges the reviewed sha.

POST-MERGE GATES RUN ON THE LANDED TREE, decided: the post-merge re-gate runs in a clean
detached worktree of the landed sha under the scratch root, with the unit's build cache, and
the worktree is reaped when the gates end; the operator's working directory is never the
gated tree. A landing git refuses for local changes records a lesson naming each blocking
path and any unit branch whose tip holds identical content, and charges no remediation
attempt.

CONSTRAINTS WALK: crash-resume - both lists are log-derived (definition pin plus
`UnitProposed`; the landing record's sha), never process state. Empty template gates - a
workflow authored with no gates keeps running ungated, with the validate warning. Repeated
proposals - a same-id refine never changes the gate list. Legacy logged proposals carrying
`gates` - unioned, so a replayed history gates at least as much as it did.
A crashed
`git worktree remove` - its residue is old and unlocked, so it is still healed. A spawn halted
before its checkpoint - it has no result and no fresh marker, so its tree is still captured.
A crash between the post-merge worktree's creation and its reap - the scratch reaper owns
the directory. A reviewed sha that is not an ancestor of the branch tip - restored the same
way; the lesson carries both shas.

## Global constraints

- Hyphens, never em dashes. Both feature lanes green (fmt, clippy -D warnings, test, default
  and --no-default-features) plus the core lane; no-os-kill and reap audits green.
- No new event type; no new crate dependency.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves A REFINED UNIT KEEPS ITS GATES: a proposal that supersedes a baseline, a
  same-id refine, and an unmatched sub-unit each produce a stage whose gates are the fan-out
  template's unioned with any the proposal names, and a resumed window re-derives the same
  list. This criterion OWNS the harvest's gate inheritance and the protocol wording; the
  guard is criterion 2's and the integration re-gate criterion 3's, NOT this one's.
- [ ] a test proves NO UNGATED FAN-OUT UNIT PASSES SILENTLY: a fan-out unit with an empty gate
  list under a template that declares gates fails the step with a message naming both before
  the unit spawns, and `rigger validate` warns on a fan-out template that declares no gates.
  This criterion OWNS the runtime guard and the validate warning; inheritance is criterion
  1's, NOT this one's.
- [ ] a test proves A RESUMED INTEGRATION RE-GATES WHAT IT LANDED: when integration resumes
  after its landing was recorded but before its post-merge gates completed, the re-gate runs
  against the landed commit's tree, so a real-merge landing runs its gates and only a landing
  whose tree was already gated replays a cache hit. This criterion OWNS the resume path's
  commit resolution only; gate inheritance is criterion 1's, NOT this one's.
- [ ] a test proves HEAL NEVER TOUCHES A LIVE ADD: an admin entry that carries `locked`, or is
  younger than the grace period, survives the heal though its `commondir` is missing, an old
  unlocked corrupt entry is healed, and two threads creating worktrees in one repository both
  succeed across 50 rounds. This criterion OWNS the heal predicate and the serialization of
  `Worktree::create`; nothing else in this spec touches `src/worktree.rs`'s heal.
- [ ] a test proves THE CHECKPOINT FIRES ONLY FOR A HALTED SPAWN: a dirty unit worktree is
  committed as the halted-spawn checkpoint only when the named spawn has no real result and
  no spawn of the unit has a fresh liveness marker; otherwise the tree and the branch are left
  untouched. This criterion OWNS the checkpoint's guard only; review residue is criterion
  6's, NOT this one's.
- [ ] a test proves A REVIEW ROUND LEAVES THE TREE IT REVIEWED: when the round's last result
  is recorded with the unit worktree dirty or its tip moved, the conductor records a lesson
  naming the residue, restores the reviewed sha, charges no attempt, and integration merges
  the reviewed sha. This criterion OWNS the reviewed-sha record, the residue handling and the
  reviewer protocol sentence; the checkpoint's guard is criterion 5's, NOT this one's.
- [ ] a test proves POST-MERGE GATES RUN ON THE LANDED TREE: the post-merge re-gate runs in a
  clean worktree of the landed sha, so an untracked or modified file in the operator's working
  directory cannot change a verdict, and the worktree is reaped when the gates end. This
  criterion OWNS where the post-merge gates run; which commit they address is criterion 3's,
  NOT this one's.
- [ ] a test proves A REFUSED LANDING NAMES ITS PATHS: a landing git refuses for local changes
  records a lesson naming each blocking path and any unit branch whose tip holds identical
  content, and charges no remediation attempt. This criterion OWNS the refusal's record only.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
