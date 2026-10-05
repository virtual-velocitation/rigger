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
  belong to the 153 runs before the current one and 2,929 (63 MB) to the run stream before its
  first `RunStarted`, and nothing removes them: `rigger reset --runs` (`reset_runs`,
  `src/cli/hygiene.rs`) prunes `graph.db` and deletes no event.
- The knowledge the hive keeps, every other type, is 37,750 events and 58 MB. Once this spec
  lands, the live log holds that knowledge, the ledger and the current run's episodes, and
  nothing else.

## Design

**UNIT ORDER AND BASE, decided here.** Criterion 1 needs nothing. Criterion 2 needs 1 (it records
the type criterion 1 declares). Criteria 3 and 4 need 2. Criterion 5 needs 3. Criteria 6 and 7 need
5 alone: criterion 5 owns `Store::count_derived` beside `Store::shed_derived`, the one selection
both read. Criterion 8 needs 2, 3 and 5. Criterion 9 needs 1; criteria 10 and 11 need 9; criterion
12 needs 9, 10 and 11; criterion 13 needs 12; criterion 14 needs 13; criterion 15 needs 5 and 14;
criterion 16 needs 9 and 13. Criterion 17 needs all sixteen. The spec is launched on rigger-run. On
the tree that holds criterion 2 and not 3, an owed `rigger setup` rebuild folds each
`GenerationIngested` through no arm, so the rebuilt graph lacks the facts ingested since criterion
2; that tree exists only on the run branch between two landings, and criterion 3 closes it.

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

**PERCEPTION IS A LEDGER ENTRY.** Both ingest sinks, the run's `RunCtx::emit_keyed_batch`
(`crates/rigger-conductor/src/conductor.rs`) and `rigger graph build`'s `ingest_tree`
(`src/cli/graph.rs`), record a `GenerationIngested` (Notes) and no derived event. A sink skips a
batch only when it is CURRENT: `ingest::batch_is_current(store, graph, keyed)`, which replaces
`ingest::batch_is_latest_recorded` at both sinks, answers true only when the log's latest recorded
generation of the batch's identity (`ingest::latest_generation`) and `graph.db`'s current generation
of it (`Projection::current_generation(identity)`, a new port read of the `generations` table) both
equal the batch's generation. The run's sink asks it once per identity per process, as
`ReplayKeys::install` asks today. For a batch that is not current the sink reads the file's bytes
once, hashes them (`git hash-object`, never written) and extracts them through its half's `(path,
bytes, excluded)` function (THE REBUILD block), keyed by `key_batch`; when that extraction is itself
current nothing is recorded, and otherwise the sink records one entry naming that extraction's
generation, that blob and the walk's out-of-line flag (`out_of_line_test_module_files`) and folds
that extraction, never the walk's batch (for `gc`, the persisted symbols index's lowering). So an
entry's generation and blob come from the same bytes for all three halves. A path that holds no file
records an empty blob and the batch the function returns for no bytes (for `gc`, the boundary
sentinel `empty_structural_boundary_event`). An index that lags the disk can key a changed file at a
generation both sides hold, so the walk records nothing for it until the index is refreshed (`rigger
reindex`, or an integration's own reindex); criterion 4's advisory names that file. The entry
carries `eventstore::META_GROUP` (the identity) and the replay key
`<prefix>/<file>@<generation>#<n>`, where n is the batch's event count and so no event of the batch
carries that key. The group lookup (`EventStore::latest_in_group`) therefore answers a ledger entry
on both backends, and `ingest::latest_generation`'s type-first check admits the ledger type beside
the derived types.

