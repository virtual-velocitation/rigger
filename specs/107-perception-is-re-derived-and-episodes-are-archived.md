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
type criterion 1 declares). Criterion 3 needs 2: the rebuild learns the ledger before any sink
writes it, so no tree records an entry a rebuild cannot fold. Criterion 4 needs 3, whose `(path,
bytes, excluded)` functions the sinks reuse. Criterion 5 needs 4, whose entries its advisory reads.
Criterion 6 needs nothing. Criterion 7 needs 4 and 6. Criteria 8 and 9 need 7 alone: criterion 7
owns `Store::count_derived` beside `Store::shed_derived`, the one selection both read. Criterion 10
needs 7. Criterion 11 needs 1; criteria 12 and 13 need 11; criterion 14 needs 11, 12 and 13;
criterion 15 needs 14; criterion 16 needs 15; criterion 17 needs 16; criterion 18 needs 7 and 17,
and criterion 19 needs 18, since 7, 18 and 19 rewrite the `rigger-reset-store` skill in turn.
Criterion 20 needs all nineteen. The spec is launched on rigger-run. A test that fails because of a
unit's change is that unit's to move, whichever criterion owns the surface it asserts. On a tree
holding 7 and not 8 or 9, `count_derived_duplicates` and `DerivedPreview` stand without the oracle
tests criterion 7 deletes, and the menu line and the bloat advisory still word the compaction
through `plan_derived_prune`. Criterion 10 refuses a derived append only after criterion 7 ships the
migration, so the binary that refuses one can always migrate a store that holds them. On a tree
holding 11 and not 12, the `RunArchive` write issues `create` alone, so a span whose ref exists
fails with git's error and stays pending. Production archives only from criterion 17 on, after
criterion 13 moved `rigger replay` and `rigger stats --all` onto the archive and criterion 16
decided which projects cannot archive, so no tree archives a span its readers cannot read.

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
fold leaves a hole in the `applied` ledger that `rigger setup` pays (spec 101); criterion 2 owns it.

**THE REBUILD RE-EXTRACTS THE LEDGER.** `rigger setup`'s rebuild (`rebuild_owed_graph`,
`src/cli/setup.rs`, over `Projector::rebuild`) folds the log's live selection as spec 101 decided
and hands each `GenerationIngested` to `apply_generation` with a function that re-extracts its
batch. One TOTAL function per half takes `(path, bytes, excluded)`, the bytes possibly absent, and
returns the batch the walk lowers today. For `gc`, UTF-8 bytes that `extract::extract` parses under
the grammar `registry::for_path` resolves yield `extract_events` over `for_extraction(.., excluded)`
and, unless excluded, `proof_events`; every other input (no bytes, bytes that are not UTF-8, a path
with no grammar whatever its bytes, a failed extraction) yields exactly
`empty_structural_boundary_event(path, "unknown", false)`, the batch `file_batches` gives a path the
index lacks. For `gd`, `file_batch`'s concept and link extraction of UTF-8 bytes, else the empty
batch. For `gw`, the parse `config_store::load_workflow` applies to the file's bytes
(`parse_yaml_naming_unknown_keys`, then the stage names), split out as a bytes form that
`load_workflow` keeps calling after its file read, then `workflowdef::extract_events`, else (no
bytes, bytes that are not UTF-8, a failed parse) the empty batch, as `project_events` answers.
`excluded` changes only a parsed `gc` batch. An empty batch keys no event, so it has no generation,
resolves no entry and is never recorded (`batches_within` and `project_batches` skip it). Criterion
3 owns them; the sinks reuse them. RESOLUTION IS BY GENERATION, from three sources in one fixed
order for every entry: (1) the entry's blob from the repository's object database (one `git cat-file
--batch` process per rebuild), when the entry names a blob and git holds it, git's zero-byte blob
counting as held; (2) the tree's file at the path; (3) no bytes. The entry resolves at the first
source whose batch has the recorded generation; the blob is where to look first, never the test.
Source 3 resolves every entry recorded from an input the function maps to the no-bytes batch (a
deleted path, bytes that are not UTF-8, a path with no grammar, a failed extraction), whatever
became of the bytes; for `gd` and `gw` it yields the empty batch, which resolves nothing. Outside a
git repository, or when the batch process cannot start, source 1 is skipped. So an entry recorded
from uncommitted bytes still resolves while the tree's file extracts to its generation. An entry no
source resolves folds unresolved, and the rebuild prints how many entries it folded so, how many of
those are their identity's latest entry, and that the next default-lane ingest that walks those
files (`rigger graph build`, or a run's project-ingest pass) restores them (criterion 4 asserts it),
since the identity holds no current generation; an identity whose file the tree no longer holds
stays retired. The light lane compiles no extraction, so every entry that is not a re-recording
folds unresolved and is counted. The rebuild reads no archive (an episodic event changes neither the
live projection nor the fold state, criterion 1), the `applied` rows of archived or shed positions
are outside spec 101's comparison surface, and an archive delete landing between the rebuild's
position read (`read_live_positions`) and its selection read (`read_live_selection`) removes only
positions whose fold changes no fact, so the rebuild needs no lock beyond `graph.db.lock`.

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
- *A migrated identity:* its earliest surviving recording carries the identity's earliest pre-ledger
  valid-time (MIGRATION), so a fact asserted from that recording on takes it, and a rewritten entry
  stands at its batch's first row's position, where the incremental graph kept each fact's own run
  start and newest row.
- *Two entries of one generation:* append and fold are not atomic, so the incremental graph dates
  the facts by whichever entry folded first, and a rebuild by the lower position.
- *An identity restored after an unresolved latest entry:* its facts take the restoring entry's
  valid-time and position.
- *Facts only an unkeyed pre-ledger derived event asserted:* the live graph keeps them after the
  migration sheds that event, and the first rebuild retires them, since no entry asserts them.

**PERCEPTION IS A LEDGER ENTRY.** Both ingest sinks, the run's `RunCtx::emit_keyed_batch`
(`crates/rigger-conductor/src/conductor.rs`) and `rigger graph build`'s `ingest_tree`
(`src/cli/graph.rs`), record a `GenerationIngested` (Notes) through the ledger form of
`FoldingStore` and no derived event. A sink skips a batch only when it is CURRENT:
`ingest::batch_is_current(store, graph, keyed)`, which replaces `ingest::batch_is_latest_recorded`
at both sinks, answers true only when the log's latest recorded generation of the batch's identity
(`ingest::latest_generation`) and `graph.db`'s current generation of it
(`Projection::current_generation`) both equal the batch's generation, so an identity an unresolved
fold left with no current generation is never current. `batch_is_current` alone decides the ledger
write, with no key-level suppression. The run's sink reads `Projection::current_generation` on every
pass and memoizes only the log side per process (`LoggedGenerations`: identity to the latest logged
generation, filled from an identity's first `latest_generation` answer and from each entry it
records), so a long-lived `rigger run` or `rigger serve` restores an identity a rebuild left
unresolved at its next pass; another process's entry stales the memo, costing at most one
re-recording (WHAT A REBUILD REPRODUCES), and a failed append leaves it as it was. `ReplayKeys`
(`crates/rigger-conductor/src/replay_keys.rs`) keeps only its plain key set (`seeded`, `insert`,
`contains`) for the run's lifecycle keyed emits, its run-start seed excluding `GenerationIngested`
beside the derived types; its generations map, `install`, `forget` and `Ticket` go, and an
un-migrated store's derived keys are read only through `latest_generation`. A revert A, B, A records
three entries, two under one replay key, and the graph ends on A's facts; no store or reader treats
a replay key as unique: the group lookup answers the latest by position and the rebuild folds each
entry at its own. For a batch that is not current the sink reads the file's bytes once, hashes them
(`git hash-object --stdin`, never written) and extracts them through its half's `(path, bytes,
excluded)` function (THE REBUILD block), keyed by `key_batch`; when that extraction is itself
current nothing is recorded, and otherwise the sink records one entry naming that extraction's
generation, that blob and the walk's out-of-line flag (`out_of_line_test_module_files`) and folds
that extraction, never the walk's batch (for `gc`, the persisted symbols index's lowering). A path
that holds no file, which only `file_batches` names, records an entry with no blob and `gc`'s batch
for no bytes, which source 3 resolves. An index that lags the disk can key a changed file at a
generation both sides hold, so the walk records nothing for it until the index is refreshed (`rigger
reindex`, or an integration's own reindex); criterion 5's advisory names that file. The entry
carries `eventstore::META_GROUP` (the identity) and the replay key
`<prefix>/<file>@<generation>#<n>`, where n is the batch's event count and so no event of the batch
carries that key. The group lookup (`EventStore::latest_in_group`) therefore answers a ledger entry
on both backends, and `ingest::latest_generation`'s type-first check admits the ledger type beside
the derived types. The hash runs outside a repository too (`git hash-object` without `-w`), one
process per batch that is not current, since no SHA-1 crate is a direct dependency. Every production
batch is keyed by `key_batch`, whose keys always name an identity; a caller's batch whose key names
none records nothing and fails the emit naming the key. Criterion 4 owns the sinks.

