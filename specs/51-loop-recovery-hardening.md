# 51 - Loop recovery hardening: reviewer re-park, courier wait, serialized worktree ops

**Goal:** three recovery gaps surfaced by driving the loop through real outages (an exhausted API
quota, long steps, concurrent worktree lifecycles) leave a run needing hand surgery where the loop
should recover itself. Each is a product defect a consumer hits the same way; each fix ships in the
binary or the embedded workflow. After this spec, an externally killed reviewer re-runs instead of
wedging the run, a long step survives being backgrounded by the driving harness, and worktree
create/sweep can no longer corrupt each other.

1. **A reviewer whose result is an ERROR wedges the run.** When a review-stage spawn (lens or
   adversary) dies externally (agent killed mid-run: quota exhaustion, crash) and an error result is
   recorded for it (by the death courier or the operator), the conductor's replay treats the error as
   the REVIEW OUTCOME and fails the whole step with the error text - every subsequent step replays the
   same error and fails identically, so the run is wedged and the operator's only recourse is a fresh
   run (losing in-flight work). An error is not a verdict: review must COMPLETE, so an errored review
   spawn should be re-driven.
2. **The courier cannot survive its step being backgrounded.** The driving harness caps a foreground
   command's runtime; a step that runs longer (a heavy integration wave) is converted to a BACKGROUND
   task, and the courier - forbidden from monitors and unable to wait - returns a placeholder sentinel
   as the step's error, stopping the driver. The re-run-on-timeout rule already covers killed steps;
   the auto-backgrounded step needs its one sanctioned wait.
3. **Worktree create and sweep race and corrupt.** `rigger step` sweeps terminal worktrees and creates
   new ones in the same lifecycle, and a kill mid-either leaves a half-removed worktree admin entry (a
   zero-length `commondir`) that makes EVERY later `git worktree add` fail with
   `failed to read .git/worktrees/<name>/commondir` - observed repeatedly. Worktree mutations need to
   be serialized and self-healing.

## Design

### Reviewer error re-park (`src/conductor.rs`)

When the conductor's replay reaches a REVIEW-stage spawn (lens / adversary / adjudicator) whose
recorded result is an ERROR, it does not adopt the error as the stage outcome and does not fail the
step: it classifies the error as an infrastructure fault on that spawn (no remediation attempt charged
to the unit - the work was never judged), RE-PARKS the same spawn (a fresh attempt of the same review),
and the step returns it in the next wave. The implementer path is unchanged (an implementer error is
already a charged failed attempt feeding remediation). A repeated error on the SAME re-parked review
spawn escalates through the existing bounded-budget path rather than looping forever.

### Courier auto-background wait (`workflows/rigger.js`, embedded source)

The step-courier prompt keeps its foreground-blocking rule and placeholder prohibition, and gains the
ONE exception: if the harness converts the running step to a background task (the tool result names a
background task instead of the command's output), the courier must WAIT for that task's output file to
contain the step's single JSON line and return it verbatim - polling the file is the sanctioned wait
here - or, if the output cannot be obtained, fall back to the existing re-run rule (recorded gate
results make re-runs resume past finished work). Returning a sentinel or placeholder remains forbidden;
the null-step guard in the driver stays as the last line of defense.

### Serialized, self-healing worktree ops (`src/conductor.rs` / the worktree lifecycle)

- All worktree MUTATIONS (add, remove, sweep) in a step happen under the step's existing serialization
  (the step lock already guarantees one step at a time; within a step, sweep completes before any add
  begins - no interleaving).
- Removal goes through `git worktree remove` (falling back to prune) rather than bare directory
  deletion, so git's own bookkeeping stays consistent.
- SELF-HEALING: before adding a worktree, the conductor detects and prunes corrupt admin entries (a
  missing or zero-length `commondir`/`gitdir` under the repository's worktree metadata) so one crashed
  lifecycle can never permanently block every later add. The healing is narrow - only provably-corrupt
  entries are removed; a healthy registered worktree is never touched.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The event log stays the source of truth: the re-park is a replay-classification change plus new
  parked spawns - no recorded event is mutated; a rebuild replays identically.
- Review integrity is not weakened: an errored review NEVER counts as an approval or a rejection; the
  unit's verdict still comes only from a completed review, and the fail-closed integration gate is
  unchanged.
- The fixes SHIP: conductor changes in the binary, the courier rule in the embedded
  `workflows/rigger.js` (a consumer's `rigger setup` receives it); nothing lives only in an installed
  copy.

## Done when

- [ ] a test proves REVIEWER ERROR RE-PARK: with a review-stage spawn whose recorded result is an
  error, the next step does NOT fail - it classifies the error as an infra fault (no attempt charged)
  and re-parks the same review spawn in the returned wave; a completed real verdict on the re-parked
  spawn then flows into the normal adjudication path. This criterion OWNS the re-park.
- [ ] a test proves the ESCALATION BOUND: repeated errors on the same re-parked review spawn escalate
  through the bounded path instead of re-parking forever. This criterion OWNS the bound; it does NOT
  own the re-park (criterion 1).
- [ ] a test proves the COURIER WAIT RULE: the embedded workflow source instructs the courier to wait
  on an auto-backgrounded step's output for the wave JSON (the sanctioned exception), keeps the
  placeholder prohibition, and keeps the foreground rule for the normal case. This criterion OWNS the
  courier amendment (a structural assertion on the embedded source, as spec 44's courier tests do).
- [ ] a test proves WORKTREE SELF-HEALING: with a corrupt worktree admin entry (zero-length
  `commondir`) present, the next worktree add succeeds (the corrupt entry is pruned first), and a
  healthy registered worktree is never pruned by the healing. This criterion OWNS the self-heal.
- [ ] a test proves SWEEP-BEFORE-ADD ORDERING: within one step, no worktree add begins until the
  terminal-worktree sweep has completed (the mutations are serialized), pinned at the lifecycle seam.
  This criterion OWNS the ordering; it does NOT own the self-heal (criterion 4).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
