# Design-intent gaps

Status: open work list, assessed 2026-07-01 against [architecture.md](architecture.md) after the three-gap dogfood run (PR #7: 3 units, 3/3 integrated first-attempt, 0 escalations).

This document records where the implementation currently falls short of the design intent, with the evidence that surfaced each gap and the shape of the fix. It is the feed for the next loop runs: each gap is written so it can be lifted into a spec's "Done when" criteria with little editing. Remove entries as they close.

## How these were found

Dogfooding. Rigger ran on its own spec; the run's telemetry (`rigger stats`, `rigger peers`), the `/workflows` display, and independent verification of the run's output are the evidence base. The through-line: the memory layer and the review economics are delivering as designed (78.6% first-pass yield, 0% escalations, decisions demonstrably inherited across agents); the gaps concentrate in the **native workflow driver**, which implements the loop's shape but not all of the conductor's safeguards.

---

## Gap 1: the native workflow driver runs units sequentially; the design says fan-out

**Intent.** Independent stages run concurrently in isolated worktrees (`architecture.md` at-a-glance; `.rigger/workflow.yml`: `strategy: fan-out`, `partition: by-blast-radius`). Worktree isolation exists precisely to make parallel units safe.

**Reality.** `workflows/rigger.js` iterates `for (const unit of plan.units)` - each unit fully integrates before the next starts. Three independent units took 68 minutes wall-clock, strictly serial.

**Evidence.** The PR #7 run: `u1-install-nolock`, `u2-workflow-perunit-phase`, `u3-contract-broaden` had disjoint file footprints and no dependency edges, yet ran end-to-end sequentially.

**Fix shape.** Partition the planner's unit DAG by blast radius; run disjoint waves concurrently (each unit already gets its own worktree and branch); serialize only the integrate step per wave. The per-unit phase labels landed by u2 already make the display correct for overlapping units.

## Gap 2: the primary driver lacks the budget breaker and the autonomy ratchet

**Intent.** A spawn-cap circuit breaker bounds every unattended run (`BudgetExhausted` aborts; `workflow.yml` warns "never set it back to 0"); gates carry per-gate autonomy that ratchets up on clean passes and demotes on failure.

**Reality.** The JS driver has only `maxRetries`. The breaker and the ratchet live in the Rust conductor - which is now the *fallback* driver. The runaway-loop protection is strongest exactly where it is least used. The five-hour churn incident that motivated the breaker happened on a driver without one; the primary driver today is again a driver without one.

**Fix shape.** Either port the breaker (count spawns, abort at the cap) and honor `defaults.budget` from `workflow.yml` in the JS driver - or close Gap 7 below, which makes this moot.

## Gap 3: the driver under-emits the event vocabulary, blinding the metrics

**Intent.** "Every meaningful thing an agent does - a decision, a file touched, a gate passed or failed - gets written to the event log." The metrics projection (`rigger stats`) folds first-pass yield, per-gate remediation, and review verdicts from those events.

**Reality.** `rigger stats` after 14 units: "gates - (no gate runs recorded)"; review shows 1 approved / 4 rejected against 11+ units that visibly passed review. The JS driver runs gates and reviews but does not emit `GateVerdict` / consistent review-status events, so the projection cannot see them.

**Fix shape.** The gate agent and adjudicator prompts (or the driver code around them) emit the existing vocabulary - no new event types - after each gate run and each verdict. The metrics module already knows how to fold them (see the `d-metrics-projection` decision in the log).

## Gap 4: constraints outside the gate suite slip through the three-tier review

**Intent.** Spec global constraints bind every unit; the adjudicator is the strict last line for anything the gates cannot see.

**Reality.** u1 shipped em dashes in comments despite the spec's explicit "hyphens not em dashes" global constraint. The implementer violated it and all three review tiers missed it; it was caught only by post-run human inspection (fixed in a follow-up commit on the same PR).

**Fix shape.** Two layers, both cheap: (a) mechanically checkable style constraints become a gate (a grep for the em-dash character over the diff suffices for this one); (b) the adjudicator prompt gains an explicit step - "re-read the spec's Global constraints section and verify each against the diff" - so non-mechanical constraints get a named check instead of relying on ambient attention.

## Gap 5: the installed workflow copy drifts from the source

**Intent.** `rigger setup` installs `.claude/workflows/rigger.js` so `/rigger` is immediately runnable; the repo's `workflows/rigger.js` is the source of truth (embedded into the binary and asserted by test).

**Reality.** After u2 changed `workflows/rigger.js`, the installed `.claude/workflows/rigger.js` remained the old version - and stays stale until someone remembers to re-run `rigger setup`. A `/rigger` invocation in the window between merge and re-setup runs the old driver silently.

**Fix shape.** `rigger setup` becomes safely re-runnable and drift-aware (compare installed vs embedded; refresh on mismatch), and something ambient surfaces the drift - the simplest candidate: `rigger validate` (already the config checker) warns when the installed copy differs from the embedded one.

## Gap 6: the log does not record which model actually ran

**Intent.** Model tiers are aliases by design (`model: sonnet`, never a pinned ID) so the fleet upgrades when the driver does. Correct - and it means the *log* is the only place the resolved model could be recorded, and today it is not recorded anywhere.

**Reality.** When the harness's `sonnet` alias moves (4.6 to 5), cross-run quality comparisons - did first-pass yield change with the model? - are unanswerable from the event log. The run that produced PR #7 ran on Sonnet 4.6 / Opus 4.8; that fact lives in this sentence, not in the store.

**Fix shape.** The driver stamps the resolved model (and the alias it resolved from) into the events it already emits per agent spawn - metadata on `UnitStarted` / gate / verdict events, no new event type.

## Gap 7 (structural, subsumes 1-3): the primary and fallback drivers have inverted

**Observation.** Gaps 1-3 share one root. The Rust conductor holds the real machinery - ledger, breaker, ratchet, remediation policy, full event emission - and the JS workflow driver reimplements a thin subset of it. But the JS driver is the *primary* interface (`/rigger`), so the best-engineered path is the least-traveled one, and every conductor improvement must now be manually mirrored into JS or silently diverge.

**Fix shape.** Make the JS driver a thin client of the conductor instead of a reimplementation: it connects to `rigger serve` (the MCP bridge that already exists), pulls assignments via `rigger_next`, reports via `rigger_result`, and keeps only the Claude-Code-native concerns (spawning agents, progress display). One loop implementation, two faces. This is the recommended next loop run; closing it closes Gaps 1-3 in the same stroke.

## Gap 8: agent-config improvements are stranded uncommitted in the working tree

**Intent.** Agent definitions are config, versioned like everything else; improvements to them land through review like everything else.

**Reality.** Two deliberate, design-aligned improvements sit as uncommitted modifications: `.rigger/agents/rust-engineer.md` promotes `model: sonnet` to `model: opus` (novel implementation belongs on the judgment tier), and `.rigger/agents/sdet.md` narrows `tools:` from write-capable to read-only `[Read, Grep, Glob, Bash]` (reviewers must not be able to edit their way past a finding). Made during the 2026-07-01 session, never committed - so the running fleet and the versioned fleet disagree, and a fresh clone gets the weaker config.

**Fix shape.** Commit both via a small PR. Then close the class: the loop's setup/validate path should flag tracked `.rigger/` files with uncommitted modifications at run start, so config drift between "what runs" and "what is versioned" is surfaced, not discovered by accident.

## Gap 9: `rigger setup` artifacts permanently dirty `git status`

**Intent.** `rigger setup` makes a repo loop-ready in one command; re-running it is a no-op on an already-configured repo.

**Reality.** Setup writes files git then reports as noise forever: scaffolded default agents (`implementer.md`, `devils-advocate.md`, `reviewer.architecture.md`, `reviewer.technical.md`) land untracked next to the repo's committed, customized agents - generic duplicates of specialized ones; `.claude/` (the installed workflow + a SessionStart hook in `settings.json`) and `.rigger/shim/` (including `node_modules/`) are neither tracked nor gitignored. Every setup leaves a permanently dirty status, which trains people to ignore `git status` - the opposite of what a gate-driven loop wants.

**Fix shape.** Three parts: (a) setup does not scaffold a default agent when the workflow's referenced agents already exist (scaffolding is for empty repos); (b) machine-local installs (`.claude/`, `.rigger/shim/`) get `.gitignore` entries written by setup itself; (c) decide per repo whether the scaffolded agents are content (commit them) or artifacts (ignore them) - the current half-state is the only wrong answer. Kin to Gap 5 (setup drift-awareness); a single setup-hygiene unit can close both.

## Gap 10: stale unknown-provenance branches accumulate

**Intent.** The loop's branch lifecycle is self-cleaning: unit branches and the run branch are deleted once their content lands (PR #7's twelve `rigger/u/*` branches were pruned this way).