**THE LEDGER ANSWERS THE INDEX-LAG ADVISORY.** `rigger validate`'s graph index-lag advisory
(`read_graph_index_lag`, `src/cli/validate.rs`) reads the run stream through
`EventStore::read_stream_typed` with `TypeSelection::Only` of the derived types and
`GenerationIngested`, never the whole stream, and opens `graph.db` read-only as
`retired_entities_advisory_for` does. `ingest::project_scoped_latest_generations` answers each
identity's latest generation from a derived event's key or a ledger entry's generation, and
`graph_index_lag` names a sampled file when the generation of its current bytes through its half's
`(path, bytes, excluded)` function, with `excluded` computed as the walk computes it
(`out_of_line_test_module_files` over the persisted symbols index) and never taken from an entry,
differs from that generation or from `graph.db`'s current generation
(`Projection::current_generation`), the two sides the sinks' check reads; a generation hashes the
whole batch, so equal generations are equal key sets. So a file an unresolved fold left with no
current generation is named. The advisory's wording is unchanged. In the light lane
`graph_index_lag_sample` stays the stub that samples nothing, as today.

**MIGRATION CONVERTS THE BACKLOG IN PLACE.** `rigger reset --derived` keeps its two refusals (a live
writer, `refuse_derived_reset_if_live`; a `graph.db` that owes its rebuild) and becomes the one-time
migration. It appends nothing, folds nothing and writes nothing to `graph.db`. In ONE sqlite
transaction (`Store::shed_derived`), for every identity whose latest recording (a derived row or a
ledger entry, keyed alike) is a derived row, it rewrites IN PLACE the lowest-position row of that
latest generation's unbroken run (its rows above the last row of any other generation): position,
stream, id, revision and recorded-time stay; type, data and meta become the identity's
`GenerationIngested` (that generation, the blob id of the tree's file at that path, `excluded`
computed as the walk computes it from the persisted symbols index (`false` in the light lane, which
compiles no index), its group and replay key, whose `#<n>` counts the distinct replay keys of that
generation among the identity's derived rows; no reader compares `n`). It then deletes every
remaining row of a derived type under the project's prefix, keyed or unkeyed; the same selection
backs the read-only count (`Store::count_derived`: rows, unkeyed rows and identities). EARLIEST
SURVIVING RECORDING: in the same transaction, every identity that sheds derived rows ends with its
earliest surviving recording carrying the identity's earliest recorded valid-time, the rewritten row
when no ledger entry of the identity precedes it and otherwise its earliest ledger entry, re-dated
in place. Identity and generation are cut from each row's replay key (`ingest::derived_key_parts`,
as `plan_derived_prune` cuts them), never from its group: 42,563 of this store's keyed derived rows
carry no `META_GROUP`. The rewrite keeps every column a uniqueness rule covers (the primary key,
`UNIQUE(stream, revision)`), as `prune_derived_index`'s in-place re-date does today. The kept row
already has its `applied` row, and a batch is one append with no knowledge event inside it, so a
rebuild folds the entry where the live graph first folded that generation, on the same side of every
alias and knowledge event (its dating is an instance of WHAT A REBUILD REPRODUCES). On this store:
688 rows rewritten and 228,812 deleted, with the write lock held 1.9 s on a copy, inside the 5000 ms
busy timeout an appender waits. A crash rolls the transaction back and a rerun starts over; a rerun
after success says there is no derived event to shed. A path the tree does not hold gets an entry
with no blob: unless its generation is `gc`'s batch for no bytes (a deletion already ingested, which
source 3 resolves), the entry resolves nowhere at a rebuild, whose unresolved fold retires its
facts; in the live graph the next ingest naming a `gc` path (`file_batches`) records the batch for
no bytes and retires them, and no walk names a deleted `gd` or `gw` path, as today. So an
out-of-line test module's entry records `excluded: true` and resolves at a rebuild; an entry whose
generation the tree's flag no longer reproduces folds unresolved and the next ingest restores it
(THE REBUILD block). An unkeyed derived event names no identity (`keyed_derived_event`), so no entry
re-asserts it; this store holds none. The migration reports the entries converted, the rows shed
and, on its own line, the unkeyed rows shed, then reclaims space (`Store::reclaim_space`) and
reports the bytes reclaimed. `prune_derived_index` and its generation compaction go: criterion 7's
unit deletes or re-homes its tests and deletes the three `count_derived_duplicates_*` oracle tests
of `tests/reset_menu_previews_periphery.rs` that compare the preview against it, and criterion 8's
unit removes the preview they covered; the live selection a rebuild folds (`read_live_selection`,
over `plan_derived_prune`) is unchanged, so a store not yet migrated still folds each identity's
latest derived generation. A connected console tab meets nothing: its provider reads the current
run, which holds no derived row, so the floor guard of `serve_console_stream` never fires; criterion
7 rewrites that guard's comment, which cites `prune_derived_index`. Sqlite only, as today. The
`--derived` text says the same: the usage text in `src/main.rs`, the flag list `reset_modes` prints,
the `rigger-reset-store` skill (procedure and anti-move) and the "Event log hygiene" section of
`skills/using-rigger/SKILL.md` and `docs/handbook/using-rigger.md` and the `--derived` guidance in
`crates/rigger-domain/src/docs.rs` describe a one-time migration that converts each file's latest
batch into a ledger entry and leaves no derived event behind.

