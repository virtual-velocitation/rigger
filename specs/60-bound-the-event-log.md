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
  the LATEST batch recorded for that same file. The partition is decided BY EVENT TYPE FIRST,
  and only then by key shape. The discriminator is the four derived index types - code-owned
  constants, not a string convention - so an event of ANY other type is never eligible for
  project-scoped suppression whatever its replay key looks like, and every other replay key
  (unit lifecycle, gate verdicts, breaker trips) stays seeded from the current run's slice
  exactly as today. Key shape is read only AFTER the type says the event is derived, and then
  it is the WHOLE key that identifies the file and its generation
  (`<prefix>/<file>@<hash>#<i>`: the remainder must carry an `@` and a `#<digits>` tail), never
  a sniff of the leading `gc/` / `gd/` segment - `src/ingest.rs::key_batch` takes its prefix
  from the CALLER, so a prefix sniff rests on an unenforced cross-module naming habit and a
  unit or stage id equal to `gc` or `gd` would make its own lifecycle keys read as project
  facts. Type-first is what makes Global constraint 3 a property of the code rather than of a
  convention: a non-derived event cannot be dropped by this path because it never reaches the
  comparison. An unchanged file therefore appends ZERO events on every subsequent run forever; a
  changed file - INCLUDING a file reverted to content it held at any earlier point - hashes
  differently from its latest recorded batch, re-emits its whole batch, and supersedes its prior
  structural edges by the existing spec-29a mechanism. (An ever-recorded key set would wedge the
  revert case: the reverted content's keys match old records, nothing re-emits, and the graph
  stays on the superseded version forever.) The misleading comment on `ingest_project_batches`
  (which claims project-scoped dedup already holds) starts telling the truth.
