# 107 - Perception is re-derived and episodes are archived: the log keeps knowledge

**Goal:** rigger is a hive mind: its graph holds the project's whole understanding and serves
each persona the slice it needs, and the append-only log is only the persistence underneath.
That log keeps what the hive decided, learned and did; it stops holding what the hive merely
perceived (the derived index, which the tree re-derives) and how a finished run spent itself
(its episodes, which move to git). Measured on the 2026-10-05 store (288,636 events, 430 MB of
payload, a 580 MB file):

- The derived index (`ingest::DERIVED_INDEX_TYPES`) is 229,500 events, 79.5% of the rows,
  recording 753 generations of 688 files. Every ingest records a changed file's whole extracted
  batch: of the current run's 20,927 events, 19,946 are derived events for 29 file generations.
  `graph.db` folds each batch as it is appended (`FoldingStore::append_and_fold`,
  `crates/rigger-grounder/src/ingest.rs`), no one-shot command reads them (spec 101), and the
  store accepts a derived append from any caller.
- A run's mechanics, the episodic types (Notes), are 21,386 events and 348 MB, 81% of the
  payload; `SpawnRequested` alone is 7,242 events and 343 MB of prompts. 17,988 of them (278 MB)
  belong to the 153 runs before the current one, and nothing removes them: `rigger reset --runs`
  (`reset_runs`, `src/cli/hygiene.rs`) prunes `graph.db` and deletes no event.
- The knowledge the hive keeps, every other type, is 37,750 events and 58 MB.

## Design

**UNIT ORDER AND BASE, decided here.** Criterion 1 needs nothing. Criterion 2 needs 1 (it records
the type criterion 1 declares). Criterion 3 needs 2, criterion 4 needs 3, and criterion 5 needs 2,
3 and 4. Criterion 6 needs 1; criteria 7 and 8 need 6; criterion 9 needs 6, 7 and 8; criterion 10
needs 9 and criterion 11 needs 10. Criterion 12 needs all eleven. The spec is launched on
rigger-run.

**THE INVARIANT, amended here so no unit has to.** Section 2.1 of
`docs/architecture-addendum-context-management.md` and its non-goal "Do NOT prune the event log"
now read: the log is the source of truth for KNOWLEDGE, the tree for STRUCTURE, and git retains a
finished run's EPISODES; the graph is rebuildable from the log and the tree. Criterion 1's unit
edits both passages and says why in one paragraph.

**THREE CLASSES OF EVENT, decided by type.** A new domain module, `retention`
(`crates/rigger-domain/src/retention.rs`), holds the classes. DERIVED is
`ingest::DERIVED_INDEX_TYPES`. EPISODIC is `retention::EPISODIC_TYPES`, the Notes list: the types
no cross-run fold reads and whose graph fold changes neither the live projection nor the fold
state (`FileTouched` and `GateVerdict` fold through no-op arms, the rest through none); only the
`applied` row of their position is written. KNOWLEDGE is
`retention::KNOWLEDGE_TYPES`: every type of `run::read::CARRY_OVER_TYPES`,
`run::read::ADOPTION_TYPES`, `run::MINT_DECISION_TYPES` and `run::RUN_CLOSURE_TYPES`; every type
outside the derived index whose fold arm changes the graph (`SpawnResult`, `AliasDefined`,
`AliasUnresolved`, `CommunityAssigned`, `ConceptDerived`, `ConceptRealized`); and the two new
types, `GenerationIngested` and `RunArchived`, whose `TYPE_` constants `retention` declares. The
offline pass results are knowledge because a pass reads the graph at one moment, so the tree
cannot re-derive them. `retention::class_of(type_)` answers DERIVED, then EPISODIC, and KNOWLEDGE
for every other type, so a type no list names (this store holds 43 `ReviewVerdict` events no
constant declares) is kept live: never refused, never archived. Every `TYPE_` constant under
`src/` and `crates/` sits in exactly one of the three lists, asserted by a source scan in
`tests/`, so a new type is classified the day it is added.

