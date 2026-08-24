# 61 - Canary truth and throughput: real attribution, honest rates, parallel tiers

**Goal:** make `rigger canary` report what actually happened and finish in the time the work
takes (issues #24 and #22). Three defects:

1. **Attribution is always empty.** `caught_by` came back `[]` on every item of a live run -
   including all four planted defects the panel correctly rejected - printing `0/4 (0.0%)`
   per tier and causing a real misdiagnosis. The catch test
   (`src/canary.rs::Finding::catches`) requires `about` to equal `item.anchor` EXACTLY; live
   reviewers name the same file as absolute paths, worktree-relative paths, or inside text.
2. **The false-positive rate is not reported.** Rejecting a known-good control burns a
   remediation cycle and, repeated, escalates a correct unit - yet it must be reconstructed
   from raw per-item records. `verdict_correct` is already present and sufficient.
3. **Everything runs serially.** One child at a time - even the four tier-1 lenses declared
   parallel - and independent corpus items queue. Measured: ~22 min/item, 2.5+ hours for 6
   items, nothing on stdout until the end.
4. **The model-drift gate over-triggers, on unreliable data.** `--if-model-changed` treats
   any resolved-id change as the full-alarm event, so a same-tier snapshot bump mandates the
   multi-hour panel re-measure only a genuine tier re-point warrants - and the resolved ids
   it compares are agent-prose self-reports, which models get wrong.

## Design

- **Attribution** (`src/canary.rs`): a tier catches when it raises a finding about the anchor
  under tolerant matching - repo-relative, absolute, or path-suffix spellings of the same
  file all match. Tolerance is BOUNDED at path-segment boundaries: `corpus/a.rs` matches
  anchor `a.rs` (suffix begins at `/`), `extra.rs` does not. `caught_by` is populated from
  the findings each tier actually raised; a tier that raised nothing reads honestly as such.
- **No fake zeros** (scorecard render): a rate prints ONLY when its inputs were measured.
  Correct rejections with empty attribution render `n/a` with a one-line reason, never
  `0/N (0.0%)`.
- **False positives first-class** (scorecard render): the summary reports control items as
  their own line - known-good approved vs rejected - visible at the same glance as the catch
  rate.
- **Parallel tiers and sharded items** (`src/canary.rs`, reusing `crate::parallel`): within
  an item the tier-1 lenses run concurrently; the adversary follows the lenses and the
  adjudicator the adversary (legitimately sequential). Independent items shard across
  workers. `--jobs <n>` caps total concurrent spawns; default greater than 1 and sane for
  the corpus size. One scorecard aggregates regardless of sharding.
- **Observable progress**: the parent prints a per-item stdout line as each item completes
  (id, verdict correctness, tiers caught, elapsed).
- **Per-tier model pinning** (`src/canary.rs`, `src/main.rs`): repeatable `--model <tier>=<id>`
  (tiers: `lens`, `adversary`, `adjudicator`) overrides the named tier's model for THIS run
  only - config untouched, ids passed through verbatim. The scorecard header records binary
  build, corpus content hash, and each tier's RESOLVED model id - sourced from the
  AUTHORITATIVE MODEL IDENTITY criterion's structured `AgentResult::resolved_model`, never
  the pre-spawn configured `model_for_attempt` alias - so an A/B arm is auditable from its
  scorecard alone (the instrument `docs/experiments/2026-08-11-lens-model-ab-protocol.md`
  pre-registers). Per-item records also carry each tier's finding count (the over-flagging
  measure).
- **Drift severity, decided here** (`src/canary.rs`, and the validate warning text): the
  resolved-id comparison splits an id at its trailing date suffix (`-YYYYMMDD`). Same base
  with a differing date is SNAPSHOT drift: `--if-model-changed` reports it on stdout and
  exits successfully WITHOUT running the panel, and `rigger validate` words its warning as
  an advisory (run the panel when convenient), not a mandate. A differing base - a tier or
  family re-point such as opus -> sonnet - is MODEL change and keeps today's behavior: the
  panel runs. An explicit `rigger canary` with no `--if-model-changed` flag always runs
  regardless of drift class, so an operator who wants the measurement anyway just asks.
- **Authoritative model identity, decided here** (`src/canary.rs`, and the spawn-result
  path that records worker metadata): the resolved model id recorded for any spawn -
  canary tiers and run workers alike - is read from the runner's STRUCTURED metadata (the
  CLI's machine-readable result output), never from agent prose self-report. A spawn whose
  runner metadata carries no model id records none and is reported unmeasured - no fake or
  defaulted value. The model-drift warning (`rigger validate` and `--if-model-changed`)
  keys on these authoritative per-tier ids, so a worker's mistaken claim can neither
  forge nor mask drift.
- **Per-spawn timing in stats** (`src/metrics.rs` / `src/main.rs::cmd_stats`): `rigger stats`
  pairs each recorded spawn request with its result by spawn id and reports duration
  aggregates (per tier/agent: count, total, mean). An unpaired request (dead worker) is
  excluded from every aggregate and reported as its own count - a fabricated or zero
  duration never enters a mean. If pairing needs more than the existing result event
  carries, it gains a meta stamp on the EXISTING event - no new event type.

## Notes (non-criteria)

- Tier semantics unchanged: lenses are collectively one tier for catch purposes; the
  adjudicator's position-bias probe stays as is.
- Corpus format, defect classes, and fail-closed adjudication authority untouched.
- Crash-resume disposition, decided here: a canary run is ONE-SHOT and run-scope-free;
  an interrupted run restarts from scratch; no resume machinery is owed.