**THE ENTRY AND ITS BATCH FOLD AS ONE.** The `Projection` port gains `apply_generation(entry,
batch)`, where `batch` is a function it calls at most once and which answers the entry's batch or
that it is unresolved. One transaction writes one `applied` row at the entry's position. An entry
naming its identity's current generation in `graph.db` is a RE-RECORDING: it writes only that row,
never calls `batch` and changes no fact. Any other entry calls `batch`. A resolved batch folds each
event at the entry's position and valid-time under its own replay key, so spec 101's generation rule
applies unchanged, and installs the entry's generation as current. An unresolved batch retires the
prior generation's facts, as a batch that extracts to nothing does, and installs NO current
generation for the identity, so `batch_is_current` is false for it until an entry folds resolved.
`FoldingStore` gains the ledger form of its append-then-fold body; both sinks use it, and a ledger
entry reaches the graph through no other fold. `rigger emit` already refuses both new types
(`EMITTABLE_TYPES`). A process that dies between the append and the fold leaves a hole in the
`applied` ledger that `rigger setup` pays, as spec 101 decided.

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
while the tree's file extracts to its generation, a prose-only edit included. A rebuild that
resolves every entry folds the events the sinks folded, at the same positions and valid-times, in
the same order, and reaches the same projection; criterion 3's fixture records no `RunStarted`, so
the rebuild's run-closure prune drops nothing from either side. An entry no source resolves folds
unresolved, and the rebuild prints how many entries it folded so, how many of those are their
identity's latest entry, and that the next default-lane ingest that walks those files (`rigger graph
build`, or a run's project-ingest pass) restores them. That restoration is real: the identity holds
no current generation in `graph.db`, so the sink's check is false, the sink records an entry from
the file's current bytes, and that entry is no re-recording, so it folds resolved and installs its
generation. The restored facts carry the restoring entry's valid-time, and an identity whose file
the tree no longer holds stays retired, which is the tree's state. The light lane compiles no
extraction, so every entry that is not a re-recording folds unresolved and is counted, and the first
default-lane ingest restores them. The rebuild reads no archive, since an episodic event changes
neither the live projection nor the fold state (criterion 1), and the `applied` rows of archived
positions are outside spec 101's comparison surface and owe nothing once the log no longer holds
them. An archive delete that lands between the rebuild's position read (`read_live_positions`) and
its selection read (`read_live_selection`), each one read transaction, removes only episodic
positions, whose fold changes no fact; a removed position is neither folded nor owed, so the rebuild
needs no lock beyond `graph.db.lock`.

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
with no current generation is named. The advisory's wording is unchanged. On the tree that holds
criterion 2 and not 4, the advisory reads only derived events, so it names a file changed since
criterion 2 as lagging although the graph holds it; that false advisory changes no exit status and
criterion 4 ends it.

**MIGRATION SHEDS THE BACKLOG.** `rigger reset --derived` keeps its two refusals (a live writer,
`refuse_derived_reset_if_live`; a `graph.db` that owes its rebuild) and becomes the one-time
migration. One sqlite selection, every row of a derived type under the project's prefix, keyed or
unkeyed, backs the migration's delete (`Store::shed_derived`) and the read-only count
(`Store::count_derived`: rows, unkeyed rows and identities); criterion 5 owns both. Before any write
it compares, for every identity whose latest recording (`ingest::latest_generation`) is a derived
event, that generation with `graph.db`'s current one. An identity that disagrees and whose path the
tree holds refuses the whole migration, naming every such identity and the remedy that reconciles
it: `rigger graph build`, whose check records and folds the tree's generation for each identity its
walk visits; `rigger setup` does not, since it rebuilds only a graph that owes its rebuild. An
identity that disagrees and whose path the tree does not hold is recorded with an empty blob at the
generation the batch for no bytes yields, the entry a deletion ingest records, and folds as no
re-recording, retiring what the graph held. Every other identity gets a `GenerationIngested` naming
its latest generation, the blob id of the tree's file at that path (empty when the path holds no
file) and `excluded: false`; an out-of-line test module's recorded batch is the boundary sentinel,
which asserts no fact, so a rebuild that counts it unresolved holds the same projection. The entry's
valid-time is the earliest valid-time among its identity's recorded rows, so a later rebuild dates
the identity's facts no later than the file's first recording; the accepted difference is that a
fact first asserted by a later pre-ledger generation takes the identity's earliest valid-time. Each
such entry names its identity's current generation, so its fold is a re-recording: the migration
hands `apply_generation` no batch, writes one `applied` row and re-dates no fact. It then deletes
every derived event in one transaction, unkeyed ones included, reports the entries recorded, the
events shed and, on its own line, the unkeyed events shed, and reclaims space (RECLAMATION below),
rewriting only a file that holds free pages, and reports the bytes reclaimed. An unkeyed derived
event names no identity (`keyed_derived_event`), so no entry re-asserts it. Today the rebuild's live
selection keeps it (`plan_derived_prune` never touches a row with no key) and the fold attributes
its facts to the log itself, so no later generation retires them. After the shed its facts stay in
the live graph until the next rebuild, which keeps only what an entry asserts; every file the tree
holds is re-asserted by its keyed identity's entry, so only facts no current file asserts are lost,
the accepted difference between the live graph and a rebuilt one. This store holds no unkeyed
derived event. A crash between an entry's append and its fold leaves a hole: the graph owes its
rebuild, `--derived` refuses, and `rigger setup`'s rebuild folds the live selection's derived rows
and then the entry, which names the generation the identity already holds and so is a re-recording
whether or not its bytes resolve. A rerun, or one resumed after a crash, finds each migrated
identity's latest recording is its ledger entry, records entries only for the rest and sheds what
remains; with nothing left it says there is no derived event to shed. `--derived` no longer runs
spec 101's generation compaction: `prune_derived_index` goes, its tests are deleted or re-homed by
criterion 5's unit, and the live selection a rebuild folds (`read_live_selection`, over
`plan_derived_prune`) is unchanged, so on a store not yet migrated it still holds each identity's
latest derived generation. Sqlite only, as `--derived` is today. The `--derived` text says the same:
the usage text in `src/main.rs`, the flag list `reset_modes` prints, the `rigger-reset-store` skill
(procedure and anti-move) and the "Event log hygiene" section of `skills/using-rigger/SKILL.md` and
`docs/handbook/using-rigger.md` and the `--derived` guidance in `crates/rigger-domain/src/docs.rs`
describe a one-time migration that records a ledger entry per file, sheds the derived index and
leaves no derived event behind.

**RECLAMATION STAGES IN MEMORY.** One sqlite `Store` method, `Store::reclaim_space`, split out of
the post-commit reclamation `prune_derived_index` runs today (`compact_in_place`, with its on-disk
before and after and its report of a failure beside the counts), is the only reclamation; criterion
5 owns it. Today its `VACUUM` stages a full copy of the log in SQLite's temporary directory
(`SQLITE_TMPDIR`, else `TMPDIR`, else `/var/tmp`), which can be a small partition. The method sets
`PRAGMA temp_store = MEMORY` on its own connection before the `VACUUM`, so the copy is staged in the
process's memory: nothing is staged on disk outside the store's own files, no setting is
process-global, a crash leaves nothing behind, and no reaper is needed; the write-ahead log's
uncommitted frames are rolled back at the next open. Measured on a copy of this store after the
migration's and the first archive's deletes (229,500 derived and 20,917 episodic rows), with
`SQLITE_TMPDIR` and `TMPDIR` naming a directory that does not exist: the `VACUUM` and its truncating
checkpoint succeed in 0.44 s, the file goes from 580 MB to 92 MB, the process's peak resident memory
rises by 94 MB, about the compacted size, the write-ahead log beside the store peaks at 92 MB, and
no file outside the store's own is opened. The memory is bounded by the compacted size, which after
this spec is the knowledge, the ledger and one run's episodes. When the memory is not there the
`VACUUM` fails and rolls back, the live file is as it was, the deletes stay, and the verb prints the
failure beside its counts. `VACUUM INTO` with a backup copy into the live file is NOT this method:
an append landing between the snapshot and the copy would be overwritten. Both verbs that call it
hold the step lock their probe took, under live-writer facts that are dead
(`LiveWriterFacts::reasons` empty), so two reclaimers never meet; `--derived --force-live` skips the
probe and its lock as it does today, and the operator owns that risk. An appender that arrives
anyway, such as an operator's own `rigger emit`, meets the write lock the `VACUUM` holds for its
whole run: it waits out the busy timeout (5000 ms, `crate::sqlite::open_connection`) and, if the
`VACUUM` outlasts it, fails with `database is locked` and does not retry, as `prune_derived_index`
documents today; the measured 0.44 s is well under it.

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
NOT this refusal. Reading recorded derived rows is unchanged. A contract test whose property is
about the store (group lookup, ordering, any type) is re-expressed over `GenerationIngested` entries
and keeps running on both backends; criterion 2's unit does this for the group lookup's reference
test, since its ledger reading is what that test now pins. A test that needs rows written before
this spec runs on sqlite only, inserting them with raw SQL through one helper per test boundary that
needs one (the root `tests/common/`, a crate's `test_support`), built by criterion 5's unit for its
own boundary; criterion 8's unit adds the others and moves every other test that records a derived
event through a store onto them. No KurrentDB store is migrated by this spec, so no KurrentDB test
seeds pre-ledger rows.

**CRITERIA 2 AND 8 SPLIT AT THE APPEND.** Criterion 2 stops both sinks appending derived events;
criterion 8 refuses them at the store, after criterion 5 ships the migration, so the binary that
refuses a derived append can always migrate a store that holds them.

**CRITERIA 5, 6 AND 7 SPLIT AT THE MIGRATION.** On the tree that holds criterion 5 and not 6 or 7,
the menu's `--derived` line and the bloat advisory still count and word the generation compaction
through `plan_derived_prune`, which stays for the live selection; that tree exists only on the run
branch between landings, and criteria 6 and 7 end it.

**A FINISHED RUN'S EPISODES MOVE TO GIT.** A run is the span of the run stream from its `RunStarted`
to the next `RunStarted`, the boundary `run::current_run` applies. The PRELUDE is the span of the
run stream below the first `RunStarted`, which belongs to no run and is never the current run. A
span (a run or the prelude) is PENDING when it holds a live episodic event and a `RunStarted` above
it exists, whatever its `RunArchived` state, so a span interrupted after its `RunArchived` and a
span refused stay pending; a store with no `RunStarted` has no pending span. A pending span is
ARCHIVABLE when none of its spawns is live by `liveness::live_spawns` (criterion 12; the prelude's
spawns are judged under an empty run id, so a spawn with no marker is not live) and no live driver
registration of this store exists other than the archiving caller's own (criterion 14). One domain
use case, `archive::archive_run` (`crates/rigger-domain/src/archive.rs`), archives one span as one
read hands it, in this order: it serializes the span's episodic events in position order (Notes),
writes the bytes as a git blob under the span's ref through a new `RunArchive` port, reads them back
through the ref and compares them byte for byte, appends one `RunArchived` (Notes) unless the span's
latest `RunArchived` already names that blob, then deletes exactly those positions in one
transaction through a new port method `EventStore::delete_archived(stream, positions)`. A span is
identified by the position of the `RunStarted` that opens it, a value the log guarantees unique and
well formed, never by its run id, which the log does not: a run's ref is
`refs/rigger/archive/run/<position>`, written at the fixed width Notes gives so the refs list in run
order, and its `RunArchived` group is keyed by the same position; the prelude's ref is
`refs/rigger/archive/prelude`, with its own group, and no decimal position equals `prelude`.
`RunArchived.run` still records the run id, so a person reads which run a ref holds. Knowledge
events stay. A span holding no episodic event is not pending: it writes no ref and no event. A
failed ref write or read-back (a full disk, a repository error) deletes nothing and appends no
`RunArchived`; the span stays pending, costs one span read per trigger, and every trigger names it
with git's error. The blob id is the archive's digest. No step or one-shot fold reads an earlier
run's episodic event (spec 101: the run slice from its boundary, the knowledge types by type) and an
episodic event changes no graph state a rebuild compares, so archiving one changes no fold. THE
READER AUDIT, by what each read materializes: the readers of `run::read::read_current_run` read the
current run's slice and knowledge types by type; the critique store, the canary stream and
`progress.db` are other stores. These production reads materialize the whole run stream, earlier
spans' episodic rows included, and use only the part named: `cmd_playbooks` (`LessonLearned`),
`stats_lines` without `--all` (the current run), `read_model_drift` (`UnitStatus`), `reset_menu`
(the run-closure types), `refuse_derived_reset_if_live` (the current run's spawns),
`read_run_units_or_why` (the current run) and `read_order_signatures` (every row, through
`read_all`). Archiving shrinks what each materializes and changes none of their answers, so none of
them changes here; the order-signature advisory's answer holds because deleting rows only lowers a
stream's running revision maximum, so it never flags a remaining row and stops reporting only rows
the live store no longer holds. Four whole-stream reads do change: `cmd_replay` and `stats_lines
--all` read the archive (criterion 11), `read_graph_index_lag` reads by type (criterion 4), and
`reset_runs` reads by type (criterion 15). `RunArchive` is declared in the domain beside
`EventStore`; its one adapter, in `crates/rigger-worktree-git`, runs `git hash-object -w --stdin`,
`git update-ref` and `git cat-file blob`. Git compresses its objects, so no compression crate is
added (`Cargo.lock` carries none). Git is the retention system: the ref is local until the operator
pushes it, and nothing rigger does deletes one. BACKEND SCOPE: the sqlite store deletes; the
KurrentDB adapter answers each of the three archive port methods (`delete_archived` and the two
reads below) with an unsupported-operation error. Whether a project can archive at all (a backend
that cannot delete, a project outside a git repository) is decided by `run_archiving` before any
archive read (ARCHIVING RUNS AT TWO TRIGGERS). Every `EventStore` implementation (both adapters,
`Namespaced`, `FoldingStore`, the test doubles) gains the three methods in criterion 9's unit.

**FINDING A PENDING RUN READS THE INDEX, NOT THE BACKLOG.** Two new port reads, both criterion 9's:
`EventStore::first_of_types(stream, from, types)` answers the revision of the oldest event at or
after revision `from` whose type is named, never its data; the sqlite adapter answers it from
`idx_events_stream_type`, one seek per named type. `EventStore::read_span_typed(stream, from, to,
selection)` reads the events at revisions `from` up to `to` that `selection` admits, in position
order. `archive_pending` finds the current run's boundary with `last_position(RunStarted)` (none
means nothing is pending) and the first run's with `first_of_types(0, [RunStarted])`, then asks
`first_of_types` for the oldest episodic event from revision 0: none, or one at or past the current
boundary, means nothing is pending. One below the first boundary makes the prelude pending, by the
same rule as any run (criterion 13; on the tree before it, the search starts at the first boundary
and the prelude stays live). Otherwise it steps `first_of_types(.., [RunStarted])` forward to the
run whose span holds that event. For the prelude or a run, it reads that span alone through
`read_span_typed` with `TypeSelection::Except` of the derived types, archives it or names why it is
not archivable, and asks again from the span's upper boundary. A trigger with nothing pending costs
three index reads however many spans are archived, a backlog is held one span at a time, and each
span left pending (a live spawn, a refusal) adds one span read to every trigger while it lasts.

**AN INTERRUPTED ARCHIVE COMPLETES.** The serialization is a pure function of the rows, so the next
archive of an interrupted span re-serializes its live episodic events to the same blob. A ref
already naming that blob is left as it stands, and the resume reads the blob back through the ref
and compares it byte for byte, exactly as the first pass does, before its delete, so the read-back
precedes every delete. A `RunArchived` carries `META_GROUP` (the ref's name below `refs/rigger/`)
and a replay key naming its blob (Notes), so the group lookup answers the span's latest
`RunArchived` and its blob without reading the stream: one naming the same blob is not appended
again, and one naming another blob is followed by a new one, which readers and validate take as the
span's. A span whose ref already names a blob its live episodic events do not serialize to is
REFUSED: nothing is written or deleted, and every trigger names it in one line with the ref, the
blob that ref names, and the exit, one command that moves the ref aside so the span archives afresh
at the next trigger while the old blob stays recoverable under the new name: `printf 'create
refs/rigger/aside/<blob> <blob>\ndelete <ref> <blob>\n' | git update-ref --stdin`. Until then it
costs one span read per trigger. Since each span has its own position, no other span's archive can
meet its ref, so this refusal is the only one. The delete is one transaction, so no interruption
leaves part of a span deleted. The prelude resumes and refuses exactly as a run does.