**RECLAMATION STAGES IN MEMORY.** One sqlite `Store` method, `Store::reclaim_space`, split out of
the reclamation `prune_derived_index` runs after its commit today (`compact_in_place`, its on-disk
before and after and its failure report), is the only reclamation; criterion 6 owns it, and
`prune_derived_index` calls it until criterion 7 removes that function. It sets `PRAGMA temp_store =
MEMORY` on its own connection before its `VACUUM`, so the copy SQLite stages is held in the
process's memory, never in its temporary directory (`SQLITE_TMPDIR`, else `TMPDIR`, else `/var/tmp`,
which can be a small partition): no setting is process-global, a crash leaves nothing behind and no
reaper is needed. It rewrites only a file that holds free pages, so a rerun reclaims what a skipped
reclamation left. Measured on a copy after both deletes, with both temporary directory variables
naming no directory: 0.44 s, 580 MB to 92 MB, 94 MB more peak memory and no file opened outside the
store's own. Without that memory the `VACUUM` rolls back, the deletes stay, and the verb prints the
failure beside its counts. Both verbs that call it hold the step lock their probe took, under
live-writer facts that are dead (`LiveWriterFacts::reasons` empty), so two reclaimers never meet;
`--derived --force-live` skips the probe and its lock as today, at the operator's risk. An appender
that arrives anyway waits for the `VACUUM`'s write lock inside the busy timeout (5000 ms,
`crate::sqlite::open_connection`).

**THE OPERATOR IS TOLD WHAT THE MIGRATION SHEDS.** The bare `rigger reset` menu's `--derived` line
(`reset_menu`, `derived_menu_line`) reads `Store::count_derived`, so its count never drifts from
what `--derived` deletes: on a store holding derived events it names the events and files the
migration sheds into ledger entries and the flag; on a store holding none it says there is no
derived event to shed; a server-backed store's line is unchanged. `count_derived_duplicates` and
`DerivedPreview` go. `rigger validate`'s log-bloat advisory (`bloat_advisory_for`) reads the same
count: a store holding any derived event is named with the events and files left and `rigger reset
--derived` as the migration; a store holding none prints nothing, and a store that is not sqlite
prints nothing, as `bloat_advisory_for` does there today, since it has no migration to name.
`measure_derived_duplication`, `DerivedDuplication` and `BLOAT_DUPLICATION_THRESHOLD` go. The menu's
`--runs` line keeps previewing the graph prune alone; archiving is not previewed.