- Concurrency-vs-determinism, decided here: fan-out and sharding criteria are pinned with
  the DETERMINISTIC test driver; a live-model run is not what they measure. Parallelism must
  not reorder or alter what the fake driver's serial run scores.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any
  external tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism where it counts: parallel execution must not change any scored outcome or
  scorecard content, pinned by test.
- Honest reporting: no scorecard line may render a rate whose attribution was not measured.

## Done when

- [ ] a test proves TOLERANT ATTRIBUTION: a finding naming the anchor as an absolute path, a
  repo-relative path, or a segment-boundary path suffix scores the raising tier into
  `caught_by`, and a finding about an unrelated file - including one whose name merely ENDS
  with the anchor's text without a segment boundary - still does not. This criterion OWNS the
  catch test.
- [ ] a test proves NO FAKE ZEROS: a scorecard whose items were correctly rejected but carry
  empty attribution renders the per-tier line as `n/a` with a reason, never `0/N (0.0%)`.
  This criterion OWNS the per-tier catch-rate render branch (n/a vs `0/N`), computed from
  `CanaryOutcome`'s EXISTING `caught_by`/`verdict_correct` fields; it adds no new
  `CanaryOutcome` field and does not touch the FINDINGS VOLUME criterion's finding-count
  field/aggregate or the FALSE POSITIVES criterion's control line.
- [ ] a test proves FALSE POSITIVES ARE FIRST-CLASS: a corpus with a rejected known-good
  control renders a control/false-positive line reporting it directly on the summary. This
  criterion OWNS the control/false-positive summary line, computed from `CanaryOutcome`'s
  EXISTING `planted`/`verdict_approved` fields (no new field needed); it does not touch the
  NO FAKE ZEROS criterion's per-tier n/a branch or the FINDINGS VOLUME criterion's
  finding-count field/aggregate.
- [ ] a test proves LENS FAN-OUT: within one item the tier-1 lenses run concurrently while the
  adversary observes all lens findings and the adjudicator observes the adversary's - pinned
  at the scheduling seam with the deterministic test driver, scored outcomes identical to the
  serial order's. This criterion OWNS restructuring `score_item`'s inner tier-1 lens loop onto
  `crate::parallel`, keeping the adversary and adjudicator sequential after it; it does not
  touch `run_canary`'s outer per-item loop or the `--jobs` cap, both the ITEM SHARDING
  criterion's.
- [ ] a test proves ITEM SHARDING AND THE JOBS CAP: independent corpus items run concurrently,
  `--jobs <n>` bounds total concurrent spawns (with a default greater than 1), and the single
  aggregated scorecard is identical to a serial run's. This criterion OWNS restructuring
  `run_canary`'s outer per-item loop onto `crate::parallel` AND the `--jobs` total-concurrent-
  spawn cap/budget that bounds BOTH this outer sharding and the LENS FAN-OUT criterion's inner
  lens fan-out together; it is the LENS FAN-OUT criterion's CONSUMER of the already-built inner
  concurrency, not a second implementer of `score_item`'s lens loop.
- [ ] a test proves PROGRESS: each completed item emits a per-item stdout line (id, verdict
  correctness, caught tiers, elapsed) before the final scorecard. This criterion OWNS the
  per-item stdout progress line, hooked at the per-item completion point the ITEM SHARDING
  criterion's restructuring produces; it does not touch the sharding mechanism or the `--jobs`
  cap itself.
- [ ] a test proves MODEL PINNING: `--model lens=<id>` resolves the lens tier's agents to the
  pinned id for the run (other tiers untouched, config file unmodified), and the scorecard
  header records binary build, corpus hash, and every tier's resolved model id. The header's
  resolved-model-id source is `AgentResult::resolved_model`, never `model_for_attempt` - this
  criterion is the AUTHORITATIVE MODEL IDENTITY criterion's CONSUMER for that value, NOT a
  second implementer of resolved-model recording.
- [ ] a test proves FINDINGS VOLUME: each per-item record carries the count of findings each
  tier raised, and the scorecard aggregates it per tier. This criterion OWNS the per-tier
  finding-count field on `CanaryOutcome` and the findings-volume aggregate line in the
  scorecard render; the NO FAKE ZEROS criterion's n/a branch and the FALSE POSITIVES
  criterion's control line are each the OTHER's, not this one's.
- [ ] a test proves SPAWN TIMING: `rigger stats` reports per-agent duration aggregates derived
  by pairing recorded spawn requests with their results by spawn id, excludes unpaired
  requests from every aggregate while reporting their count, with no new event type.
- [ ] a test proves AUTHORITATIVE MODEL IDENTITY: the resolved model id recorded for a
  spawn comes from the runner's structured metadata, a conflicting agent-prose claim never
  enters the record, and a spawn with no metadata id records none and reports as
  unmeasured rather than defaulted. This criterion OWNS resolved-model recording; the
  `--if-model-changed` gate is the DRIFT SEVERITY criterion's, NOT this one's, and the
  MODEL PINNING criterion's scorecard header is this criterion's CONSUMER of the resolved
  id, never a second producer of it.
- [ ] a test proves DRIFT SEVERITY: a resolved-id change differing only in the trailing date
  suffix is classified as snapshot drift - `--if-model-changed` reports it and exits
  successfully without running the panel - while a change in the id's base still runs the
  panel. This criterion OWNS the drift-severity classifier and the `--if-model-changed` gate.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