**THE ARCHIVE IS READ ON DEMAND.** `archive::read_archived(archived)` resolves the ref a span's
latest `RunArchived` names, checks that it names the recorded blob, parses the rows and yields the
events in position order; a missing ref or a different blob is an error naming the span and the ref.
Criterion 9 owns it, because the archive's own read-back uses it. Both readers merge archive rows
and live rows by position and take a position present on both sides once, the live row, so a span
that is archived and not yet deleted (between an append and its delete, after an interruption, or
refused) is counted once. `rigger replay <run>` (`cmd_replay`, `src/cli/mod.rs`) resolves the run id
to its `RunStarted`, the first match as `baseline_run_slice` takes it today, then to the span's
latest `RunArchived` by that position, merges the archived events with the run's live events so
before it slices the baseline, and refuses naming the ref when it is missing; a second `RunStarted`
sharing a run id is a span with its own ref that `replay` does not address by id, as today; the
prelude has no run id for it to name, and no run's slice reaches below the first `RunStarted`, so
replay never reads the prelude's archive. `rigger stats --all` (`stats_lines`) merges every archived
span's events, the prelude's included, with the live log so and prints one line naming each missing
ref. No step, one-shot command or rebuild reads an archive.

**ARCHIVING RUNS AT TWO TRIGGERS.** The step lock (`.rigger/step.lock`, `acquire_step_lock`) is the
only serialization between archivers. One function, `archive_pending`
(`crates/rigger-driver/src/archiving.rs`), finds the pending spans as above, leaves each that has a
live spawn and names it, hands the rest to `archive::archive_run` in position order, and names each
span it refuses. Every caller reaches it through one wiring helper in `src/cli/run.rs`,
`run_archiving(held, own)`, which pairs it with the git adapter and decides, in this order, and
returns its outcome to its caller as one `ArchiveOutcome` value (criterion 12's: the totals, every
span named, or a skip and its reason): the PERMANENT skips, a backend that cannot delete (any store
but sqlite) and a project outside a git repository; the TRANSIENT skips, the lock rule and the
registry rule; then the archive, with its totals and every span it named. The lock rule: handed the
step lock its caller already holds, it archives; handed none, it takes the step lock without
waiting, around the archive alone, and releases it after; when another process holds the lock it
archives nothing. The registry rule: a registry entry records the project root, the store and a
driver heartbeat, keyed on root and store (`Instance::id`), and records no run and no process, so
the rule is judged for the store. `own` is the entry id the caller's `register_run_instance` wrote,
or none; when `live_driver_registrations` (moved beside the helper, the count `live_writer_facts`
takes) finds a live driver entry of this store other than `own`, it archives nothing. A second
driver at the caller's own root shares the caller's entry and is not told apart; it can drive only
the current run, whose slice archiving never touches. CADENCE: a driver trigger prints nothing for a
permanent skip, which is a property of the project and not an event; it prints one line for a
transient skip (naming the held lock, or the live driver, which can last until its heartbeat ages
out) and one line per named span; `rigger reset --runs` prints every outcome, each permanent skip
once per invocation. The conductor's one trigger: at the start of every `conductor::run`, once
`run_store::ensure_started` or the driver's own pinned or fresh mint has fixed the current run, it
calls the archiving handle `Deps` carries and hands the outcome back, and with none wired it
archives nothing. `rigger step` (`cmd_step`) wires the helper's value with the step lock it holds
for the whole step and its registration; `rigger run` (`run_cli`) and `rigger serve` and `rigger
workflow` (both `run_workflow`) wire it with no lock and their registration; each prints the outcome
by the cadence above. The replay's isolated re-drive and the canary wire none, since they run on
isolated stores. `rigger reset --runs` (`reset_runs`) runs in this order: its probe
(`live_writer_facts` over the current run's slice, read through `run::read::read_current_run`),
which takes the step lock and keeps it to the end; the helper, handed that lock and no registration;
the closure read BY TYPE (`EventStore::read_stream_typed` with
`TypeSelection::Only(&run::RUN_CLOSURE_TYPES)`), which `superseded_graph_nodes` and
`superseded_edge_boundary` answer the same over as over the whole stream; `close_landed_units` over
the current run's slice; the graph prune and the graph file's compaction as today; then
`Store::reclaim_space`, which rewrites only a file holding free pages so a rerun reclaims what a
skipped reclamation left, only while the probe still holds the step lock and its live-writer facts
are dead, else one line saying reclamation was skipped and why, with the archive and its delete
standing. The whole-stream read of `reset_runs` goes. It prints one line with the spans, events and
bytes archived and one with the bytes reclaimed. The conductor trigger never reclaims: a later
append reuses the freed pages. A driver that dies holding the lock releases it with its process, and
the next trigger completes the interrupted span. The first trigger on a store that predates this
spec pays the whole backlog once; measured on this store's 153 earlier runs and prelude (349.9 MB of
serialized rows), git writes 1.5 s and reads back 0.5 s per 100 MB, about 9 s in all with the row
reads, so no budget knob is added. An existing store normally pays it at `rigger reset --runs`,
which the documented pre-run procedure already runs. The `--runs` text says the same: its usage text
in `src/main.rs`, the flag list `reset_modes` prints, the `rigger-reset-store` skill and the
`--runs` guidance in `crates/rigger-domain/src/docs.rs` describe the archive, the reclamation, its
skip and the two rules.