**THE STORE REFUSES A DERIVED APPEND.** Both adapters' `append` (`sqlite.rs` and `kurrentdb.rs`
under `crates/rigger-store-sqlite/src/eventstore/`) refuse a batch holding any DERIVED event,
through `retention::class_of`, naming the type and writing no event of the batch. The refusal lives
at the adapters, the seam every writer passes; a check in a caller, a wrapper or a test double is
NOT this refusal. A contract test whose property is about the store (group lookup, ordering, any
type) is re-expressed over `GenerationIngested` entries and keeps running on both backends;
criterion 4's unit does this for the group lookup's reference test, since its ledger reading is what
that test now pins. A test that needs rows written before this spec runs on sqlite only, inserting
them with raw SQL through one PRE-LEDGER ROW HELPER per test boundary (the root `tests/common/`, a
crate's `test_support`). The first unit whose change needs a helper at a boundary builds it there,
and later units reuse it: criterion 4 builds the root one, since its sinks leave the index-lag,
`--derived`, menu and bloat tests of `tests/cli.rs` without derived rows, and moves those tests onto
it; criterion 10 moves every other test that records a derived event through a store onto a helper,
building any boundary's that is still missing. No KurrentDB store is migrated by this spec, so no
KurrentDB test seeds pre-ledger rows.

**A FINISHED RUN'S EPISODES MOVE TO GIT.** A run is the span of the run stream from its `RunStarted`
to the next `RunStarted`, the boundary `run::current_run` applies. The PRELUDE is the span below the
first `RunStarted`, which belongs to no run and is never the current run. A span (a run or the
prelude) is PENDING when it holds a live episodic event and a `RunStarted` above it exists, whatever
its `RunArchived` state, so a span interrupted after its `RunArchived` stays pending; a store with
no `RunStarted` has no pending span. EVERY PENDING SPAN IS ARCHIVABLE, with no liveness condition:
once the next `RunStarted` is recorded, nothing a spawn or driver of an earlier span can still do
reads that span's episodic rows from the live log, since each such path (`rigger result`, `prompt`,
`scratch`, `resume-unit`, `status`, the hooks, the dash and `conductor::run`) reads the current run
through `read_current_run`, or `progress.db`. A still-running spawn of an archived span therefore
meets what it meets today: `rigger prompt` finds no request of it in the current run, and its late
`rigger result` is recorded in the current run as an orphan (`result_advisories`). One domain use
case, `archive::archive_run` (`crates/rigger-domain/src/archive.rs`), archives one span as one read
hands it, in this order: it serializes the span's episodic events in position order (Notes), writes
the bytes as a git blob under the span's ref through a new `RunArchive` port, reads them back
through the ref and compares them byte for byte, appends one `RunArchived` (Notes) unless the span's
latest `RunArchived` already names that blob, then deletes exactly those log positions in one
transaction through a new port method `EventStore::delete_archived(stream, positions)`. The delete
holds the store's write lock for that transaction, measured on a copy of this store at 0.27 s for
the prelude's 2,929 rows (63 MB) and 0.07 s for the largest run's, so an appender that arrives
meanwhile, a worker's `rigger result` included, waits inside the 5000 ms busy timeout and lands. A
SPAN DESCRIPTOR (the ref, the group, the `run` field) names each span, and `archive_run` and both
readers are span-kind-agnostic over it (criteria 11 and 13); criterion 15 adds only the prelude's
descriptor and its branch of `archive_pending`. Within a store a span is identified by the position
of its `RunStarted`, which the log guarantees unique, never by its run id, which it does not; refs
are repository-wide, so a run's ref is `refs/rigger/archive/<store>/run/<position>`, at the fixed
width Notes gives so the refs list in run order, and the prelude's
`refs/rigger/archive/<store>/prelude`, where `<store>` is the event id of the store's first
`RunStarted`, a knowledge event this spec never deletes, so a recreated store or a second store in
one repository gets its own namespace. A descriptor is built only from a lowercase UUID, as
`Event::new` mints for every `RunStarted` (the log stores an id as handed, and another can carry `/`
or `..` and name a ref outside the namespace); any other id yields none, which `archive_pending`
returns as a permanent skip naming it. The group stays keyed by position alone
(`archive/run/<position>`, `archive/prelude`), since a group lives in one store's log, and no
decimal position equals `prelude`. Knowledge events stay. A span holding no episodic event is not
pending: it writes no ref and no event. A failed ref write or read-back, the only way a span stays
pending after a trigger, deletes nothing and appends no `RunArchived`; the span costs one span read
per trigger, and every trigger names it with its error. The blob id is the archive's digest.
Archiving changes no fold: no step or one-shot fold reads an earlier run's episodic event (spec
101). THE READER AUDIT: the readers of `run::read::read_current_run` read the current run's slice
and knowledge types by type; the critique store, the canary stream and `progress.db` are other
stores. `cmd_playbooks`, `stats_lines` without `--all`, `read_model_drift`, `reset_menu`,
`refuse_derived_reset_if_live`, `read_run_units_or_why`, `read_order_signatures` and `reset_runs`'
closure read materialize the whole run stream and use only knowledge types or the current run, so
archiving shrinks what they read and changes none of their answers (deleting rows only lowers a
stream's running revision maximum). Three whole-stream reads change: `cmd_replay` and `stats_lines
--all` read through `read_history` (criterion 13) and `read_graph_index_lag` reads by type
(criterion 5). A console tab meets nothing either: its provider holds no earlier span's row.
`RunArchive` is declared in the domain beside `EventStore`; its one adapter, in
`crates/rigger-worktree-git`, runs `git hash-object -w --stdin`, `git update-ref --stdin` and `git
cat-file blob`. Git compresses the blob, so no compression crate is added. Git is the retention
system: the ref is local until the operator pushes it, and no blob rigger wrote ever loses its last
ref by rigger's hand. BACKEND SCOPE: the sqlite store deletes; the KurrentDB adapter answers each of
the three archive port methods (`delete_archived` and the two reads below) with an
unsupported-operation error. Every `EventStore` implementation (both adapters, `Namespaced`,
`FoldingStore`, the test doubles) gains the three methods in criterion 11's unit.

**FINDING A PENDING RUN READS THE INDEX, NOT THE BACKLOG.** A span is a range of log POSITIONS: from
its `RunStarted`'s position up to the next `RunStarted`'s (the prelude: from the stream's start up
to the first `RunStarted`'s), whatever revision a row carries. Two new port reads, both criterion
11's and both anchored on position exactly as `read_stream_typed` is:
`EventStore::first_of_types(stream, from, types)` answers the position of the oldest event at or
after log position `from` whose type is named, never its data (on sqlite one seek of
`idx_events_stream_type` per named type), and `EventStore::read_span_typed(stream, from, to,
selection)` reads the events at log positions from `from` below `to` that `selection` admits, in
position order. `archive_pending` takes the current run's boundary from `last_position(RunStarted)`
(none: nothing is pending), anchored on the position of the event at that revision as
`read_stream_typed` anchors `from`, and the first run's from `first_of_types(0, [RunStarted])`, then
the oldest episodic event from position 0: none, or one at or past the current boundary's position,
means nothing is pending; one below the first boundary makes the prelude pending (criterion 15;
before it, the search starts at the first boundary); otherwise it steps `first_of_types(..,
[RunStarted])` forward to the run whose span holds that event. So a row is archived only when it
lies below the current boundary in position order: a current-run row a stale writer reissued at a
low revision (`Error::OutOfOrder`) lies above the boundary and in no earlier span. Only when
something is pending does it read the first `RunStarted`'s id for the ref's store component. It
reads each span alone through `read_span_typed` with `TypeSelection::Except` of the derived types,
archives it or names why not, and asks again from the span's upper boundary. A trigger with nothing
pending costs three port reads, each index-only, and a backlog is held one span at a time.

**AN INTERRUPTED ARCHIVE COMPLETES.** The serialization is a pure function of the rows, so the next
archive of an interrupted span re-serializes its live episodic events to the same blob. The
`RunArchive` write issues one `git update-ref --stdin` transaction by the ref's state: a missing ref
gets `create <ref> <blob>`; a ref already naming that blob is left as it stands; a ref naming
another blob `<old>` (a diverged copy, CONSTRAINTS WALK) gets `update refs/rigger/aside/<old> <old>`
then `update <ref> <blob> <old>`, which keeps `<old>` reachable, is a no-op on an aside ref already
naming it, and applies neither line when the ref moved meanwhile (git exits 128: `cannot lock ref
'<ref>': is at <x> but expected <old>`). The archive then proceeds as for any span (read-back,
`RunArchived`, delete), its result naming the span, the ref, the blob moved aside and its aside ref.
A `RunArchived` carries `META_GROUP` (`archive/run/<position>` or `archive/prelude`) and a replay
key naming its blob (Notes), so the group lookup answers the span's latest `RunArchived` and its
blob without reading the stream: one naming the same blob is not appended again, and one naming
another blob is followed by a new one, which readers and validate take as the span's. The delete is
one transaction and no row enters a span below the current boundary (sqlite gives each append a
position above every one it has used, `AUTOINCREMENT`, and the migration rewrites rows in place into
knowledge), so a span's episodic rows are all live or all gone. The prelude resumes as a run does.

**THE ARCHIVE IS READ ON DEMAND.** `archive::read_archived(ref, blob)` resolves the ref, checks that
it names `blob`, parses the rows and yields the events in position order; a missing ref or a
different blob is an error naming the ref. Criterion 11 owns it: `archive_run`'s read-back hands it
the ref and blob it is about to record, `read_history` those the span's latest `RunArchived`
records. One domain function, `archive::read_history` (criterion 13's), over `&dyn EventStore` and
`&dyn RunArchive`, serves both readers the spans they name. It reads the live rows first and the
`RunArchived` index second, the log-prefix argument `read_run` makes. A span whose range holds an
episodic row in that live read is served from those rows and its ref is not resolved; a span with
none is served from `read_archived` of its latest `RunArchived`, its archived events interleaved by
position with its live knowledge rows. No span is read from both sides, and an archive whose delete
lands between the two reads appended its `RunArchived` before that delete, so the index read finds
it; a pending span (interrupted after its `RunArchived`, or whose ref was deleted by hand) reads
complete with no missing-ref error. `rigger replay <run>` (`cmd_replay`, `src/cli/mod.rs`) resolves
the run id to its `RunStarted`, the first match as `baseline_run_slice` takes it today, reads that
span through `read_history` before it slices the baseline, and refuses naming the ref when it is
missing; a later `RunStarted` sharing that run id is not addressed by id, and no run id names the
prelude, as today. `rigger stats --all` (`stats_lines`) reads every span, the prelude's included,
through `read_history` and prints one line naming each missing ref. Neither command folds a
`RunArchived` or a `GenerationIngested`: `metrics::project` (`crates/rigger-domain/src/metrics.rs`),
which both fold, ignores every type it does not name, and `replay_trajectory` keeps only
`SpawnResult` and `GateVerdict`, so the `RunArchived` an archive appends to the current run changes
no line of their output. No step, one-shot command or rebuild reads an archive.

**ARCHIVING RUNS AT TWO TRIGGERS.** One function, `archive_pending`
(`crates/rigger-driver/src/archiving.rs`), finds the pending spans as above and hands each to
`archive::archive_run` in position order. It takes its store only wired through `FoldingStore`, by
its parameter type, so an unfolded `RunArchived` cannot be written: each one writes its `applied`
row, and the graph prune `reset --runs` runs after it meets no hole (`Projector::prune` refuses a
graph that owes its rebuild). It returns an `ArchiveOutcome` (criterion 14's, every variant), which
is DATA: the totals, each span named (a failed write or read-back with git's error, a blob moved
aside), a permanent skip (a store id no descriptor is built from, a store that cannot delete, a
project outside a git repository) or the transient skip of a held lock. ONE render function in
`src/cli/run.rs` turns an outcome into lines in two cadences: the DRIVER cadence prints one line per
named span and nothing else (a permanent skip is the project's property, and the next trigger
retries a held lock); the RESET cadence prints every outcome, each skip once, and the totals; a skip
is reported only when there was something to skip. WHETHER THIS PROJECT CAN ARCHIVE is one decision
function beside it: a store that cannot delete (any but sqlite), where `delete_archived` would fail
at every trigger, and a project outside a git repository, where `git hash-object -w` would, are
PERMANENT skips; criterion 16 owns the decision and the render. Every caller reaches the archive
through one wiring helper, `run_archiving(held)` in `src/cli/run.rs`, criterion 17's whole: it calls
the decision, pairs `archive_pending` with the git adapter and applies the LOCK RULE: handed the
step lock its caller holds, it archives; handed none, it takes the lock without waiting around the
archive alone, and returns the transient skip when another process holds it. The step lock alone
serializes archivers; without it two could each find no `RunArchived` for a span and each append
one. The conductor's trigger: at the start of every `conductor::run`, once
`run_store::ensure_started` or the driver's own pinned or fresh mint has fixed the current run, it
calls the archiving handle `Deps` carries and hands the outcome back; unwired, it archives nothing.
`rigger step` (`cmd_step`) wires the helper with the step lock it holds for the whole step; `rigger
run` (`run_cli`), `rigger serve` and `rigger workflow` (both `run_workflow`) wire it with no lock,
each printing the outcome in the driver cadence; the replay's isolated re-drive and the canary, on
isolated stores, wire none. `rigger reset --runs` (`reset_runs`) runs, in order: its probe
(`live_writer_facts` over the current run's slice), which takes the step lock and keeps it to the
end; the helper, handed that lock, its outcome printed in the reset cadence; its ONE whole-stream
closure read, as `reset_runs` documents it, now after the archive, when the stream no longer holds
the backlog; `close_landed_units` over the current run's slice; the graph prune and the graph file's
compaction as today; then `Store::reclaim_space`, only while the probe still holds the lock and its
live-writer facts are dead, since an appender that outlasts the `VACUUM`'s busy timeout fails and
does not retry, which would lose a live run's `rigger result`, else one line saying reclamation was
skipped and why, the archive and its delete standing. It prints one line with the bytes reclaimed;
on a store that is not sqlite the archive is a permanent skip and the reclamation prints its own
skip line. The conductor trigger never reclaims: a later append reuses the freed pages. A driver
that dies holding the lock releases it with its process, and the next trigger completes the
interrupted span. The first trigger on an older store pays the whole backlog once, about 9 s for
this store's 349.9 MB serialized, so no budget knob is added; the documented pre-run `rigger reset
--runs` normally pays it. The `--runs` text says the same: its usage text in `src/main.rs`, the flag
list `reset_modes` prints, the `rigger-reset-store` skill and the `--runs` guidance in
`crates/rigger-domain/src/docs.rs` describe the archive, its reclamation and skip and the lock rule.

**VALIDATE NAMES A LOST ARCHIVE REF.** A deleted archive ref is recoverable only while git still
holds its blob: the `RunArchived` records the blob id, so one `git update-ref` restores the ref, and
git prunes an unreachable object once its grace period passes. `rigger validate` runs before every
launch, so its report is the notice that arrives inside that window. It reads every `RunArchived`
through one `EventStore::read_stream_typed` of that type, keeps each span's latest, lists the refs
under its own store's namespace, `refs/rigger/archive/<store>/`, with their blob ids through
`RunArchive::list` (one `git for-each-ref` process), and compares the two as sets. For the spans
whose latest `RunArchived` names a ref that is missing or names another blob, the prelude's
included, and whose recorded `first` position holds no live episodic row (one `first_of_types` from
`first` each; a pending span's ref is rewritten by the next trigger), it asks git whether it still
holds each recorded blob through `RunArchive::holds` (one `git cat-file --batch-check` process over
all of them), so the advisory starts at most two git processes (one `list`, one `holds`) whatever
the span count. A span whose blob git holds prints one advisory line: the span (its run id and
position, or `prelude`), the full ref, the recorded blob id and the restore command: `git update-ref
<ref> <blob>` for a missing ref, and for a ref naming another blob `<old>` the two instructions the
`RunArchive` write issues, `printf 'update refs/rigger/aside/<old> <old>\nupdate <ref> <blob>
<old>\n' | git update-ref --stdin`. Obeying either reaches a ref naming the recorded blob, which is
silent, and leaves no blob unreachable; a ref moved meanwhile makes git refuse and write nothing.
The spans whose blob git no longer holds are named together on ONE line, their count, positions and
full refs, as lost for good, with no command, a line accepted to repeat since the log still records
archives nothing can restore. The advisory never fails validate; criterion 19 adds both methods to
the port and its git adapter. No `RunArchived`, a project outside a git repository and a KurrentDB
store print nothing; the advisory writes and carries nothing, so a repeated validate prints the same
lines. The `rigger-reset-store` skill names the advisory and its lines.

**CONSTRAINTS WALK, decided.**
- *Revert:* a file reverted to earlier content records a new entry with that generation, and it
  resolves from the object database or the tree by generation.
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the later
  fold is a re-recording, and its dating is an instance of WHAT A REBUILD REPRODUCES.
- *Cold start:* nothing is carried in memory between processes; the sinks' check asks the store's
  group lookup and `graph.db`, and pending runs are found from the type index.
- *Existing data:* an older store keeps its derived events readable, folds them as spec 101 decided
  and answers the sinks' check from them until `rigger reset --derived` migrates it; a `graph.db`
  built before this spec needs no rebuild, since no fold rule or projection version changes.
- *Recreated store:* a store recreated in the same repository, or a second store beside it, has
  another first `RunStarted` and so another namespace; the old store's refs stay in git untouched
  and unreferenced, which is the retention rule.
- *Copied store:* a copied or restored `events.db` shares its namespace; a diverged timeline's span
  finds its ref naming the earlier timeline's blob and moves it aside, and the abandoned copy's
  validate then reports that ref. Two diverged copies in one repository are kept consistent no
  further: nothing is lost, each older blob staying under its aside ref.

**STATE PLACEMENT.** The ledger and the archive index are the log (`GenerationIngested`,
`RunArchived`); the class of a type is code (`retention`); the archived bytes are git; `graph.db` is
a projection. A sidecar file listing archived runs, an in-memory set of ingested generations, a
`.rigger/` marker or a cache of resolved blobs is NOT an implementation of any of them;
`LoggedGenerations` is a per-process memo of the log's latest generation, not the ledger.

**OUT OF SCOPE.** `progress.db`, streams other than the run stream, the superseded ledger entries
(each stays live), archiving and migration on KurrentDB, a deleted `graph.db` (forbidden by
`crates/rigger-domain/src/docs.rs`, unchanged here) and everything of spec 108.

## Notes (non-criteria)

`GenerationIngested { prefix, file, generation, blob, excluded }`: `prefix` is `gc`, `gd` or `gw`;
`blob` is 40 lowercase hex digits, or empty for an entry with no blob, recorded when the path held
no file; `excluded` is true only for a `gc` batch lowered as an out-of-line test module.

`RunArchived { run, ref, blob, events, bytes, first, last }`: `run` is the run id, or empty for the
prelude, which no run owns; `ref` is `refs/rigger/archive/<store>/run/<position>` or
`refs/rigger/archive/<store>/prelude`, where `<store>` is the event id of the store's first
`RunStarted` and `<position>` is the global position (`Position`, a `u64`) of the span's
`RunStarted`, written in decimal zero-padded to 20 digits, the width of the largest `u64`; `events`
and `bytes` count what the blob holds; `first` and `last` are the lowest and highest archived
positions. It carries `META_GROUP` `archive/run/<position>` or `archive/prelude` and the replay key
`<group>@<blob>#<events>`, the ledger entry's key form.

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
- The gates cannot see the KurrentDB half of criteria 4 and 10 where the contract suite's container
  is unreachable; the adjudicator demands that run's evidence.

## Done when

- [ ] a test proves THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under `src/` or `crates/` declares sits in exactly one class list, and folding each episodic type leaves the live projection unchanged,
  asserted by a source scan under `tests/` and a fold test in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, with no type of the four cross-run lists
  classified episodic. This criterion OWNS `retention`, `class_of`, the two new type constants and
  the addendum amendment; what a class causes is criteria 10 and 11's, NOT this one's.
- [ ] a test proves THE ENTRY AND ITS BATCH FOLD AS ONE: `apply_generation` folds a resolved batch at the entry's position and installs its generation, installs none for an unresolved one, and changes no fact for a re-recording,
  asserted at the graph seam in `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` with
  hand-built entries and no sink, with the unresolved fold retiring the prior generation's facts,
  `Projection::current_generation` answering each state and the ledger form of `FoldingStore`
  writing one `applied` row per entry. This criterion OWNS `apply_generation` with its resolved,
  unresolved and re-recording rules, `Projection::current_generation` and the ledger form of
  `FoldingStore`; the rebuild is criterion 3's and the sinks criterion 4's, NOT this one's.
- [ ] a test proves THE GRAPH REBUILDS FROM LEDGER AND TREE: a `graph.db` rebuilt by `rigger setup` from entries that all resolve, recorded by one process, equals the one their incremental folds built on spec 101's comparison surface,
  with the incremental side folded from hand-built entries through the ledger form of
  `FoldingStore`, over generations that drop a design link and a code entity, with an entry whose
  blob git does not hold resolved from a tree file that extracts to its generation, a deleted file's
  entry with no blob resolved from no bytes and a tree that is not a git repository resolving from
  its files; and, over a log holding an entry no source resolves, the rebuild reaching the same live
  facts and current generations for every identity whose latest entry resolves, a fact re-asserted
  across a superseded unresolved entry dated by the next resolved one, and the unresolved entry
  counted in the report. This criterion OWNS re-extraction, the three `(path, bytes, excluded)`
  functions, resolution by generation and its fallback outside a repository, the unresolved count
  and its report line and the rule of WHAT A REBUILD REPRODUCES; the fold rule is criterion 2's, the
  sinks and the restoration they make criterion 4's and the migration criterion 7's, NOT this one's.
- [ ] a test proves PERCEPTION IS A LEDGER ENTRY: ingesting a changed file through either ingest sink appends one `GenerationIngested` whose generation and blob come from the bytes it read, and no derived event,
  asserted through a recording store at `RunCtx::emit_keyed_batch` and at `ingest_tree`, with an
  unchanged file recording nothing, a file whose generation the log holds and `graph.db` does not
  recorded again, an identity a rebuild left unresolved restored by the next `rigger graph build`, a
  deleted file recording an entry with no blob, an index lowering that lags the file's bytes
  recording the bytes' generation, a tree that is not a git repository recording the blob id `git
  hash-object` gives, a batch whose key names no identity failing the emit and recording nothing, a
  revert A, B, A recording three entries and leaving A's facts, a long-lived run restoring an
  identity a rebuild left unresolved at its next pass, and the contract suite's group lookup
  answering a ledger entry on both backends. This criterion OWNS both sinks' write path and hash,
  `ingest::batch_is_current`, `LoggedGenerations`, what remains of `ReplayKeys`, the ledger reading
  of `latest_generation`, the root pre-ledger row helper and the moves of the tests its sinks break;
  the fold rule is criterion 2's, the rebuild and the three `(path, bytes, excluded)` functions
  criterion 3's, the index-lag advisory criterion 5's and the refusal criterion 10's, NOT this
  one's.
- [ ] a test proves THE LEDGER ANSWERS THE INDEX-LAG ADVISORY: `rigger validate` names a sampled file whose current bytes extract to a generation other than its latest entry's or other than `graph.db`'s current one,
  and names no file whose bytes extract to the generation both hold, asserted in `tests/cli.rs`,
  with an out-of-line test module whose entries record its boundary batch not named and the light
  lane's stub sampling nothing. This criterion OWNS the ledger reading of
  `project_scoped_latest_generations`, `graph_index_lag`'s two-sided comparison and validate's typed
  read; the `(path, bytes, excluded)` functions are criterion 3's, the sinks' entries criterion 4's,
  `Projection::current_generation` criterion 2's, the bloat advisory criterion 9's and the
  archive-ref advisory criterion 19's, NOT this one's.
- [ ] a test proves RECLAMATION STAGES IN MEMORY: `Store::reclaim_space` on a sqlite store holding free pages rewrites the file smaller and reports the bytes reclaimed, with its own connection reporting `temp_store` as memory,
  asserted in `crates/rigger-store-sqlite/src/eventstore/sqlite.rs`, with a file holding no free
  pages left unrewritten. This criterion OWNS `Store::reclaim_space` and `prune_derived_index`'s
  call of it; the verbs that call it are criteria 7 and 18's, NOT this one's.
- [ ] a test proves MIGRATION CONVERTS IN PLACE: `rigger reset --derived` rewrites each identity's latest derived batch's first row into its entry, deletes every other derived row and reports the counts,
  over three generations of a file, a deleted file, an identity whose latest recording is already a
  ledger entry above derived rows, an out-of-line test module and an unkeyed derived event, the
  unkeyed count on its own line, asserted in `tests/cli.rs`, with each rewritten entry at its
  batch's first position, every identity's earliest surviving recording dated at its earliest
  recorded valid-time, the test module's entry recording `excluded: true`, nothing written to
  `graph.db`, a rebuild of the migrated store reaching the live facts and current generations the
  store held before for the files the tree holds, the bytes reclaimed reported and nothing to shed
  when run again. This criterion OWNS the migration, `Store::shed_derived`, `Store::count_derived`,
  the deletion of `prune_derived_index` and its oracle tests and the `--derived` text; the
  reclamation is criterion 6's, the menu line and the bloat advisory criteria 8 and 9's, the rebuild
  criterion 3's and the pre-ledger row helper criterion 4's, NOT this one's.
- [ ] a test proves THE RESET MENU PREVIEWS THE MIGRATION: bare `rigger reset` on a store holding derived events prints the count of events and files `rigger reset --derived` then sheds, and says there is no derived event to shed once it has,
  asserted in `tests/cli.rs`. This criterion OWNS the menu's `--derived` line; the count and the
  migration are criterion 7's, the bloat advisory criterion 9's, NOT this one's.
- [ ] a test proves THE BLOAT ADVISORY NAMES THE MIGRATION: `rigger validate` on a store holding any derived event prints one warning naming the events and files left and `rigger reset --derived`, and prints none once migrated,
  asserted in `tests/cli.rs`. This criterion OWNS `bloat_advisory_for`'s new reading; the count and
  the migration are criterion 7's, the index-lag advisory criterion 5's and the archive-ref advisory
  criterion 19's, NOT this one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`). This criterion OWNS the refusal and
  moving every remaining test that records a derived event through a store onto a pre-ledger row
  helper, building any boundary's still missing; the sinks that stop emitting and the root helper
  are criterion 4's, NOT this one's.
