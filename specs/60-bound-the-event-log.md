# 60 - Bound the event log: project-scoped ingest dedup and supported compaction

**Goal:** stop the event store growing without bound (issue #26). The project-ingest pass re-emits
the ENTIRE derived index (`EdgeInferred` / `CodeEntityExtracted` / `DocLinkExtracted` /
`DocConceptExtracted`) on every run: measured 39.5x payload duplication on a large external
project (3.5M events, 1.5 GiB log, 41 GiB conductor RSS, OOM-killed three times in one session)
and 47.9x on THIS repo's own store (1.32M of 1.48M events are `EdgeInferred`; 98.3% of the log is
re-derivable index). The spec-49 keyed-batch dedup machinery is correct and already stamps every
ingest event with its `<prefix>/<file>@<hash>#<i>` replay key - the defect is one scoping line:
`replayed_keys` is seeded from `crate::run::current_run(&all_prior)` (the CURRENT run's slice,
`src/conductor.rs` around line 1339), so a NEW run sees none of the prior runs' ingest keys and
re-appends the whole index. Run-scoping is right for unit-lifecycle replay (the Gap 11 zombie
fix) and wrong for derived project facts, which are project-scoped: a file's content hash does
not change because a new run started.

## Design

- **Project-scoped ingest dedup** (`src/conductor.rs`): the dedup consulted by the ingest emit
  path is project-scoped, derived from the whole stream (`all_prior`, already read at run start
  - no extra store round-trip), not from the current-run slice. The comparison is LATEST-PER-FILE,
  not ever-recorded: a file's batch is suppressed only when its content hash equals the hash of
  the LATEST batch recorded for that same file. The partition is by key shape: ingest keys carry
  the `gc/` / `gd/` prefixes (`src/ingest.rs::key_batch`), every other replay key (unit
  lifecycle, gate verdicts, breaker trips) stays seeded from the current run's slice exactly as
  today. An unchanged file therefore appends ZERO events on every subsequent run forever; a
  changed file - INCLUDING a file reverted to content it held at any earlier point - hashes
  differently from its latest recorded batch, re-emits its whole batch, and supersedes its prior
  structural edges by the existing spec-29a mechanism. (An ever-recorded key set would wedge the
  revert case: the reverted content's keys match old records, nothing re-emits, and the graph
  stays on the superseded version forever.) The misleading comment on `ingest_project_batches`
  (which claims project-scoped dedup already holds) starts telling the truth.
- **Storage-level idempotency guard** (`src/eventstore/`, append seam): defense in depth so a
  regression upstream can never re-bloat the log. Appending an event of one of the four derived
  index types whose replay key is already recorded is a storage no-op (the append reports
  success and writes nothing). ONLY the derived types get content identity: domain events
  legitimately repeat (two identical `ReviewFinding`s mean the finding was raised twice), so
  every non-derived type keeps per-append identity untouched. Implementation may use a lazily
  created index over the derived types' replay keys or an in-connection seen-set - whatever the
  store layer makes honest - but the observable contract is the no-op.
- **Supported compaction** (`src/main.rs`, `rigger reset`): a new `rigger reset --derived` prunes
  the accumulated duplication from an EXISTING bloated log: for each of the four derived types it
  keeps the LATEST event per distinct replay key, deletes the rest, and vacuums, so the file
  shrinks on disk. Every non-derived event survives byte-for-byte; the graph projection stays
  consistent (it is an upsert projection - all copies fold to the same rows) and `rigger status`
  / `rigger validate` read the compacted store correctly. The command prints what it removed
  (rows per type, bytes reclaimed). `--derived` composes with the existing `--runs` the way the
  flags read: each prunes its own accumulation.
- **Docs** (`docs/architecture.md` + the handbook render, if either states ingest/log behavior):
  whatever prose describes the ingest dedup or `rigger reset` re-renders to the new truth, so the
  drift gates stay green.

## Notes (non-criteria)

- The deeper question the issue raises - whether derived index facts belong in an append-only log
  at all - is answered HERE as: they stay, bounded. The log remains the single mutation authority
  and replay-keying source the batched fold depends on; with project-scoped dedup the derived
  slice of the log converges to one event per distinct fact plus genuine change history, which is
  the same asymptote a separate table would have, without forking the ingest path in two.
- Replay RSS follows log size: bounding the log bounds the conductor's whole-stream read. No
  streaming-replay work is in scope.
- No new event type is introduced anywhere in this spec.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- Fail-safe direction: dedup and compaction may only ever DROP redundant derived-index appends;
  no path may drop, reorder, or rewrite a non-derived event.
- The graph projection's contract is untouched, stated precisely: after any mix of dedup,
  compaction, and re-ingest, the live graph equals what a cold rebuild from the CURRENT tree
  would produce. Suppression may only ever skip appends that are redundant AGAINST THE LATEST
  recorded state per file - it must never leave the graph on a superseded version of any file
  (the revert case above is part of this contract, not an exception to it).

## Done when

- [ ] a test proves UNCHANGED-TREE RUNS APPEND NOTHING: a second run (fresh `RunStarted`) over an
  unchanged tree appends zero derived-index events, because the ingest dedup keys are seeded from
  the whole stream rather than the current run's slice. This criterion OWNS the seeding fix.
- [ ] a test proves RUN-SCOPING SURVIVES: a non-ingest replay key recorded by a PRIOR run does
  not suppress the current run's own keyed emit (the Gap 11 boundary still holds for unit
  lifecycle), pinned at the same seeding seam.
- [ ] a test proves the CHANGE PATH: editing one file between runs re-emits exactly that file's
  batch under its new hash keys and supersedes its prior structural edges; untouched files still
  append nothing.
- [ ] a test proves the STORAGE GUARD: appending a derived-type event with an already-recorded
  replay key is a storage no-op, while appending an identical-payload domain event (e.g.
  `ReviewFinding`) still appends a new row. This criterion OWNS the store-layer defense.
- [ ] a test proves COMPACTION: `rigger reset --derived` on a store seeded with duplicated
  derived events keeps the latest event per distinct key, preserves every non-derived event,
  shrinks the file, and leaves a store that `rigger validate` reads clean and whose fold yields
  the unchanged live graph.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
