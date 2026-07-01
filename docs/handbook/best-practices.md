# Best practices: replacing SDLC stages with agents

Everything below was learned running Rigger on itself - including the failures. Stated as practice, roughly in the order you will need them.

## Specs and scope

**1. Machine-verifiable "done" or it does not run.** Every delegated stage terminates in a check a machine can run. If you cannot write the check, you have a design conversation to finish, not a spec to run. The loop's entry gate enforces this; honor it rather than gaming criteria into vagueness.

**2. Write the verification into the criterion.** "Done when X works" invites an agent to declare victory. "Done when X works, *verified by actually running Y and observing Z*" tells the implementer what evidence to produce and the reviewer what to demand. The strongest specs also state the fallback when the ideal is impossible, so a blocked agent ships the documented fallback instead of stalling or silently shipping less.

**3. Scope changes are human decisions - both directions.** Agents never silently add work (scope-creep flags, not quiet units) and never silently drop it (no "follow-up" framing, no TODO-comment deferrals, no stubs presented as done). A discovered need is surfaced; a too-big criterion is escalated. The comfortable middle path - defer it and call it complete - is the one thing the loop is built to make impossible.

## Verification

**4. Gates before reviewers, always.** Machine checks are cheap and impartial; reviewer attention is the scarce resource. Nothing reaches review that has not passed fmt, lint, build, and test. A red gate is a remediation trigger, not a review topic.

**5. Gates mirror CI byte-for-byte.** Same commands, same toolchain versions, run locally in the loop before anything lands. CI's job is confirmation, never discovery. When CI catches something the gates missed, that is a gate defect - fix the gate library, not just the code.

**6. Map every criterion to the check that can actually see it.** Gate suites have blind spots: a lock file hides fresh-resolve skew from every cargo gate; a JS artifact is invisible to `cargo test`; install flows, docs, and live-server behavior all live outside the default suite. For each criterion ask "which check proves this?" - and when the answer is "none of the existing ones", either add a gate or route explicit evidence to the adjudicator. A green gate on a criterion it cannot see is worse than no gate: it manufactures false confidence.

**7. Verify the criterion, not a proxy.** A grep for a path prefix "proves" decoupling that a re-export trivially defeats; a compile pass "proves" a deletion that was never committed; unit tests on hand-built fixtures "prove" interactive flows the production wire-up never triggers. When a check can be satisfied without the property holding, an agent under pressure will eventually satisfy it exactly that way - not from malice, from gradient. Test the real property.

## Review

**8. Make review adversarial by construction, and never soften it to converge.** Parallel friendly lenses converge on the obvious. The three-tier shape - lenses, then an adversary rewarded for proving the lenses wrong, then a neutral adjudicator - exists because it keeps catching real defects that single-pass review waves through. When cycles drag, the fix is better first-pass work or deeper remediation (`max_retries`), never a friendlier adversary. A weakened adversary launders defects as "reviewed".

**9. Every finding is a prevention failure - mine it.** A reviewer catching a defect means the implementer shipped it and every upstream check missed it. Fix the instance, then improve the system: the defect class goes into the reviewers' hunt-lists (detection) and into the implementer's prompt or a gate (prevention). Prevention beats detection - a prevented defect costs no cycle. This is the self-improvement loop; run it every time, and the same failure class stops recurring.

**10. Fix the class, not the citation.** A finding at `file.rs:120` almost always has siblings - the same stale comment in three neighbors, the same missed guard on the other spawn path, the same stub in the parallel module. Remediation that fixes only the cited line converges over many cycles; remediation that sweeps the whole class converges in one. Instruct implementers accordingly.

**11. Never trust an agent's self-report - re-run the checks at integration.** Agents report "verified, gate green, file deleted" and are sometimes wrong. The conductor (or you) re-runs the gates on the integrated state and spot-checks the claims (the deletion is in the commit; the count matches). Trust the evidence, not the summary. Radical transparency is the reciprocal duty on every agent: report exactly what was done, skipped, and still open - concealing a hole from review is the one unforgivable agent behavior, because the defect ships anyway with "reviewed" stamped on it.