- [ ] a test proves A RUN'S EPISODES ARE ARCHIVED: archiving a run's span writes its episodic events under `refs/rigger/archive/<store>/run/<position>`, records one `RunArchived` and deletes exactly those events, its knowledge events staying,
  asserted against `archive::archive_run` over a sqlite store and a fixture git repository, with a
  span holding no episodic event writing no ref and no event, a `RunArchive` double failing its
  write deleting nothing, a second store in the same repository archiving the same positions under
  its own namespace, and a first `RunStarted` id that is not a lowercase UUID yielding no span
  descriptor. This criterion OWNS the span-kind-agnostic use case, the span descriptor with its
  store component and that validation, the `RunArchived` shape and its group and replay key,
  `read_archived`, the `RunArchive` port and its git adapter's `create` write,
  `EventStore::delete_archived`, `first_of_types` and `read_span_typed`, both anchored on position;
  the write to an existing ref and the resume are criterion 12's and every trigger criteria 14, 15,
  17 and 18's, NOT this one's.
- [ ] a test proves AN INTERRUPTED ARCHIVE COMPLETES: an archive stopped after its ref was written, and one stopped after its `RunArchived` was recorded, are each completed by the next archive of the span after a read-back of the blob,
  with no second ref, no second `RunArchived` and every episodic event of the span gone, a span
  whose ref names another blob archived after that blob is kept under `refs/rigger/aside/<blob>`
  with its result naming the ref, both blobs and the aside ref and an aside ref already naming that
  blob left as it is, asserted against `archive::archive_run` with a store double that fails on
  command and a fixture git repository. This criterion OWNS the resume and the write's two
  existing-ref states; the first pass is criterion 11's and every line criterion 16's, NOT this
  one's.
