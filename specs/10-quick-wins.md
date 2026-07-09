# 10 - Wave 1 quick wins: plan critique, declarative failures, liveness, model cascade

**Goal:** four freestanding, zero-principle-conflict improvements from the improvement program ([docs/research/2026-07-03-improvement-program.md](../docs/research/2026-07-03-improvement-program.md), Wave 1).

## Design

**Unit 1 - adversarial plan-critique gate.** A `plan-critique` stage between `plan` and `implement` in the scaffold and this repo's workflow: the existing adversary + adjudicator personas review the PROPOSED unit DAG before any implementer spawns - shared-blast-radius splits across units, mitigation-ownership ambiguity, and open dispositions (handbook rules 6-8) are named review targets in the stage prompt. A reject feeds back to the planner (bounded by the existing remediation depth); an approve releases the fan-out. Owns: pre-fan-out decomposition review. Exclusion: per-unit review is untouched.

**Unit 2 - declarative failure taxonomy.** An ordered `failure_rules:` block in workflow.yml - `{match: {exit_status?, signal?, output_regex?}, class: infra|product|flaky, limit, backoff: {duration, factor, max}}`, first match wins - replaces the hand-coded infra/product classification sites in the conductor (degenerate-reviewer, dead-worker, gate failures). Per-class limits; infra never charges the unit an attempt (spec-07 semantics preserved as the DEFAULT rules shipped in the scaffold). Gate failures gain the three-way outcome: on a non-manual gate failure rerun up to N (config); all-fail = fail (remediate + ratchet demotion as today), mixed = a `FlakyVerdict`-annotated pass-with-warning that NEVER demotes the autonomy ratchet. Owns: all failure classification. Exclusion: the spawn-budget breaker stays the global cap above all rules.

**Unit 3 - agent liveness.** Spawn requests carry `max_wall_clock` (config, per-role default); workers touch a per-spawn liveness marker under the scratch root on a heartbeat interval (driver-framed instruction, same mechanism family as the scratch policy); `rigger step` treats a spawn whose marker is stale beyond the wall-clock as an infra failure routed through unit 2's rules (recorded on the spawn's id, never a new event type), so a HUNG agent can no longer stall a wave invisibly. Sequenced after unit 2 (consumes its taxonomy). Exclusion: dead-worker (exit) handling is unchanged driver territory.

**Unit 4 - cheap-first model cascade.** Agent frontmatter accepts `model_ladder: [tier, ...]` (existing `model:` remains valid as a one-rung ladder); a unit's first attempt resolves the first rung and each remediation attempt advances one rung (clamped at the last). The resolved rung rides the existing model stamping (Gap 6), so `rigger stats`/the log show rung escalation per attempt. Scaffold ships the implementer on a ladder; reviewers keep fixed tiers (judgment is not laddered). Owns: model selection per attempt. Exclusion: review-tier flexing is Wave 4's, not this unit's.

## Global constraints

- Hyphens, not em dashes. New event types: NONE (FlakyVerdict rides as annotation/metadata on the existing GateVerdict). Idiomatic Rust; both lanes green (fmt, clippy, build, test, style). Existing behavior contracts keep their tests passing; spec-07 infra semantics are preserved as shipped default rules.

## Done when

- [ ] a plan-critique stage reviews the proposed unit DAG with the existing adversary and adjudicator before fan-out, naming rules 6-8 violations as review targets, with reject feeding planner remediation and approve releasing the wave - wired in the scaffold workflow and this repo's, pinned by a test that a decomposition splitting one blast radius across units draws a reject
- [ ] failure classification folds from an ordered `failure_rules:` config block (exit/signal/output-regex matchers; infra|product|flaky classes; per-class limits; backoff) replacing the hand-coded sites, with shipped defaults preserving spec-07 semantics and their existing tests; a mixed rerun outcome annotates the gate verdict as flaky and never demotes the autonomy ratchet, pinned
- [ ] spawns carry a wall-clock bound and workers a heartbeat marker; a stale in-flight spawn is classified through the failure rules as infra (no attempt charged) and surfaced in the step output, pinned by a test with a synthetic stale marker
- [ ] agent definitions accept `model_ladder`, attempts resolve successive rungs under remediation (clamped), the resolved rung is visible in the logged model stamps, and a single `model:` behaves exactly as today - pinned including the ladder-advance-on-retry path
