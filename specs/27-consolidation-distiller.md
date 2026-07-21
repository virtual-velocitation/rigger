# 27 - Sleep-phase consolidation: a findings/decisions distiller

**Goal:** bound cross-run graph growth automatically. A between-runs distiller folds
OLDER-THAN-CURRENT-RUN findings and decisions into per-file digest nodes - raw events kept and
retrievable via `rigger peers` - so grounding stays lean over months, not just after a manual
`reset --runs`. This is Workstream C (section 5) of the context-management addendum: the only capability
that bounds cross-run growth automatically (measured: 87% of the graph was dead-run cruft that
`reset --runs` sheds by hand).

## Design

Model the distiller on `src/playbooks.rs`, which already consolidates `LessonLearned` into a
rebuildable projection. Mirror its shape:

- `distill(events) -> Vec<Digest>` - fold the target events into a `BTreeMap` keyed by file, each
  value a digest (summary + count + contributing run ids), dedup-by-normalized-summary, sorted for
  determinism (mirror `playbooks::distill`, `src/playbooks.rs`).
- a `Digest` projection struct with a stable slug id (`fnv1a_64` over the file + summary, mirroring
  `playbooks::Playbook` / `POOL_SUBDIR`), the per-file summary, the trigger file, and the fold
  count.
- `rebuild(events, dir)` - the rebuildable-projection entry: re-derive from the log and rewrite the
  digest pool, exactly as `playbooks::rebuild` clears and rewrites `*.md`.

Scope by run boundary: only findings/decisions OLDER than the current run (events before the
latest `RunStarted` boundary) are consolidated; current-run items stay raw. This reuses the same
`RunStarted`-boundary attribution the `reset --runs` prune (`Projector::prune`,
`src/contextgraph/sqlite.rs`) and the LIVE/HISTORICAL peer labels (spec 21) already use. It is the
AUTOMATIC form of what `reset --runs` does by hand.

The distiller is a projection over the append-only log (section 2.1): it introduces NO new event type
(like `playbooks.rs`, it reads existing events); the raw `DecisionMade`/`ReviewFinding` events are
never deleted, so `rigger peers` still retrieves them. `LessonLearned` is out of the distiller's
scope - `playbooks.rs` remains the authority for lessons, which are preserved untouched.

Depends on spec 25 (disposition-expiry) for run-scoped invalidation: a resolved finding is already
invalidated by disposition; consolidation folds the OLD, still-live-but-stale remainder.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Determinism by construction: `distill` folds into `BTreeMap`/`BTreeSet` and emits a sorted
  `Vec`; identical input yields byte-identical digests.
- Projection over the log, source of truth untouched: the distiller adds NO event type and DELETES
  nothing; a rebuild re-derives every digest from the events.
- Load-bearing decision preserved: consolidation summarizes by AGE (older than the current run),
  never scoping grounding to the active run - the current run's raw items stay, and consolidated
  raw events stay retrievable via `rigger peers`.

## Done when

- [ ] a test proves the distiller consolidates OLDER-THAN-CURRENT-RUN findings/decisions into
  per-file digest nodes: given a log with an old run A and a current run B, A's findings/decisions
  about file F fold into one digest for F, while B's (current-run) items are left raw and
  un-consolidated. This criterion OWNS the older-than-current-run per-file fold (modeled on
  `playbooks::distill`/`rebuild`).
- [ ] a test proves the raw events remain RETRIEVABLE via `rigger peers` after consolidation: the
  digest summarizes, but the underlying `DecisionMade`/`ReviewFinding` events are not deleted and a
  `rigger peers` query still returns them. This criterion OWNS the raw-retrievability / projection-
  rebuild guarantee; it does NOT own the fold (criterion 1).
- [ ] a test proves `LessonLearned` is PRESERVED: lessons are outside the distiller's scope and
  survive it untouched (`playbooks.rs` remains authoritative for lessons). This criterion OWNS
  lesson-preservation; it does NOT own the fold or retrievability (criteria 1-2).
- [ ] a test proves consolidation is RUN-SCOPED and DETERMINISTIC: it partitions old vs current by
  the latest `RunStarted` boundary (the same attribution `reset --runs` uses), and identical input
  yields byte-identical digest output. This criterion OWNS run-boundary scoping + determinism; it
  does NOT own the fold, retrievability, or lesson-preservation (criteria 1-3).
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
