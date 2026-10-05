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
criterion 5. Criterion 8 needs 2, 3 and 5. Criterion 9 needs 1; criteria 10 and 11 need 9; criterion
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
(`src/cli/graph.rs`), append for each batch the first-sight check
(`ingest::batch_is_latest_recorded`) does not find recorded one `GenerationIngested` (Notes) and no
derived event. The first-sight check keeps asking about the batch the walk keyed (for `gc`, the
persisted symbols index's lowering). For a batch it does not find recorded, the sink reads the
file's bytes once, hashes them (`git hash-object`, never written) and extracts them through its
half's `(path, bytes, excluded)` function (THE REBUILD block), keyed by `key_batch`; the entry names
that extraction's generation, that blob and the walk's out-of-line flag
(`out_of_line_test_module_files`), and the sink folds that extraction, never the walk's batch. So an
entry's generation and blob come from the same bytes for all three halves. A path that holds no file
records an empty blob and the batch the function returns for no bytes (for `gc`, the boundary
sentinel `empty_structural_boundary_event`). When the extraction's generation is its identity's
latest recorded one, nothing is recorded. An index that lags the disk can key a changed file at its
recorded generation, so the walk records nothing for it until the index is refreshed (`rigger
reindex`, or an integration's own reindex); criterion 4's advisory names that file. The entry
carries `eventstore::META_GROUP` (the identity) and the replay key
`<prefix>/<file>@<generation>#<n>`, where n is the batch's event count and so no event of the batch
carries that key. The group lookup (`EventStore::latest_in_group`) therefore answers a ledger entry
on both backends, and `ingest::latest_generation`'s type-first check admits the ledger type beside
the derived types.

**THE ENTRY AND ITS BATCH FOLD AS ONE.** The `Projection` port gains `apply_generation(entry,
batch)`, where `batch` is a function it calls at most once. One transaction writes one `applied` row
at the entry's position. An entry naming its identity's current generation in `graph.db` is a
RE-RECORDING: it writes only that row, never calls `batch` and changes no fact. Any other entry
calls `batch` and folds each event at the entry's position and valid-time under its own replay key,
so spec 101's generation rule applies unchanged. `FoldingStore` gains the ledger form of its
append-then-fold body; both sinks use it, and a ledger entry reaches the graph through no other
fold. `rigger emit` already refuses both new types (`EMITTABLE_TYPES`). A process that dies between
the append and the fold leaves a hole in the `applied` ledger that `rigger setup` pays, as spec 101
decided.

**THE REBUILD RE-EXTRACTS THE LEDGER.** `rigger setup`'s rebuild (`rebuild_owed_graph`,
`src/cli/setup.rs`, over `Projector::rebuild`) folds the log's live selection as spec 101 decided
and hands each `GenerationIngested` to `apply_generation` with a function that re-extracts its
batch. The bytes come from the repository's object database by blob id, through one `git cat-file
--batch` process per rebuild, else from the tree's file at that path when its bytes hash to that id.
One function per half takes `(path, bytes, excluded)` and returns the batch the sinks key:
`extract::extract` under the grammar `registry::for_path` resolves from the path's extension, then
`extract_events` and `proof_events`, for `gc`; `file_batch`'s concept and link extraction for `gd`;
for `gw`, the parse `config_store::load_workflow` applies to the file's bytes
(`parse_yaml_naming_unknown_keys`, then the stage names), split out as a bytes form that
`load_workflow` keeps calling after its file read, then `workflowdef::extract_events`, which reads
nothing else. The sinks and the rebuild both call it, so a rebuild that resolves every entry folds
the events the sinks folded, at the same positions and valid-times, in the same order, and reaches
the same projection; criterion 3's fixture records no `RunStarted`, so the rebuild's run-closure
prune drops nothing from either side. An entry with an empty blob re-extracts the batch for no
bytes, folds it and is resolved. An entry whose blob resolves nowhere, or whose re-extracted
generation differs from the recorded one, folds as a generation with an empty batch: the prior
generation's facts retire, as when a file extracts to nothing, and the rebuild prints how many
entries it folded so and how many of those are their identity's latest entry. A fact first asserted
by such an entry carries a later generation's valid-time. An unresolved latest entry whose bytes the
tree no longer holds is superseded by the next ingest's newer generation of that file. The light
lane compiles no extraction, so every entry that is not a re-recording folds unresolved and is
counted. The rebuild reads no archive, since an episodic event changes neither the live projection
nor the fold state (criterion 1), and the `applied` rows of archived positions are outside spec
101's comparison surface and owe nothing once the log no longer holds them. An archive delete that
lands between the rebuild's position read (`read_live_positions`) and its selection read
(`read_live_selection`), each one read transaction, removes only episodic positions, whose fold
changes no fact; a removed position is neither folded nor owed, so the rebuild needs no lock beyond
`graph.db.lock`.

**THE LEDGER ANSWERS THE INDEX-LAG ADVISORY.** `rigger validate`'s graph index-lag advisory
(`read_graph_index_lag`, `src/cli/validate.rs`) reads the run stream through
`EventStore::read_stream_typed` with `TypeSelection::Only` of the derived types and
`GenerationIngested`, never the whole stream. `ingest::project_scoped_latest_generations` answers
each identity's latest generation from a derived event's key or a ledger entry's generation, and
`graph_index_lag` compares it with the generation of the file's current bytes through its half's
`(path, bytes, excluded)` function; a generation hashes the whole batch, so equal generations are
equal key sets. The advisory's wording is unchanged. On the tree that holds criterion 2 and not 4,
the advisory reads only derived events, so it names a file changed since criterion 2 as lagging
although the graph holds it; that false advisory changes no exit status and criterion 4 ends it.

**MIGRATION SHEDS THE BACKLOG.** `rigger reset --derived` keeps its two refusals (a live writer,
`refuse_derived_reset_if_live`; a `graph.db` that owes its rebuild) and becomes the one-time
migration. One sqlite selection, every row of a derived type under the project's prefix, backs the
migration's delete (`Store::shed_derived`) and the read-only count (`Store::count_derived`: rows and
identities). For every identity whose latest recording (`ingest::latest_generation`) is a derived
event it records a `GenerationIngested` naming that generation, the blob id of the tree's file at
that path (empty when the path holds no file) and `excluded: false`; an out-of-line test module's
recorded batch is the boundary sentinel, which asserts no fact, so a rebuild that counts it
unresolved holds the same projection. The entry's valid-time is the earliest valid-time among its
identity's recorded rows, so a later rebuild dates the identity's facts no later than the file's
first recording; the one accepted difference is that a fact first asserted by a later pre-ledger
generation takes the identity's earliest valid-time. It refuses, naming the identity, when
`graph.db`'s current generation for an identity differs from the log's latest. Each entry names its
identity's current generation, so its fold is a re-recording: the migration hands `apply_generation`
no batch, writes one `applied` row and re-dates no fact. It then deletes every derived event, keyed
or unkeyed, in one transaction, when it deleted anything reclaims the file's free pages through one
sqlite `Store` method split out of the post-commit reclamation `prune_derived_index` runs today
(`compact_in_place`, with its on-disk before and after; RECLAMATION below), and reports the entries
recorded, the events shed and the bytes reclaimed. A crash between an entry's append and its fold
leaves a hole: the graph owes its rebuild, `--derived` refuses, and `rigger setup`'s rebuild folds
the live selection's derived rows and then the entry, which names the generation the identity
already holds and so is a re-recording whether or not its bytes resolve. A rerun, or one resumed
after a crash, finds each migrated identity's latest recording is its ledger entry, records entries
only for the rest and sheds what remains; with nothing left it says there is no derived event to
shed. `--derived` no longer runs spec 101's generation compaction: `prune_derived_index` goes, its
tests are deleted or re-homed by criterion 5's unit, and the live selection a rebuild folds
(`read_live_selection`, over `plan_derived_prune`) is unchanged, so on a store not yet migrated it
still holds each identity's latest derived generation. Sqlite only, as `--derived` is today. This
store holds no unkeyed derived event. The `--derived` text says the same: the usage text in
`src/main.rs`, the flag list `reset_modes` prints, the `rigger-reset-store` skill (procedure and
anti-move) and the "Event log hygiene" section of `skills/using-rigger/SKILL.md` and
`docs/handbook/using-rigger.md` and the `--derived` guidance in `crates/rigger-domain/src/docs.rs`
describe a one-time migration that records a ledger entry per file, sheds the derived index and
leaves no derived event behind.

**RECLAMATION STAGES BESIDE THE STORE.** Today `compact_in_place` runs `VACUUM`, which stages a
full copy of the log in SQLite's temporary directory (`SQLITE_TMPDIR`, else `TMPDIR`, else
`/var/tmp`), possibly a small partition; an unlinked temporary file leaves nothing after a crash,
and a failure is carried back beside the counts while the deletes stay. The split-out method
stages instead at `events.db.compact` beside the store, on the store's own filesystem, through
`VACUUM INTO`, then copies it into the live file in one transaction through SQLite's backup API
(the pattern `Projector::rebuild` already uses for `graph.db.pruned`) and removes it; open
connections stay valid. Its reaper is the method itself: it removes a leftover `events.db.compact`
before it stages, and a crash leaves at most that one file until the next `rigger reset --derived`
or `--runs` reclamation. When the space is not there, the staging or the copy fails, the partial
staged file is removed, the live file is left as it was (the backup's transaction rolls back), the
deletes stay, and the verb prints the failure beside its counts. Measured on a copy of this store
after the migration's and the first archive's deletes (229,500 derived and 20,917 episodic rows):
the file goes from 581 MB to 91 MB in 0.6 s, and the peak extra disk is 184 MB (the staged copy
plus the write-ahead log frames, about twice the compacted size), the same as today's `VACUUM`.

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
span refused for a mismatched blob stay pending; a store with no `RunStarted` has no pending span. A
pending span is ARCHIVABLE when none of its spawns is live by `liveness::live_spawns` (criterion 12;
the prelude's spawns are judged under an empty run id, so a spawn with no marker is not live) and no
live driver registration of this store exists other than the archiving caller's own (criterion 14).
One domain use case, `archive::archive_run` (`crates/rigger-domain/src/archive.rs`), archives one
span as one read hands it, in this order: it serializes the span's episodic events in position order
(Notes), writes the bytes as a git blob under the span's ref through a new `RunArchive` port, reads
them back through the ref and compares them byte for byte, appends one `RunArchived` (Notes) unless
the group lookup already answers one for the span, then deletes exactly those positions in one
transaction through a new port method `EventStore::delete_archived(stream, positions)`. A run's ref
is `refs/rigger/archive/run/<run-id>` and the prelude's is `refs/rigger/archive/prelude`: every
run's ref sits under `run/`, so no run id (a minted `Uuid::new_v4`, or any other string a store
holds, such as this store's one `r1`) can name the prelude's ref or a ref git would refuse beside
it. Knowledge events stay. A span holding no episodic event is not pending: it writes no ref and no
event. A failed ref write or read-back (a ref name git refuses, a full disk, a repository error)
deletes nothing and appends no `RunArchived`; the span stays pending and every trigger names it with
git's error. The blob id is the archive's digest. No step or one-shot fold reads an earlier run's
episodic event (spec 101: the run slice from its boundary, the knowledge types by type) and an
episodic event changes no graph state a rebuild compares, so archiving one changes no fold. Every
other production reader of the run stream reads the current run's slice
(`run::read::read_current_run`), a knowledge type by type, or a store other than the project's run
stream (the critique store, the canary stream, `progress.db`); the two archive readers are below.
`rigger validate`'s order-signature advisory (`watch::order_signatures` over `read_all`) is the one
other reader that sees an earlier run's episodic rows: deleting rows only lowers a stream's running
revision maximum, so it never flags a remaining row and stops reporting only rows the live store no
longer holds. `RunArchive` is declared in the domain beside `EventStore`; its one adapter, in
`crates/rigger-worktree-git`, runs `git hash-object -w --stdin`, `git update-ref` and `git cat-file
blob`. Git compresses its objects, so no compression crate is added (`Cargo.lock` carries none). Git
is the retention system: the ref is local until the operator pushes it, and nothing rigger does
deletes one. BACKEND SCOPE: the sqlite store deletes; the KurrentDB adapter answers each of the
three archive port methods (`delete_archived` and the two reads below) with an unsupported-operation
error, and archiving there is skipped with a line naming the backend. A project outside a git
repository is skipped with a line saying so. Every `EventStore` implementation (both adapters,
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
`read_span_typed` with `TypeSelection::Except` of the derived types, archives it or names why it
is not archivable, and asks again from the span's upper boundary. A trigger with nothing pending
costs three index reads however many spans are archived, and a backlog is held one span at a
time.

**AN INTERRUPTED ARCHIVE COMPLETES.** The serialization is a pure function of the rows, so the next
archive of an interrupted run re-serializes its live episodic events to the same blob: a ref already
naming that blob is left as it stands, a `RunArchived` the group lookup already answers for the span
(its `META_GROUP` is the ref's name below `refs/rigger/`) is not appended again, and the delete then
runs. A span whose ref names a blob its live episodic events do not serialize to is refused by name
and nothing is deleted; it stays pending, so every trigger names it again. The delete is one
transaction, so no interruption leaves part of a span deleted. The prelude resumes and refuses
exactly as a run does.

**THE ARCHIVE IS READ ON DEMAND.** `archive::read_archived(archived)` resolves the ref a
`RunArchived` names, checks that it names the recorded blob, parses the rows and yields the events
in position order; a missing ref or a different blob is an error naming the span and the ref.
Criterion 9 owns it, because the archive's own read-back uses it. `rigger replay <run>`
(`cmd_replay`, `src/cli/mod.rs`) merges an archived run's events with its knowledge events by
position before it slices the baseline (`baseline_run_slice`), and refuses naming the ref when it is
missing; the prelude has no run id for it to name, and no run's slice reaches below the first
`RunStarted`, so replay never reads the prelude's archive. `rigger stats --all` (`stats_lines`)
folds every archived span's events, the prelude's included, with the live log and prints one line
naming each missing ref. No step, one-shot command or rebuild reads an archive.

**ARCHIVING RUNS AT TWO TRIGGERS.** The step lock (`.rigger/step.lock`, `acquire_step_lock`) is the
only serialization between archivers. One function, `archive_pending`
(`crates/rigger-driver/src/archiving.rs`), finds the pending spans as above, leaves each that has a
live spawn and names it, and hands the rest to `archive::archive_run` in position order. Every
caller reaches it through one wiring helper in `src/cli/run.rs`, `run_archiving(held, own)`, which
pairs it with the git adapter, the lock rule and the registry rule. The lock rule: handed the step
lock its caller already holds, it archives; handed none, it takes the step lock without waiting,
around the archive alone, and releases it after; when another process holds the lock it archives
nothing and prints one line naming the held lock. The registry rule: a registry entry records the
project root, the store and a driver heartbeat, keyed on root and store (`Instance::id`), and
records no run and no process, so the rule is judged for the store. `own` is the entry id the
caller's `register_run_instance` wrote, or none; when `live_driver_registrations` (moved beside the
helper, the count `live_writer_facts` takes) finds a live driver entry of this store other than
`own`, it archives nothing and prints one line naming the live driver. A second driver at the
caller's own root shares the caller's entry and is not told apart; it can drive only the current
run, whose slice archiving never touches. The conductor's one trigger: at the start of every
`conductor::run`, once `run_store::ensure_started` or the driver's own pinned or fresh mint has
fixed the current run, it calls the archiving handle `Deps` carries, and with none wired it archives
nothing. `rigger step` (`cmd_step`) wires the helper's value with the step lock it holds for the
whole step and its registration; `rigger run` (`run_cli`) and `rigger serve` and `rigger workflow`
(both `run_workflow`) wire it with no lock and their registration; the replay's isolated re-drive
and the canary wire none, since they run on isolated stores. `rigger reset --runs` calls the same
helper with the step lock its probe takes and no registration, prints one line with the runs, events
and bytes archived, keeps its graph prune, and, when the archive deleted anything, reclaims the
file's free pages through criterion 5's reclamation method and prints the bytes reclaimed. The
conductor trigger never reclaims: a later append reuses the freed pages. A driver that dies holding
the lock releases it with its process, and the next trigger completes the interrupted run. The first
trigger on a store that predates this spec pays the whole backlog once; measured on this store's 153
earlier runs and prelude (349.9 MB of serialized rows), git writes 1.5 s and reads back 0.5 s per
100 MB, about 9 s in all with the row reads, so no budget knob is added. An existing store normally
pays it at `rigger reset --runs`, which the documented pre-run procedure already runs. The `--runs`
text says the same: its usage text in `src/main.rs` and the `rigger-reset-store` skill describe the
archive, the reclamation and the two rules.

**CRITERIA 9, 12, 13, 14 AND 15 SPLIT AT THE TRIGGER.** Criterion 9 builds the use case and calls it
from no production path. Criterion 12 adds `archive_pending`, the `Deps` handle and the conductor
trigger, exercised by a conductor its test wires; criterion 13 extends `archive_pending` to the
prelude; criterion 14 adds `run_archiving` and wires it into the three drivers, which is when
production archives; criterion 15 moves `rigger reset --runs` onto the helper. All of them land
after criterion 11 moved `rigger replay` and `rigger stats --all` onto the archive, so no tree
archives a run its readers cannot read. A test that reads an earlier run's episodic events from the
live log after a later `RunStarted` is the unit's to move onto `read_archived` whose change makes it
fail.

**VALIDATE NAMES A LOST ARCHIVE REF.** A deleted archive ref is recoverable only while git still
holds its blob: the `RunArchived` records the blob id, so one `git update-ref` restores the ref, and
git prunes an unreachable object once its grace period passes. `rigger validate` runs before every
launch, so its report is the notice that arrives inside that window. It reads every `RunArchived`
through one `EventStore::read_stream_typed` of that type and lists the refs under
`refs/rigger/archive/` with their blob ids through one git call, `RunArchive::list` (one `git
for-each-ref` process, never one per archived span), a method criterion 16 adds to the port and its
git adapter, and compares the two as sets. It prints one advisory line per `RunArchived` whose ref
is missing or names a blob other than the recorded one, the prelude's included: the span (the run
id, or `prelude`), the ref, the recorded blob id and the restore command `git update-ref <ref>
<blob>`. The advisory never fails validate, like every validate advisory: the run it warns about is
already finished and the remedy is one command the operator runs. No `RunArchived` prints nothing; a
project outside a git repository and a KurrentDB store, where nothing is archived, print nothing;
the advisory writes nothing, so a repeated validate prints the same lines, it carries nothing
between processes, and a ref restored to its recorded blob is silent again. The `rigger-reset-store`
skill names the advisory and its restore command.

**CONSTRAINTS WALK, decided.**
- *Revert:* a file reverted to earlier content records a new entry with that generation, and its
  blob resolves from the object database or the tree.
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the
  second entry is a re-recording both incrementally and at a rebuild, so it changes no fact in
  both.
- *Cold start:* nothing is carried in memory between processes; the first-sight check asks the
  store's group lookup, and pending runs are found from the type index.
- *Existing data:* a store written before this spec keeps its derived events readable, folds them
  as spec 101 decided and answers the first-sight check from them until `rigger reset --derived`
  migrates it; a `graph.db` built before this spec needs no rebuild, since no fold rule or
  projection version changes.
- *Concurrent drivers:* a `rigger step` holds the step lock for its whole step, so a `rigger run`
  starting meanwhile archives nothing and says so, and the step's own trigger archives the pending
  runs; a live driver at another root keeps every caller from archiving until its heartbeat ages
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
`blob` is 40 lowercase hex digits, or empty when the path held no file; `excluded` is true only
for a `gc` batch lowered as an out-of-line test module.

`RunArchived { run, ref, blob, events, bytes, first, last }`: `run` is the run id, or empty for
the prelude, which no run owns; `ref` is `refs/rigger/archive/run/<run>` or
`refs/rigger/archive/prelude`; `events` and `bytes` count what the blob holds; `first` and `last`
are the lowest and highest archived positions. It carries `META_GROUP` `archive/run/<run>` or
`archive/prelude`.

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
- No EPISODIC event leaves the live store before its bytes are read back from git. A DERIVED event
  is shed only by the migration, after its identity's ledger entry is recorded, because the tree
  re-derives it.
- Every archive and every rebuild is deterministic: the same rows and the same tree yield the
  same bytes.
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
  `graph.db` holding the file's facts, an unchanged file recording nothing, a deleted file recording
  an empty blob, an index lowering that lags the file's bytes recording the bytes' generation, a
  second entry of one generation changing no fact, and the contract suite's group lookup answering a
  ledger entry on both backends. This criterion OWNS both sinks' write path, the three `(path,
  bytes, excluded)` functions, `apply_generation` and its re-recording rule, the ledger form of
  `FoldingStore` and the ledger reading of the first-sight check; the rebuild is criterion 3's, the
  index-lag advisory criterion 4's and the store's refusal criterion 8's, NOT this one's.
- [ ] a test proves THE GRAPH REBUILDS FROM LEDGER AND TREE: a `graph.db` rebuilt through `rigger setup` from a log of ledger entries equals the one the ingest sinks built incrementally from that log,
  compared on spec 101's comparison surface (the live projection plus the fold state that decides
  future folds) over generations that drop a design link and a code entity, while an entry whose
  blob resolves nowhere is counted in the rebuild's report and a deleted file's empty-blob entry is
  not. This criterion OWNS re-extraction, blob resolution and the unresolved count; the migration is
  criterion 5's and `apply_generation` criterion 2's, NOT this one's.
- [ ] a test proves THE LEDGER ANSWERS THE INDEX-LAG ADVISORY: `rigger validate` on a store of ledger entries names a file whose current bytes extract to a generation other than its recorded entry's,
  and names no file whose bytes extract to its entry's generation, asserted in `tests/cli.rs`. This
  criterion OWNS the ledger reading of `project_scoped_latest_generations` and `graph_index_lag` and
  validate's typed read; the entries are criterion 2's and the archive-ref advisory criterion 16's,
  NOT this one's.
- [ ] a test proves MIGRATION SHEDS THE BACKLOG: `rigger reset --derived` on a store holding three generations of one file and a deleted file records a ledger entry per identity, sheds every derived event and reports both counts,
  leaving the live projection of `graph.db` unchanged, each entry dated at its identity's earliest
  recorded valid-time, the file smaller on disk and the bytes reclaimed reported, and saying there
  is nothing to shed when run again, with the staged copy staged beside the store and removed and a
  leftover one removed first, asserted in `tests/cli.rs`. This criterion OWNS the migration,
  `Store::shed_derived`, the reclamation method, the pre-ledger row helpers and the `--derived`
  text; the menu line and the bloat advisory are criteria 6 and 7's, the rebuild that later reads
  the entries is criterion 3's, NOT this one's.
- [ ] a test proves THE RESET MENU PREVIEWS THE MIGRATION: bare `rigger reset` on a store holding derived events prints the count of events and files `rigger reset --derived` then sheds, and says there is no derived event to shed once it has,
  asserted in `tests/cli.rs`. This criterion OWNS `Store::count_derived` and the menu's `--derived`
  line; the migration is criterion 5's and the bloat advisory criterion 7's, NOT this one's.
- [ ] a test proves THE BLOAT ADVISORY NAMES THE MIGRATION: `rigger validate` on a store holding any derived event prints one warning naming the events and files left and `rigger reset --derived`, and prints none once migrated,
  asserted in `tests/cli.rs`. This criterion OWNS `bloat_advisory_for`'s new reading; the count is
  criterion 6's and the migration criterion 5's, NOT this one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`). This criterion OWNS the refusal and
  moving every test that records a derived event through a store onto the pre-ledger row helpers;
  the sinks that stop emitting are criterion 2's, NOT this one's.
