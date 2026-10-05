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
type criterion 1 declares). Criterion 3 needs 2, so no tree records an entry a rebuild cannot fold.
Criterion 4 needs 3, whose `(path, bytes, excluded)` functions the sinks reuse. Criterion 5 needs 4,
whose entries its advisory reads. Criterion 6 needs nothing. Criterion 7 needs 4 and 6. Criteria 8
and 9 need 7 alone: criterion 7 owns `Store::count_derived` beside `Store::shed_derived`, the one
selection both read. Criterion 10 needs 5 and 7. Criterion 11 needs all ten. The spec is launched on
rigger-run. A test a unit's change breaks is that unit's to move (TEST DISPOSITIONS and DOCUMENT
EDITS, Notes). Every symbol a criterion adds has a production caller at its landing or is a `pub`
library item, so no interim tree fails the dead-code lint. A Design sentence about a later criterion
describes the integrated result, reached by earlier units' tests only through fixtures: on a tree
holding 3 and not 4 no sink records an entry, so the rebuild's report line is reachable only from
hand-built entries and its promise of restoration holds from criterion 4 on. On a tree holding 4
or 7 and not 5, `rigger validate`'s index-lag advisory still reads derived events alone
(`project_scoped_latest_generations` skips every other type), so it samples no identity whose
recordings are all ledger entries, every migrated one included, and reports no lag for it, and it
compares an identity still holding derived events against the latest of them, until criterion 5
lands. On a tree holding 7 and not 8 or 9, `count_derived_duplicates` and `DerivedPreview` stand
without the oracle tests criterion 7 deletes, and the menu line and the bloat advisory still word
the compaction through `plan_derived_prune`. Criterion 10 refuses a derived append only after
criterion 7 ships the migration, so on sqlite a refusing binary can always migrate; a KurrentDB
store keeps its derived events readable and unmigrated.

**THE INVARIANT, amended here so no unit has to.** Section 2.1 of
`docs/architecture-addendum-context-management.md` and its non-goal "Do NOT prune the event log" now
read: the log is the source of truth for KNOWLEDGE and the tree for STRUCTURE; the graph is
rebuildable from the log and the tree, as THIS binary extracts it (WHAT A REBUILD REPRODUCES).
Criterion 1's unit edits those passages and the others DOCUMENT EDITS gives it, and says why in one
paragraph.

**THREE CLASSES OF EVENT, decided by type.** A new domain module, `retention`
(`crates/rigger-domain/src/retention.rs`), holds the classes under `ingest`'s feature gate, so the
console's `core` build of `rigger-domain` still builds. DERIVED is `ingest::DERIVED_INDEX_TYPES`.
EPISODIC is `retention::EPISODIC_TYPES`, the Notes list: types no cross-run fold reads whose fold
changes neither the live projection nor the fold state (`FileTouched` and `GateVerdict` through
no-op arms, the rest through none), writing only their `applied` row. KNOWLEDGE is
`retention::KNOWLEDGE_TYPES`: every type of `run::read::CARRY_OVER_TYPES`,
`run::read::ADOPTION_TYPES`, `run::MINT_DECISION_TYPES` and `run::RUN_CLOSURE_TYPES`; every type
outside the derived index whose fold arm changes the graph (`SpawnResult`, `AliasDefined`,
`AliasUnresolved`, `CommunityAssigned`, `ConceptDerived`, `ConceptRealized`); and the new type
`GenerationIngested`, whose `TYPE_` constant, payload type and parse `retention` declares, with ONE
constructor, `GenerationIngested::event(n)`, building the whole event (payload, `META_GROUP` and
replay key, Notes) from the five fields and the batch's event count. The group lookup
(`EventStore::latest_in_group`), `latest_generation` and `project_scoped_latest_generations` read
identity and generation from the replay key as `derived_key_parts` cuts it; the fold and the rebuild
read the payload. The offline pass results are knowledge: a pass reads the graph at one moment,
which the tree cannot re-derive. `retention::class_of(type_)` answers DERIVED, then EPISODIC, and
KNOWLEDGE for every other type, so a type no list names (this store holds 43 `ReviewVerdict` events
no constant declares) is kept live and never refused. Every `TYPE_` constant under `src/` and
`crates/` sits in exactly one of the three lists, asserted by a source scan in `tests/`, so a new
type is classified the day it is added: a `TYPE_` constant that neither the enumeration above nor
the Notes list names is added to `KNOWLEDGE_TYPES`, and EPISODIC is exactly the Notes list.