**CRITERIA 9, 12, 13, 14 AND 15 SPLIT AT THE TRIGGER.** Criterion 9 builds the use case and calls it
from no production path. Criterion 12 adds `archive_pending`, the `Deps` handle and the conductor
trigger, exercised by a conductor its test wires; criterion 13 extends `archive_pending` to the
prelude; criterion 14 adds `run_archiving`, its skip decisions and the cadence and wires them into
the three drivers, which is when production archives; criterion 15 moves `rigger reset --runs` onto
the helper. All of them land after criterion 11 moved `rigger replay` and `rigger stats --all` onto
the archive, so no tree archives a span its readers cannot read. A test that reads an earlier run's
episodic events from the live log after a later `RunStarted` is the unit's to move onto
`read_archived` whose change makes it fail.

**VALIDATE NAMES A LOST ARCHIVE REF.** A deleted archive ref is recoverable only while git still
holds its blob: the `RunArchived` records the blob id, so one `git update-ref` restores the ref, and
git prunes an unreachable object once its grace period passes. `rigger validate` runs before every
launch, so its report is the notice that arrives inside that window. It reads every `RunArchived`
through one `EventStore::read_stream_typed` of that type, keeps each span's latest, and lists the
refs under `refs/rigger/archive/` with their blob ids through one git call, `RunArchive::list` (one
`git for-each-ref` process, never one per archived span), a method criterion 16 adds to the port and
its git adapter, and compares the two as sets. It prints one advisory line per span whose latest
`RunArchived` names a ref that is missing or names another blob, the prelude's included: the span
(its run id and position, or `prelude`), the ref, the recorded blob id and the restore command `git
update-ref <ref> <blob>`. The advisory never fails validate, like every validate advisory: the span
it warns about is already finished and the remedy is one command the operator runs. No `RunArchived`
prints nothing; a project outside a git repository and a KurrentDB store, where nothing is archived,
print nothing; the advisory writes nothing, so a repeated validate prints the same lines, it carries
nothing between processes, and a ref restored to its recorded blob is silent again. The
`rigger-reset-store` skill names the advisory and its restore command.