- [ ] a test proves THE ARCHIVE READS BACK: `rigger replay <run>` and `rigger stats --all` print, over a store whose earlier run is archived, the bytes of standard output they print over that store before archiving,
  with the `RunArchived` standing in the current run, a span archived and not yet deleted printing
  those bytes too, and each naming the missing ref of a span whose rows are gone, asserted in
  `tests/cli.rs`; and `read_history` serving once, from its live rows, a span archived between its
  live read and its index read, over a store double that archives during the first read, and a
  pending span whose ref was deleted with no missing-ref error, asserted in
  `crates/rigger-domain/src/archive.rs`. This criterion OWNS `read_history`, its read order and both
  commands' calls of it; `read_archived` itself is criterion 11's and validate's archive-ref
  advisory criterion 19's, NOT this one's.
- [ ] a test proves A CONDUCTOR RUN ARCHIVES ITS PREDECESSORS: a `conductor::run` wired with an archiving handle archives every pending earlier run of the run stream and leaves the current run's events live,
  asserted through `conductor::run` over a fixture repository, with each `RunArchived` holding its
  `applied` row in `graph.db`, an unwired conductor archiving nothing, a run interrupted after its
  `RunArchived` completed, a current-run row at a reissued low revision left live, a span with no
  descriptor returned as a permanent skip naming the id, and a trigger with nothing pending issuing
  three port reads, each index-only, counted on a recording store; each case is asserted on the
  outcome as data. This criterion OWNS `archive_pending` and its `FoldingStore`-only store
  parameter, `ArchiveOutcome` with every variant, the `Deps` handle and the conductor trigger; the
  descriptor and its validation are criterion 11's, the prelude criterion 15's, the decision and the
  render criterion 16's, `run_archiving` and the drivers' wiring criterion 17's and the reset
  trigger criterion 18's, NOT this one's.