**PERCEPTION IS A LEDGER ENTRY.** Both ingest sinks, the run's `RunCtx::emit_keyed_batch`
(`crates/rigger-conductor/src/conductor.rs`) and `rigger graph build`'s `ingest_tree`
(`src/cli/graph.rs`), append for each batch the first-sight check
(`ingest::batch_is_latest_recorded`) does not find recorded one `GenerationIngested` (Notes) and
no derived event. The entry names the batch's identity, its generation (the `<hash>` of its replay
keys), the git blob id of the file's bytes at ingest time (`git hash-object`, never written; empty
when the path holds no file, as for a deleted file's boundary batch) and whether the code walk
lowered the file as an out-of-line test module (`out_of_line_test_module_files`). It carries
`eventstore::META_GROUP` (the identity) and the replay key `<prefix>/<file>@<generation>#<n>`,
where n is the batch's event count and so no event of the batch carries that key. The group
lookup (`EventStore::latest_in_group`) therefore answers a ledger entry on both backends, and
`ingest::latest_generation`'s type-first check admits the ledger type beside the derived types.
`ingest::project_scoped_latest_generations` reads ledger entries too, and `graph_index_lag`
compares a file's current generation with the recorded one.

**THE ENTRY AND ITS BATCH FOLD AS ONE.** The `Projection` port gains
`apply_generation(entry, batch)`: one transaction and one `applied` row at the entry's position,
each batch event folded at the entry's position and valid-time under its own replay key, so spec
101's generation rule applies unchanged. `FoldingStore` gains the ledger form of its
append-then-fold body; both sinks use it, and a ledger entry reaches the graph through no other
fold. `rigger emit` already refuses both new types (`EMITTABLE_TYPES`). A process that dies
between the append and the fold leaves a hole in the `applied` ledger that `rigger setup` pays, as
spec 101 decided.

**THE REBUILD RE-EXTRACTS THE LEDGER.** `rigger setup`'s rebuild (`rebuild_owed_graph`,
`src/cli/setup.rs`, over `Projector::rebuild`) folds the log's live selection as spec 101 decided
and hands each `GenerationIngested` to `apply_generation` with the batch it re-extracts. The bytes
come from the repository's object database by blob id, through one `git cat-file --batch` process
per rebuild, else from the tree's file at that path when its bytes hash to that id. One function
per half takes `(path, bytes, excluded)` and returns the batch the walk would key:
`extract::extract`, then `extract_events` and `proof_events`, for `gc`; `file_batch`'s concept and
link extraction for `gd`; for `gw`, the parse `config_store::load_workflow` applies to the file's
bytes (`parse_yaml_naming_unknown_keys`, then the stage names), split out as a bytes form that
`load_workflow` keeps calling after its file read, then `workflowdef::extract_events`, which reads
nothing else. The `gc` half resolves the language from the path's extension, so a file indexed
under a `--language` override re-extracts to a different generation and is counted unresolved. The
walk and the rebuild both call it, and `key_batch` keys its output. The sinks fold each batch at
its entry's position and valid-time, so a rebuild that resolves every entry folds the events the
sinks folded, in the same order, and reaches the same projection; criterion 3's fixture records no
`RunStarted`, so the rebuild's run-closure prune drops nothing from either side. An entry whose
bytes resolve nowhere, or whose re-extracted generation differs from the recorded one, folds as a
generation with an empty batch: the prior generation's facts retire, as when a file extracts to
nothing, and the rebuild prints how many entries it folded so and how many of those are their
identity's latest entry. A fact first asserted by such an entry carries a later generation's
valid-time. An unresolved latest entry whose bytes the tree no longer holds is superseded by the
next ingest's newer generation of that file. The light lane compiles no extraction, so every entry
folds unresolved and is counted. The rebuild reads no archive, since an episodic event changes
neither the live projection nor the fold state (criterion 1), and the `applied` rows of archived
positions are outside spec 101's comparison surface and owe nothing once the log no longer holds
them.