**CONSTRAINTS WALK, decided.**
- *Revert:* a file reverted to earlier content records a new entry with that generation, and it
  resolves from the object database or the tree by generation.
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the second
  entry is a re-recording both incrementally and at a rebuild, so it changes no fact in both.
- *Cold start:* nothing is carried in memory between processes; the sinks' check asks the store's
  group lookup and `graph.db`, and pending runs are found from the type index.
- *Existing data:* a store written before this spec keeps its derived events readable, folds them as
  spec 101 decided and answers the sinks' check from them until `rigger reset --derived` migrates
  it; a `graph.db` built before this spec needs no rebuild, since no fold rule or projection version
  changes.
- *Concurrent drivers:* a `rigger step` holds the step lock for its whole step, so a `rigger run`
  starting meanwhile archives nothing and says so, and the step's own trigger archives the pending
  spans; a live driver at another root keeps every caller from archiving until its heartbeat ages
  out.
- *Archive revert:* an operator who deletes an archive ref loses that span's episodes and nothing
  else; every reader names the missing ref, and `rigger validate` names it with its restore command
  while git still holds the blob (criterion 16).
- *Prelude:* a store with no episodic event below its first `RunStarted` writes no prelude ref and
  no event; a store with no `RunStarted` archives nothing; once archived the prelude holds no live
  episodic event and is never pending again; an interrupted prelude archive completes at the next
  trigger as a run's does; this store's prelude (2,929 events, 63 MB) is archived by the first
  trigger with its earlier runs.

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
  and on a resume alike. A KEYED derived event is shed only by the migration, after its identity's
  ledger entry is recorded, because the tree re-derives it; an UNKEYED derived event names no
  identity and is shed and counted by the migration.