- [ ] a test proves A RUN'S EPISODES ARE ARCHIVED: archiving a run's span writes its episodic events under `refs/rigger/archive/run/<run-id>` and records one `RunArchived`, then deletes exactly those events while its knowledge events stay,
  asserted against `archive::archive_run` over a sqlite store and a fixture git repository, with a
  span holding no episodic event writing no ref and no event. This criterion OWNS the use case,
  `read_archived`, the `RunArchive` port and its git adapter, `EventStore::delete_archived`,
  `first_of_types` and `read_span_typed`; resuming an interrupted archive is criterion 10's and
  every trigger criteria 12, 13, 14 and 15's, NOT this one's.
- [ ] a test proves AN INTERRUPTED ARCHIVE COMPLETES: an archive stopped after its ref was written, and one stopped after its `RunArchived` was recorded, are each completed by the next archive of the run,
  with no second ref, no second `RunArchived` and every episodic event of the run gone, asserted
  against `archive::archive_run` with a store double that fails on command. This criterion OWNS the
  resume and the refusal of a mismatched blob; the archive's first pass is criterion 9's, NOT this
  one's.
- [ ] a test proves THE ARCHIVE READS BACK: `rigger replay <run>` and `rigger stats --all` print, over a store whose earlier run is archived, the bytes of standard output they print over that store before archiving,
  and each names a missing archive ref, asserted in `tests/cli.rs`. This criterion OWNS both
  commands' archive reads; `read_archived` itself is criterion 9's and validate's archive-ref
  advisory criterion 16's, NOT this one's.