**MIGRATION SHEDS THE BACKLOG.** `rigger reset --derived` keeps its two refusals (a live writer,
`refuse_derived_reset_if_live`; a `graph.db` that owes its rebuild) and becomes the one-time
migration. For every identity whose latest recording (`ingest::latest_generation`) is a derived
event it records a `GenerationIngested` naming that generation, the blob id of the tree's file at
that path (empty when the path holds no file) and `excluded: false`; an out-of-line test module's
recorded batch is the boundary sentinel, which asserts no fact, so a rebuild that counts it
unresolved holds the same projection. It refuses, naming the identity, when `graph.db`'s current
generation for an identity differs from the log's latest. It folds each entry as a re-recording of
its identity's current generation (one `applied` row, no fact changes), then deletes every derived
event, keyed or unkeyed, in one transaction through the store path `prune_derived_index` uses
(`crates/rigger-store-sqlite/src/eventstore/sqlite.rs`), and reports the entries recorded and the
events shed. A rerun, or a run resumed after a crash between the entries and the delete, finds
each identity's latest recording is its ledger entry, records nothing more and sheds what remains;
with nothing left it says there is no derived event to shed. `--derived` no longer runs spec 101's
generation compaction, whose tests criterion 4's unit deletes or re-homes; the live selection a
rebuild folds (`read_live_selection`) is unchanged, so on a store not yet migrated it still holds
each identity's latest derived generation. Sqlite only, as `--derived` is today. This store
holds no unkeyed derived event.

**THE STORE REFUSES A DERIVED APPEND.** Both adapters' `append` (`sqlite.rs` and `kurrentdb.rs`
under `crates/rigger-store-sqlite/src/eventstore/`) refuse a batch holding any DERIVED event,
through `retention::class_of`, naming the type and writing no event of the batch. The refusal
lives at the adapters, the seam every writer passes; a check in a caller, a wrapper or a test
double is NOT this refusal. Reading recorded derived rows is unchanged. A test that needs a store
written before this spec inserts its derived rows with raw SQL into the sqlite file through one
helper per test boundary that needs one (the root `tests/common/`, a crate's `test_support`),
built by criterion 4's unit for its own boundary; criterion 5's unit adds the others and moves
every other test that records a derived event through a store onto them.

**CRITERIA 2 AND 5 SPLIT AT THE APPEND.** Criterion 2 stops both sinks appending derived events;
criterion 5 refuses them at the store, after criterion 4 ships the migration, so the binary that
refuses a derived append can always migrate a store that holds them.

**A FINISHED RUN'S EPISODES MOVE TO GIT.** A run is the span of the run stream from its
`RunStarted` to the next `RunStarted`, the boundary `run::current_run` applies; events before the
first `RunStarted` belong to no run. A run is ARCHIVABLE when a later `RunStarted` exists and no
spawn requested in its span is live by `liveness::live_spawns`. One domain use case,
`archive::archive_run` (`crates/rigger-domain/src/archive.rs`), archives one run in this order: it
reads the run's episodic events, serializes them in position order (Notes), writes the bytes as a
git blob under `refs/rigger/archive/<run-id>` through a new `RunArchive` port, reads them back
through the ref and compares them byte for byte, appends one `RunArchived` (Notes), then deletes
exactly those positions in one transaction through a new port method
`EventStore::delete_archived(stream, positions)`. Knowledge events stay. A run with no episodic
event writes no ref and records a `RunArchived` of zero events, so the archiving of every finished
run is a log fact, and a run with a `RunArchived` and no live episodic event is not archived again.
The blob id is the archive's digest. No step or one-shot fold reads an earlier run's episodic event
(spec 101: the run slice from its boundary, the knowledge types by type) and an episodic event
changes no graph state a rebuild compares, so archiving one changes no fold. Every other production
reader of the run stream reads the current run's slice (`run::read::read_current_run`), a knowledge
type by type, or a store other than the project's run stream (the critique store, the canary
stream, `progress.db`); the two archive readers are below. `rigger validate`'s order-signature
advisory (`watch::order_signatures` over `read_all`) is the one other reader that sees an earlier
run's episodic rows: deleting rows only lowers a stream's running revision maximum, so it never
flags a remaining row and stops reporting only rows the live store no longer holds. `RunArchive` is
declared in the domain beside `EventStore`; its one adapter, in `crates/rigger-worktree-git`, runs
`git hash-object -w --stdin`, `git update-ref` and `git cat-file blob`. Git compresses its objects,
so no compression crate is added (`Cargo.lock` carries none). Git is the retention system: the ref
is local until the operator pushes it, and nothing rigger does deletes one. BACKEND SCOPE: the
sqlite store deletes; the KurrentDB adapter's `delete_archived` answers an unsupported-operation
error, and archiving there is skipped with a line naming the backend. A project outside a git
repository is skipped with a line saying so. Every `EventStore` implementation (both adapters,
`Namespaced`, `FoldingStore`, the test doubles) gains `delete_archived` in criterion 6's unit.