- **Storage-level idempotency guard** (`src/eventstore/`, append seam): defense in depth so a
  regression upstream can never re-bloat the log. It suppresses on the SAME test the sink uses,
  never a wider one: appending an event of one of the four derived index types is a storage
  no-op ONLY when its replay key is already recorded AS THAT FILE'S LATEST GENERATION (the
  `<prefix>/<file>@<hash>#<i>` key names the file and its content generation, so the store can
  ask that question of keys the log ALREADY carries - no new event type, no new metadata, no
  backfill). A key recorded earlier that is no longer the file's latest - the revert case - is
  NOT redundant and MUST append; an ever-recorded test here would re-wedge the revert case
  Global constraint 4 forbids. ONLY the derived types get content identity: domain events
  legitimately repeat (two identical `ReviewFinding`s mean the finding was raised twice), so
  every non-derived type keeps per-append identity untouched. Implementation may use a lazily
  created index over the derived types' replay keys or an in-connection seen-set - whatever the
  store layer makes honest - but the observable contract is the no-op, and a no-op must never
  cost more than an index seek on a log of any size. Because a suppressed append writes fewer
  events than it was handed, what the append REPORTS BACK has to say what it actually wrote:
  the one shared append-and-fold authority (`src/ingest.rs::append_and_fold_batch`) derives each
  event's fold position arithmetically as `base = last + 1 - n`, which is only true when all `n`
  events were written, so a short write must be observable at the port or every event in that
  batch folds at a position the store never issued. BACKEND SCOPE, decided here so no unit has
  to: the SUPPRESSION half is adapter-local. The embedded default store implements it; an
  adapter that does not implement it appends through, which is the fail-safe direction (it can
  only ever write MORE, never drop), and its module says so in one line. The HONESTY half is a
  PORT obligation every adapter owes, because it is what keeps the shared append-and-fold
  authority correct for whichever store is wired: the backend-agnostic contract suite
  (`src/eventstore/contract.rs`, which both adapters' tests run) is where it is pinned, so a
  caller can never derive a position the store did not issue on ANY backend. This criterion's
  own suppression test therefore drives the STORE PORT directly rather than the run's ingest
  path - it must, because the sink above it is built to never hand the store a redundant append
  in the first place (see the layering decision), and a defense whose only proof is that
  nothing calls it is not a proof.
- **Supported compaction** (`src/main.rs`, `rigger reset`): a new `rigger reset --derived` prunes
  the accumulated duplication from an EXISTING bloated log: for each of the four derived types it
  keeps the LATEST event per distinct replay key, deletes the rest, and vacuums, so the file
  shrinks on disk. Every non-derived event survives byte-for-byte; the graph projection stays
  consistent (it is an upsert projection - all copies fold to the same rows) and `rigger status`
  / `rigger validate` read the compacted store correctly. The command prints what it removed
  (rows per type, bytes reclaimed). `--derived` composes with the existing `--runs` the way the
  flags read: each prunes its own accumulation. BACKEND SCOPE, decided here so no unit has to:
  deleting rows and reclaiming the file is a mechanic of the embedded default store, so
  `--derived` is implemented there and, on any other configured backend, FAILS LOUDLY naming the
  backend it needs. Never a silent no-op: reporting a prune that did not happen is the one
  outcome an operator cannot detect.
- **Docs** (`docs/architecture.md` + the handbook prose rendered by `src/docs.rs`): whatever
  prose describes the ingest dedup or `rigger reset` re-renders to the new truth, so the drift
  gates stay green. Each prose site is named with its OWNER, so no unit inherits another's
  paragraph and none is left for a reviewer to demand of whoever is nearest: the content-keyed
  skip paragraph in `docs/architecture.md` (section 5.5) states the dedup rule and belongs to
  the criterion that changes that rule; the `rigger reset` prose rendered by `src/docs.rs` -
  which today names `--runs` as THE prune command, and whose own drift assertions in that file
  check that wording - belongs to the criterion that adds `--derived`, and `src/docs.rs` is that
  criterion's file to edit, nobody else's. A paragraph lands in the SAME unit as the code it
  describes, never in a later one.

- **The second feature lane, and what proves it** (no gate in this project's gate library runs
  it): the library runs `fmt`, `clippy`, `build`, `test` and `style`, all on DEFAULT features,
  and it is covered by the definition pin - editing it mid-run halts the run - so no unit may
  add a lane to it. The lane is therefore proved the way a criterion no gate can see must be:
  by naming the evidence and giving it an owner. The lane sweep runs, over the INTEGRATED change
  set and in its own worktree, `cargo fmt --check`, `cargo clippy --no-default-features
  --all-targets -- -D warnings`, `cargo build --no-default-features` and `cargo test
  --no-default-features`, and records the transcript as its evidence; it also OWNS every
  conditional-compilation fix the change set needs to compile and pass in that lane, which no
  sibling's gates can surface because every sibling gate runs the default lane only. The
  repository CI runs the same battery on both lanes, so the evidence is reproducible outside
  the run.

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
  the whole stream rather than the current run's slice. This criterion OWNS the seeding fix at
  BOTH ingest sinks (the run's keyed emit and the cold graph build), and with it the type-first
  suppression predicate itself - the predicate IS the seeding fix, so writing it is this
  criterion's work and no reviewer may charge it to criterion 2. It is the AUTHORITATIVE layer:
  what this rule lets through is what the log receives on the run path. EXCLUSIONS, so no
  reviewer demands them here: this criterion does NOT own the store-layer guard (criterion 4),
  does NOT own the revert proof (criterion 3), does NOT own the `rigger reset` prose (criterion
  5), and is NOT asked to run the second feature lane (criterion 6). It DOES re-render the
  content-keyed skip paragraph in `docs/architecture.md`, because that paragraph states the rule
  this criterion changes.
- [ ] a test proves RUN-SCOPING SURVIVES: a non-ingest replay key recorded by a PRIOR run does
  not suppress the current run's own keyed emit (the Gap 11 boundary still holds for unit
  lifecycle), pinned at the same seeding seam. It pins the TYPE gate as much as the key gate: a
  non-derived event is ineligible for project-scoped suppression whatever its key looks like.
  This criterion adds no implementation of its own - the predicate it pins is criterion 1's -
  and it owns no docs paragraph.
- [ ] a test proves the CHANGE PATH: editing one file between runs re-emits exactly that file's
  whole batch and supersedes its prior structural edges, while untouched files still append
  nothing. A REVERT IS A CHANGE, and this criterion OWNS that proof: a test drives a file BACK
  to content it held at an earlier RECORDED generation and asserts the live graph then equals a
  cold rebuild from the current tree (Global constraint 4). No other criterion owns the revert
  case, so no other unit is asked for that test. The proof runs over the FULL suppression stack -
  the sink rule of criterion 1 AND the storage guard of criterion 4 both in place - because a
  revert that survives one layer and is swallowed by the other is exactly the outcome this
  contract forbids, and it is invisible to either layer's own test. This criterion adds no
  production code and owns no docs paragraph; it is a proof unit.
- [ ] a test proves the STORAGE GUARD: appending a derived-type event with an already-recorded
  replay key that is STILL that file's LATEST recorded generation is a storage no-op, while a key
  the file has since moved past (the revert case) still appends, and an identical-payload domain
  event (e.g. `ReviewFinding`) still appends a new row. This criterion OWNS the store-layer
  defense, and with it the honesty of the shared append-and-fold authority under a partially
  suppressed append: what the append reports back must name what was written, so
  `src/ingest.rs::append_and_fold_batch` never folds an event at a position the store did not
  issue - and `src/ingest.rs` is THIS criterion's file to change; criteria 1 and 3 do not alter
  that authority's contract. It is the SUBORDINATE layer: the sink rule of criterion 1 decides
  what reaches the store on the run path, and this guard exists so a regression upstream can
  never re-bloat the log. It therefore applies the SAME latest-per-file test (never a wider,
  ever-recorded one), and it proves itself by driving the STORE PORT directly, since a correct
  sink is built to hand it nothing redundant. It does NOT own the sink rule and does NOT own the
  revert proof, and no reviewer may reject this criterion for lacking either.
- [ ] a test proves COMPACTION: `rigger reset --derived` on a store seeded with duplicated
  derived events keeps the latest event per distinct key, preserves every non-derived event,
  shrinks the file, and leaves a store that `rigger validate` reads clean and whose fold yields
  the unchanged live graph. This criterion OWNS the `rigger reset` prose rendered by
  `src/docs.rs` and its drift assertions there (they name `--runs` as THE prune command today),
  so `src/docs.rs` is this criterion's file to change and no other's.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
  This criterion OWNS the second lane end to end: it runs the sweep over the INTEGRATED change
  set of the five criteria above, records the transcript as the evidence (no gate in this
  project's library runs that lane, and the library is definition-pinned so no unit may add one),
  and it owns every conditional-compilation fix the change set needs to build and pass there.
  No sibling is asked to run that lane, and this criterion changes no behavior of its own.
