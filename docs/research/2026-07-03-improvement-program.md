# The improvement program: research synthesis, 2026-07-03

Four research streams (harness/loop design survey, durable-execution and CI-orchestrator patterns, embedded-dashboard design, review-quality calibration), each grounded in this tree before surveying externally. This document is the feed for the next specs, as design-intent-gaps.md was for specs 04-09. Full agent reports are summarized; the load-bearing claims were verified against the code.

**Meta-finding:** rigger's machine-verifiable-done (R6) is exactly the oracle the test-time-compute literature names as its open problem - every pick-the-winner step that is approximate elsewhere (AlphaCode-style sampling, cascade routing, speculative attempts) is EXACT here, because the gates plus the adjudicator are the selector. Several items below simply cash that in.

## Wave 1 - quick wins, zero principle conflicts (candidate spec 10)

1. **Adversarial plan-critique gate before fan-out.** Run the existing review panel on the DECOMPOSITION before any implementer spawns (a `plan-critique` stage between plan and implement; shared-blast-radius splits named in its prompt). Directly attacks the spec-05 failure class (40 rejections / 5 escalations from a bad decomposition) at the cheapest moment. Pure config: a new YAML stage reusing existing agents.
2. **Declarative failure taxonomy.** The infra-vs-product distinction hand-coded across ~15 conductor sites (spec 07 lineage) violates R1; replace with ordered `failure_rules:` in workflow.yml (match exit/signal/output-regex -> class {infra|product|flaky}, per-class limits, exponential backoff), Buildkite/Argo-style. Includes the Bazel three-way gate outcome: rerun-on-fail N times, mixed = `FlakyVerdict`, which never demotes the autonomy ratchet.
3. **Heartbeat/liveness for spawned agents.** Today a HUNG (not dead) agent stalls a wave invisibly. Spawns carry max wall-clock + heartbeat interval; staleness folds as an infra failure through item 2's rules (Temporal HeartbeatTimeout pattern).
4. **Cheap-first model cascade.** `model:` becomes `model_ladder: [haiku, sonnet, opus]`; a unit's first attempt runs the cheapest rung and remediation escalates the rung. The gate+adjudicator verdict is a stronger router than FrugalGPT's learned scorer; resolved-model stamping (Gap 6) already logs the evidence.

## Wave 2 - observability (candidate spec 11): `rigger dash` + review-quality telemetry

- **`rigger dash`:** an embedded loopback server (tiny_http or hand-rolled; NO tokio/axum in the default build) serving one `include_str!`-embedded self-contained HTML page, polling `/api/events?since=<position>` (the store's own subscription primitive is poll-based; SSE adds nothing at this volume); `--export` renders the same template to a static shareable file. Past = ledger/metrics folds; present = `spawn::step_result`'s pending frontier (already computed); future = ready frontier + declared DAG + budget runway, STRUCTURAL ONLY (no ETAs, no likelihoods - the one place a dashboard could fake "done"). Widget order: review-economics panel and live-wave panel (zero new backend), unit swimlanes, gate heatmap, budget burn-down, escalation inbox, decision-graph inspector. NOT in v1: any browser write surface (the conductor stays the sole mutation authority), multi-project views, JS toolchains, time scrubbing.
- **Review-quality telemetry (the cheap six, ordered):** lens-overlap audit (zero schema change, retroactive); rejection-flip-flop rate (reject then approve on the SAME tree sha = reviewer noise; needs one metadata field - the sha on verified/UnitFailed/reviewed events); finding-survival per lens (adjudicator's required JSON line grows `upheld`/`discarded` arrays - prompt-only, parsed from evidence already logged); cause-tagged rejections ({genuine-defect|spec-ambiguity|decomposition-conflict|infra-fault} - prompt-only; automates the spec-05 triage a human did once); cost-per-real-catch per tier (derived, uses Gap-6 model stamps); adversary precision (derived). The dash renders these. Today rigger counts verdicts but never measures whether a rejection was CORRECT.

## Wave 3 - verify only what changed (candidate spec 12)

- **Content-addressed gate caching:** stamp `GateVerdict` with `input_digest = hash(gate cmd + tree-SHA of input paths)`; a matching prior green verdict answers the gate as a logged cache-hit event citing the prior position. The current cache is replay-only, keyed (unit, attempt, gate).
- **Staleness propagation (Dagster model):** on integrate, mark downstream TOUCHES-intersecting units stale and re-gate exactly those - the invalidation that makes the cache safe across merges.
- **TOUCHES-graph gate selection:** optional per-gate `inputs:` globs; the inner remediation loop runs only intersecting gates; integrate stays exhaustive (R6 preserved where "done" is asserted); skips are logged with reasons.
- **Saga compensation:** `UnitCompensated{commit}` + reverse-order revert when a later unit proves an integrated one wrong; staleness is the trigger; the ledger already records every integrating commit.

## Wave 4 - the harness improves itself (candidate specs 13+)

- **Definition pinning:** `DefinitionPinned{hash}` of workflow.yml + agent prompts at RunStarted; a step under a drifted definition halts loudly (with an explicit recorded rebase escape). Closes the silent-divergence hazard of mid-campaign config edits under replay. R1 carve-out: free for new runs, pinned for live ones.
- **Trajectory replay/eval:** `rigger replay <run-id> --against <config-rev>` - past runs as the regression corpus for prompt/config edits, diffing stats vs baseline. The log already IS the trajectory; only the eval layer is missing. Pairs with pinning for reproducible baselines.
- **First-green-wins speculation:** K parallel attempts per unit under the spawn budget (greedy marginal-gain allocation from per-class yield history); worktree isolation makes collisions impossible and the adjudicator is the exact winner-selector. Config-declared K, every allocation an event.
- **Risk-tiered review depth:** declarative `review_tiers:` map (blast-radius size x first-pass gates -> lenses-only | +adversary); adjudicator and gates mandatory on every tier; any bandit tuning runs shadow-mode first with off-policy evaluation over logged verdicts.
- **Seeded-defect canary corpus + model-change re-baseline:** the only recall measurement (everything else measures precision); re-run automatically when a tier's resolved model changes (Gap-6 stamps make the trigger free). Real new infrastructure - justified only if Wave 2's telemetry shows noise persisting.
- **Distilled playbooks:** post-run distillation of LessonLearned into a deduplicated, trigger-scoped insight pool (ExpeL/Devin-playbook pattern) retrieved by blast-radius relevance inside the existing byte budgets; must remain a rebuildable projection.

## Deliberately excluded

Mid-run dynamic replanning (genuine tension with anti-fragmentation; plan-critique removes most of the need), fold-compaction snapshots (not biting at current event volumes; R2 hazard when trusted over the log), browser control surfaces, multi-project control planes.
