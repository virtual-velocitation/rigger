# 107 - Perception is re-derived and episodes are archived: the log keeps knowledge

**Goal:** rigger is a hive mind: its graph holds the project's whole understanding and serves each
persona the slice it needs, and the append-only log is only the persistence underneath. That log
keeps what the hive decided, learned and did; it stops holding what the hive merely perceived (the
derived index, which the tree re-derives) and how a finished run spent itself (its episodes, which
move to git). Measured on the 2026-10-05 store (288,636 events, 430 MB of payload, a 580 MB file):

- The derived index (`ingest::DERIVED_INDEX_TYPES`) is 229,500 events, 79.5% of the rows, recording
  753 generations of 688 files. Every ingest records a changed file's whole extracted batch: of the
  current run's 20,927 events, 19,946 are derived events for 29 file generations. `graph.db` folds
  each batch as it is appended (`FoldingStore::append_and_fold`,
  `crates/rigger-grounder/src/ingest.rs`), no one-shot command reads them (spec 101), and the store
  accepts a derived append from any caller.
- A run's mechanics, the episodic types (Notes), are 21,386 events and 348 MB, 81% of the payload;
  `SpawnRequested` alone is 7,242 events and 343 MB of prompts. 17,988 of them (278 MB) belong to
  the 153 runs before the current one and 2,929 (63 MB) to the run stream before its first
  `RunStarted`, and nothing removes them: `rigger reset --runs` (`reset_runs`, `src/cli/hygiene.rs`)
  prunes `graph.db` and deletes no event.
- The knowledge the hive keeps, every other type, is 37,750 events and 58 MB. Once this spec lands,
  the live log holds that knowledge, the ledger and the current run's episodes, and nothing else.

## Design

**UNIT ORDER AND BASE, decided here.** Criterion 1 needs nothing. Criterion 2 needs 1 (it folds the
type criterion 1 declares) and criterion 3 needs 2. Criteria 4 and 5 need 3. Criterion 6 needs
nothing. Criterion 7 needs 4 and 6. Criteria 8 and 9 need 7 alone: criterion 7 owns
`Store::count_derived` beside `Store::shed_derived`, the one selection both read. Criterion 10 needs
7. Criterion 11 needs 1; criteria 12 and 13 need 11; criterion 14 needs 11, 12 and 13; criterion 15
needs 14; criterion 16 needs 15; criterion 17 needs 7 and 16, and criterion 18 needs 17, since 7, 17
and 18 rewrite the `rigger-reset-store` skill in turn. Criterion 19 needs all eighteen. The spec is
launched on rigger-run. A test that fails because of a unit's change is that unit's to move,
whichever criterion owns the surface it asserts. Each tree between two landings exists only on the
run branch: on the one that holds criterion 3 and not 4, an owed `rigger setup` rebuild folds each
`GenerationIngested` through no arm and lacks the facts ingested since, until criterion 4; on the
one that holds 7 and not 8 or 9, the menu's `--derived` line and the bloat advisory still word the
generation compaction through `plan_derived_prune`, which stays for the live selection. Criterion 10
refuses a derived append only after criterion 7 ships the migration, so the binary that refuses one
can always migrate a store that holds them. Production archives only from criterion 16 on, after
criterion 13 moved `rigger replay` and `rigger stats --all` onto the archive, so no tree archives a
span its readers cannot read.

**THE INVARIANT, amended here so no unit has to.** Section 2.1 of
`docs/architecture-addendum-context-management.md` (its sentence on `reset --runs` included) and its
non-goal "Do NOT prune the event log" now read: the log is the source of truth for KNOWLEDGE, the
tree for STRUCTURE, and git retains a finished run's EPISODES; the graph is rebuildable from the log
and the tree. Criterion 1's unit edits those passages and says why in one paragraph.

**THREE CLASSES OF EVENT, decided by type.** A new domain module, `retention`
(`crates/rigger-domain/src/retention.rs`), holds the classes. DERIVED is
`ingest::DERIVED_INDEX_TYPES`. EPISODIC is `retention::EPISODIC_TYPES`, the Notes list: the types no
cross-run fold reads and whose graph fold changes neither the live projection nor the fold state
(`FileTouched` and `GateVerdict` fold through no-op arms, the rest through none); only the `applied`
row of their position is written. KNOWLEDGE is `retention::KNOWLEDGE_TYPES`: every type of
`run::read::CARRY_OVER_TYPES`, `run::read::ADOPTION_TYPES`, `run::MINT_DECISION_TYPES` and
`run::RUN_CLOSURE_TYPES`; every type outside the derived index whose fold arm changes the graph
(`SpawnResult`, `AliasDefined`, `AliasUnresolved`, `CommunityAssigned`, `ConceptDerived`,
`ConceptRealized`); and the two new types, `GenerationIngested` and `RunArchived`, whose `TYPE_`
constants `retention` declares. The offline pass results are knowledge because a pass reads the
graph at one moment, so the tree cannot re-derive them. `retention::class_of(type_)` answers
DERIVED, then EPISODIC, and KNOWLEDGE for every other type, so a type no list names (this store
holds 43 `ReviewVerdict` events no constant declares) is kept live: never refused, never archived.
Every `TYPE_` constant under `src/` and `crates/` sits in exactly one of the three lists, asserted
by a source scan in `tests/`, so a new type is classified the day it is added.

**THE ENTRY AND ITS BATCH FOLD AS ONE.** The `Projection` port gains `apply_generation(entry,
batch)`, where `batch` is a function it calls at most once and which answers the entry's batch or
that it is unresolved. One transaction writes one `applied` row at the entry's position. An entry
naming its identity's current generation in `graph.db` is a RE-RECORDING: it writes only that row,
never calls `batch` and changes no fact. Any other entry calls `batch`. A resolved batch folds each
event at the entry's position and valid-time under its own replay key, so spec 101's generation rule
applies unchanged, and installs the entry's generation as current. An unresolved batch retires the
prior generation's facts, as a batch that extracts to nothing does, and installs NO current
generation for the identity. `Projection::current_generation(identity)`, a new port read of the
`generations` table, answers that current generation. `FoldingStore` gains the ledger form of its
append-then-fold body, and a ledger entry reaches the graph through no other fold. `rigger emit`
already refuses both new types (`EMITTABLE_TYPES`). A process that dies between the append and the
fold leaves a hole in the `applied` ledger that `rigger setup` pays, as spec 101 decided. Criterion
2 owns this fold rule.