- [ ] a test proves A CONDUCTOR RUN ARCHIVES ITS PREDECESSORS: a `conductor::run` wired with an archiving handle archives every pending earlier run of the run stream and leaves the current run's events live,
  asserted through `conductor::run` over a fixture repository, with an unwired conductor archiving
  nothing, a run interrupted after its `RunArchived` completed, a run with a live spawn left
  unarchived and named, and a trigger with nothing pending issuing only index reads. This criterion
  OWNS `archive_pending`, the `Deps` handle and the conductor trigger; the prelude is criterion
  13's, the drivers' wiring, the step lock and the registry rule criterion 14's and the reset
  trigger criterion 15's, NOT this one's.
- [ ] a test proves THE PRELUDE IS ARCHIVED: a wired `conductor::run` on a store holding episodic events below its first `RunStarted` archives them under `refs/rigger/archive/prelude` with one `RunArchived` whose `run` is empty,
  deleting them while the knowledge below that boundary stays, asserted through `conductor::run`
  over a fixture repository, with a prelude archive interrupted after its `RunArchived` completed at
  the next trigger, `rigger stats --all` printing the same bytes before and after, and a store with
  no `RunStarted` archiving nothing. This criterion OWNS the prelude's span, ref, `run` field and
  its branch of `archive_pending`; every run's archive is criteria 9 and 12's and the drivers'
  wiring criterion 14's, NOT this one's.