- Every archive and every rebuild is deterministic: the same rows and the same tree yield the same
  bytes.
- The gates cannot see the KurrentDB half of criteria 2 and 8 where the contract suite's container
  is unreachable; the adjudicator demands that run's evidence.

## Done when

- [ ] a test proves THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under `src/` or `crates/` declares sits in exactly one class list, and folding each episodic type leaves the live projection unchanged,
  asserted by a source scan under `tests/` and a fold test in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, with no type of the four cross-run lists
  classified episodic. This criterion OWNS `retention`, `class_of`, the two new type constants and
  the addendum amendment; what a class causes (the refusal, the archive) is criteria 8 and 9's, NOT
  this one's.
- [ ] a test proves PERCEPTION IS A LEDGER ENTRY: ingesting a changed file through either ingest sink appends one `GenerationIngested` whose generation and blob come from the bytes it read, and no derived event,
  asserted through a recording store at `RunCtx::emit_keyed_batch` and at `ingest_tree`, with
  `graph.db` holding the file's facts, an unchanged file recording nothing, a file whose generation
  the log holds and `graph.db` does not recorded and folded again, a deleted file recording an empty
  blob, an index lowering that lags the file's bytes recording the bytes' generation, a second entry
  of one generation changing no fact, an unresolved fold leaving its identity with no current
  generation, and the contract suite's group lookup answering a ledger entry on both backends. This
  criterion OWNS both sinks' write path, `ingest::batch_is_current`,
  `Projection::current_generation`, the three `(path, bytes, excluded)` functions,
  `apply_generation` with its re-recording and unresolved rules, the ledger form of `FoldingStore`
  and the ledger reading of `latest_generation`; the rebuild is criterion 3's, the index-lag
  advisory criterion 4's and the store's refusal criterion 8's, NOT this one's.
