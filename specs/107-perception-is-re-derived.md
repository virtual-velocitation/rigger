# 107 - Perception is re-derived: the log keeps knowledge, never the derived index

**Goal:** rigger is a hive mind: its graph holds the project's whole understanding and serves each
persona the slice it needs, and the append-only log is only the persistence underneath. That log
keeps what the hive decided, learned and did; it stops holding what the hive merely perceived (the
derived index, which the tree re-derives). Measured on the 2026-10-05 store (288,636 events, 430 MB
of payload, a 580 MB file):

- The derived index (`ingest::DERIVED_INDEX_TYPES`) is 229,500 events, 79.5% of the rows, recording
  753 generations of 688 files. Every ingest records a changed file's whole extracted batch: of the
  current run's 20,927 events, 19,946 are derived events for 29 file generations. `graph.db` folds
  each batch as it is appended (`FoldingStore::append_and_fold`,
  `crates/rigger-grounder/src/ingest.rs`), no one-shot command reads them (spec 101), and the store
  accepts a derived append from any caller.
- The knowledge the hive keeps, every type neither derived nor episodic (Notes), is 37,750 events
  and 58 MB. Once this spec lands, the live log holds that knowledge, the ledger and the runs'
  episodes, and no derived event.

## Design

**UNIT ORDER AND BASE, decided here.** Criterion 1 needs nothing. Criterion 2 needs 1 (it folds the
type criterion 1 declares). Criterion 3 needs 2: the rebuild learns the ledger before any sink
writes it, so no tree records an entry a rebuild cannot fold. Criterion 4 needs 3, whose `(path,
bytes, excluded)` functions the sinks reuse. Criterion 5 needs 4, whose entries its advisory reads.
Criterion 6 needs nothing. Criterion 7 needs 4 and 6. Criteria 8 and 9 need 7 alone: criterion 7
owns `Store::count_derived` beside `Store::shed_derived`, the one selection both read. Criterion 10
needs 7. Criterion 11 needs all ten. The spec is launched on rigger-run. A test that fails because
of a unit's change is that unit's to move, whichever criterion owns the surface it asserts. A Design
sentence about a later criterion's behaviour describes the integrated result, and an earlier unit's
tests reach it only through fixtures: on a tree holding 3 and not 4 no sink records an entry, so the
rebuild's report line is reachable only from hand-built entries and its promise of restoration holds
from criterion 4 on. On a tree holding 7 and not 8 or 9, `count_derived_duplicates` and
`DerivedPreview` stand without the oracle tests criterion 7 deletes, and the menu line and the bloat
advisory still word the compaction through `plan_derived_prune`. Criterion 10 refuses a derived
append only after criterion 7 ships the migration, so on sqlite the binary that refuses one can
always migrate a store that holds them; a KurrentDB store keeps its derived events readable and
unmigrated, the sinks no longer emitting any.

**THE INVARIANT, amended here so no unit has to.** Section 2.1 of
`docs/architecture-addendum-context-management.md` and its non-goal "Do NOT prune the event log" now
read: the log is the source of truth for KNOWLEDGE and the tree for STRUCTURE; the graph is
rebuildable from the log and the tree. Criterion 1's unit edits those passages and says why in one
paragraph.

