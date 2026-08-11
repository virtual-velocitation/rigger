# 61 - Canary truth and throughput: real attribution, honest rates, parallel tiers

**Goal:** make `rigger canary` report what actually happened and finish in the time the work
takes (issues #24 and #22). Two defects and one cost problem:

1. **Attribution is always empty.** `caught_by` came back `[]` on EVERY corpus item of a live
   run - including all four planted defects the panel correctly rejected - so the scorecard
   printed `0/4 (0.0%)` per tier. That reads as "tier 1 and tier 2 are blind" when the per-item
   records show the opposite, and it caused a real misdiagnosis (the actual defect was both
   known-good controls being rejected - a false-positive problem with the opposite fix). The
   catch test (`src/canary.rs::Finding::catches`) requires a finding's `about` entry to equal
   `item.anchor` EXACTLY; a live reviewer that names the same file any other way (absolute path,
   worktree-relative path, or with surrounding text) never matches. Root-cause the live-path
   miss and make attribution hold for how real reviewers actually name files.
2. **The false-positive rate is not reported.** Rejecting a known-good control is arguably the
   more expensive failure (it burns a remediation cycle on correct work and, repeated, exhausts
   `max_retries` and escalates a unit that was already right), yet today it must be
   reconstructed by reading raw per-item records. `verdict_correct` is already present and
   sufficient.
3. **Everything runs serially.** One child process at a time across the whole run - including
   the four tier-1 lenses that `workflow.yml` declares parallel and the run loop already fans
   out - and corpus items, which are fully independent, run one after another. Measured: ~22
   minutes per item, 2.5+ hours for a 6-item corpus, with NOTHING on stdout until the end (the
   only liveness signal was inspecting child processes).

## Design

- **Attribution** (`src/canary.rs`): a tier catches the defect when it raises a finding about
  the anchor under tolerant matching - the finding's `about` entry and the item's `anchor`
  refer to the same file whether the reviewer spelled it repo-relative, absolute, or as a path
  suffix. The scored `caught_by` is populated from the findings each tier actually raised, per
  tier, exactly as the in-process tests already model. If a live run's findings genuinely
  cannot be attributed (a tier raised nothing), the per-tier line says so honestly.
- **No fake zeros** (`src/canary.rs`, scorecard render): a rate is printed ONLY when its inputs
  were actually measured. When attribution recovered nothing for any item that was
  nevertheless correctly rejected, the per-tier line renders `n/a` with a one-line reason
  rather than `0/N (0.0%)` - an unpopulated field must not look like a measurement.
- **False positives first-class** (`src/canary.rs`, scorecard render): the summary reports the
  control (non-planted) items as their own line - how many known-good controls were approved
  vs rejected - so a false-positive problem is visible at the same glance as the catch rate.
- **Parallel tiers and sharded items** (`src/canary.rs`, reusing `crate::parallel`): within an
  item, the tier-1 lenses run concurrently (they are independent by the same declaration the
  run loop honors); the adversary still follows the lenses and the adjudicator still follows
  the adversary (each consumes the prior tier's findings - legitimately sequential).
  Independent corpus items shard across workers. A `--jobs <n>` flag caps total concurrent
  agent spawns; its default is greater than 1 and sane for the corpus size (not unbounded).
  One scorecard aggregates everything regardless of sharding.
- **Observable progress** (`src/canary.rs`): the parent prints a per-item line to stdout as
  each item completes (id, verdict correctness, tiers caught, elapsed), so a multi-hour run is
  visibly alive without inspecting child processes.
- **Per-tier model pinning for measurement runs** (`src/canary.rs`, `src/main.rs`): a repeatable
  `--model <tier>=<id>` flag (tiers: `lens`, `adversary`, `adjudicator`) overrides the model the
  named tier's agents resolve for THIS canary run only - config untouched, aliases allowed but
  ids passed through verbatim so an experiment can pin exact ids. The scorecard header records
  the binary build, the corpus content hash, and each tier's RESOLVED model id (from the same
  `model_for_attempt` authority), so an A/B arm is auditable from its scorecard alone. This is
  the instrument `docs/experiments/2026-08-11-lens-model-ab-protocol.md` pre-registers; the
  per-item records additionally carry the count of findings each tier raised (the over-flagging
  measure), alongside the existing attribution.
- **Per-spawn timing in stats** (`src/metrics.rs` / `src/main.rs::cmd_stats`): per-agent wall
  time becomes derivable from the log: `rigger stats` pairs each recorded spawn request with
  its recorded result by spawn id and reports duration aggregates (per tier/agent: count,
  total, mean), so "which tier dominates a run's wall clock" is answerable with data. If the
  recorded result event does not already carry what pairing needs, it gains a meta stamp on
  the EXISTING event - no new event type.

## Notes (non-criteria)

- Tier semantics are unchanged: lenses are collectively one tier for catch purposes; the
  adjudicator's position-bias stability probe (natural + reversed order) stays as is.
- The corpus format, defect classes, and fail-closed adjudication authority are untouched.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism where it counts: parallel execution must not change any scored outcome or the
  scorecard's content (ordering of concurrent completion may not leak into scoring), pinned by
  test.
- Honest reporting: no line on the scorecard may render a rate whose underlying attribution
  was not measured.

## Done when

- [ ] a test proves TOLERANT ATTRIBUTION: a finding naming the anchor as an absolute path, a
  repo-relative path, or a path suffix scores the raising tier into `caught_by`, and a finding
  about an unrelated file still does not. This criterion OWNS the catch test.
- [ ] a test proves NO FAKE ZEROS: a scorecard whose items were correctly rejected but carry
  empty attribution renders the per-tier line as `n/a` with a reason, never `0/N (0.0%)`.
- [ ] a test proves FALSE POSITIVES ARE FIRST-CLASS: a corpus with a rejected known-good
  control renders a control/false-positive line reporting it directly on the summary.
- [ ] a test proves LENS FAN-OUT: within one item the tier-1 lenses run concurrently while the
  adversary observes all lens findings and the adjudicator observes the adversary's - pinned
  at the scheduling seam, with scored outcomes identical to the serial order's.
- [ ] a test proves ITEM SHARDING AND THE JOBS CAP: independent corpus items run concurrently,
  `--jobs <n>` bounds total concurrent spawns (with a default greater than 1), and the single
  aggregated scorecard is identical to a serial run's.
- [ ] a test proves PROGRESS: each completed item emits a per-item stdout line (id, verdict
  correctness, caught tiers, elapsed) before the final scorecard.
- [ ] a test proves MODEL PINNING: `--model lens=<id>` resolves the lens tier's agents to the
  pinned id for the run (other tiers untouched, config file unmodified), and the scorecard
  header records binary build, corpus hash, and every tier's resolved model id.
- [ ] a test proves FINDINGS VOLUME: each per-item record carries the count of findings each
  tier raised, and the scorecard aggregates it per tier.
- [ ] a test proves SPAWN TIMING: `rigger stats` reports per-agent duration aggregates derived
  by pairing recorded spawn requests with their results by spawn id, with no new event type.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