**Reality.** Local branches `wf-run`, `wf/metrics-project`, and `work-limit-resume` predate the current branch discipline and carry no obvious mapping to a merged PR; they were left in place because their provenance could not be established during cleanup. Branches that outlive their run erode the "branch = in-flight work" signal the loop relies on.

**Fix shape.** Byran disposition: inspect each (`git log main..<branch>`) and delete or PR what remains. Then close the class - the loop records branch creation in the event log, so a `rigger validate` (or `stats`) check can list local branches with no corresponding open unit and flag them as residue.

---

## Closed

Move entries here when they land, with the closing PR.

- **Fresh `cargo install` required `--locked`** - closed by PR #7 (u1): `ort-sys` pinned exact, `install-nolock` CI job guards the fresh-resolve path.
- **`/workflows` phase display implied a false global stage order** - closed by PR #7 (u2): per-unit `opts.phase` progress groups; `meta` stays a pure literal, asserted by test.
- **EventStore contract too narrow to trust SQLite as a KurrentDB proxy** - narrowed by PR #7 (u3): four checks added (exact-revision concurrency, nonzero-revision subscription resume, never-appended reads, distinct-stream concurrent appends), enforced against both adapters in CI. Breadth remains a judgment call; revisit when a real KurrentDB behavior diverges.