- [ ] a test proves THE GRAPH REBUILDS FROM LEDGER AND TREE: a `graph.db` rebuilt through `rigger setup` from a log of ledger entries equals the one the ingest sinks built incrementally from that log,
  compared on spec 101's comparison surface (the live projection plus the fold state that decides
  future folds) over generations that drop a design link and a code entity, with an entry whose blob
  git does not hold resolved from a tree file that extracts to its generation, a deleted file's
  empty-blob entry resolved and not counted, and an entry no source resolves counted in the
  rebuild's report and its file restored by the next `rigger graph build`. This criterion OWNS
  re-extraction, resolution by generation, the unresolved count and its report line; the migration
  is criterion 5's and `apply_generation` and the sinks' check criterion 2's, NOT this one's.
- [ ] a test proves THE LEDGER ANSWERS THE INDEX-LAG ADVISORY: `rigger validate` names a sampled file whose current bytes extract to a generation other than its latest entry's or other than `graph.db`'s current one,
  and names no file whose bytes extract to the generation both hold, asserted in `tests/cli.rs`.
  This criterion OWNS the ledger reading of `project_scoped_latest_generations`, `graph_index_lag`'s
  two-sided comparison and validate's typed read; the entries and `Projection::current_generation`
  are criterion 2's, the bloat advisory criterion 7's and the archive-ref advisory criterion 16's,
  NOT this one's.
- [ ] a test proves MIGRATION SHEDS THE BACKLOG: `rigger reset --derived` on a store holding three generations of one file, a deleted file and an unkeyed derived event records an entry per identity, sheds them all and reports the counts,
  the unkeyed count on its own line, leaving the live projection of `graph.db` unchanged, each entry
  dated at its identity's earliest recorded valid-time, the file smaller on disk with the bytes
  reclaimed reported and the reclamation's own connection reporting `temp_store` as memory, a graph
  that disagrees on a file the tree holds refused before any write naming `rigger graph build`, and
  nothing to shed when run again, asserted in `tests/cli.rs`. This criterion OWNS the migration,
  `Store::shed_derived`, `Store::count_derived`, `Store::reclaim_space`, the pre-ledger row helpers
  and the `--derived` text; the menu line and the bloat advisory are criteria 6 and 7's, the
  archive-ref advisory criterion 16's, the rebuild that later reads the entries criterion 3's, NOT
  this one's.
- [ ] a test proves THE RESET MENU PREVIEWS THE MIGRATION: bare `rigger reset` on a store holding derived events prints the count of events and files `rigger reset --derived` then sheds, and says there is no derived event to shed once it has,
  asserted in `tests/cli.rs`. This criterion OWNS the menu's `--derived` line;
  `Store::count_derived` and the migration are criterion 5's and the bloat advisory criterion 7's,
  NOT this one's.
- [ ] a test proves THE BLOAT ADVISORY NAMES THE MIGRATION: `rigger validate` on a store holding any derived event prints one warning naming the events and files left and `rigger reset --derived`, and prints none once migrated,
  asserted in `tests/cli.rs`. This criterion OWNS `bloat_advisory_for`'s new reading; the count and
  the migration are criterion 5's, the index-lag advisory criterion 4's and the archive-ref advisory
  criterion 16's, NOT this one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`). This criterion OWNS the refusal and
  moving every test that records a derived event through a store onto the pre-ledger row helpers;
  the sinks that stop emitting are criterion 2's, NOT this one's.
- [ ] a test proves A RUN'S EPISODES ARE ARCHIVED: archiving a run's span writes its episodic events under `refs/rigger/archive/run/<position>` and records one `RunArchived`, then deletes exactly those events while its knowledge events stay,
  asserted against `archive::archive_run` over a sqlite store and a fixture git repository, with a
  span holding no episodic event writing no ref and no event and a failed ref write deleting
  nothing. This criterion OWNS the use case, the `RunArchived` shape and its group and replay key,
  `read_archived`, the `RunArchive` port and its git adapter, `EventStore::delete_archived`,
  `first_of_types` and `read_span_typed`; resuming and refusing are criterion 10's, every trigger
  criteria 12, 13, 14 and 15's and whether a project can archive criterion 14's, NOT this one's.
