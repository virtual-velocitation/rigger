# 11 - Wave 2 observability: review-quality telemetry + `rigger dash`

**Goal:** measure whether the review tier is RIGHT (not just how often it rejects), and render the run - past, present, future - as an embedded local website. Program Wave 2.

## Design

**Unit 1 - review-quality telemetry (the cheap six).** Rigger counts verdicts but never measures rejection correctness; the spec-05 post-mortem that found every escalation was a process artifact was manual. Deliver, in one unit (shared blast radius: conductor emit sites, reviewer prompts, metrics folds, stats rendering):

- Worktree HEAD sha stamped as metadata on the review-boundary events (`verified`, review-reject `UnitFailed`, `reviewed`), mirroring the sha `UnitIntegrated` already carries.
- The adjudicator's required JSON line grows `"upheld": [finding-ids]`, `"discarded": [finding-ids]`, and on reject `"cause": "genuine-defect"|"spec-ambiguity"|"decomposition-conflict"|"infra-fault"` - prompt change only; the raw output is already durably logged, so capture needs no conductor change.
- New `metrics.rs` folds, rendered by `rigger stats`: rejection flip-flop rate (reject then approve on the SAME sha = reviewer noise); finding-survival per lens actor (upheld/raised, via the conductor-stamped META_ACTOR); lens-overlap rate (same-file finding duplication across actors - retroactive over history); cause-split rejection/escalation rates; adversary precision (adversary-only findings upheld); cost-per-upheld-finding per tier (spawn counts x Gap-6 model stamps).

Owns: all review-quality measurement. Exclusions: acting on the measurements (tiered review, canaries) is Wave 4; no new event types - metadata and prompt-contract extensions only.

**Unit 2 - `rigger dash`, sequenced after unit 1.** An embedded observability page over the existing projections:

- `rigger dash` serves ONE self-contained `include_str!`-embedded HTML page (vanilla JS/CSS, no build step) on `127.0.0.1` via a minimal synchronous HTTP layer (tiny_http-class or hand-rolled; NO tokio/axum in the default build), with read-only JSON endpoints (`/api/state`, `/api/events?since=<position>`) that are thin adapters over `ledger::project`, `metrics::project`, `spawn::step_result`, and `contextgraph::subgraph` - no new business logic. Client polls (1-2s); the store's own subscription primitive is poll-based, so SSE is explicitly out.
- `rigger dash --export <path>` renders the same template with the JSON inlined: a static, shareable, after-the-run artifact.
- Views: PAST - unit lifecycle swimlanes (status transitions, dashed retries), gate remediation heatmap, review outcomes including unit 1's quality panel, decision history with superseded entries struck through; PRESENT - the live wave (pending frontier, elapsed-since-parked), in-flight units, an action-needed inbox (ManualReview, UnitEscalated); FUTURE - structural only: ready frontier, declared DAG, budget runway, uncovered criteria. NO ETAs, NO likelihoods, NO fabricated certainty.
- Review-verdict rendering MUST reuse `metrics.rs`'s classification (there is no verdict event type; it is inferred from UnitStatus transitions).

Owns: all HTTP serving and rendering. Exclusions (v1 hard lines): no write/control surface of any kind (the conductor stays the sole mutation authority; control goes through the CLI); loopback only, no auth/TLS; no multi-project view; no time-scrubber; single-page vanilla HTML only.

## Global constraints

- Hyphens, not em dashes. New event types: NONE. No tokio/axum/async-runtime dependencies in the default build; any new dependency is small, synchronous, and justified in the unit's decision record. Both lanes green. `rigger stats` output remains backward-compatible (new sections append; existing lines unchanged for scripts).

## Done when

- [ ] review-boundary events carry the worktree sha, the adjudicator JSON contract carries upheld/discarded/cause, and `rigger stats` reports flip-flop rate, per-lens finding survival, lens overlap, cause-split rejections, adversary precision, and per-tier cost-per-upheld-finding - each fold pinned by a synthetic-log test, with existing stats lines unchanged
- [ ] `rigger dash` serves the embedded single-file page on loopback with live-polling past/present/future views over the existing projections (verdicts via metrics.rs's classification), `--export` writes the equivalent static file, the default build gains no async runtime, and a test drives the JSON endpoints against a seeded store while a structural check pins that no mutating endpoint exists