**PERCEPTION IS A LEDGER ENTRY.** Both ingest sinks, the run's `RunCtx::emit_keyed_batch`
(`crates/rigger-conductor/src/conductor.rs`) and `rigger graph build`'s `ingest_tree`
(`src/cli/graph.rs`), record a `GenerationIngested` (Notes) through the ledger form of
`FoldingStore` and no derived event. A sink skips a batch only when it is CURRENT:
`ingest::batch_is_current(store, graph, keyed)`, which replaces `ingest::batch_is_latest_recorded`
at both sinks, answers true only when the log's latest recorded generation of the batch's identity
(`ingest::latest_generation`) and `graph.db`'s current generation of it
(`Projection::current_generation`) both equal the batch's generation, so an identity an unresolved
fold left with no current generation is never current. The run's sink asks once per identity per
process, as `ReplayKeys::install` does today. For a batch that is not current the sink reads the
file's bytes once, hashes them (`git hash-object`, never written) and extracts them through its
half's `(path, bytes, excluded)` function (THE REBUILD block), keyed by `key_batch`; when that
extraction is itself current nothing is recorded, and otherwise the sink records one entry naming
that extraction's generation, that blob and the walk's out-of-line flag
(`out_of_line_test_module_files`) and folds that extraction, never the walk's batch (for `gc`, the
persisted symbols index's lowering). A path that holds no file records an empty blob and the batch
the function returns for no bytes (for `gc`, the boundary sentinel
`empty_structural_boundary_event`). An index that lags the disk can key a changed file at a
generation both sides hold, so the walk records nothing for it until the index is refreshed (`rigger
reindex`, or an integration's own reindex); criterion 5's advisory names that file. The entry
carries `eventstore::META_GROUP` (the identity) and the replay key
`<prefix>/<file>@<generation>#<n>`, where n is the batch's event count and so no event of the batch
carries that key. The group lookup (`EventStore::latest_in_group`) therefore answers a ledger entry
on both backends, and `ingest::latest_generation`'s type-first check admits the ledger type beside
the derived types. Criterion 3 owns the sinks.

**THE REBUILD RE-EXTRACTS THE LEDGER.** `rigger setup`'s rebuild (`rebuild_owed_graph`,
`src/cli/setup.rs`, over `Projector::rebuild`) folds the log's live selection as spec 101 decided
and hands each `GenerationIngested` to `apply_generation` with a function that re-extracts its
batch. One function per half takes `(path, bytes, excluded)` and returns the batch the sinks key:
`extract::extract` under the grammar `registry::for_path` resolves from the path's extension, then
`extract_events` and `proof_events`, for `gc`; `file_batch`'s concept and link extraction for `gd`;
for `gw`, the parse `config_store::load_workflow` applies to the file's bytes
(`parse_yaml_naming_unknown_keys`, then the stage names), split out as a bytes form that
`load_workflow` keeps calling after its file read, then `workflowdef::extract_events`, which reads
nothing else. The sinks and the rebuild both call it. RESOLUTION IS BY GENERATION: the function
takes the entry's bytes from the repository's object database by blob id (one `git cat-file --batch`
process per rebuild), else from the tree's file at that path, else no bytes for an empty blob, and
the entry resolves when the extraction of those bytes yields the recorded generation; the blob is
where to look first, never the test. So an entry recorded from uncommitted bytes still resolves
while the tree's file extracts to its generation, a prose-only edit included. An entry no source
resolves folds unresolved, and the rebuild prints how many entries it folded so, how many of those
are their identity's latest entry, and that the next default-lane ingest that walks those files
(`rigger graph build`, or a run's project-ingest pass) restores them: the identity holds no current
generation, so `batch_is_current` is false and the entry the sink records from the file's bytes
folds resolved; an identity whose file the tree no longer holds stays retired, the tree's state. The
light lane compiles no extraction, so every entry that is not a re-recording folds unresolved and is
counted. The rebuild reads no archive (an episodic event changes neither the live projection nor the
fold state, criterion 1), the `applied` rows of archived or shed positions are outside spec 101's
comparison surface, and an archive delete landing between the rebuild's position read
(`read_live_positions`) and its selection read (`read_live_selection`) removes only positions whose
fold changes no fact, so the rebuild needs no lock beyond `graph.db.lock`.

**WHAT A REBUILD REPRODUCES, stated once.** For every identity whose latest entry resolves, a
rebuild reaches the same live facts and the same current generation as the incremental folds. A
derived fact's valid-time is that of the first entry, in log order, of the unbroken run of resolved
entries that assert it up to the latest, and its recorded position (the graph's `source`, which
`assert_link` keeps at the newest recording) is that of the newest entry of that run that folded its
batch, since a re-recording folds none. Both equal the incremental graph's when every entry of the
identity resolves and one process recorded each of its generations, and there the rebuild equals the
incremental graph on spec 101's comparison surface (the live projection plus the fold state that
decides future folds). Its instances are the only accepted differences:

- *An unresolved superseded entry:* a fact re-asserted across it takes the valid-time and position
  of the next resolved entry, since the unresolved fold retired it.
- *A migrated identity:* its facts take the identity's earliest pre-ledger valid-time and the
  position of its latest batch's first row (MIGRATION), where the incremental graph kept each fact's
  own run start and newest row.
- *Two entries of one generation:* append and fold are not atomic across processes, so the
  incremental graph dates the facts by whichever entry folded first, and a rebuild by the lower
  position.
- *An identity restored after an unresolved latest entry:* its facts take the restoring entry's
  valid-time and position.
- *Facts only an unkeyed pre-ledger derived event asserted:* the live graph keeps them after the
  migration sheds that event, and the first rebuild retires them, since no entry asserts them.