- [ ] a test proves AN INTERRUPTED ARCHIVE COMPLETES: an archive stopped after its ref was written, and one stopped after its `RunArchived` was recorded, are each completed by the next archive of the span after a read-back of the blob,
  with no second ref, no second `RunArchived` and every episodic event of the span gone, and a span
  whose ref names another blob refused naming the ref, that blob and the move-aside command, then
  archived afresh once that command has run, asserted against `archive::archive_run` with a store
  double that fails on command. This criterion OWNS the resume, the refusal and its line; the
  archive's first pass is criterion 9's and the cadence of the line criterion 14's, NOT this one's.
- [ ] a test proves THE ARCHIVE READS BACK: `rigger replay <run>` and `rigger stats --all` print, over a store whose earlier run is archived, the bytes of standard output they print over that store before archiving,
  with a span archived and not yet deleted printing those bytes too, and each naming a missing
  archive ref, asserted in `tests/cli.rs`. This criterion OWNS both commands' archive reads and
  their merge by position; `read_archived` itself is criterion 9's and validate's archive-ref
  advisory criterion 16's, NOT this one's.
- [ ] a test proves A CONDUCTOR RUN ARCHIVES ITS PREDECESSORS: a `conductor::run` wired with an archiving handle archives every pending earlier run of the run stream and leaves the current run's events live,
  asserted through `conductor::run` over a fixture repository, with an unwired conductor archiving
  nothing, a run interrupted after its `RunArchived` completed, a run with a live spawn left
  unarchived and named, and a trigger with nothing pending issuing only index reads. This criterion
  OWNS `archive_pending`, `ArchiveOutcome`, the `Deps` handle and the conductor trigger; the prelude
  is criterion 13's, the drivers' wiring, the skips, the cadence, the step lock and the registry
  rule criterion 14's and the reset trigger criterion 15's, NOT this one's.
- [ ] a test proves THE PRELUDE IS ARCHIVED: a wired `conductor::run` on a store holding episodic events below its first `RunStarted` archives them under `refs/rigger/archive/prelude` with one `RunArchived` whose `run` is empty,
  deleting them while the knowledge below that boundary stays, asserted through `conductor::run`
  over a fixture repository, with a prelude archive interrupted after its `RunArchived` completed at
  the next trigger, `rigger stats --all` printing the same bytes before and after, and a store with
  no `RunStarted` archiving nothing. This criterion OWNS the prelude's span, ref, `run` field and
  its branch of `archive_pending`; every run's archive is criteria 9 and 12's and the drivers'
  wiring criterion 14's, NOT this one's.
- [ ] a test proves EVERY DRIVER ARCHIVES UNDER THE STEP LOCK: `run_archiving` handed no lock archives a pending span when the step lock is free and releases it, and archives nothing when another process holds it, printed in one line,
  asserted in the tests of `src/cli/run.rs` and through `rigger step` in `tests/cli.rs`, with
  `cmd_step`, `run_cli` and `run_workflow` each wiring `Deps` through that helper, a live driver
  entry of another root archiving nothing with one line, the caller's own entry blocking nothing,
  and a project outside a git repository archiving nothing with no line at a `rigger step`. This
  criterion OWNS the helper, both permanent skips, the lock rule, the registry rule, the driver
  cadence and the three drivers' wiring; the conductor trigger is criterion 12's and the reset
  trigger and its lines criterion 15's, NOT this one's.
- [ ] a test proves `rigger reset --runs` ARCHIVES: on a store whose earlier runs were never archived, it archives each archivable span, leaves a span with a live spawn unarchived naming it, and prints the archived totals,
  with no whole-stream read, the file smaller on disk and the bytes reclaimed printed, reclamation
  skipped with a line saying why while a live writer holds, and a project outside a git repository
  named once, asserted in `tests/cli.rs`. This criterion OWNS the reset trigger, its order, its
  report, its reclamation call and its skip line, its permanent-skip lines and the `--runs` text;
  the helper and the driver cadence are criterion 14's, the conductor trigger criterion 12's, the
  reclamation method and `--derived` text criterion 5's, the menu's `--derived` line criterion 6's,
  the bloat advisory criterion 7's and the skill's archive-ref advisory text criterion 16's, NOT
  this one's.
- [ ] a test proves VALIDATE NAMES A LOST ARCHIVE REF: `rigger validate` on a store with an archived run whose ref was deleted prints one advisory naming the run, the ref, the recorded blob and `git update-ref`, and still exits 0,
  asserted in `tests/cli.rs`, with a ref naming another blob reported the same way, the prelude's
  ref covered, a store whose refs all match printing nothing, a restored ref silent again and the
  refs listed by one git process. This criterion OWNS the advisory, `RunArchive::list` and the
  skill's text on it; naming a missing ref at read time is criterion 11's, the index-lag advisory
  criterion 4's and the bloat advisory criterion 7's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This criterion OWNS only the lanes over the integrated result.