- [ ] a test proves THE PRELUDE IS ARCHIVED: a wired `conductor::run` on a store holding episodic events below its first `RunStarted` archives them under `refs/rigger/archive/<store>/prelude` with one `RunArchived` whose `run` is empty,
  deleting them while the knowledge below that boundary stays, asserted through `conductor::run`
  over a fixture repository, with a prelude archive interrupted after its `RunArchived` completed at
  the next trigger, `rigger stats --all` printing the same bytes before and after, and a store with
  no `RunStarted` archiving nothing. This criterion OWNS only the prelude's span descriptor (its
  span, ref, group and empty `run` field) and its branch of `archive_pending`; every run's archive
  is criteria 11 and 14's and the drivers' wiring criterion 17's, NOT this one's.
- [ ] a test proves A PROJECT THAT CANNOT ARCHIVE IS SKIPPED: the decision answers a permanent skip naming why for a store that cannot delete and for a project outside a git repository, and the render prints each outcome's lines in both cadences,
  asserted in the tests of `src/cli/run.rs` over hand-built outcomes, with the driver cadence
  printing one line per named span (a failed write, a blob moved aside) and none for a skip or for
  nothing pending, and the reset cadence printing every outcome, each skip once, and the totals.
  This criterion OWNS the decision function, the render and every line's text; the store-id skip is
  criteria 11 and 14's, `run_archiving` and the drivers' wiring criterion 17's and the reset's
  reclamation lines criterion 18's, NOT this one's.