- [ ] a test proves EVERY DRIVER ARCHIVES UNDER THE STEP LOCK: `run_archiving` handed no lock archives a pending run when the step lock is free and releases it, and archives nothing with one line naming the lock when another process holds it,
  asserted in the tests of `src/cli/run.rs` and through `rigger step` in `tests/cli.rs`, with
  `cmd_step`, `run_cli` and `run_workflow` each wiring `Deps` through that helper, a live driver
  entry of another root archiving nothing with one line, and the caller's own entry blocking
  nothing. This criterion OWNS the helper, the three drivers' wiring, the lock rule and the registry
  rule; the conductor trigger is criterion 12's and the reset trigger criterion 15's, NOT this
  one's.
- [ ] a test proves `rigger reset --runs` ARCHIVES: on a store whose earlier runs were never archived, it archives each archivable run, leaves a run with a live spawn unarchived naming it, and prints the archived totals,
  with the file smaller on disk and the bytes reclaimed printed, asserted in `tests/cli.rs`. This
  criterion OWNS the reset trigger, its report, its reclamation call and the `--runs` text; the
  helper is criterion 14's, the conductor trigger criterion 12's, and the reclamation method and
  `--derived` text criterion 5's, the menu's `--derived` line criterion 6's, the bloat advisory
  criterion 7's and the skill's archive-ref advisory text criterion 16's, NOT this one's.
- [ ] a test proves VALIDATE NAMES A LOST ARCHIVE REF: `rigger validate` on a store with an archived run whose ref was deleted prints one advisory naming the run, the ref, the recorded blob and `git update-ref`, and still exits 0,
  asserted in `tests/cli.rs`, with a ref naming another blob reported the same way, the prelude's
  ref covered, a store whose refs all match printing nothing, a restored ref silent again and the
  refs listed by one git process. This criterion OWNS the advisory, `RunArchive::list` and the
  skill's text on it; naming a missing ref at read time is criterion 11's, the index-lag advisory
  criterion 4's and the bloat advisory criterion 7's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This
  criterion OWNS only the lanes over the integrated result.