**THREE CLASSES OF EVENT, decided by type.** A new domain module, `retention`
(`crates/rigger-domain/src/retention.rs`), holds the classes. DERIVED is
`ingest::DERIVED_INDEX_TYPES`. EPISODIC is `retention::EPISODIC_TYPES`, the Notes list: the types no
cross-run fold reads and whose graph fold changes neither the live projection nor the fold state
(`FileTouched` and `GateVerdict` fold through no-op arms, the rest through none); only the `applied`
row of their position is written. KNOWLEDGE is `retention::KNOWLEDGE_TYPES`: every type of
`run::read::CARRY_OVER_TYPES`, `run::read::ADOPTION_TYPES`, `run::MINT_DECISION_TYPES` and
`run::RUN_CLOSURE_TYPES`; every type outside the derived index whose fold arm changes the graph
(`SpawnResult`, `AliasDefined`, `AliasUnresolved`, `CommunityAssigned`, `ConceptDerived`,
`ConceptRealized`); and the new type `GenerationIngested`, whose `TYPE_` constant `retention`
declares. The offline pass results are knowledge because a pass reads the graph at one moment, so
the tree cannot re-derive them. `retention::class_of(type_)` answers DERIVED, then EPISODIC, and
KNOWLEDGE for every other type, so a type no list names (this store holds 43 `ReviewVerdict` events
no constant declares) is kept live and never refused. Every `TYPE_` constant under `src/` and
`crates/` sits in exactly one of the three lists, asserted by a source scan in `tests/`, so a new
type is classified the day it is added.

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
append-then-fold body, and a ledger entry reaches the graph through no other fold:
`Projection::apply` and `apply_batch` refuse a `GenerationIngested`, naming `apply_generation`, and
write no `applied` row, so an entry appended through a plain append is a hole the owed rebuild pays
through the ledger fold. `rigger emit` already refuses the new type (`EMITTABLE_TYPES`). A process
that dies between the append and the fold leaves a hole in the `applied` ledger that `rigger setup`
pays (spec 101); criterion 2 owns it.

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
--batch` process per rebuild), when the entry names a blob and git holds it; (2) the tree's file at
the path; (3) no bytes. The entry resolves at the first source whose batch has the recorded
generation; the blob is where to look first, never the test. Source 3 resolves every entry recorded
from an input the function maps to the no-bytes batch (a deleted path, bytes that are not UTF-8, a
path with no grammar, a failed extraction), whatever became of the bytes; for `gd` and `gw` it
yields the empty batch, which resolves nothing. Outside a git repository, or when the batch process
cannot start or fails, source 1 is skipped for every entry it has not answered. So an entry recorded
from uncommitted bytes still resolves while the tree's file extracts to its generation. Both the
sinks and the migration hash the bytes as read, with no filter, so a file whose stored blob differs
from its working-tree bytes (an end-of-line conversion, a clean filter, LFS) never resolves from the
object database and resolves from the tree's file while it is unchanged. An entry no source resolves
folds unresolved, and the rebuild prints how many entries it folded so, how many of those are their
identity's latest entry, and that the next default-lane ingest that walks those files (`rigger graph
build`, or a run's project-ingest pass) restores them (criterion 4 asserts it), since the identity
holds no current generation; an identity whose file the tree no longer holds stays retired. The
light lane compiles no extraction, so every entry that is not a re-recording folds unresolved and is
counted. The `applied` rows of shed positions are outside spec 101's comparison surface.

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
write, with no key-level suppression. `run::read::read_run`'s run slice excludes
`GenerationIngested` beside the derived types, so no reader of `read_current_run` (the conductor,
the one-shot commands, the console provider of `serve_console_stream`) meets an entry, and none
needs one. The run's sink reads `Projection::current_generation` on every pass and memoizes only the
log side per process (`LoggedGenerations`: identity to the latest logged generation, filled from an
identity's first `latest_generation` answer and from each entry it records), so a long-lived `rigger
run` or `rigger serve` restores an identity a rebuild left unresolved at its next pass; another
process's entry stales the memo, costing at most one re-recording (WHAT A REBUILD REPRODUCES), and a
failed append leaves it as it was. `ReplayKeys` (`crates/rigger-conductor/src/replay_keys.rs`) keeps
only its plain key set (`seeded`, `insert`, `contains`) for the run's lifecycle keyed emits, its
run-start seed excluding `GenerationIngested` beside the derived types; its generations map,
`install`, `forget` and `Ticket` go, and an un-migrated store's derived keys are read only through
`latest_generation`. A revert A, B, A records three entries, two under one replay key, and the graph
ends on A's facts; no store or reader treats a replay key as unique: the group lookup answers the
latest by position and the rebuild folds each entry at its own. For a batch that is not current the
sink reads the file's bytes once, hashes them (`git hash-object --stdin`, never written; a hash
process that cannot start or exits non-zero fails that batch's emit under `sink_walked_batches` and
records nothing, so the next pass tries again) and extracts them through its half's `(path, bytes,
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
generation among the identity's derived rows; no reader compares `n`). The caller hands
`Store::shed_derived` the blob and `excluded` as a TOTAL function from identity to (blob, excluded),
computed before the transaction over the tree: it reads the identities `Store::count_derived`
selects, hashes the files the tree holds among them with one `git hash-object --no-filters
--stdin-paths` process (no `-w`, so it runs outside a repository too; fed only existing paths, since
a missing one aborts it), and takes `excluded` from `out_of_line_test_module_files`; an identity it
does not know, a deleted path or one first recorded after that read, gets no blob and `false`. A
hash process that cannot start or exits non-zero, a path vanishing after the existence check
included, fails the verb with git's error before the transaction opens, and a rerun starts over. It
then deletes every remaining row of a derived type under the project's prefix, keyed or unkeyed; the
same selection backs the read-only count (`Store::count_derived`: rows, unkeyed rows and
identities). EARLIEST SURVIVING RECORDING: in the same transaction, every identity that sheds
derived rows ends with its earliest surviving recording carrying the identity's earliest recorded
valid-time, the rewritten row when no ledger entry of the identity precedes it and otherwise its
earliest ledger entry, re-dated in place. Identity and generation are cut from each row's replay key
(`ingest::derived_key_parts`, as `plan_derived_prune` cuts them), never from its group: 42,563 of
this store's keyed derived rows carry no `META_GROUP`. The rewrite keeps every column a uniqueness
rule covers (the primary key, `UNIQUE(stream, revision)`), as `prune_derived_index`'s in-place
re-date does today. The kept row already has its `applied` row, and a batch is one append with no
knowledge event inside it, so a rebuild folds the entry where the live graph first folded that
generation, on the same side of every alias and knowledge event (its dating is an instance of WHAT A
REBUILD REPRODUCES). On this store: 688 rows rewritten and 228,812 deleted, with the write lock held
1.9 s on a copy, inside the 5000 ms busy timeout an appender waits. A crash rolls the transaction
back and a rerun starts over; a rerun after success says there is no derived event to shed. A path
the tree does not hold gets an entry with no blob: unless its generation is `gc`'s batch for no
bytes (a deletion already ingested, which source 3 resolves), the entry resolves nowhere at a
rebuild, whose unresolved fold retires its facts; in the live graph the next ingest naming a `gc`
path (`file_batches`) records the batch for no bytes and retires them, and no walk names a deleted
`gd` or `gw` path, as today. So an out-of-line test module's entry records `excluded: true` and
resolves at a rebuild; an entry whose generation the tree's flag no longer reproduces folds
unresolved and the next ingest restores it (THE REBUILD block). An unkeyed derived event names no
identity (`keyed_derived_event`), so no entry re-asserts it; this store holds none. The migration
reports the entries converted, the rows shed and, on its own line, the unkeyed rows shed, then
reclaims space (`Store::reclaim_space`) and reports the bytes reclaimed. `prune_derived_index` and
its generation compaction go: criterion 7's unit deletes or re-homes its tests and deletes the three
`count_derived_duplicates_*` oracle tests of `tests/reset_menu_previews_periphery.rs` that compare
the preview against it, and criterion 8's unit removes the preview they covered; the live selection
a rebuild folds (`read_live_selection`, over `plan_derived_prune`) is unchanged, so a store not yet
migrated still folds each identity's latest derived generation. A connected console tab meets
nothing: its provider reads the current run, which holds no derived row, so the floor guard of
`serve_console_stream` never fires; criterion 7 rewrites that guard's comment, which cites
`prune_derived_index`. Sqlite only, as today. The `--derived` text says the same: the usage text in
`src/main.rs`, the flag list `reset_modes` prints, the `rigger-reset-store` skill (procedure and
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
reclamation left. Measured on a copy after the migration's delete, with both temporary directory
variables naming no directory: 1.44 s, 581 MB to 445 MB and 454 MB more peak memory, with no
temporary file. Without that memory the `VACUUM` rolls back, the deletes stay, and the verb prints
the failure beside its counts. `rigger reset --derived` calls it holding the step lock its probe
took, under live-writer facts that are dead (`LiveWriterFacts::reasons` empty), so two reclaimers
never meet; `--derived --force-live` skips the probe and its lock as today, at the operator's risk.
An appender that arrives anyway waits for the `VACUUM`'s write lock inside the busy timeout (5000
ms, `crate::sqlite::open_connection`).

**THE OPERATOR IS TOLD WHAT THE MIGRATION SHEDS.** The bare `rigger reset` menu's `--derived` line
(`reset_menu`, `derived_menu_line`) reads `Store::count_derived`, so its count never drifts from
what `--derived` deletes: on a store holding derived events it names the events and files the
migration sheds into ledger entries and the flag; on a store holding none it says there is no
derived event to shed; a server-backed store's line says the migration does not run there, without
the word compaction. `count_derived_duplicates` and `DerivedPreview` go. `rigger validate`'s
log-bloat advisory (`bloat_advisory_for`) reads the same count: a store holding any derived event is
named with the events and files left and `rigger reset --derived` as the migration; a store holding
none prints nothing, and a store that is not sqlite prints nothing, as `bloat_advisory_for` does
there today, since it has no migration to name. `measure_derived_duplication`, `DerivedDuplication`
and `BLOAT_DUPLICATION_THRESHOLD` go.

**THE STORE REFUSES A DERIVED APPEND.** Both adapters' `append` (`sqlite.rs` and `kurrentdb.rs`
under `crates/rigger-store-sqlite/src/eventstore/`) refuse a batch holding any DERIVED event,
through `retention::class_of`, naming the type and writing no event of the batch. The refusal lives
at the adapters, the seam every writer passes; a check in a caller, a wrapper or a test double is
NOT this refusal. A contract test whose property is about the store (group lookup, ordering, any
type) is re-expressed over `GenerationIngested` entries and keeps running on both backends;
criterion 4's unit adds a separate contract case for a ledger entry and leaves the group lookup's
reference test (`latest_generation_answers_what_the_reference_answers_on_the_same_log`) on derived
events, which the store accepts until criterion 10; criterion 5's unit re-expresses it over ledger
entries, and criterion 10, if it lands first, moves its derived seeding onto a pre-ledger row helper
like every such test. A test that needs rows written before this spec runs on sqlite only, inserting
them with raw SQL through one PRE-LEDGER ROW HELPER per test boundary (the root `tests/common/`, a
crate's `test_support`). The first unit whose change needs a helper at a boundary builds it there,
and later units reuse it: criterion 4 builds the root one, since its sinks leave the index-lag,
`--derived`, menu and bloat tests of `tests/cli.rs` without derived rows, and moves those tests onto
it; criterion 10 moves every other test that records a derived event through a store onto a helper,
building any boundary's that is still missing. No KurrentDB store is migrated by this spec, so no
KurrentDB test seeds pre-ledger rows.

**CONSTRAINTS WALK, decided.**
- *Revert:* a file reverted to earlier content records a new entry with that generation, and it
  resolves from the object database or the tree by generation.
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the later
  fold is a re-recording, and its dating is an instance of WHAT A REBUILD REPRODUCES.
- *Cold start:* nothing is carried in memory between processes; the sinks' check asks the store's
  group lookup and `graph.db`.
- *Existing data:* an older store keeps its derived events readable, folds them as spec 101 decided
  and answers the sinks' check from them until `rigger reset --derived` migrates it; a `graph.db`
  built before this spec needs no rebuild, since no fold rule or projection version changes.
- *Output streams:* the rebuild's report, the migration's counts and the menu line print on standard
  output, as `rebuild_owed_graph`, `reset_derived` and `reset_menu` print today, and `rigger graph
  build`'s `graph build: ingested N code-ingest event(s)` line keeps its stream and its meaning, N
  counting the batch events `ingest_tree` folds, not the entries it appends; and the bloat and
  index-lag advisories on standard error beside validate's other advisories; no line reaches `rigger
  step`'s standard output.

**STATE PLACEMENT.** The ledger is the log (`GenerationIngested`); the class of a type is code
(`retention`); `graph.db` is a projection. An in-memory set of ingested generations, a `.rigger/`
marker or a cache of resolved blobs is NOT an implementation of any of them; `LoggedGenerations` is
a per-process memo of the log's latest generation, not the ledger.

**OUT OF SCOPE.** `progress.db`, streams other than the run stream, the superseded ledger entries
(each stays live), the episodic events, which this spec classifies and leaves live, migration on
KurrentDB, a deleted `graph.db` (forbidden by `crates/rigger-domain/src/docs.rs`, unchanged here)
and everything of spec 108.

## Notes (non-criteria)

`GenerationIngested { prefix, file, generation, blob, excluded }`: `prefix` is `gc`, `gd` or `gw`;
`blob` is 40 lowercase hex digits, or empty for an entry with no blob, recorded when the path held
no file; `excluded` is true only for a `gc` batch lowered as an out-of-line test module.

`retention::EPISODIC_TYPES`: `SpawnRequested`, `GateVerdict`, `GatePromoted`, `GateDemoted`,
`UnitProposed`, `BlastRadiusComputed`, `ScopeCreep`, `FileTouched`, `SpecDefect`, `ManualReview`,
`DeferredGateFailed`, `TaskAborted`, `BudgetExhausted`, `AgentProgress`, `SpawnLaunched`,
`StopFailure`. The last three are recorded in `progress.db` and classified only for completeness.

`GenerationIngested` is the spec's one new event type; its whole point is the shape of the log.

## Global constraints

- Hyphens, never em dashes, in every added line.
- One new event type and no others; no new dependency.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).
- A KEYED derived event is shed only by the migration, in the transaction that rewrites its
  identity's latest batch into a ledger entry, because the tree re-derives it; an UNKEYED derived
  event names no identity and is shed and counted by the migration.
- Every rebuild is deterministic: the same rows, the same tree and the same object database yield
  the same graph.
- The gates cannot see the KurrentDB half of criteria 4 and 10 where the contract suite's container
  is unreachable; the adjudicator demands that run's evidence.

## Done when

- [ ] a test proves THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under `src/` or `crates/` declares sits in exactly one class list, and folding each episodic type leaves the live projection unchanged,
  asserted by a source scan under `tests/` and a fold test in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, with no type of the four cross-run lists
  classified episodic. This criterion OWNS `retention`, `class_of`, the `GenerationIngested`
  constant and the addendum amendment; what a class causes is criterion 10's, NOT this one's.
- [ ] a test proves THE ENTRY AND ITS BATCH FOLD AS ONE: `apply_generation` folds a resolved batch at the entry's position and installs its generation, installs none for an unresolved one, and changes no fact for a re-recording,
  asserted at the graph seam in `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` with
  hand-built entries and no sink, with the unresolved fold retiring the prior generation's facts,
  `Projection::current_generation` answering each state and the ledger form of `FoldingStore`
  writing one `applied` row per entry, and the generic fold refusing a `GenerationIngested` naming
  `apply_generation` with no `applied` row. This criterion OWNS `apply_generation` with its
  resolved, unresolved and re-recording rules, `Projection::current_generation`, the generic fold's
  refusal of the type and the ledger form of `FoldingStore`; the rebuild is criterion 3's and the
  sinks criterion 4's, NOT this one's.
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
  failing hash process doing the same, a revert A, B, A recording three entries and leaving A's
  facts, a long-lived run restoring an identity a rebuild left unresolved at its next pass, and the
  contract suite's group lookup answering a ledger entry on both backends. This criterion OWNS both
  sinks' write path and hash, `ingest::batch_is_current`, `LoggedGenerations`, what remains of
  `ReplayKeys`, `read_run`'s exclusion of the ledger type, the graph build line's count, the ledger
  reading of `latest_generation`, the root pre-ledger row helper and the moves of the tests its
  sinks break; the fold rule is criterion 2's, the rebuild and the three `(path, bytes, excluded)`
  functions criterion 3's, the index-lag advisory criterion 5's and the refusal criterion 10's, NOT
  this one's.
- [ ] a test proves THE LEDGER ANSWERS THE INDEX-LAG ADVISORY: `rigger validate` names a sampled file whose current bytes extract to a generation other than its latest entry's or other than `graph.db`'s current one,
  and names no file whose bytes extract to the generation both hold, asserted in `tests/cli.rs`,
  with an out-of-line test module whose entries record its boundary batch not named and the light
  lane's stub sampling nothing. This criterion OWNS the ledger reading of
  `project_scoped_latest_generations`, `graph_index_lag`'s two-sided comparison, validate's typed
  read and the re-expression of the group lookup's reference test over ledger entries; the `(path,
  bytes, excluded)` functions are criterion 3's, the sinks' entries criterion 4's,
  `Projection::current_generation` criterion 2's and the bloat advisory criterion 9's, NOT this
  one's.
- [ ] a test proves RECLAMATION STAGES IN MEMORY: `Store::reclaim_space` on a sqlite store holding free pages rewrites the file smaller and reports the bytes reclaimed, with its own connection reporting `temp_store` as memory,
  asserted in `crates/rigger-store-sqlite/src/eventstore/sqlite.rs`, with a file holding no free
  pages left unrewritten. This criterion OWNS `Store::reclaim_space` and `prune_derived_index`'s
  call of it; the verb that calls it is criterion 7's, NOT this one's.
- [ ] a test proves MIGRATION CONVERTS IN PLACE: `rigger reset --derived` rewrites each identity's latest derived batch's first row into its entry, deletes every other derived row and reports the counts,
  over three generations of a file, a deleted file, an identity whose latest recording is already a
  ledger entry above derived rows, an out-of-line test module and an unkeyed derived event, the
  unkeyed count on its own line, asserted in `tests/cli.rs`, with each rewritten entry at its
  batch's first position, every identity's earliest surviving recording dated at its earliest
  recorded valid-time, the test module's entry recording `excluded: true`, nothing written to
  `graph.db`, a rebuild of the migrated store reaching the live facts and current generations the
  store held before for the files the tree holds, the bytes reclaimed reported, nothing to shed when
  run again, and a failing hash process failing the verb before any row changes. This criterion OWNS
  the migration, `Store::shed_derived`, `Store::count_derived`, the deletion of
  `prune_derived_index` and its oracle tests and the `--derived` text; the reclamation is criterion
  6's, the menu line and the bloat advisory criteria 8 and 9's, the rebuild criterion 3's and the
  pre-ledger row helper criterion 4's, NOT this one's.
- [ ] a test proves THE RESET MENU PREVIEWS THE MIGRATION: bare `rigger reset` on a store holding derived events prints the count of events and files `rigger reset --derived` then sheds, and says there is no derived event to shed once it has,
  asserted in `tests/cli.rs`. This criterion OWNS the menu's `--derived` line, its server-backed
  wording included; the count and the migration are criterion 7's, the bloat advisory criterion 9's,
  NOT this one's.
- [ ] a test proves THE BLOAT ADVISORY NAMES THE MIGRATION: `rigger validate` on a store holding any derived event prints one warning naming the events and files left and `rigger reset --derived`, and prints none once migrated,
  asserted in `tests/cli.rs`. This criterion OWNS `bloat_advisory_for`'s new reading; the count and
  the migration are criterion 7's, and the index-lag advisory criterion 5's, NOT this one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`). This criterion OWNS the refusal and
  moving every remaining test that records a derived event through a store onto a pre-ledger row
  helper, building any boundary's still missing; the sinks that stop emitting and the root helper
  are criterion 4's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This criterion OWNS only the lanes over the integrated result.