**AN INTERRUPTED ARCHIVE COMPLETES.** The serialization is a pure function of the rows, so the
next archive of an interrupted run re-serializes its live episodic events to the same blob: a ref
already naming that blob is left as it stands, a `RunArchived` already naming it is not appended
again, and the delete then runs. A run whose `RunArchived` names a blob its live episodic events do
not serialize to is refused by name and nothing is deleted. The delete is one transaction, so no
interruption leaves part of a run deleted.

**THE ARCHIVE IS READ ON DEMAND.** `archive::read_archived(run)` resolves the ref a `RunArchived`
names, checks that it names the recorded blob, parses the rows and yields the events in position
order; a missing ref or a different blob is an error naming the run and the ref. Criterion 6 owns
it, because the archive's own read-back uses it. `rigger replay <run>` (`cmd_replay`,
`src/cli/mod.rs`) merges an archived run's events with its knowledge events by position before it
slices the baseline (`baseline_run_slice`), and refuses naming the ref when it is missing.
`rigger stats --all` (`stats_lines`) folds every archived run's events with the live log and
prints one line naming each missing ref. No step, one-shot command or rebuild reads an archive.

**ARCHIVING RUNS AT TWO TRIGGERS.** The step lock (`.rigger/step.lock`, `acquire_step_lock`) is the
only serialization of archiving. One function, `archive_pending`
(`crates/rigger-driver/src/archiving.rs`), decides and archives the pending runs: it takes the
earlier runs that hold a `RunStarted` and no `RunArchived` (two typed reads of knowledge types),
leaves each that is not archivable and names it, and hands the rest to `archive::archive_run` in
position order. Every caller reaches it through one wiring helper in `src/cli/run.rs`,
`run_archiving(held)`, which pairs it with the git adapter and the lock rule: handed the step lock
its caller already holds, it archives; handed none, it takes the step lock without waiting, around
the archive alone, and releases it after; when another process holds the lock it archives nothing
and prints one line naming the held lock. The conductor's one trigger: at the start of every
`conductor::run`, once `run_store::ensure_started` or the driver's own pinned or fresh mint has
fixed the current run, it calls the archiving handle `Deps` carries, and with none wired it
archives nothing. `rigger step` (`cmd_step`) wires the helper's value with the step lock it holds
for the whole step; `rigger run` (`run_cli`) and `rigger serve` and `rigger workflow` (both
`run_workflow`) wire it with no lock; the replay's isolated re-drive and the canary wire none,
since they run on isolated stores. `rigger reset --runs` calls the same helper with the step lock
its probe takes (`live_writer_facts`), prints one line with the runs, events and bytes archived,
and keeps its graph prune; a reset that cannot take the step lock archives nothing and says so. A
driver that dies holding the lock releases it with its process, and the next trigger completes the
interrupted run (AN INTERRUPTED ARCHIVE COMPLETES). On a store that predates this spec, the first
trigger archives every earlier run once.

**CRITERIA 6, 9, 10 AND 11 SPLIT AT THE TRIGGER.** Criterion 6 builds the use case and calls it
from no production path. Criterion 9 adds `archive_pending`, the `Deps` handle and the conductor
trigger, exercised by a conductor its test wires; criterion 10 adds `run_archiving` and wires it
into the three drivers, which is when production archives; criterion 11 moves `rigger reset --runs`
onto the helper. All three land after criterion 8 moved `rigger replay` and `rigger stats --all`
onto the archive, so no tree archives a run its readers cannot read. A test that reads an earlier
run's episodic events from the live log after a later `RunStarted` is the unit's to move onto
`read_archived` whose change makes it fail.

**CONSTRAINTS WALK, decided.**
- *Revert:* a file reverted to earlier content records a new entry with that generation, and its
  blob resolves from the object database or the tree.
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the
  second entry folds as a re-recording, and a rebuild re-extracts both to the same batch.
- *Cold start:* nothing is carried in memory between processes; the first-sight check asks the
  store's group lookup.
- *Existing data:* a store written before this spec keeps its derived events readable, folds them
  as spec 101 decided and answers the first-sight check from them until `rigger reset --derived`
  migrates it; a `graph.db` built before this spec needs no rebuild, since no fold rule or
  projection version changes.