**THE LEDGER ANSWERS THE INDEX-LAG ADVISORY.** `rigger validate`'s graph index-lag advisory
(`read_graph_index_lag`, `src/cli/validate.rs`) reads the run stream through
`EventStore::read_stream_typed` with `TypeSelection::Only` of the derived types and
`GenerationIngested`, never the whole stream, and opens `graph.db` read-only as
`retired_entities_advisory_for` does. `ingest::project_scoped_latest_generations` answers each
identity's latest generation from a derived event's key or a ledger entry's generation, and
`graph_index_lag` names a sampled file when the generation of its current bytes through its half's
`(path, bytes, excluded)` function differs from that generation or from `graph.db`'s current
generation (`Projection::current_generation`), the two sides the sinks' check reads; a generation
hashes the whole batch, so equal generations are equal key sets. So a file an unresolved fold left
with no current generation is named. The advisory's wording is unchanged. Between the landings of
criteria 3 and 5 the advisory, reading only derived events, names a file changed since as lagging;
that changes no exit status.

**MIGRATION CONVERTS THE BACKLOG IN PLACE.** `rigger reset --derived` keeps its two refusals (a live
writer, `refuse_derived_reset_if_live`; a `graph.db` that owes its rebuild) and becomes the one-time
migration. It appends nothing, folds nothing and writes nothing to `graph.db`. In ONE sqlite
transaction (`Store::shed_derived`), for every identity whose latest recording (a derived row or a
ledger entry, keyed alike) is a derived row, it rewrites IN PLACE the lowest-position row of that
latest generation's unbroken run (its rows above the last row of any other generation): position,
stream, id, revision and recorded-time stay; type, data and meta become the identity's
`GenerationIngested` (that generation, the blob id of the tree's file at that path, `excluded:
false`, its group and replay key); the valid-time becomes the earliest among the identity's recorded
rows. It then deletes every remaining row of a derived type under the project's prefix, keyed or
unkeyed; the same selection backs the read-only count (`Store::count_derived`: rows, unkeyed rows
and identities). Identity and generation are cut from each row's replay key
(`ingest::derived_key_parts`, as `plan_derived_prune` cuts them), never from its group: 42,563 of
this store's keyed derived rows carry no `META_GROUP`. `prune_derived_index` makes the same class of
write today (`UPDATE events SET valid_from = ?2 WHERE position = ?1` in one `BEGIN IMMEDIATE`
transaction), and nothing refuses it: the primary key and the stream and revision `UNIQUE(stream,
revision)` covers are kept, and `id` carries no uniqueness rule. The kept row already has its
`applied` row, and a batch is one append with no knowledge event inside it, so a rebuild folds the
entry where the live graph first folded that generation, on the same side of every alias and
knowledge event (its dating is an instance of WHAT A REBUILD REPRODUCES). On this store: 688 rows
rewritten (for the 8 identities that recorded their latest generation more than once, the first
recording's first row) and 228,812 deleted, with the write lock held 1.9 s on a copy, inside the
5000 ms busy timeout an appender such as an operator's `rigger emit` waits. A crash rolls the
transaction back and a rerun starts over; a rerun after success says there is no derived event to
shed. A path the tree does not hold gets an empty blob: unless its generation is the batch for no
bytes (a deletion already ingested, which resolves), the entry resolves nowhere at a rebuild, and
the next ingest naming the path (an integration's own reindex, `file_batches`) finds the batch for
no bytes not current under `batch_is_current` and retires the identity's facts. An out-of-line test
module's recorded batch is the boundary sentinel, which asserts no fact, so its entry resolving
nowhere changes none. An unkeyed derived event names no identity (`keyed_derived_event`), so no
entry re-asserts it; this store holds none. The migration reports the entries converted, the rows
shed and, on its own line, the unkeyed rows shed, then reclaims space (`Store::reclaim_space`) and
reports the bytes reclaimed. `prune_derived_index` and its generation compaction go, criterion 7's
unit deleting or re-homing its tests; the live selection a rebuild folds (`read_live_selection`,
over `plan_derived_prune`) is unchanged, so a store not yet migrated still folds each identity's
latest derived generation. Sqlite only, as today. The `--derived` text says the same: the usage text
in `src/main.rs`, the flag list `reset_modes` prints, the `rigger-reset-store` skill (procedure and
anti-move) and the "Event log hygiene" section of `skills/using-rigger/SKILL.md` and
`docs/handbook/using-rigger.md` and the `--derived` guidance in `crates/rigger-domain/src/docs.rs`
describe a one-time migration that converts each file's latest batch into a ledger entry and leaves
no derived event behind.

**RECLAMATION STAGES IN MEMORY.** One sqlite `Store` method, `Store::reclaim_space`, split out of
the reclamation `prune_derived_index` runs after its commit today (`compact_in_place`, its on-disk
before and after and its failure report), is the only reclamation; criterion 6 owns it, and
`prune_derived_index` calls it until criterion 7 removes that function. It sets `PRAGMA temp_store =
MEMORY` on its own connection before its `VACUUM`, so the copy SQLite stages is held in the
process's memory, never in its temporary directory (`SQLITE_TMPDIR`, else `TMPDIR`, else `/var/tmp`,
which can be a small partition): no setting is process-global, a crash leaves nothing behind and no
reaper is needed. It rewrites only a file that holds free pages, so a rerun reclaims what a skipped
reclamation left. Measured on a copy of this store after the migration's and the first archive's
deletes, with both temporary directory variables naming a directory that does not exist: 0.44 s, 580
MB to 92 MB, 94 MB more peak memory (about the compacted size: the knowledge, the ledger and one
run's episodes), a 92 MB write-ahead log beside the store and no file opened outside the store's
own. Without that memory the `VACUUM` rolls back, the deletes stay, and the verb prints the failure
beside its counts. `VACUUM INTO` copied back into the live file is NOT this method: an append
landing between the snapshot and the copy would be lost. Both verbs that call it hold the step lock
their probe took, under live-writer facts that are dead (`LiveWriterFacts::reasons` empty), so two
reclaimers never meet; `--derived --force-live` skips the probe and its lock as today, at the
operator's risk. An appender that arrives anyway waits for the `VACUUM`'s write lock inside the busy
timeout (5000 ms, `crate::sqlite::open_connection`).

**THE OPERATOR IS TOLD WHAT THE MIGRATION SHEDS.** The bare `rigger reset` menu's `--derived` line
(`reset_menu`, `derived_menu_line`) reads `Store::count_derived`, so its count never drifts from
what `--derived` deletes: on a store holding derived events it names the events and files the
migration sheds into ledger entries and the flag; on a store holding none it says there is no
derived event to shed; a server-backed store's line is unchanged. `count_derived_duplicates` and
`DerivedPreview` go. `rigger validate`'s log-bloat advisory (`bloat_advisory_for`) reads the same
count: a store holding any derived event is named with the events and files left and `rigger reset
--derived` as the migration; a store holding none prints nothing. `measure_derived_duplication`,
`DerivedDuplication` and `BLOAT_DUPLICATION_THRESHOLD` go. The menu's `--runs` line keeps previewing
the graph prune alone; archiving is not previewed.

**THE STORE REFUSES A DERIVED APPEND.** Both adapters' `append` (`sqlite.rs` and `kurrentdb.rs`
under `crates/rigger-store-sqlite/src/eventstore/`) refuse a batch holding any DERIVED event,
through `retention::class_of`, naming the type and writing no event of the batch. The refusal lives
at the adapters, the seam every writer passes; a check in a caller, a wrapper or a test double is
NOT this refusal. A contract test whose property is about the store (group lookup, ordering, any
type) is re-expressed over `GenerationIngested` entries and keeps running on both backends;
criterion 3's unit does this for the group lookup's reference test, since its ledger reading is what
that test now pins. A test that needs rows written before this spec runs on sqlite only, inserting
them with raw SQL through one PRE-LEDGER ROW HELPER per test boundary (the root `tests/common/`, a
crate's `test_support`). The first unit whose change needs a helper at a boundary builds it there,
and later units reuse it: criterion 3 builds the root one, since its sinks leave the index-lag,
`--derived`, menu and bloat tests of `tests/cli.rs` without derived rows, and moves those tests onto
it; criterion 10 moves every other test that records a derived event through a store onto a helper,
building any boundary's that is still missing. No KurrentDB store is migrated by this spec, so no
KurrentDB test seeds pre-ledger rows.

**A FINISHED RUN'S EPISODES MOVE TO GIT.** A run is the span of the run stream from its `RunStarted`
to the next `RunStarted`, the boundary `run::current_run` applies. The PRELUDE is the span of the
run stream below the first `RunStarted`, which belongs to no run and is never the current run. A
span (a run or the prelude) is PENDING when it holds a live episodic event and a `RunStarted` above
it exists, whatever its `RunArchived` state, so a span interrupted after its `RunArchived` and a
span refused stay pending; a store with no `RunStarted` has no pending span. A pending span is
ARCHIVABLE when none of its spawns is live by `liveness::live_spawns` (criterion 14; the prelude's
spawns are judged under an empty run id, so a spawn with no marker is not live) and no live driver
registration of this store exists other than the archiving caller's own (criterion 16). One domain
use case, `archive::archive_run` (`crates/rigger-domain/src/archive.rs`), archives one span as one
read hands it, in this order: it serializes the span's episodic events in position order (Notes),
writes the bytes as a git blob under the span's ref through a new `RunArchive` port, reads them back
through the ref and compares them byte for byte, appends one `RunArchived` (Notes) unless the span's
latest `RunArchived` already names that blob, then deletes exactly those positions in one
transaction through a new port method `EventStore::delete_archived(stream, positions)`. The delete
holds the store's write lock for that transaction: measured on a copy of this store as the port
method issues it, the prelude's 2,929 rows (63 MB) held it 0.27 s and the largest run's 2,019 rows
(12 MB) 0.07 s, so an appender that arrives meanwhile, such as a worker's `rigger emit` or `rigger
result`, waits inside the 5000 ms busy timeout and lands. A span is identified by the position of
the `RunStarted` that opens it, a value the log guarantees unique and well formed, never by its run
id, which the log does not: a run's ref is `refs/rigger/archive/run/<position>`, written at the
fixed width Notes gives so the refs list in run order, and its `RunArchived` group is keyed by the
same position; the prelude's ref is `refs/rigger/archive/prelude`, with its own group, and no
decimal position equals `prelude`. Knowledge events stay. A span holding no episodic event is not
pending: it writes no ref and no event. A failed ref write or read-back deletes nothing and appends
no `RunArchived`; the span stays pending, costs one span read per trigger, and every trigger names
it with git's error. The blob id is the archive's digest. Archiving changes no fold: no step or
one-shot fold reads an earlier run's episodic event (spec 101). THE READER AUDIT: the readers of
`run::read::read_current_run` read the current run's slice and knowledge types by type; the critique
store, the canary stream and `progress.db` are other stores. `cmd_playbooks`, `stats_lines` without
`--all`, `read_model_drift`, `reset_menu`, `refuse_derived_reset_if_live`, `read_run_units_or_why`
and `read_order_signatures` materialize the whole run stream and use only knowledge types or the
current run, so archiving shrinks what they read and changes none of their answers (the
order-signature advisory's included: deleting rows only lowers a stream's running revision maximum,
so it flags no remaining row). Four whole-stream reads change: `cmd_replay` and `stats_lines --all`
read the archive (criterion 13), `read_graph_index_lag` reads by type (criterion 5), and
`reset_runs` reads by type (criterion 17). `RunArchive` is declared in the domain beside
`EventStore`; its one adapter, in `crates/rigger-worktree-git`, runs `git hash-object -w --stdin`,
`git update-ref` and `git cat-file blob`. Git compresses the blob, so no compression crate is added.
Git is the retention system: the ref is local until the operator pushes it, and nothing rigger does
deletes one. BACKEND SCOPE: the sqlite store deletes; the KurrentDB adapter answers each of the
three archive port methods (`delete_archived` and the two reads below) with an unsupported-operation
error. Every `EventStore` implementation (both adapters, `Namespaced`, `FoldingStore`, the test
doubles) gains the three methods in criterion 11's unit.

**FINDING A PENDING RUN READS THE INDEX, NOT THE BACKLOG.** Two new port reads, both criterion 11's:
`EventStore::first_of_types(stream, from, types)` answers the revision of the oldest event at or
after revision `from` whose type is named, never its data (on sqlite one seek of
`idx_events_stream_type` per type), and `EventStore::read_span_typed(stream, from, to, selection)`
reads the events at revisions `from` up to `to` that `selection` admits, in position order.
`archive_pending` finds the current run's boundary with `last_position(RunStarted)` (none: nothing
is pending) and the first run's with `first_of_types(0, [RunStarted])`, then the oldest episodic
event from revision 0: none, or one at or past the current boundary, means nothing is pending; one
below the first boundary makes the prelude pending (criterion 15; before it, the search starts at
the first boundary); otherwise it steps `first_of_types(.., [RunStarted])` forward to the run whose
span holds that event. It reads that span alone through `read_span_typed` with
`TypeSelection::Except` of the derived types, archives it or names why not, and asks again from the
span's upper boundary. A trigger with nothing pending costs three index reads, a backlog is held one
span at a time, and each span left pending (a live spawn, a refusal) adds one span read per trigger.

**AN INTERRUPTED ARCHIVE COMPLETES.** The serialization is a pure function of the rows, so the next
archive of an interrupted span re-serializes its live episodic events to the same blob. A ref
already naming that blob is left as it stands, and the resume reads it back and compares it byte for
byte before its delete, as the first pass does. A `RunArchived` carries `META_GROUP` (the ref's name
below `refs/rigger/`) and a replay key naming its blob (Notes), so the group lookup answers the
span's latest `RunArchived` and its blob without reading the stream: one naming the same blob is not
appended again, and one naming another blob is followed by a new one, which readers and validate
take as the span's. A span whose ref already names a blob its live episodic events do not serialize
to is REFUSED: nothing is written or deleted, and every trigger names it in one line with the ref,
the blob that ref names, and the exit, one command that moves the ref aside so the span archives
afresh at the next trigger while the old blob stays recoverable under the new name: `printf 'create
refs/rigger/aside/<blob> <blob>\ndelete <ref> <blob>\n' | git update-ref --stdin`. Until then it
costs one span read per trigger. Since each span has its own position, no other span's archive can
meet its ref, so this refusal is the only one. The delete is one transaction, so no interruption
leaves part of a span deleted. The prelude resumes and refuses exactly as a run does.

**THE ARCHIVE IS READ ON DEMAND.** `archive::read_archived(archived)` resolves the ref a span's
latest `RunArchived` names, checks that it names the recorded blob, parses the rows and yields the
events in position order; a missing ref or a different blob is an error naming the span and the ref.
Criterion 11 owns it, because the archive's own read-back uses it. Both readers merge archive rows
and live rows by position and take a position present on both sides once, the live row, so a span
that is archived and not yet deleted (between an append and its delete, after an interruption, or
refused) is counted once. `rigger replay <run>` (`cmd_replay`, `src/cli/mod.rs`) resolves the run id
to its `RunStarted`, the first match as `baseline_run_slice` takes it today, then to its span's
latest `RunArchived`, merges the archived events with the run's live events before it slices the
baseline, and refuses naming the ref when it is missing; a later `RunStarted` sharing that run id is
not addressed by id, as today, and replay never reads the prelude's archive, which no run id names
and no run's slice reaches. `rigger stats --all` (`stats_lines`) merges every archived span's
events, the prelude's included, with the live log and prints one line naming each missing ref.
Neither command folds a `RunArchived` or a `GenerationIngested`: `metrics::project`
(`crates/rigger-domain/src/metrics.rs`), which both fold, ignores every type it does not name, and
`replay_trajectory` keeps only `SpawnResult` and `GateVerdict`, so the `RunArchived` an archive
appends to the current run changes no line of their output. No step, one-shot command or rebuild
reads an archive.

**ARCHIVING RUNS AT TWO TRIGGERS.** The step lock (`.rigger/step.lock`, `acquire_step_lock`) is the
only serialization between archivers. One function, `archive_pending`
(`crates/rigger-driver/src/archiving.rs`), finds the pending spans as above, leaves and names each
that has a live spawn, hands the rest to `archive::archive_run` in position order, and names each
span it refuses. Every caller reaches it through one wiring helper, `run_archiving(held, own)` in
`src/cli/run.rs`, which pairs it with the git adapter and with the caller's store wired through
`FoldingStore` over the caller's graph (`folding_into`), so each `RunArchived` writes its `applied`
row and the graph prune `reset --runs` runs next meets no hole. It returns one `ArchiveOutcome`
(criterion 14's: the totals and every span named, or a skip and its reason), deciding in this order:
the PERMANENT skips, a store that cannot delete (any but sqlite) and a project outside a git
repository; the TRANSIENT skips, the lock rule and the registry rule; then the archive. The lock
rule: handed the step lock its caller holds, it archives; handed none, it takes the lock without
waiting around the archive alone, and archives nothing when another process holds it. The registry
rule is judged for the store, since an entry (`Instance::id`, keyed on root and store) records a
heartbeat and no run or process: when `live_driver_registrations` (moved beside the helper, the
count `live_writer_facts` takes) finds a live driver entry of this store other than `own`, the entry
the caller's `register_run_instance` wrote, it archives nothing. A second driver at the caller's own
root shares its entry and is not told apart, accepted: when it mints a fresh run, the first driver's
run stops being current and a trigger can archive its episodes, which are then in git, while the
first driver still steps it; two drivers at one root are outside what a step lock or a registry
entry can tell apart. CADENCE: a driver prints nothing for a permanent skip, a property of the
project and not an event, one line for a transient skip (the held lock, or the live driver until its
heartbeat ages out) and one line per named span; `rigger reset --runs` prints every outcome, each
permanent skip once. The conductor's trigger: at the start of every `conductor::run`, once
`run_store::ensure_started` or the driver's own pinned or fresh mint has fixed the current run, it
calls the archiving handle `Deps` carries and hands the outcome back; unwired, it archives nothing.
`rigger step` (`cmd_step`) wires the helper with the step lock it holds for the whole step and its
registration; `rigger run` (`run_cli`), `rigger serve` and `rigger workflow` (both `run_workflow`)
wire it with no lock and their registration; the replay's isolated re-drive and the canary, on
isolated stores, wire none. `rigger reset --runs` (`reset_runs`) runs, in order: its probe
(`live_writer_facts` over the current run's slice from `run::read::read_current_run`), which takes
the step lock and keeps it to the end; the helper, handed that lock and no registration; the closure
read BY TYPE (`EventStore::read_stream_typed` with `TypeSelection::Only(&run::RUN_CLOSURE_TYPES)`),
over which `superseded_graph_nodes` and `superseded_edge_boundary` answer as over the whole stream;
`close_landed_units` over the current run's slice; the graph prune and the graph file's compaction
as today; then `Store::reclaim_space`, only while the probe still holds the lock and its live-writer
facts are dead, else one line saying reclamation was skipped and why, the archive and its delete
standing. On a store that is not sqlite the archive and the reclamation are both skipped, and its
one permanent-skip line says both. It prints one line with the spans, events and bytes archived and
one with the bytes reclaimed, and reads the whole stream nowhere. The conductor trigger never
reclaims: a later append reuses the freed pages. A driver that dies holding the lock releases it
with its process, and the next trigger completes the interrupted span. The first trigger on a store
that predates this spec pays the whole backlog once: for this store's 153 earlier runs and prelude
(349.9 MB serialized) git writes 1.5 s and reads back 0.5 s per 100 MB, about 9 s with the row
reads, so no budget knob is added; an existing store normally pays it at `rigger reset --runs`,
which the documented pre-run procedure runs. The `--runs` text says the same: its usage text in
`src/main.rs`, the flag list `reset_modes` prints, the `rigger-reset-store` skill and the `--runs`
guidance in `crates/rigger-domain/src/docs.rs` describe the archive, the reclamation, its skip and
the two rules.

**VALIDATE NAMES A LOST ARCHIVE REF.** A deleted archive ref is recoverable only while git still
holds its blob: the `RunArchived` records the blob id, so one `git update-ref` restores the ref, and
git prunes an unreachable object once its grace period passes. `rigger validate` runs before every
launch, so its report is the notice that arrives inside that window. It reads every `RunArchived`
through one `EventStore::read_stream_typed` of that type, keeps each span's latest, lists the refs
under `refs/rigger/archive/` with their blob ids through `RunArchive::list` (one `git for-each-ref`
process), and compares the two as sets. For the spans whose latest `RunArchived` names a ref that is
missing or names another blob, the prelude's included, it asks git whether it still holds each
recorded blob through `RunArchive::holds` (one `git cat-file --batch-check` process over all of
them), so validate runs at most two git processes whatever the span count. A span whose blob git
holds prints one advisory line: the span (its run id and position, or `prelude`), the ref, the
recorded blob id and the restore command `git update-ref <ref> <blob>`. The spans whose blob git no
longer holds are named together on ONE line, their count and positions, as lost for good, with no
command; that line repeats at every validate and is the accepted permanent advisory, since the log
still records archives nothing can restore. Criterion 18 adds both methods to the port and its git
adapter. The advisory never fails validate: the span is finished and the remedy is the operator's.
No `RunArchived` prints nothing; a project outside a git repository and a KurrentDB store, where
nothing is archived, print nothing; the advisory writes nothing, so a repeated validate prints the
same lines, it carries nothing between processes, and a ref restored to its recorded blob is silent
again. The `rigger-reset-store` skill names the advisory, its restore command and the lost-for-good
line.

**CONSTRAINTS WALK, decided.**
- *Revert:* a file reverted to earlier content records a new entry with that generation, and it
  resolves from the object database or the tree by generation.
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the later
  fold is a re-recording, and which entry dates the facts is an instance of WHAT A REBUILD
  REPRODUCES.
- *Cold start:* nothing is carried in memory between processes; the sinks' check asks the store's
  group lookup and `graph.db`, and pending runs are found from the type index.
- *Existing data:* a store written before this spec keeps its derived events readable, folds them as
  spec 101 decided and answers the sinks' check from them until `rigger reset --derived` migrates
  it; a `graph.db` built before this spec needs no rebuild, since no fold rule or projection version
  changes.

**STATE PLACEMENT.** The ledger and the archive index are the log (`GenerationIngested`,
`RunArchived`); the class of a type is code (`retention`); the archived bytes are git; `graph.db` is
a projection. A sidecar file listing archived runs, an in-memory set of ingested generations, a
`.rigger/` marker or a cache of resolved blobs is NOT an implementation of any of them.

**OUT OF SCOPE.** `progress.db`, streams other than the run stream, the superseded ledger entries
(each stays live), archiving and migration on KurrentDB, a deleted `graph.db` (forbidden by
`crates/rigger-domain/src/docs.rs`, unchanged here) and everything of spec 108.

## Notes (non-criteria)

`GenerationIngested { prefix, file, generation, blob, excluded }`: `prefix` is `gc`, `gd` or `gw`;
`blob` is 40 lowercase hex digits, or empty when the path held no file; `excluded` is true only for
a `gc` batch lowered as an out-of-line test module.

`RunArchived { run, ref, blob, events, bytes, first, last }`: `run` is the run id, or empty for the
prelude, which no run owns; `ref` is `refs/rigger/archive/run/<position>` or
`refs/rigger/archive/prelude`, where `<position>` is the global position (`Position`, a `u64`) of
the span's `RunStarted`, written in decimal zero-padded to 20 digits, the width of the largest
`u64`; `events` and `bytes` count what the blob holds; `first` and `last` are the lowest and highest
archived positions. It carries `META_GROUP` `archive/run/<position>` or `archive/prelude` and the
replay key `<group>@<blob>#<events>`, the ledger entry's key form.

The archive blob holds one JSON object per archived row, in position order, keys in this order:
`position`, `stream`, `type`, `id`, `data`, `meta`, `valid_from`, `recorded_at`, `revision`. `data`
is the row's bytes as a JSON string when they are UTF-8, else lowercase hex under `data_hex`; `meta`
is its JSON object; times are integer nanoseconds since the epoch.

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
- No EPISODIC event leaves the live store before its bytes are read back from git, on a first pass
  and on a resume alike. A KEYED derived event is shed only by the migration, in the transaction
  that rewrites its identity's latest batch into a ledger entry, because the tree re-derives it; an
  UNKEYED derived event names no identity and is shed and counted by the migration.
- Every archive is deterministic: the same rows yield the same bytes. Every rebuild is
  deterministic: the same rows, the same tree and the same object database yield the same graph.
- The gates cannot see the KurrentDB half of criteria 3 and 10 where the contract suite's container
  is unreachable; the adjudicator demands that run's evidence.

## Done when

- [ ] a test proves THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under `src/` or `crates/` declares sits in exactly one class list, and folding each episodic type leaves the live projection unchanged,
  asserted by a source scan under `tests/` and a fold test in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, with no type of the four cross-run lists
  classified episodic. This criterion OWNS `retention`, `class_of`, the two new type constants and
  the addendum amendment; what a class causes (the refusal, the archive) is criteria 10 and 11's,
  NOT this one's.
- [ ] a test proves THE ENTRY AND ITS BATCH FOLD AS ONE: `apply_generation` folds a resolved batch at the entry's position and installs its generation, installs none for an unresolved one, and changes no fact for a re-recording,
  asserted at the graph seam in `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` with
  hand-built entries and no sink, with the unresolved fold retiring the prior generation's facts,
  `Projection::current_generation` answering each state and the ledger form of `FoldingStore`
  writing one `applied` row per entry. This criterion OWNS `apply_generation` with its resolved,
  unresolved and re-recording rules, `Projection::current_generation` and the ledger form of
  `FoldingStore`; the sinks are criterion 3's and the rebuild criterion 4's, NOT this one's.
- [ ] a test proves PERCEPTION IS A LEDGER ENTRY: ingesting a changed file through either ingest sink appends one `GenerationIngested` whose generation and blob come from the bytes it read, and no derived event,
  asserted through a recording store at `RunCtx::emit_keyed_batch` and at `ingest_tree`, with an
  unchanged file recording nothing, a file whose generation the log holds and `graph.db` does not
  recorded again, a deleted file recording an empty blob, an index lowering that lags the file's
  bytes recording the bytes' generation, and the contract suite's group lookup answering a ledger
  entry on both backends. This criterion OWNS both sinks' write path, `ingest::batch_is_current`,
  the three `(path, bytes, excluded)` functions, the ledger reading of `latest_generation`, the root
  pre-ledger row helper and the moves of the tests its sinks break; the fold rule is criterion 2's,
  the rebuild criterion 4's, the index-lag advisory criterion 5's and the store's refusal criterion
  10's, NOT this one's.
- [ ] a test proves THE GRAPH REBUILDS FROM LEDGER AND TREE: a `graph.db` rebuilt by `rigger setup` from entries that all resolve, recorded by one process, equals the one the sinks built on spec 101's comparison surface,
  over generations that drop a design link and a code entity, with an entry whose blob git does not
  hold resolved from a tree file that extracts to its generation and a deleted file's empty-blob
  entry resolved; and, over a log holding an entry no source resolves, the rebuild reaching the same
  live facts and current generations for every identity whose latest entry resolves, a fact
  re-asserted across a superseded unresolved entry dated by the next resolved one, and the
  unresolved entry counted in the report with its file restored by the next `rigger graph build`.
  This criterion OWNS re-extraction, resolution by generation, the unresolved count and its report
  line and the rule of WHAT A REBUILD REPRODUCES; the fold rule is criterion 2's, the sinks' check
  criterion 3's and the migration criterion 7's, NOT this one's.
- [ ] a test proves THE LEDGER ANSWERS THE INDEX-LAG ADVISORY: `rigger validate` names a sampled file whose current bytes extract to a generation other than its latest entry's or other than `graph.db`'s current one,
  and names no file whose bytes extract to the generation both hold, asserted in `tests/cli.rs`.
  This criterion OWNS the ledger reading of `project_scoped_latest_generations`, `graph_index_lag`'s
  two-sided comparison and validate's typed read; the entries are criterion 3's,
  `Projection::current_generation` criterion 2's, the bloat advisory criterion 9's and the
  archive-ref advisory criterion 18's, NOT this one's.
- [ ] a test proves RECLAMATION STAGES IN MEMORY: `Store::reclaim_space` on a sqlite store holding free pages rewrites the file smaller and reports the bytes reclaimed, with its own connection reporting `temp_store` as memory,
  asserted in `crates/rigger-store-sqlite/src/eventstore/sqlite.rs`, with a file holding no free
  pages left unrewritten. This criterion OWNS `Store::reclaim_space` and `prune_derived_index`'s
  call of it; the verbs that call it are criteria 7 and 17's, NOT this one's.
- [ ] a test proves MIGRATION CONVERTS IN PLACE: `rigger reset --derived` over three generations of a file, a deleted file and an unkeyed derived event rewrites each latest batch's first row into its entry and deletes the rest,
  reporting the entries, the rows shed and, on its own line, the unkeyed rows shed, asserted in
  `tests/cli.rs`, with each entry at its batch's first position and dated at its identity's earliest
  recorded valid-time, nothing written to `graph.db`, a rebuild of the migrated store reaching the
  live facts and current generations the store held before for the files the tree holds, the bytes
  reclaimed reported and nothing to shed when run again. This criterion OWNS the migration,
  `Store::shed_derived`, `Store::count_derived` and the `--derived` text; the reclamation is
  criterion 6's, the menu line and the bloat advisory criteria 8 and 9's, the rebuild criterion 4's
  and the pre-ledger row helper criterion 3's, NOT this one's.
- [ ] a test proves THE RESET MENU PREVIEWS THE MIGRATION: bare `rigger reset` on a store holding derived events prints the count of events and files `rigger reset --derived` then sheds, and says there is no derived event to shed once it has,
  asserted in `tests/cli.rs`. This criterion OWNS the menu's `--derived` line;
  `Store::count_derived` and the migration are criterion 7's and the bloat advisory criterion 9's,
  NOT this one's.
- [ ] a test proves THE BLOAT ADVISORY NAMES THE MIGRATION: `rigger validate` on a store holding any derived event prints one warning naming the events and files left and `rigger reset --derived`, and prints none once migrated,
  asserted in `tests/cli.rs`. This criterion OWNS `bloat_advisory_for`'s new reading; the count and
  the migration are criterion 7's, the index-lag advisory criterion 5's and the archive-ref advisory
  criterion 18's, NOT this one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`). This criterion OWNS the refusal and
  moving every remaining test that records a derived event through a store onto a pre-ledger row
  helper, building any boundary's still missing; the sinks that stop emitting and the root helper
  are criterion 3's, NOT this one's.
- [ ] a test proves A RUN'S EPISODES ARE ARCHIVED: archiving a run's span writes its episodic events under `refs/rigger/archive/run/<position>` and records one `RunArchived`, then deletes exactly those events while its knowledge events stay,
  asserted against `archive::archive_run` over a sqlite store and a fixture git repository, with a
  span holding no episodic event writing no ref and no event and a failed ref write deleting
  nothing. This criterion OWNS the use case, the `RunArchived` shape and its group and replay key,
  `read_archived`, the `RunArchive` port and its git adapter, `EventStore::delete_archived`,
  `first_of_types` and `read_span_typed`; resuming and refusing are criterion 12's, every trigger
  criteria 14, 15, 16 and 17's and whether a project can archive criterion 16's, NOT this one's.
- [ ] a test proves AN INTERRUPTED ARCHIVE COMPLETES: an archive stopped after its ref was written, and one stopped after its `RunArchived` was recorded, are each completed by the next archive of the span after a read-back of the blob,
  with no second ref, no second `RunArchived` and every episodic event of the span gone, and a span
  whose ref names another blob refused naming the ref, that blob and the move-aside command, then
  archived afresh once that command has run, asserted against `archive::archive_run` with a store
  double that fails on command. This criterion OWNS the resume, the refusal and its line; the
  archive's first pass is criterion 11's and the cadence of the line criterion 16's, NOT this one's.
- [ ] a test proves THE ARCHIVE READS BACK: `rigger replay <run>` and `rigger stats --all` print, over a store whose earlier run is archived, the bytes of standard output they print over that store before archiving,
  with the `RunArchived` standing in the current run, a span archived and not yet deleted printing
  those bytes too, and each naming a missing archive ref, asserted in `tests/cli.rs`. This criterion
  OWNS both commands' archive reads and their merge by position; `read_archived` itself is criterion
  11's and validate's archive-ref advisory criterion 18's, NOT this one's.
- [ ] a test proves A CONDUCTOR RUN ARCHIVES ITS PREDECESSORS: a `conductor::run` wired with an archiving handle archives every pending earlier run of the run stream and leaves the current run's events live,
  asserted through `conductor::run` over a fixture repository, with an unwired conductor archiving
  nothing, a run interrupted after its `RunArchived` completed, a run with a live spawn left
  unarchived and named, and a trigger with nothing pending issuing only index reads. This criterion
  OWNS `archive_pending`, `ArchiveOutcome`, the `Deps` handle and the conductor trigger; the prelude
  is criterion 15's, the drivers' wiring, the skips, the cadence, the step lock and the registry
  rule criterion 16's and the reset trigger criterion 17's, NOT this one's.
- [ ] a test proves THE PRELUDE IS ARCHIVED: a wired `conductor::run` on a store holding episodic events below its first `RunStarted` archives them under `refs/rigger/archive/prelude` with one `RunArchived` whose `run` is empty,
  deleting them while the knowledge below that boundary stays, asserted through `conductor::run`
  over a fixture repository, with a prelude archive interrupted after its `RunArchived` completed at
  the next trigger, `rigger stats --all` printing the same bytes before and after, and a store with
  no `RunStarted` archiving nothing. This criterion OWNS the prelude's span, ref, `run` field and
  its branch of `archive_pending`; every run's archive is criteria 11 and 14's and the drivers'
  wiring criterion 16's, NOT this one's.
- [ ] a test proves EVERY DRIVER ARCHIVES UNDER THE STEP LOCK: `run_archiving` handed no lock archives a pending span when the step lock is free and releases it, and archives nothing when another process holds it, printed in one line,
  asserted in the tests of `src/cli/run.rs` and through `rigger step` in `tests/cli.rs`, with each
  `RunArchived` holding its `applied` row in `graph.db`, `cmd_step`, `run_cli` and `run_workflow`
  each wiring `Deps` through that helper, a live driver entry of another root archiving nothing with
  one line, the caller's own entry blocking nothing, and a project outside a git repository
  archiving nothing with no line at a `rigger step`. This criterion OWNS the helper, its
  `FoldingStore` wiring, both permanent skips, the lock rule, the registry rule, the driver cadence
  and the three drivers' wiring; the conductor trigger is criterion 14's and the reset trigger and
  its lines criterion 17's, NOT this one's.
- [ ] a test proves `rigger reset --runs` ARCHIVES: on a store whose earlier runs were never archived, it archives each archivable span, leaves a span with a live spawn unarchived naming it, and prints the archived totals,
  with no whole-stream read, the graph prune meeting no hole, the file smaller on disk and the bytes
  reclaimed printed, reclamation skipped with a line saying why while a live writer holds, and a
  project outside a git repository named once, asserted in `tests/cli.rs`, and a store that is not
  sqlite printing one permanent-skip line naming the archive and the reclamation, asserted over a
  store double in the tests of `src/cli/hygiene.rs`. This criterion OWNS the reset trigger, its
  order, its report, its reclamation call and its skip lines, its permanent-skip lines and the
  `--runs` text; the helper, its wiring and the driver cadence are criterion 16's, the reclamation
  method criterion 6's, the `--derived` text criterion 7's and the skill's archive-ref advisory text
  criterion 18's, NOT this one's.
- [ ] a test proves VALIDATE NAMES A LOST ARCHIVE REF: `rigger validate` on a store with an archived run whose ref was deleted prints one advisory naming the run, the ref, the recorded blob and `git update-ref`, and still exits 0,
  asserted in `tests/cli.rs`, with a ref naming another blob reported the same way, the prelude's
  ref covered, spans whose blob git no longer holds named on one lost-for-good line with no command,
  a store whose refs all match printing nothing, a restored ref silent again and at most two git
  processes run. This criterion OWNS the advisory, its lost-for-good line, `RunArchive::list`,
  `RunArchive::holds` and the skill's text on them; naming a missing ref at read time is criterion
  13's, the index-lag advisory criterion 5's and the bloat advisory criterion 9's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This criterion OWNS only the lanes over the integrated result.