## Fleet economics

**12. Tier models by judgment, not by stage name.** Opus where judgment is the product (planning, adversarial review, adjudication, novel design); Sonnet for execution against explicit specs; Haiku for formatting and summaries. The corollary: invest the expensive model in producing a spec so explicit that the cheap model cannot misread it. Intelligence in the plan, economy in the execution.

**13. Aliases, never pinned model IDs.** `model: sonnet`, resolved by the driver at spawn time. Pinning a version into agent files reintroduces rot in the exact place designed to be config. When a new tier ships, update the driver once and audit output quality - do not chase model IDs through your fleet. And when comparing run quality across time, record which resolution actually ran; "sonnet" last month and "sonnet" today may be different models.

**14. Cap everything that can spin.** A non-zero spawn `budget` on every unattended run (zero means unlimited; unlimited means a rejected unit can churn for hours). Bounded `max_retries` with escalation. `recurse: false` on workers - the capability constraint, not a prompt instruction, is what actually prevents a recursive fan-out from eating your quota. Fixed remediation depth, hard abort on breaker trip.

**15. Isolate files, share decisions.** Worktree isolation plus blast-radius partitioning means parallel agents cannot corrupt each other's work; the live event log means they still see each other's reasoning. Both halves matter - isolation without shared memory recreates the blind fleet; shared files without isolation recreates the merge hell.

## Memory

**16. Emit at decision time, not report time.** A decision recorded when made is visible to concurrent peers before they collide with it; a decision batched into the final report arrives after everyone who needed it already guessed. Wire the emission verbs into prompts and treat "worked silently" as a defect.

**17. Lessons are structural memory, not prompt payload.** When something bites, write a `LessonLearned` attached to the files and decisions it concerns. Grounding resurfaces it exactly when the next agent approaches the same ground - which scales; ever-growing prompts do not. Keep prompts about role and method; keep facts in the graph, where supersession keeps them current.

**18. Supersede, never delete.** When a decision is overruled, append the correction; the bi-temporal graph marks the old belief invalid-as-of. History stays queryable, stale beliefs cannot resurface with false confidence, and no agent ever needs "cleanup" authority over the log.

## Humans

**19. Automate stages in order of verifiability.** Verification first (it is just CI, earlier), then well-specified implementation, then review (advisory before gating), then planning, then docs. Design and requirements last, if ever - they are where wrong calls are cheapest to make and most expensive to discover.

**20. Ratchet autonomy; never grant it.** New gates and new workflows start at `manual` or `auto_notify`. Promotion is earned by clean passes; any failure while autonomous demotes back to manual automatically. The asymmetry is deliberate: trust accumulates slowly and evaporates instantly, and a graduated gate must never become a silent hole.

**21. Escalation is a feature - tune its threshold, do not suppress it.** An escalation with the full evidence trail is the loop working as designed. If escalations are too frequent, improve specs and prompts; if too rare, your bounds are too loose and defects are being retried into submission instead of surfaced.

**22. The loop lands on a run branch; humans land on main.** Worktree commits merge into the run branch on green; the run branch reaches `main` through your PR and your review. Outward-facing actions - merges, releases, anything public - stay human. The fleet produces; you ship.

## Starting out

**23. Dogfood on a small, real spec first.** One or two criteria, `auto_notify`, low budget - then *read the run*: every decision emitted, every verdict, what integrated. Calibrate prompts, gates, and tiers against what actually happened before scaling up. The loop is a power tool; learn its kick on a scrap piece.

**24. When the loop fails, fix the loop.** Every operational failure - a driver that stalls, a display that misleads, an install that frictions - is itself a spec waiting to be written. Rigger's own gaps get closed by running Rigger on them. That reflex, applied consistently, is what makes an agentic SDLC compound: the tool improves at the same cadence as the product.