- *Concurrent drivers:* a `rigger step` holds the step lock for its whole step, so a `rigger run`
  starting meanwhile archives nothing and says so, and the step's own trigger archives the pending
  runs.
- *Archive revert:* an operator who deletes an archive ref loses that run's episodes and nothing
  else, and every reader names the missing ref.

**STATE PLACEMENT.** The ledger and the archive index are the log (`GenerationIngested`,
`RunArchived`); the class of a type is code (`retention`); the archived bytes are git; `graph.db`
is a projection. A sidecar file listing archived runs, an in-memory set of ingested generations, a
`.rigger/` marker or a cache of resolved blobs is NOT an implementation of any of them.

**OUT OF SCOPE.** Events before the first `RunStarted` (2,929 episodic events, 63 MB on this
store), `progress.db`, streams other than the run stream, the superseded ledger entries (each
stays live), archiving and migration on KurrentDB, a deleted `graph.db` (forbidden by
`crates/rigger-domain/src/docs.rs`, unchanged here) and everything of spec 108.

## Notes (non-criteria)

`GenerationIngested { prefix, file, generation, blob, excluded }`: `prefix` is `gc`, `gd` or `gw`;
`blob` is 40 lowercase hex digits, or empty when the path held no file; `excluded` is true only
for a `gc` batch lowered as an out-of-line test module.

`RunArchived { run, ref, blob, events, bytes, first, last }`: `ref` is
`refs/rigger/archive/<run>`; `events` and `bytes` count what the blob holds; `first` and `last`
are the lowest and highest archived positions. A run with no episodic event records an empty
`ref` and `blob`, zero `events` and `bytes`, and its `RunStarted`'s position as `first` and `last`.

The archive blob holds one JSON object per archived row, in position order, keys in this order:
`position`, `stream`, `type`, `id`, `data`, `meta`, `valid_from`, `recorded_at`, `revision`.
`data` is the row's bytes as a JSON string when they are UTF-8, else lowercase hex under
`data_hex`; `meta` is its JSON object; times are integer nanoseconds since the epoch.

`retention::EPISODIC_TYPES`: `SpawnRequested`, `GateVerdict`, `GatePromoted`, `GateDemoted`,
`UnitProposed`, `BlastRadiusComputed`, `ScopeCreep`, `FileTouched`, `SpecDefect`, `ManualReview`,
`DeferredGateFailed`, `TaskAborted`, `BudgetExhausted`, `AgentProgress`, `SpawnLaunched`,
`StopFailure`. The last three are recorded in `progress.db` and classified only for completeness.

`GenerationIngested` and `RunArchived` are the spec's two new event types; its whole point is the
shape of the log.

## Global constraints

- Hyphens, never em dashes, in every added line.
- Two new event types and no others; no new dependency.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).
- Nothing is deleted from the live store before its bytes are read back from git.
- Every archive and every rebuild is deterministic: the same rows and the same tree yield the
  same bytes.
- The gates cannot see the KurrentDB half of criteria 2 and 5 where the contract suite's container
  is unreachable; the adjudicator demands that run's evidence.

## Done when

- [ ] a test proves THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under `src/` or `crates/` declares sits in exactly one class list, and folding each episodic type leaves the live projection unchanged,
  asserted by a source scan under `tests/` and a fold test in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, with no type of the four cross-run
  lists classified episodic. This criterion OWNS `retention`, `class_of`, the two new type
  constants and the addendum amendment; what a class causes (the refusal, the archive) is
  criteria 5 and 6's, NOT this one's.
- [ ] a test proves PERCEPTION IS A LEDGER ENTRY: ingesting a changed file through either ingest sink appends one `GenerationIngested` naming its generation and blob and no derived event, while `graph.db` holds the file's facts,
  asserted through a recording store at `RunCtx::emit_keyed_batch` and at `ingest_tree`, with
  an unchanged file recording nothing and a deleted file recording an empty blob. This criterion
  OWNS both sinks' write path, `apply_generation`, the ledger form of `FoldingStore` and the
  ledger reading of the first-sight check and of `graph_index_lag`; the rebuild is criterion 3's
  and the store's refusal criterion 5's, NOT this one's.