- [ ] a test proves EVERY DRIVER ARCHIVES UNDER THE STEP LOCK: `run_archiving` handed no lock archives a pending span when the step lock is free and releases it, and returns the transient skip when another process holds it,
  asserted in the tests of `src/cli/run.rs` and through `rigger step` in `tests/cli.rs`, with a
  project that cannot archive archiving nothing, `cmd_step` wiring `Deps` through that helper with
  the lock it holds and `run_cli` and `run_workflow` with none, each printing the outcome in the
  driver cadence. This criterion OWNS `run_archiving` whole (the decision's call, the lock rule, the
  `archive_pending` call) and the three drivers' wiring; the decision and the render are criterion
  16's, the conductor trigger criterion 14's and the reset trigger criterion 18's, NOT this one's.
- [ ] a test proves `rigger reset --runs` ARCHIVES: on a store whose earlier runs were never archived, it archives each pending span before its whole-stream closure read and prints the archived totals,
  with the graph prune meeting no hole, the file smaller on disk and the bytes reclaimed printed,
  reclamation skipped with a line saying why while a live writer holds, and a project outside a git
  repository named once, asserted in `tests/cli.rs` as the reset cadence's caller, and a store that
  is not sqlite printing the archive's permanent skip and the reclamation's skip line, asserted over
  a store double in the tests of `src/cli/hygiene.rs`. This criterion OWNS the reset trigger, its
  order, its reclamation call and lines, and the `--runs` text; the render is criterion 16's,
  `run_archiving` criterion 17's, `reclaim_space` criterion 6's, the `--derived` text criterion 7's
  and the skill's archive-ref advisory text criterion 19's, NOT this one's.
- [ ] a test proves VALIDATE NAMES A LOST ARCHIVE REF: `rigger validate` on a store with an archived run whose ref was deleted prints one advisory naming the run, the ref, the recorded blob and `git update-ref`, and still exits 0,
  asserted in `tests/cli.rs`, with a ref naming another blob printed with the aside transaction, a
  pending span whose ref was deleted not reported, the prelude's ref covered, spans whose blob git
  no longer holds named on one lost-for-good line with their full refs and no command, a missing ref
  of another store in the same repository not reported, a store whose refs all match printing
  nothing and a ref restored by either printed command silent again; and the advisory calling
  `RunArchive::list` once and `RunArchive::holds` at most once whatever the span count, counted on a
  double of the archive port in the tests of `src/cli/validate.rs`. This criterion OWNS the
  advisory, its lost-for-good line, `RunArchive::list`, `RunArchive::holds` and the skill's text on
  them; naming a missing ref at read time is criterion 13's, the index-lag advisory criterion 5's
  and the bloat advisory criterion 9's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This criterion OWNS only the lanes over the integrated result.