**THE ENTRY AND ITS BATCH FOLD AS ONE.** The `Projection` port gains `apply_generation(entry,
batch)`, where `batch` is a function it calls at most once and which answers the entry's batch or
that it is unresolved. The port is ungated and compiles in the `core` build, where `retention` does
not, so `entry` is the entry's `Event`: the store-gated sqlite projector parses its payload through
`retention`, and the generic fold's refusal compares the event's type with `retention`'s constant.
It has three outcomes, each one transaction: (a) an entry whose position
`applied` already holds folds nothing and never calls `batch` (`fold_new`'s per-position guard); (b)
a RE-RECORDING, an entry naming its identity's current generation, writes only its `applied` row and
never calls `batch`; (c) any other entry writes its `applied` row and calls `batch`. A resolved
batch folds event by event at the entry's position and valid-time through the existing `fold`
(`advance_generation`, the event's arm with its `fresh` head, `retire_unheld_nodes`), so spec 101's
generation rule applies unchanged and installs the entry's generation. An entry no source resolves
folds NOTHING: the identity's facts and current generation stay as they stood, and the sinks'
two-sided check heals it, since the log's latest generation is then not the graph's current one and
the next ingest of the file records again. `Projection::current_generation(identity)`, a new port
read of the `generations` table, answers it. `FoldingStore` gains a ledger form, the only fold an
entry reaches: `Projection::apply` and `apply_batch` refuse a `GenerationIngested`, naming
`apply_generation`, and write no `applied` row, a lost fold like any other that marks `graph.db`
owing its rebuild. `rigger emit` already refuses the new type (`EMITTABLE_TYPES`). A failed or
refused fold writes no `applied` row (criterion 2's), as a process dying between append and fold
leaves none, and `rigger setup` pays that hole through the ledger fold (criterion 3's).

**THE REBUILD RE-EXTRACTS THE LEDGER.** `rigger setup`'s rebuild (`rebuild_owed_graph`,
`src/cli/setup.rs`, over `Projector::rebuild`) folds the log's live selection as spec 101 decided
and hands each `GenerationIngested` to `apply_generation` with a function that re-extracts its
batch. One TOTAL function per half takes `(path, bytes, excluded)`, the bytes possibly absent, and
returns the batch the walk lowers today. For `gc`, UTF-8 bytes `extract::extract` parses under
`registry::for_path`'s grammar yield `extract_events` over `for_extraction(.., excluded)` and,
unless excluded, `proof_events`; every other input (no bytes, non-UTF-8 bytes, a path with no
grammar, a failed extraction) yields exactly `empty_structural_boundary_event(path, "unknown",
false)`, as `file_batches` gives a path the index lacks. For `gd`, `file_batch`'s concept and link
extraction of UTF-8 bytes, else the empty batch. For `gw`, the parse `config_store::load_workflow`
applies to the file's bytes (`parse_yaml_naming_unknown_keys`, then the stage names), split out as a
bytes form that `load_workflow` keeps calling after its file read, then
`workflowdef::extract_events`, else (no bytes, bytes that are not UTF-8, a failed parse) the empty
batch, as `project_events` answers. `excluded` changes only a parsed `gc` batch. An empty batch keys
no event, so it has no generation, resolves no entry and is never recorded (`batches_within` and
`project_batches` skip it). Criterion 3 owns them; the sinks reuse them. ONE ROOT: every entry's
path is relative to `tree_root(store_dir)` (`src/cli/mod.rs`), the git top-level of the directory
holding the store's `.rigger/`, else that directory, and every reader of the tree uses it: both
sinks, whose walk roots (`git_repo()` else `.` for `rigger graph build`, `deps.repo` for a run) it
equals today, the rebuild's tree source and `git cat-file` process, the advisory, the migration's
reads and symbols index, and `in_walk_scope`. The walk's scope (`walk_guarded`) reads the tree's
committed `.gitignore` files, so it is no predicate of the path alone: a path outside it (an
integration's file list can name one) is handed no bytes by the sinks, whose entry for it names no
blob, the rebuild's source 2 and the advisory, all asking `grounder::in_walk_scope(root, path)`,
the one answer to whether an ingest reads the path: true for the workflow definition's path
(`.rigger/workflow.yml`, the `gw` half's one path, which the walk's hidden-entry rule skips and
every caller reads from the ONE ROOT unconditionally), and for every other path whether
`walk_guarded_within` over that one name visits it. Every path a half reads today answers true:
the `gd` half's paths and the symbols index's `gc` paths are that walk's own visits, so only an
integration's file list names a path outside it. RESOLUTION IS BY GENERATION, from three
sources in one fixed order for every entry: (1) the entry's blob from the repository's object
database (one `git cat-file --batch` process per rebuild, with `GIT_NO_LAZY_FETCH=1`, so a partial
clone's missing blob is one git does not hold; an older git behaves as it does, and the process's
adapter owns the variable unasserted), when the entry names a blob and git holds it; (2) the tree's
file at the path; (3) no bytes. The entry resolves at the first source whose batch has the recorded
generation; the blob is where to look first, never the test. Source 3 resolves every entry recorded
from an input the function maps to the no-bytes batch, whatever became of the bytes; for `gd` and
`gw` it yields the empty batch, which resolves nothing. Outside a git repository, or when the batch
process cannot start or fails, source 1 is skipped for every entry it has not answered, so an entry
recorded from uncommitted bytes resolves while the tree's file extracts to its generation. The one
hash function applies no filter, so a file whose stored blob differs from its working-tree bytes
(end-of-line conversion, a clean filter, LFS) resolves from the tree's file while it is unchanged,
never from the object database. An entry no source resolves folds nothing (THE ENTRY AND ITS BATCH
block). The rebuild's report is ONE number, computed from the stores once it ends: the identities
whose current generation in `graph.db` is not their latest entry's generation, none held included,
and whose path holds a regular file in scope under the ONE ROOT that its half's function maps to a
batch that is not empty (every `gc` input is): exactly the ones the next default-lane ingest of the
file restores, printed with the note that the next default-lane ingest of those files restores
them (criterion 4 asserts it). A resumed rebuild prints the number a single-pass one does. A
deleted `gc` file's identity holds `gc`'s batch for no bytes once the walk's deletion ingest
recorded it, so that ingest retires the file's facts, at a rebuild as live; a `gd` or `gw` identity
whose file is gone or extracts to the empty batch is named by no ingest, so live it keeps what its
last fold left, and after a rebuild it holds what its latest resolvable entry gave, none for a
migrated identity with no earlier entry (WHAT A REBUILD REPRODUCES). An identity whose path holds no
such file is not counted: what it holds after a rebuild, nothing for a gone file with no resolvable
entry, is its correct state. The light lane compiles no extraction, so there every entry folds
nothing and the number counts every identity holding an entry whose path holds a regular file in
scope, since it cannot tell which files extract to the empty batch. The `applied` rows of shed
positions are outside spec 101's comparison surface.

**WHAT A REBUILD REPRODUCES, stated once.** An entry no source resolves is as if absent from the
fold. For every identity whose latest entry resolves, a rebuild holds the same live facts and
current generation as the incremental folds; for one whose latest entry does not, it holds those of
its latest resolvable entry, none if there is none, until the next ingest. Dating follows the fold:
a design link's or a node assertion's valid-time is that of the first entry, in log order, of the
unbroken run of folded entries that assert it up to the latest, and its recorded position (the
graph's `source`, which `assert_link` keeps at the newest recording) is that of the newest entry of
that run that folded its batch, since a re-recording folds none; a code structural edge dates at,
and is sourced from, the latest entry that folded its file's batch, in both graphs, since each
generation's `fresh` head retires the file's structural edges. When every entry of the identity
resolves and one process recorded each generation, the rebuild equals the incremental graph on spec
101's comparison surface (the live projection plus the fold state). A generation is the hash of what
THIS binary extracts (`key_batch`), so after a change of extractor, grammar or payload no source
reproduces an older entry's generation, every such entry folds nothing at a rebuild, and the derived
layer returns through the next ingest. Its instances are the only accepted differences:

- *An unresolved superseded entry:* the rebuild folds its neighbours as adjacent, so a design link
  or node assertion its batch dropped and the next resolved entry re-asserts keeps the earlier
  valid-time, where the incremental graph dated it at the re-assertion.
- *A migrated identity:* its earliest surviving recording carries the identity's earliest pre-ledger
  valid-time (MIGRATION), so a fact asserted from that recording on takes it, and a rewritten entry
  stands at the first position of its identity's latest derived batch, where the incremental graph
  kept each fact's own run start and newest row.
- *Two entries of one generation:* append and fold are not atomic, so the incremental graph dates
  the facts by whichever entry folded first, and a rebuild by the lower position.
- *An identity restored after an unresolved latest entry:* a fact its latest resolvable entry did
  not assert takes the restoring entry's valid-time and position.
- *Facts only an unkeyed pre-ledger derived event asserted:* the live graph keeps them after the
  migration sheds that event, and the first rebuild retires them, since no entry asserts them.

**PERCEPTION IS A LEDGER ENTRY.** Both ingest sinks, the run's `RunCtx::emit_keyed_batch`
(`crates/rigger-conductor/src/conductor.rs`) and `rigger graph build`'s `ingest_tree`
(`src/cli/graph.rs`), record a `GenerationIngested`, built by its one constructor (THREE CLASSES),
through the ledger form of `FoldingStore` and no derived event. A sink skips a batch only when it is
CURRENT: `ingest::batch_is_current(logged, graph, batch)`, which replaces
`ingest::batch_is_latest_recorded` at both sinks, is a pure predicate over three values handed to
it: the log's latest generation of the identity (from `LoggedGenerations` at the run's sink, from
`ingest::latest_generation` at `ingest_tree`), the graph's current generation
(`Projection::current_generation`) and the batch's generation. It answers true only when the first
two both equal the third, so an identity whose current generation is not its latest entry's is never
current. A graph that owes its rebuild, which a sink learns from `Projection::rebuild_owed`, is
handed as owed, and the predicate then answers from the log side alone, so an owed graph records at
most one entry per generation and the owed rebuild pays the fold. A sink with no projection, or
one whose graph cannot be opened, is handed "owed" too, so `batch_is_current` answers from the log
side alone and records at most one entry per generation. `batch_is_current` alone decides
the ledger write, with no key-level suppression and no wrapper store. `run::read::read_run`'s run
slice excludes `GenerationIngested` beside the derived types, so no reader of `read_current_run`
meets an entry. The run's sink reads `Projection::current_generation` on every walk and memoizes
only the log side per process (`LoggedGenerations`, filled from an identity's first
`latest_generation` answer and each entry it records). A WALK is a whole-tree walk, at most one per
`conductor::run` (the `ingested` guard stays for throughput; its revert reason goes with the
extended key set), or an integration reindex of a merge's files; each driver calls `conductor::run`
once per process, so a long-lived `rigger run` or `rigger serve` restores an identity a rebuild left
behind at the next reindex naming its file, and any process at its whole-tree walk; another
process's entry stales the memo, costing at most one re-recording (WHAT A REBUILD REPRODUCES), and a
failed append leaves it as it was. `ReplayKeys` keeps only its plain key set (`seeded`, `insert`,
`contains`) for lifecycle keyed emits, its seed excluding `GenerationIngested` beside the derived
types; its generations map, `install`, `forget` and `Ticket` go. A revert A, B, A records three
entries, two under one replay key, and ends on A's facts; no reader treats a replay key as unique.
For a batch that is not current the sink reads the file's bytes once under the ONE ROOT (an absent
path reads as no bytes; any other failed read fails that batch's emit under `sink_walked_batches`),
hashes them through the ONE HASH FUNCTION, bytes to object id, which production binds to `git
hash-object --stdin` in the ONE ROOT (`worktree::hash_blob`, `crates/rigger-worktree-git`: never
written, no filter, outside a repository too, since no SHA-1 crate is a direct dependency) and a
test binds as it needs; the conductor takes it as a new `Deps` field, `hash_blob`, so it spawns no
process, `ingest_tree` as a parameter, and the migration from `reset_derived`, which takes it as a
parameter that `cmd_reset` binds to `worktree::hash_blob`, so a test of `src/cli/hygiene.rs`'s
tests module hands it a failing one (`tests/cli.rs` drives the binary and cannot). A failed hash
fails that batch's emit and records nothing. The sink then extracts the bytes through its half's
`(path, bytes, excluded)` function (THE REBUILD block), keyed by `key_batch`; when that extraction
is itself current nothing is recorded, and otherwise, for an extraction that is not empty, the sink
records one entry naming that extraction's generation, that blob and the walk's out-of-line flag
and folds that extraction, never the walk's batch. A sink whose own extraction of the bytes it read
is the empty batch (a `gd` or `gw` file deleted, truncated or made unparsable between the walk and
the sink's read) records nothing and its emit succeeds, the identity keeping what its last fold
left. A `gc` path that holds no file records an entry with no blob and `gc`'s batch for no bytes,
which source 3 resolves. A lagging index lowering whose generation is NOT the one both sides hold is
not current, so the sink reads the bytes and records the BYTES' generation; a lagging lowering whose
generation both sides hold is current, so nothing is recorded until a reindex, and that case is
criterion 5's advisory's alone, which names the file. The group lookup
answers a ledger entry on both backends, and `latest_generation`'s type-first check admits the
ledger type. A batch whose key names no identity (none from `key_batch` does) records nothing and
fails the emit naming the key. Criterion 4 owns the sinks, and its recording-store assertions
observe three more things it owns: no entry in `read_current_run`'s slice, none in the seeded
key set, and N on the graph build line.

**THE LEDGER ANSWERS THE INDEX-LAG ADVISORY.** `rigger validate`'s graph index-lag advisory
(`read_graph_index_lag`, `src/cli/validate.rs`) reads the run stream through
`EventStore::read_stream_typed` with `TypeSelection::Only` of the derived types and
`GenerationIngested`, never the whole stream, and reads `graph.db` as the read-only surfaces do (no
open when the file is absent, as `graph_rebuild_owed_note` checks, else `Projector::open`, which
writes nothing to an owed file); with `graph.db` absent, owed or unreadable it compares against the
log side alone, as today. `read_graph_index_lag` takes the store as `&dyn EventStore`, which its
one caller, `cmd_validate`, opens through `with_project_store` and hands in, so criterion 5 asserts
on a recording store in `src/cli/validate.rs`'s tests module that the advisory makes ONE typed read
and no whole-stream read. `ingest::project_scoped_latest_generations` answers each identity's latest
generation from a derived event's key or a ledger entry's generation, and `graph_index_lag` names a
sampled file when the generation of its current bytes through its half's `(path, bytes, excluded)`
function, with `excluded` computed as the walk computes it, never taken from an entry, differs from
that generation or from `graph.db`'s current generation (`Projection::current_generation`), the two
sides the sinks' check reads; a generation hashes the whole batch, so equal generations are equal
key sets. So a file whose current generation is not its latest entry's is named.
`graph_index_lag_sample` stays `gc` only, its wording unchanged and its light-lane stub empty.

**MIGRATION CONVERTS THE BACKLOG IN PLACE.** `rigger reset --derived` keeps its two refusals (a live
writer, `refuse_derived_reset_if_live`; an owed `graph.db`) and becomes the one-time migration. It
appends nothing, folds nothing and writes nothing to `graph.db`. In ONE sqlite transaction
(`Store::shed_derived`), over the project's run stream, the one stream a sink ever appended a
derived event to, for every identity whose latest recording (a derived row or a ledger entry, keyed
alike) is a derived row, it rewrites IN PLACE the lowest-position row `plan_derived_prune` keeps for
the identity, the first row of its latest batch: position, stream, id, revision and recorded-time
stay; type, data and meta become the identity's `GenerationIngested` (that generation, the blob id
of the tree's file at that path, `excluded` from `out_of_line_test_module_files` over the index the
walk uses (`store::load(root)`, else `build_index(root, None)`, as `file_batches` builds it; `false`
in the light lane), its group and replay key, whose `#<n>` counts the distinct replay keys of that
generation among the identity's derived rows; no reader compares `n`). The caller hands
`Store::shed_derived` the blob and `excluded` as a TOTAL function from identity to (blob, excluded),
computed before the transaction under the ONE ROOT: it reads each file `Store::count_derived`
counts, only a REGULAR file it can read, and hashes those bytes through the one hash function; a
path absent, not regular, unreadable or outside `in_walk_scope`, an identity it does not know and
one recorded after that read get no blob and `false`. A failed hash fails the verb before the
transaction opens. Measured on a copy: 597 of this store's 688 identities name a regular file,
hashed in 0.91 s, 1.5 ms each. It then deletes every remaining row of a derived type in that stream,
keyed or unkeyed; a keyed row whose replay key does not parse names no identity, so it is shed and
counted with the unkeyed rows. The same selection backs the read-only count of ONE set:
`Store::count_derived` answers the derived events shed, how many are unkeyed, and the FILES (keyed
identities) holding one. EARLIEST SURVIVING RECORDING: in the same transaction, every identity that
sheds derived rows ends with its earliest surviving recording carrying the identity's earliest
recorded valid-time, the rewritten row when no ledger entry of the identity precedes it and
otherwise its earliest ledger entry, re-dated in place. Identity and generation are cut from each
row's replay key (`derived_key_parts`), never its group: 42,563 keyed derived rows here carry no
`META_GROUP`. The rewrite keeps every column a uniqueness rule covers (the primary key,
`UNIQUE(stream, revision)`). A spec 101 rebuild records in `applied` only the positions
`plan_derived_prune` keeps, so an incremental graph and a rebuilt one both hold the rewritten row
applied; a batch is one append with no knowledge event inside it, so the entry stands on the same
side of every alias and knowledge event as the batch it replaces (its dating is an instance of WHAT
A REBUILD REPRODUCES). On this store: 688 rows rewritten and 228,812 deleted, with the write lock
held 1.9 s on a copy, inside the 5000 ms busy timeout an appender waits. A crash rolls the
transaction back and a rerun starts over; a rerun after success says there is no derived event to
shed, and still calls `Store::reclaim_space` and prints its line. An entry with no blob resolves at
a rebuild from sources 2 and 3 alone; when neither does, the rebuild holds the identity's latest
resolvable entry's facts, none for a migrated identity with no earlier entry, while in the live
graph a deleted `gc` path's next ingest records the batch for no bytes. An out-of-line test module's
entry records `excluded: true`; one whose generation the tree's flag no longer reproduces folds
nothing at a rebuild until the next ingest. An unkeyed derived event names no identity
(`keyed_derived_event`), so no entry re-asserts it; this store holds none. So a rebuild of the
migrated store equals the live graph over the facts a keyed recording asserts, and the facts only an
unkeyed event asserted are the named exception (WHAT A REBUILD REPRODUCES). The migration reports
the entries converted (which can be fewer than the files), the rows shed and, on its own line, the
unkeyed rows shed, then reclaims space (`Store::reclaim_space`) and reports the bytes reclaimed.
`prune_derived_index` and its compaction (`PrunedDerived`, its per-type report, its injection seam)
go; the live selection a rebuild folds (`read_live_selection`) is unchanged, so an unmigrated store
still folds each identity's latest derived generation. A console tab meets nothing: its provider
reads the current run, so `serve_console_stream`'s floor guard never fires. Sqlite only, as today.
The `--derived` text, the passages DOCUMENT EDITS gives criterion 7, describes a one-time migration
that converts each file's latest batch into a ledger entry and leaves no derived event behind.

**RECLAMATION STAGES IN MEMORY.** `Store::reclaim_space`, split out of `prune_derived_index`'s
post-commit reclamation (`compact_in_place`, its injectable compacting step, its on-disk after, its
failure report), is the only reclamation; it reports against the on-disk size (main file plus
`-wal`) its caller measured BEFORE its transaction opened, the bytes the log lost across the
command, as `the_reclamation_the_command_reports_is_the_space_the_file_actually_lost` pins;
criterion 6 owns it, and `prune_derived_index` calls it until criterion 7 removes that function. It
sets `PRAGMA temp_store = MEMORY` on the `Store`'s own connection before its `VACUUM`, left there
for that connection's life, so the copy SQLite stages is held in the process's memory, never in its
temporary directory (`SQLITE_TMPDIR`, else `TMPDIR`, else `/var/tmp`, possibly a small partition):
no setting is process-global and a crash leaves nothing to reap. It rewrites only a file that holds
free pages, so a rerun reclaims what a skipped reclamation left. Measured on a copy after the
migration's delete, with neither temporary directory variable naming a directory: 1.44 s, 581 MB to
445 MB, 454 MB more peak memory, no temporary file. Without that memory the `VACUUM` rolls back, the
deletes stay, and the verb prints the failure beside its counts. `rigger reset --derived` calls it
holding its probe's step lock under dead live-writer facts (`LiveWriterFacts::reasons` empty), so
two reclaimers never meet; `--force-live` skips the probe and lock as today, at the operator's risk,
and an appender waits for the `VACUUM`'s write lock inside the 5000 ms busy timeout.

**THE OPERATOR IS TOLD WHAT THE MIGRATION SHEDS.** The bare `rigger reset` menu's `--derived` line
(`derived_menu_line`) reads `Store::count_derived`, never drifting from what `--derived` deletes: on
a store holding derived events it names the "derived events of N files" and the flag; on a store
holding none it says there is no derived event to shed; a server-backed store's line says the
migration does not run there, without the word compaction. `count_derived_duplicates` and
`DerivedPreview` go, criterion 8's. `rigger validate`'s log-bloat advisory (`bloat_advisory_for`)
reads the same count: a store holding any derived event is named with its "derived events of N
files" and `rigger reset --derived` as the migration; a store holding none, or not sqlite, prints
nothing, as today. `measure_derived_duplication`, `DerivedDuplication` and
`BLOAT_DUPLICATION_THRESHOLD` go, criterion 9's.

**THE STORE REFUSES A DERIVED APPEND.** Both adapters' `append` (`sqlite.rs` and `kurrentdb.rs`
under `crates/rigger-store-sqlite/src/eventstore/`) refuse a batch holding any DERIVED event,
through `retention::class_of`, naming the type and writing no event of the batch. The refusal lives
at the adapters, the seam every writer passes; a check in a caller, a wrapper or a test double is
NOT this refusal. A contract-suite test of a store property is re-expressed over ledger entries on
both backends (TEST DISPOSITIONS). A PRE-LEDGER ROW HELPER, one per test boundary (the root
`tests/common/`, a crate's `test_support`), inserts rows written before this spec with raw SQL, on
sqlite alone; until criterion 10 a test still appends a derived event through a store, so criterion
10 is the first unit needing one and builds each, and no KurrentDB test seeds pre-ledger rows.

**THE LIGHT LANE, decided here so no unit has to.** `--no-default-features` compiles no extraction
and no symbols index. Criteria 1, 2, 6, 8, 9 and 10 assert nothing lane-dependent. Criterion 3's
resolved folds, resolution sources, equality and dating run in the default lane only; the light lane
asserts each hand-built entry folding nothing, the hole paid and the report's number counting every
identity holding an entry whose path holds a regular file in scope. Criterion 4's sink assertions
run in the default lane only, since the light lane compiles the run's sink out and `rigger graph
build` walks nothing; the light lane asserts that build recording no entry and no derived event and
exiting 0, and the contract suite's ledger-entry case on both backends. Criterion 5's named and
unnamed files run in the default lane only; the light lane asserts the stub that samples nothing
and names no file, and the reference test over ledger entries. Criterion 7's `excluded: true` and
rebuild-equality clauses run in the default lane only; the light lane asserts every other clause,
each entry's `excluded: false` and the rebuild of the migrated store counting every converted
identity whose path holds a regular file in scope; that clause observes criterion 3's number, and
the default lane's rebuild-equality clause criterion 3's rebuild, as fixtures of the migrated store,
and neither owns any part of their rule. Criterion 11 is the two lanes themselves.

**CONSTRAINTS WALK, decided.**
- *Revert:* a file reverted to earlier content records a new entry with that generation, and it
  resolves from the object database or the tree by generation.
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the later
  fold is a re-recording. Two entries of different generations folded out of log order can leave the
  lower one current while the log's latest is the higher, so `batch_is_current` finds the batch not
  current and the next walk records again; both are instances of WHAT A REBUILD REPRODUCES.
- *Cold start:* nothing is carried in memory between processes; the sinks' check asks the store's
  group lookup and `graph.db`. On a store with no entries every batch is not current, so the first
  ingest reads, hashes and extracts every in-scope file once in the sink, beside the walk's index
  lowering: a second extraction of a file once per new generation, never per walk, serially, with
  no worker pool. The hashing is the measured 1.5 ms a file (0.91 s for 597 files, MIGRATION), the
  extraction costs one more extraction of the tree, and each walk reads
  `Projection::current_generation`, one primary-key read, for each file, beside the log side's
  group lookup, which the run's sink memoizes per process.
- *Existing data:* an older store keeps its derived events readable, folds them as spec 101 decided
  and answers the sinks' check from them until `rigger reset --derived` migrates it; a `graph.db`
  built before this spec needs no rebuild, since no fold rule or projection version changes.
- *Output streams:* the rebuild's report, the migration's counts and the menu line print on standard
  output, as today, and `rigger graph build`'s `graph build: ingested N code-ingest event(s)` line
  keeps its stream and its meaning, N counting the batch events of each appended entry that is not a
  re-recording, folded or not, beside the existing fold-loss clause; and the bloat and index-lag
  advisories on standard error; no line reaches `rigger step`'s standard output.

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
`blob` is the object id as git prints it for the repository's object format, or empty for an entry
with no blob, recorded when the path held no file; `excluded` is true only for a `gc` batch lowered
as an out-of-line test module. Its group is `<prefix>/<file>` and its replay key
`<prefix>/<file>@<generation>#<n>`, both built by `GenerationIngested::event(n)`.

TEST DISPOSITIONS. Each existing test the spec breaks or re-homes, the criterion whose change breaks
it FIRST and so moves it, and its disposition ("helper" is the pre-ledger row helper of the test's
boundary):

| Test file or named group | First broken by | Disposition |
|---|---|---|
| `crates/rigger-conductor/src/replay_keys.rs`: the generations map, `install`, `forget`, `Ticket` | 4 | deleted: in-memory first-sight seeding of derived keys dies with the map |
| conductor tests of `emit_keyed_batch`, `ingest_project_batches`, `ingest_files_into_graph` reading derived events back | 4 | re-expressed over ledger entries |
| `crates/rigger-domain/src/ingest.rs` tests of `batch_is_latest_recorded` | 4 | re-expressed over the pure `batch_is_current` |
| `tests/dedup_seeding_periphery.rs`; the `rigger graph build` tests of `tests/cli.rs` reading derived events back | 4 | re-expressed over ledger entries, sqlite |
| `tests/group_lookup_periphery.rs` but its namespace test, `tests/change_path_revert_periphery.rs` and `a_graph_build_whose_fold_is_lost_to_a_lock_says_so_and_the_next_build_refuses`, each seeded through a sink | 4 | re-expressed over ledger entries, sqlite |
| `graph_index_lag*` tests (`crates/rigger-grounder/src/ingest.rs`, `src/cli/validate.rs`) | 5 | re-expressed over ledger entries and the graph's current generation |
| `latest_generation_answers_what_the_reference_answers_on_the_same_log` (contract suite) | 5 | re-expressed over ledger entries, both backends |
| `a_compaction_that_fails_after_the_commit_still_reports_what_was_deleted`, `a_rerun_reclaims_the_space_a_failed_reclamation_left_behind` (`sqlite.rs`) | 6 | moved onto `Store::reclaim_space`'s injectable step |
| `discipline_names_reset_derived_as_the_event_logs_own_prune` (`docs.rs`), `the_committed_operator_documents_ship_the_derived_prunes_guidance` | 6 | staging sentences re-expressed as held in memory; the `--derived` wording re-expressed by 7 |
| kept properties: `tests/reset_derived_live_writer_guard_periphery.rs` (both refusals), `reset_derived_on_a_backend_that_cannot_compact_fails_loudly_naming_the_backend_it_needs` (sqlite only), `the_prune_reaches_only_the_namespace_it_was_handed_and_matches_that_prefix_literally`, `reset_accepts_each_mode_at_most_once_and_composes_the_two_in_either_order`, `each_reset_mode_sheds_only_its_own_accumulation_and_composing_them_does_exactly_both`, `the_reclamation_the_command_reports_is_the_space_the_file_actually_lost`, the console floor tests of `tests/dash_console_stream_periphery.rs` | 7 | re-homed onto `rigger reset --derived`, the console tests over a hand-built gap |
| the compaction's selection, carries and per-type report (the prune tests of `sqlite.rs`, `tests/compaction_generations_periphery.rs`, `tests/reset_derived_compaction.rs`, `tests/reset_derived_compaction_periphery.rs`), every preview- or measurement-versus-prune comparison (`measure_derived_duplication_counts_superseded_generations_as_the_prune_selects_them`, `bare_reset_previews_superseded_generations_and_the_real_prune_removes_exactly_that`) and the menu-agreement tests (`bare_reset_on_a_populated_store_reports_measured_counts_matching_a_real_prune_and_mutates_nothing`, `reset_composes_derived_with_runs_bare_reset_previews_it_and_an_unknown_mode_still_refuses`, the three `count_derived_duplicates_*` of `tests/reset_menu_previews_periphery.rs`) | 7 | deleted: the compaction's selection and its preview die; `read_live_selection`'s tests stay |
| the other menu tests of `tests/reset_menu.rs` | 8 | re-expressed over `Store::count_derived` |
| the log-bloat tests of `tests/validate_advisories.rs` | 9 | re-expressed over `Store::count_derived` |
| contract-suite tests of a store property on derived fixtures | 10 | re-expressed over ledger entries, both backends |
| `the_group_lookup_answers_each_project_namespace_only_its_own_recording`, the console tests' derived fixtures | 10 | record a `GenerationIngested` or another non-derived type |
| every other test appending a derived event through a store (root and crate) | 10 | seeded by the helper, sqlite |

DOCUMENT EDITS. Each passage the spec makes false, the criterion that rewrites it and the tests that
pin its text:

| Passage | Rewritten by | Pinned by |
|---|---|---|
| `docs/architecture-addendum-context-management.md` section 2.1, its non-goal "Do NOT prune the event log" and its "the whole graph ... is rebuildable from the log" | 1 | none |
| `crates/rigger-domain/src/docs.rs` and `skills/rigger-reset-store/SKILL.md` saying `graph.db` regenerates from `events.db` alone | 1 | none |
| where the vacuum copy is staged (WHAT IT COSTS TO RUN): `docs.rs`, `skills/using-rigger/SKILL.md`, `docs/handbook/using-rigger.md` | 6 | the two document tests of TEST DISPOSITIONS |
| the `--derived` text: usage in `src/main.rs`, the `reset_modes` flag list, the `rigger-reset-store` skill (procedure and anti-move), "Event log hygiene" of the using-rigger skill and handbook, the `--derived` guidance in `docs.rs`, `live_writer_refusal` and `cmd_reset`'s server-backed refusal, which says the migration does not run there and no longer advises pruning the server store | 7 | the same two document tests and the refusal tests of TEST DISPOSITIONS |
| the comment of `serve_console_stream`'s floor guard citing `prune_derived_index` | 7 | none |

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
  event, or a keyed one whose replay key does not parse, names no identity and is shed and counted
  by the migration.
- Every rebuild is deterministic: the same rows, the same tree and the same object database yield
  the same graph.
- The gates cannot see the KurrentDB half of criteria 4, 5 and 10 where the contract suite's
  container is unreachable; the adjudicator demands that run's evidence.

## Done when

- [ ] a test proves THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under `src/` or `crates/` declares sits in exactly one class list, and folding each episodic type leaves the live projection unchanged,
  asserted by a source scan under `tests/` and a fold test in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, with no type of the four cross-run lists
  classified episodic and the constructor's event parsing back to its five fields under its group
  and replay key. This criterion OWNS `retention` with its feature gate, `class_of`, the
  `GenerationIngested` constant, payload type, parse and constructor and the passages DOCUMENT EDITS
  gives it; what a class causes is criterion 10's, NOT this one's.
- [ ] a test proves THE ENTRY AND ITS BATCH FOLD AS ONE: `apply_generation` folds a resolved batch at the entry's position and installs its generation, installs none for an unresolved one, and changes no fact for a re-recording,
  asserted at the graph seam in `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` with
  hand-built entries and no sink, with an unresolved entry leaving the identity's facts and current
  generation as they stood, an entry whose position the `applied` ledger holds folding nothing and
  never calling `batch`, `Projection::current_generation` answering each state and the ledger form
  of `FoldingStore` writing one `applied` row per entry, the generic fold refusing a
  `GenerationIngested` naming `apply_generation` and marking the graph owed, and a failed ledger
  fold, each writing no `applied` row. This criterion OWNS `apply_generation` with its three
  outcomes, that a failed or refused fold writes no `applied` row, `Projection::current_generation`,
  the generic fold's refusal of the type and the ledger form of `FoldingStore`; the rebuild is
  criterion 3's and the sinks criterion 4's, NOT this one's.
- [ ] a test proves THE GRAPH REBUILDS FROM LEDGER AND TREE: a `graph.db` rebuilt by `rigger setup` from entries that all resolve, recorded by one process, equals the one their incremental folds built on spec 101's comparison surface,
  asserted in `tests/ledger_rebuild.rs`, with the incremental side folded from hand-built entries
  through the ledger form of `FoldingStore`, over generations that drop a design link and a code
  entity, a code structural edge dated at the latest entry that folded its file's batch, with an
  entry whose blob git does not hold resolved from a tree file that extracts to its generation, a
  deleted `gc` file's entry with no blob resolved from no bytes and a tree that is not a git
  repository resolving from its files; and, over a log holding an entry no source resolves, the
  rebuild reaching the same live facts and current generations for every identity whose latest entry
  resolves, a superseded unresolved entry folding nothing, an entry appended through a plain append
  (the generic fold's refusal, which makes the graph owe its rebuild) paid by `rigger setup` through
  the ledger fold, and the report's number counting the identity whose current generation is not its
  latest entry's and whose file the tree holds, and not one whose file is gone, the same after a
  rebuild interrupted and resumed, and a workflow definition's entry whose blob git does not hold
  resolved from the tree's `.rigger/workflow.yml`. This criterion OWNS
  re-extraction, the three `(path, bytes, excluded)` functions, `in_walk_scope`, `tree_root`,
  resolution by generation and its fallback outside a repository, the paying of a fold's hole, the
  report's number and its line and the rule of WHAT A REBUILD REPRODUCES; the fold rule is criterion
  2's, the sinks and the restoration they make criterion 4's and the migration criterion 7's, NOT
  this one's.
- [ ] a test proves PERCEPTION IS A LEDGER ENTRY: ingesting a changed file through either ingest sink appends one `GenerationIngested` whose generation and blob come from the bytes it read, and no derived event,
  asserted through a recording store at `RunCtx::emit_keyed_batch` and at `ingest_tree`, with an
  unchanged file recording nothing, a file whose generation the log holds and `graph.db` does not
  recorded again, an identity a rebuild left behind restored by the next `rigger graph build`, a
  deleted `gc` file and a `gc` path outside the walk's scope each recording an entry with no blob,
  an index lowering that lags the file's bytes at a generation other than the one both sides hold
  recording the bytes' generation, a tree that is not a git repository recording the blob id `git
  hash-object` gives, a batch whose key names no identity failing the emit and recording nothing, a
  failing hash function and a read failing for a reason other than absence doing the same, a graph
  that owes its rebuild recording one entry per generation, a revert A, B, A recording three entries
  and leaving A's facts, a long-lived run restoring an identity a rebuild left behind at the next
  integration reindex naming its file, and the contract suite's group lookup answering a ledger
  entry on both backends. This criterion OWNS both sinks' write path, the hash function with
  `Deps::hash_blob` and its production binding, the pure `ingest::batch_is_current`,
  `LoggedGenerations`, what remains of `ReplayKeys`, `read_run`'s exclusion of the ledger type, the
  graph build line's count, the ledger reading of `latest_generation` and the moves TEST
  DISPOSITIONS gives it; the fold rule is criterion 2's, the rebuild and the three `(path, bytes,
  excluded)` functions criterion 3's, the index-lag advisory criterion 5's and the refusal and the
  pre-ledger row helpers criterion 10's, NOT this one's.
- [ ] a test proves THE LEDGER ANSWERS THE INDEX-LAG ADVISORY: `rigger validate` names a sampled file whose current bytes extract to a generation other than its latest entry's or other than `graph.db`'s current one,
  and names no file whose bytes extract to the generation both hold, asserted in `tests/cli.rs`,
  with an out-of-line test module whose entries record its boundary batch not named, a `graph.db`
  that owes its rebuild compared from the log side alone, and the light lane's stub sampling
  nothing; and, in `src/cli/validate.rs`'s tests, `read_graph_index_lag` over a recording store
  making ONE `read_stream_typed` read with `TypeSelection::Only` of the derived types and
  `GenerationIngested` and no whole-stream read. This criterion OWNS the ledger reading of
  `project_scoped_latest_generations`, `graph_index_lag`'s two-sided comparison, validate's typed
  read and the moves TEST DISPOSITIONS gives it; the `(path, bytes, excluded)` functions are
  criterion 3's, the sinks' entries criterion 4's, `Projection::current_generation` criterion 2's
  and the bloat advisory criterion 9's, NOT this one's.
- [ ] a test proves RECLAMATION STAGES IN MEMORY: `Store::reclaim_space` on a sqlite store holding free pages rewrites the file smaller and reports the bytes reclaimed, with its own connection reporting `temp_store` as memory,
  asserted in `crates/rigger-store-sqlite/src/eventstore/sqlite.rs`, with the bytes reported against
  a before-size its caller hands it, a failing compacting step reported and its rerun reclaiming, a
  file holding no free pages left unrewritten and `temp_store` read back on the `Store`'s connection
  after the call. This criterion OWNS `Store::reclaim_space`, `prune_derived_index`'s call of it,
  the staging passage DOCUMENT EDITS gives it and the moves TEST DISPOSITIONS gives it; the verb
  that calls it is criterion 7's, NOT this one's.
- [ ] a test proves MIGRATION CONVERTS IN PLACE: `rigger reset --derived` rewrites each identity's latest derived batch's first row into its entry, deletes every other derived row and reports the counts,
  over three generations of a file, a latest generation recorded twice, a deleted file, an identity
  whose latest recording is already a ledger entry above derived rows, an out-of-line test module, a
  path that is not a regular file, an unkeyed derived event and a keyed one whose replay key does
  not parse, the unkeyed count on its own line, asserted in `tests/cli.rs`, with each rewritten
  entry at the first position of its identity's latest derived batch, every identity's earliest
  surviving recording dated at its earliest recorded valid-time, the test module's entry recording
  `excluded: true`, nothing written to `graph.db`, a rebuild of the migrated store reaching the live
  facts the keyed recordings asserted and the current generations the store held before for the
  files the tree holds unchanged, the bytes reclaimed reported, nothing to shed when run again while
  still reclaiming, and, in `src/cli/hygiene.rs`'s tests, `reset_derived` handed a failing hash
  function failing before any row changes. This criterion
  OWNS the migration, `Store::shed_derived`, `Store::count_derived`, the deletion of
  `prune_derived_index`, the `--derived` text DOCUMENT EDITS gives it and the moves TEST
  DISPOSITIONS gives it; the reclamation is criterion 6's, the menu line and the bloat advisory
  criteria 8 and 9's, the rebuild criterion 3's and the pre-ledger row helpers criterion 10's, NOT
  this one's.
- [ ] a test proves THE RESET MENU PREVIEWS THE MIGRATION: bare `rigger reset` on a store holding derived events prints the count of events and files `rigger reset --derived` then sheds, and says there is no derived event to shed once it has,
  asserted in `tests/cli.rs`. This criterion OWNS the menu's `--derived` line, its server-backed
  wording included, the removal of `count_derived_duplicates` and `DerivedPreview` and the moves
  TEST DISPOSITIONS gives it; the count and the migration are criterion 7's, the bloat advisory
  criterion 9's, NOT this one's.
- [ ] a test proves THE BLOAT ADVISORY NAMES THE MIGRATION: `rigger validate` on a store holding any derived event prints one warning naming the events and files left and `rigger reset --derived`, and prints none once migrated,
  asserted in `tests/cli.rs`. This criterion OWNS `bloat_advisory_for`'s new reading, the removal of
  `measure_derived_duplication`, `DerivedDuplication` and `BLOAT_DUPLICATION_THRESHOLD` and the
  moves TEST DISPOSITIONS gives it; the count and the migration are criterion 7's, and the index-lag
  advisory criterion 5's, NOT this one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`). This criterion OWNS the refusal, the
  re-expression over ledger entries of the contract-suite tests about the store but the reference
  test, every pre-ledger row helper and the moves TEST DISPOSITIONS gives it; the sinks that stop
  emitting are criterion 4's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This criterion OWNS only the lanes over the integrated result.