- [ ] a test proves THE GRAPH REBUILDS FROM LEDGER AND TREE: a `graph.db` rebuilt through `rigger setup` from a log of ledger entries equals the one the ingest sinks built incrementally from that log,
  compared on spec 101's comparison surface (the live projection plus the fold state that decides
  future folds) over generations that drop a design link and a code entity, while an entry whose
  bytes resolve nowhere is counted in the rebuild's report. This criterion OWNS re-extraction,
  blob resolution and the unresolved count; the migration is criterion 4's, NOT this one's.
- [ ] a test proves MIGRATION SHEDS THE BACKLOG: `rigger reset --derived` on a store holding three generations of one file and a deleted file records a ledger entry per identity, sheds every derived event and reports both counts,
  leaving the live projection of `graph.db` unchanged and saying there is nothing to shed when run
  again, asserted in `tests/cli.rs`. This criterion OWNS the migration, its report and the
  pre-ledger row helpers; the rebuild that later reads the entries is criterion 3's, NOT this
  one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`). This criterion OWNS the refusal and
  moving every test that records a derived event through a store onto the pre-ledger row helpers;
  the sinks that stop emitting are criterion 2's, NOT this one's.
- [ ] a test proves A RUN'S EPISODES ARE ARCHIVED: archiving an archivable run writes its episodic events under `refs/rigger/archive/<run-id>` and records one `RunArchived`, then deletes exactly those events while its knowledge events stay,
  asserted against `archive::archive_run` over a sqlite store and a fixture git repository, with
  a run holding no episodic event writing no ref and a `RunArchived` of zero events, and an
  archived run not archived again. This criterion OWNS the use case, `read_archived`, the
  `RunArchive` port and its git adapter and `EventStore::delete_archived`; resuming an
  interrupted archive is criterion 7's and every trigger criteria 9, 10 and 11's, NOT this one's.
- [ ] a test proves AN INTERRUPTED ARCHIVE COMPLETES: an archive stopped after its ref was written, and one stopped after its `RunArchived` was recorded, are each completed by the next archive of the run,
  with no second ref, no second `RunArchived` and every episodic event of the run gone, asserted
  against `archive::archive_run` with a store double that fails on command. This criterion OWNS
  the resume and the refusal of a mismatched blob; the archive's first pass is criterion 6's, NOT
  this one's.
- [ ] a test proves THE ARCHIVE READS BACK: `rigger replay <run>` and `rigger stats --all` print, over a store whose earlier run is archived, the bytes of standard output they print over that store before archiving,
  and each names a missing archive ref, asserted in `tests/cli.rs`. This criterion OWNS both
  commands' archive reads; `read_archived` itself is criterion 6's, NOT this one's.
- [ ] a test proves A CONDUCTOR RUN ARCHIVES ITS PREDECESSORS: a `conductor::run` wired with an archiving handle archives every archivable earlier run of the run stream and leaves the current run's events live,
  asserted through `conductor::run` over a fixture repository, with an unwired conductor archiving
  nothing and a run with a live spawn left unarchived and named. This criterion OWNS
  `archive_pending`, the `Deps` handle and the conductor trigger; the drivers' wiring and the step
  lock are criterion 10's and the reset trigger criterion 11's, NOT this one's.
- [ ] a test proves EVERY DRIVER ARCHIVES UNDER THE STEP LOCK: `run_archiving` handed no lock archives a pending run when the step lock is free and releases it, and archives nothing with one line naming the lock when another process holds it,
  asserted in the tests of `src/cli/run.rs` and through `rigger step` in `tests/cli.rs`, with
  `cmd_step`, `run_cli` and `run_workflow` each wiring `Deps` through that helper. This criterion
  OWNS the helper, the three drivers' wiring and the lock rule; the conductor trigger is criterion
  9's and the reset trigger criterion 11's, NOT this one's.
- [ ] a test proves `rigger reset --runs` ARCHIVES: on a store whose earlier runs were never archived, it archives each archivable run, leaves a run with a live spawn unarchived naming it, and prints the archived totals,
  asserted in `tests/cli.rs`. This criterion OWNS the reset trigger, its report and the skill and
  handbook text that describes `rigger reset --runs`; the helper is criterion 10's and the
  conductor trigger criterion 9's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
