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

**UNIT ORDER AND BASE, decided here.** The units land in the order 1 to 20, each needing the one
before it, so every interim tree is a prefix of that line. ONE rule places each position, the audit
gate's own (below): a function is owned by the criterion of its first production caller. So a split
of an existing function that keeps its caller (criterion 4) and the tree's read rule with its first
caller, the index-lag sample (criterion 3), land ahead of the rebuild (criterion 5), which first
calls the total functions over them; the readers' exclusions (7), the group lookup's ledger reading
(8) and the walk's flag (9) land ahead of the first sink that records an entry (10), each observable
on a tree whose sinks still write derived events; the rebuild lands ahead of that sink so no tree
records an entry a rebuild cannot fold; the refusals of an unfinished rebuild and a held lock (15)
land on the `--derived` verb as it stands, ahead of the migration (16), which needs the entries and
`Store::reclaim_space` (14); and the store's refusal (19) follows the migration so a refusing sqlite
binary can always migrate. The landing simulation, each position's additions with their first
production caller and what its tree holds; at every position both lanes build, each light-lane stub
landing with its function:

| Position | Adds, with its first production caller | The tree after it |
|---|---|---|
| 1 | `retention`'s lists and the payload type (constants and a type, which the audit gate does not weigh) | unchanged behaviour |
| 2 | `apply_generation` and `current_generation`, port methods landing with their implementations; the parse and `fold`'s asserter parameter, called by `fold_new` | no caller records an entry; tests fold hand-built ones |
| 3 | `in_walk_scope`, `tree_bytes`, `workflow_doc` moved (`workflowdef` calls it); first caller `graph_index_lag_sample`'s candidate test | the advisory samples no unreadable or out-of-scope recorded path |
| 4 | `events::lower_file`, `walk_exclusions` and its stub, the `gd` and `gw` bytes forms, `batch_generation`; callers `project_batches_paced`, `file_batches`, `file_batch`, `load_workflow`, `key_batch` | the walk's batches unchanged |
| 5 | the three total functions, `resolve_entry` and its stub, `BlobBatch`, `tree_root`, `fold_source`'s routing; first caller `rebuild_owed_graph` | a rebuild folds hand-built entries; both sinks write derived events |
| 6 | `perceived_generations` and the type-list parameter; first caller the report | the report prints in both lanes |
| 7 | `PERCEPTION_TYPES` in `read_run`'s two reads and the seed; existing callers | no reader meets an entry |
| 8 | the group lookup's and `latest_generation`'s ledger reading; existing callers | both sinks' checks read an entry's generation |
| 9 | `BatchSink`'s and `key_batch`'s flag; every walk and seeder | the sinks take the flag and ignore it |
| 10 | `entry_of_batch`, `batch_is_current`, `GenerationIngested::event`, `hash_blob` with `Deps::hash_blob`, the ledger form; first caller `emit_keyed_batch` | the run's sink records entries, asking the store each batch; `rigger graph build` writes derived events through `batch_is_latest_recorded` and `keyed_derived_event` |
| 11 | `LoggedGenerations`; caller the run's sink | the run's sink memoizes the log side |
| 12 | `ingest_tree`'s ledger write; removes `batch_is_latest_recorded` and `keyed_derived_event` | no sink writes a derived event |
| 13 | the two-sided advisory; removes the keys part and the parameter | the advisory reads the ledger |
| 14 | `reclaim_space`, `Reclamation`, `bytes_on_disk`; caller `prune_derived_index` | `PrunedDerived` holds the `Reclamation` |
| 15 | `rebuild_unfinished` and the rebuild lock in `reset_derived`; caller the `--derived` verb as it stands | the prune refuses an unfinished rebuild and a held lock |
| 16 | `shed_derived`, `count_derived`, `reclamation_lines`; caller `reset_derived`; removes `prune_derived_index` and `PrunedDerived` | the menu and the bloat advisory word the compaction (below) |
| 17 | `derived_count_phrase`; removes the preview | the bloat advisory still words the compaction |
| 18 | the bloat advisory's count; removes the measurement | every surface names the migration |
| 19 | the refusal with `class_of` | no derived append succeeds |
| 20 | nothing | both lanes green |

The spec is launched on rigger-run. A test a unit's change breaks is that unit's to move (TEST
DISPOSITIONS and DOCUMENT EDITS, Notes). A unit that changes a signature, a trait or a struct's
fields adapts every caller, implementor and constructor of it in the same unit with behaviour
unchanged (criterion 9's `BatchSink`, so `graph_index_lag`'s closure and the test seeders take the
flag and ignore it), and TEST DISPOSITIONS lists a test only when what it asserts changes. The audit
gate (`the_real_dead_code_ledger_is_empty`, `tests/simplification_audit.rs`) fails a production
function no production code references, `pub` or not, but passes a trait implementation's
(`is_trait_impl`) and never weighs a method a trait declares without a body, so a port method lands
with its implementations at the criterion adding it (criterion 2's two `Projection` methods), every
other function a criterion adds is referenced by production code at its landing, and one whose first
production caller lands later is owned by that later criterion. An earlier criterion whose tests
need such a function reaches it through a test-side helper under `tests/common/fixtures/` that the
owner moves into production, one function and one move (TEST DISPOSITIONS). A criterion that removes
the last production reader of an existing function, field or type removes it, and the test callers
still needing it use a test-side successor until the criterion retiring them deletes it. A Design
sentence about a later criterion describes the integrated result, reached by earlier units' tests
only through fixtures: on the trees 1 to 9 no sink records an entry, so the rebuild's report line is
reachable only from hand-built entries and its note's promise of an entry at the next ingest holds
from criterion 10 on; on the tree 1 to 10 the run's sink reads the log side from the store for each
batch, as `rigger graph build` does. On the trees 1 to 12, `rigger validate`'s index-lag advisory
still hands `project_scoped_latest_generations` the derived types, so it samples no identity whose
recordings are all ledger entries and compares one still holding derived events against the latest
of them. On the tree 1 to 16, `count_derived_duplicates` and `DerivedPreview` stand without the
oracle tests criterion 16 deletes and the menu line words the compaction through
`plan_derived_prune`; on the trees 1 to 16 and 1 to 17 the bloat advisory does too.

**THE INVARIANT, amended here so no unit has to.** Section 2.1 of
`docs/architecture-addendum-context-management.md` and its non-goal "Do NOT prune the event log" now
read: the log is the source of truth for KNOWLEDGE and the tree for STRUCTURE; the graph is
rebuildable from the log and the tree, as THIS binary extracts it (WHAT A REBUILD REPRODUCES).
Criterion 1's unit edits those passages, which state the target, and says why in one paragraph;
DOCUMENT EDITS gives each operator passage to the criterion whose change makes it false.

**THREE CLASSES OF EVENT, decided by type.** A new domain module, `retention`
(`crates/rigger-domain/src/retention.rs`), holds the classes under `ingest`'s feature gate, so the
console's `core` build of `rigger-domain` still builds. DERIVED is `ingest::DERIVED_INDEX_TYPES`,
and `retention::PERCEPTION_TYPES`, those types with `GenerationIngested`, is the one list every
reader that skips perception cites. EPISODIC is `retention::EPISODIC_TYPES`, the Notes list: types
no cross-run fold reads whose fold changes neither the live projection nor the fold state
(`FileTouched` and `GateVerdict` through no-op arms, the rest through none), writing only their
`applied` row. KNOWLEDGE is every other type: every type of `run::read::CARRY_OVER_TYPES`,
`run::read::ADOPTION_TYPES`, `run::MINT_DECISION_TYPES` and `run::RUN_CLOSURE_TYPES`; every type
outside the derived index whose fold arm changes the graph (`SpawnResult`, `AliasDefined`,
`AliasUnresolved`, `CommunityAssigned`, `ConceptDerived`, `ConceptRealized`); and the new type
`GenerationIngested`, whose `TYPE_` constant, payload type and parse `retention` declares, with ONE
constructor, `GenerationIngested::event(n)`, building the whole event (payload, `META_GROUP` and
replay key, Notes) from the five fields and the batch's event count. The group lookup
(`EventStore::latest_in_group`), `latest_generation` and `project_scoped_latest_generations` read
identity and generation from the replay key as `derived_key_parts` cuts it; the fold and the rebuild
read the payload. The offline pass results are knowledge: a pass reads the graph at one moment,
which the tree cannot re-derive. The class table is what criterion 1 asserts through the source
scan, which holds the knowledge enumeration above as its own list, and it is the classification the
episode archive (spec 114) implements when it reads EPISODIC; nothing in this spec branches on
EPISODIC against KNOWLEDGE, so `retention` declares no knowledge list, the criterion that first
reads one introducing it, and `retention::class_of(type_)` answers DERIVED for a type of the derived
list and KEPT for every other type, so a type no list names (this store holds 43 `ReviewVerdict`
events no constant declares) is kept live and never refused. Every `TYPE_` constant under `src/` and
`crates/` sits in exactly one of the three lists, asserted by a source scan in `tests/`, so a new
type is classified the day it is added: a `TYPE_` constant that neither the enumeration above nor
the Notes list names is added to the scan's knowledge list, and EPISODIC is exactly the Notes list.
Criterion 1 moves the `TYPE_` constants of `GatePromoted`, `GateDemoted`, `ScopeCreep` and
`TaskAborted`, declared only in `crates/rigger-conductor/src/conductor.rs`, into
`crates/rigger-domain/src/ledger.rs` beside `TYPE_SPEC_DEFECT`, and the conductor re-exports them,
its own `TYPE_SPEC_DEFECT` too, as it re-exports `TYPE_MANUAL_REVIEW`, so `retention` re-spells no
type string.

**THE ENTRY AND ITS BATCH FOLD AS ONE.** The `Projection` port gains `apply_generation(entry,
batch)`, where `batch` is a function it calls at most once and which answers the entry's batch, that
it is unresolved, or an error, which fails the fold. The port is ungated and compiles in the `core`
build, where `retention` does not, so `entry` is the entry's `Event`: the store-gated sqlite
projector parses its payload through `retention`, and the generic fold's refusal compares the
event's type with `retention`'s constant. It has three outcomes, each one transaction: (a) an entry
whose position `applied` already holds folds nothing and never calls `batch` (`fold_new`'s
per-position guard); (b) a RE-RECORDING, an entry naming its identity's current generation, writes
only its `applied` row and never calls `batch`; (c) any other entry writes its `applied` row and
calls `batch`. `apply_generation` answers which of the three happened, ONE enum beside `Fold` in
`crates/rigger-domain/src/contextgraph.rs`, ungated as `Fold` is, criterion 2's with the port
method, so no caller infers an outcome from an earlier read. Each outcome's transaction begins with
that `applied` insert (`record_applied`'s `INSERT OR IGNORE`), as `fold_new` begins today, so the
generation is read under the write lock and two processes folding entries of one generation meet
SINK OUTCOMES row 13, never row 15. A resolved batch folds event by event at the entry's position
and valid-time through the existing `fold` (`advance_generation`, the event's arm with its `fresh`
head, `retire_unheld_nodes`), so spec 101's generation rule applies unchanged and installs the
entry's generation. Every batch event is asserted under the ENTRY's identity and generation, never
under a key a batch event carries: the three `(path, bytes, excluded)` functions return the batch
unkeyed, `key_batch` is the grounder's, and `fold` today cuts both from each event's replay key
(`Asserter::of`, `derived_generation`), where an empty identity makes `advance_generation` return
early and install nothing. So `fold` takes the asserter as a parameter, `fold_new` handing it
`Asserter::of(e)` as today and `apply_generation` the entry's identity and generation at the entry's
valid-time, and each batch event carries the entry's position and valid-time, which `fold_event`
reads for every edge it adds; that change is criterion 2's. Outcome (c)'s resolved branch ends its
transaction exactly as `fold_new` ends: the entry's position recorded once in `applied`
(`record_applied`, which cannot guard the batch's events, since they share that position), each
batch event folded, then `relabel_owed_communities`, which labels every community `settle_node` and
the fold's arms owed through `owe_relabel_of` and empties `relabel_owed`, all in the one transaction
`Projector::transact` opens and commits; a re-recording or an unresolved entry folds nothing, so it
owes no relabel. An entry no source resolves folds NOTHING: the identity's facts and current
generation stay as they stood, and the sinks' two-sided check heals it, since the log's latest
generation is then not the graph's current one and the next ingest of the file records again.
`Projection::current_generation(identity)`, a new port read of the `generations` table, answers it.
It is a plain read of that table, as the projector's other reads are, and consults no owed mark,
which only `apply_batch` and `apply_generation` refuse on, so it answers on a `graph.db` that owes
its rebuild: there `entry_of_batch` hands the graph as owed and the predicate ignores that answer,
so an owed graph's batch takes SINK OUTCOMES row 4 or a not-current row and then row 14, and row 2
is a read that fails. `FoldingStore` gains a ledger form, the only fold an entry reaches:
`Projection::apply` and `apply_batch` refuse a `GenerationIngested`, naming `apply_generation`, and
write no `applied` row, a lost fold like any other that marks `graph.db` owing its rebuild. `rigger
emit` already refuses the new type (`EMITTABLE_TYPES`). A failed or refused fold writes no `applied`
row (criterion 2's), as a process dying between append and fold leaves none, and `rigger setup` pays
that hole through the ledger fold (criterion 5's). `Projector::apply_generation` keeps
`apply_batch`'s two guards through one inherent function both fold under: it refuses before folding
when `graph.db` owes its rebuild, and a fold that fails marks the file owed (`mark_lost_fold`) and
names `rigger setup`; the ledger form only appends, calls it and reports. `apply_generation` takes
no `FoldAccess`, whose mint would have no production caller before criterion 10, so for this one
method the rule that every fold's outcome is reported holds by convention of its one production
caller, the ledger form, which answers a `Fold` through `Fold::settle`, made public, and beside it
the outcome `apply_generation` answered when it folded; the sinks leave an entry out of N only when
that outcome is a re-recording (SINK OUTCOMES row 13). The rebuild's per-event fold (`fold_source`),
which folds through the transaction and never the port, routes an entry through the ledger fold
behind `apply_generation` as it routes every other event through `fold_new`, under the same
savepoint, `Projector::rebuild` and `fold_source` taking the re-extracting function as a new
parameter, and `check_fold_payload` judges a `GenerationIngested` by `retention`'s parse, so a
rebuild passes over an entry whose payload does not parse, as it passes over any event the fold
rejects; a re-extracted batch event's fold error is a store failure that propagates, never passed
over, since the batch is this binary's own extraction.

**THE REBUILD RE-EXTRACTS THE LEDGER.** `rigger setup`'s rebuild (`rebuild_owed_graph`,
`src/cli/setup.rs`, over `Projector::rebuild`) folds the log's live selection as spec 101 decided
and hands each `GenerationIngested` to the ledger fold behind `apply_generation` with a function
that re-extracts its batch under the entry's recorded `excluded`. One TOTAL function per half, in
the module that owns the half's extraction today so no private helper widens
(`symbols::events::bytes_batch`, `design::events::bytes_batch`, `workflowdef::bytes_batch`), takes
`(path, bytes, excluded)`, the bytes possibly absent, and returns the batch the walk lowers today.
For `gc`, UTF-8 bytes `extract::extract` parses under the grammar `registry::for_path(path, None)`
resolves, as every production caller of the index resolves it, yield `events::lower_file(path,
symbols, excluded)`, ONE function beside `extract_events` (`symbols/events.rs`) answering
`extract_events` over `for_extraction(.., excluded)` and, unless excluded, `proof_events`, which
`project_batches_paced` and `file_batches` call in place of spelling it; every other input (no
bytes, non-UTF-8 bytes, a path with no grammar, a failed extraction) yields exactly
`empty_structural_boundary_event(path, "unknown", false)`, as `file_batches` gives a path the index
lacks. For `gd`, the concept and link extraction of UTF-8 bytes split out of `file_batch`, which
keeps its disk read and calls it, else the empty batch. For `gw`, the parse
`config_store::load_workflow` applies to the file's bytes (`parse_yaml_naming_unknown_keys`, then
the stage names), split out as a bytes form that `load_workflow` keeps calling after its file read,
then `workflowdef::extract_events`, else (no bytes, bytes that are not UTF-8, a failed parse) the
empty batch, as `project_events` answers. `excluded` changes only a parsed `gc` batch. An empty
batch keys no event, so it has no generation, resolves no entry and is never recorded
(`batches_within` and `project_batches` skip it). Criterion 4 splits out `events::lower_file` and
the `gd` and `gw` bytes forms, each keeping the caller it has, and criterion 5 owns the three total
functions over them at their first production caller, the rebuild's resolver; the sinks'
`ingest::entry_of_batch` reuses them. ONE function, `ingest::walk_exclusions(root)`
(`crates/rigger-grounder/src/ingest.rs`), answers the out-of-line flag: it loads the index as the
walk does (`store::load(root)`, else `build_index(root, None)`) and answers that index with the
`gc/<path>` identities `out_of_line_test_module_files` names over it, so no identity under another
prefix is excluded; its light-lane stub, beside `graph_index_lag_sample`'s, answers
`SymbolIndex::default()`, a type both lanes compile (`symbols/mod.rs:11`), and the empty set.
`project_batches_paced` and `file_batches` call it in place of spelling the load and the set; its
fallback build is in memory only, never persisted, so on a tree with no persisted index `rigger
validate`'s advisory pays a whole-tree parse, as today (`file_batches` builds one per sampled file),
and `rigger reset --derived` pays one; criterion 4 owns it. ONE ROOT: every entry's path is relative
to `tree_root(store_dir)` (`src/cli/mod.rs`), the git top-level of the directory holding the store's
`.rigger/`, else that directory, and every reader of the tree uses it: both sinks, whose walk roots
(`git_repo()` else `.` for `rigger graph build`, a run's non-empty `deps.repo`) it equals, since
`rigger run`, `rigger step` and `rigger workflow` each open the working directory's `.rigger/` and
set `Deps::repo` to that directory's git top-level (`resolve_main_worktree_or_refuse`), empty
outside a repository, where a run walks nothing (`rigger replay` sets it empty), and criterion 10's
run sink takes its root from `Deps::repo`, the rebuild's tree source and `git cat-file` process, the
advisory, the migration's reads and symbols index, and `in_walk_scope`; criterion 10's
`ingest::entry_of_batch` calls `in_walk_scope` and criterion 16 calls `tree_root` and
`grounder::tree_bytes`, owning neither. The walk's scope (`walk_guarded`) reads the tree's committed
`.gitignore` files, so it is no predicate of the path alone: a path outside it (an integration's
file list can name one) is handed no bytes by `ingest::entry_of_batch`, whose entry for it names no
blob, and by `grounder::tree_bytes`, both asking `grounder::in_walk_scope(root, prefix, path)`, the
one answer to whether an ingest reads the path: for `gw` true for the workflow definition's path,
and for every other prefix whether `walk_guarded_within` over that one name visits it, so a `gc`
identity under a hidden directory follows the out-of-scope row of SINK OUTCOMES. Every path a half
reads today answers true: the `gd` half's paths and the symbols index's `gc` paths are that walk's
own visits, so only an integration's file list names a path outside it. ONE ungated grounder
function beside it, `grounder::tree_bytes(root, prefix, path)`, criterion 3's at its first
production caller, `graph_index_lag_sample`, which keeps a recorded `gc` path as a candidate only
when `tree_bytes` hands it bytes, in place of its `is_file` test, answers the bytes of a regular
file `in_walk_scope` admits that it can read under the ONE ROOT, and none for any other path
(outside the scope, absent, not regular, or a read that fails for any reason); the rebuild's source
2 and the report's file test, the index-lag advisory and the migration's hash pass take a file's
bytes from it alone, so for them an unreadable file hands no bytes and only a sink fails on one
(SINK OUTCOMES row 5), and its own tests prove the scope, the root and the read rule once. Criteria
5, 6, 10, 13 and 16's clauses over a `gc` path outside the walk's scope and a store in a repository
subdirectory or nested worktree assert each consumer's wiring to `grounder::tree_bytes` and the ONE
ROOT, never the rule, which criterion 3's tests in the grounder crate own. THE READ FAULT, the one
fault the read rule's test and SINK OUTCOMES row 5's test arm, is a regular in-scope file the test's
uid cannot read, the test asserting the read fails before the function or sink runs and ending there
when it succeeds, as under uid 0, since a directory at the path is outside the walk's scope; it is
ONE fixture, `tests/common/fixtures/read_fault.rs`, criterion 3's, which the grounder's and the
conductor's tests include through a `#[path]` module as they include `fold.rs` and `events.rs`, and
the root's tests reach through `tests/common`. The workflow definition's path has one spelling,
`workflow_doc`, moved out of the symbols-gated `workflowdef` into the ungated `grounder` module
beside `in_walk_scope`, `pub(crate)`, `workflowdef` calling it; criterion 3's with `in_walk_scope`.
RESOLUTION IS BY GENERATION, from three sources in one fixed order for every entry: (1) the entry's
blob from the repository's object database (one `git cat-file --batch` process per rebuild,
`worktree::BlobBatch` beside `worktree::hash_blob`, which `rebuild_owed_graph` binds into
`ingest::resolve_entry(root, entry, blobs)`, ONE grounder function in
`crates/rigger-grounder/src/ingest.rs` under `cfg(feature = "symbols")` that takes the ONE ROOT,
which `rebuild_owed_graph` binds from `tree_root(store_dir)`, and the blob source as an `Option`
(absent when source 1 is skipped), applies sources 2 and 3 itself through `grounder::tree_bytes` and
the half's `(path, bytes, excluded)` function and answers the resolved batch or none,
`rebuild_owed_graph` handing the bound function to `Projector::rebuild` and deciding nothing else;
`BlobBatch` ends by closing its standard input and waiting for the child on every exit path, never
by a signal, with `GIT_NO_LAZY_FETCH=1`, so a partial clone's missing blob is one git does not hold;
an older git behaves as it does, and the process's adapter owns the variable unasserted), when the
entry names a blob and git holds it; (2) the tree's file at the path; (3) no bytes. The entry
resolves at the first source whose batch has the recorded generation; the blob is where to look
first, never the test. An unkeyed batch's generation is ONE grounder function,
`ingest::batch_generation(batch)`, pub beside `key_batch` under its `cfg(feature = "symbols")`: the
`content_hash` of the batch's concatenated event bytes that `key_batch` computes today, which
`key_batch` calls from criterion 4 on, criterion 4 owning it at that first production caller; the
rebuild's resolver `ingest::resolve_entry` calls it from criterion 5, criterion 9 changes only
`key_batch`'s flag and criterion 10 calls `key_batch` from `ingest::entry_of_batch`. Source 3
resolves every entry recorded from an input the function maps to the no-bytes batch, whatever became
of the bytes; for `gd` and `gw` it yields the empty batch, which resolves nothing. In the default
lane, source 1 is skipped only outside a git repository or when the batch process cannot start at
all, decided ONCE before the first entry, so an entry recorded from uncommitted bytes resolves while
the tree's file extracts to its generation. A batch process that fails after it started (it dies, or
its answer is cut short, as git dies on a loose object whose body is truncated) fails the rebuild,
while an object git answers `missing` for is not held, whatever made git say so (a loose object
whose header is corrupt), and resolution falls to source 2: the batch function's error propagates
out of `fold_source`, the batch rolls back, and the rebuild stays resumable from its last committed
batch, so a resumed rebuild, given the same answer to whether the batch process starts, resolves
each entry from the sources a single pass would and prints the number a single pass does. That
failure names the object id and its remedy, restoring or removing the object, after which git
answers `missing` for it and resolution falls to source 2. The one
hash function applies no filter, so a file whose stored blob differs from its working-tree bytes
(end-of-line conversion, a clean filter, LFS) resolves from the tree's file while it is unchanged,
never from the object database. An entry no source resolves folds nothing (THE ENTRY AND ITS BATCH
block). The rebuild's report is ONE number, computed from the stores once it ends, its log side
`ingest::perceived_generations(store, stream)` (`crates/rigger-domain/src/ingest.rs`, beside
`project_scoped_latest_generations`), the ONE function making one whole `read_stream_typed` read of
`retention::PERCEPTION_TYPES`, which on a store not yet migrated holds every derived row at once, a
cost paid until `rigger reset --derived` runs, handed to `project_scoped_latest_generations` with
those types, a type-list parameter criterion 6 adds (its other callers pass the derived types,
unchanged, until criterion 13 moves them and removes the parameter), which cuts each row's replay
key and so sees a keyed derived row with no `META_GROUP`: the identities whose current generation in
`graph.db` is not the generation of their latest recording (MIGRATION), none held included, and
whose path `grounder::tree_bytes` hands bytes that its half's function maps to a batch that is not
empty (every `gc` input is; not applied in the light lane, where no extraction compiles): each of
them records an entry at its next default-lane ingest of the file (SINK OUTCOMES row 9, its fold a
re-recording of row 13 included), and a gone or out-of-scope `gc` path it does not count may record
one too, by rows 10 and 11, when an integration reindex names it; the number prints in the default
lane with a note saying the counted identities record an entry at their next ingest (criteria 10 and
12 assert it), and a zero prints without the note. A deleted `gc` file's identity holds `gc`'s batch
for no bytes once the walk's deletion ingest recorded it, so that ingest retires the file's facts,
at a rebuild as live; a `gd` or `gw` identity whose file is gone or extracts to the empty batch is
named by no ingest, so live it keeps what its last fold left, and after a rebuild it holds what its
latest resolvable entry gave, none for a migrated identity with no earlier entry (WHAT A REBUILD
REPRODUCES). An identity whose path holds no such file is not counted: what it holds after a
rebuild, nothing for a gone file with no resolvable entry, is its correct state. The `applied` rows
of shed positions are outside spec 101's comparison surface.

**WHAT A REBUILD REPRODUCES, stated once.** An entry no source resolves is as if absent from the
fold. For every identity whose latest entry resolves, a rebuild holds the same live facts and
current generation as the incremental folds, but for the instances below; for one whose latest entry
does not, it holds those of its latest resolvable entry, none if there is none, until the next
ingest; on a store not yet migrated, the identity's latest derived generation folds as spec 101
decided and stands in for its latest resolvable entry when no entry resolves. Dating follows the
fold: a design link's or a node assertion's valid-time is that of the first entry, in log order, of
the unbroken run of folded entries that assert it up to the latest, and its recorded position (the
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
  valid-time, where the incremental graph dated it at the re-assertion; a later entry of a
  generation the rebuild already holds is a re-recording there, so every fact of the identity, code
  structural edges included, is dated and sourced at the earlier entry.
- *A migrated identity:* its earliest surviving recording carries the identity's earliest pre-ledger
  valid-time (MIGRATION), so a fact asserted from that recording on takes it, and a rewritten entry
  stands at the first position of its identity's latest derived batch, where the incremental graph
  kept each fact's own run start and newest row; an identity whose latest recording was already an
  entry sheds every derived row and rewrites none, its earliest entry becoming its earliest
  surviving recording, so a rebuild sources and dates its facts at its entries alone, where the
  incremental graph kept the shed rows' positions.
- *Two entries of one generation:* append and fold are not atomic, so the incremental graph dates
  the facts by whichever entry folded first, and a rebuild by the lower position.
- *An identity restored after an unresolved latest entry:* a fact its latest resolvable entry did
  not assert takes the restoring entry's valid-time and position.
- *Facts only an unkeyed pre-ledger derived event, or a keyed one whose replay key does not parse,
  asserted:* the live graph keeps them after the migration sheds that event, and the first rebuild
  retires them, since no entry asserts them.
- *A latest generation whose kept rows span appends:* the incremental graph resolved each kept row's
  names through the aliases defined before its own append, and a rebuild folds the migrated entry
  whole at the lowest kept position, resolving every name through the aliases defined before it.
- *Entries of one identity with different generations folded out of log order:* `advance_generation`
  supersedes on any unequal generation with no position check, so the live graph holds the
  generation and facts of whichever folded last until the next walk finds the batch not current and
  records again, where a rebuild folds in log order and holds the latest by position.

**PERCEPTION IS A LEDGER ENTRY.** Both ingest sinks, the run's `RunCtx::emit_keyed_batch`
(`crates/rigger-conductor/src/conductor.rs`) and `rigger graph build`'s `ingest_tree`
(`src/cli/graph.rs`), record a `GenerationIngested`, built by its one constructor (THREE CLASSES),
through the ledger form of `FoldingStore` and no derived event; what a sink does with one batch is
SINK OUTCOMES (Notes), row by row. A batch is CURRENT when `ingest::batch_is_current(logged, graph,
batch)`, which `ingest::entry_of_batch` calls for both sinks in place of
`ingest::batch_is_latest_recorded`, removing it with `ingest::keyed_derived_event`, answers true: a
pure predicate over the log's latest generation of the identity (from the sink's log-side lookup,
`LoggedGenerations` at the run's sink and `ingest::latest_generation` at `ingest_tree`), the graph's
current generation (`Projection::current_generation`) and the batch's generation, true only when the
first two both equal the third. A graph that owes its rebuild, which `entry_of_batch` learns from
`Projection::rebuild_owed`, is handed as owed, and the predicate then answers from the log side
alone. `batch_is_current` alone decides the ledger write, with no key-level suppression and no
wrapper store. The run's sink stamps the run id (`META_RUN_ID`) on an entry as
`append_and_fold_batch` stamps every event it appends; an entry `rigger graph build` records or the
migration writes carries none, and no reader of an entry depends on it: the run id's one production
reader, `metrics::model_drift`, also needs a resolved-model stamp no entry carries.
`run::read::read_run`'s two reads, the run slice and the whole-stream read it makes when no run has
started, exclude `retention::PERCEPTION_TYPES` (criterion 7's), so no reader of `read_current_run`
meets an entry. `entry_of_batch` reads `Projection::current_generation` on every walk, and from
criterion 11 on the run's sink memoizes only the log side per process (`LoggedGenerations`),
updating the memo from the answer the function returns (looked up, or the entry's generation). A
WALK is a whole-tree walk, at most one per `conductor::run` (the `ingested` guard stays for
throughput; its revert reason goes with the extended key set), or an integration reindex of a
merge's files; a run wired to no graph or no repo walks nothing and records no entry
(`Deps::ingests`, unchanged), and `rigger graph build` fails when its graph cannot be opened, as
today. Each driver calls `conductor::run` once per process, so a long-lived `rigger run` or `rigger
serve` restores an identity a rebuild left behind at the next reindex naming its file, and any
process at its whole-tree walk. THE MEMO PROVES ITS OWN CURRENCY: a memo answer suppresses a
recording only when the log cannot have advanced for its identity since the answer was taken. The
memo is a cache of one log read and carries the proof of it, the run stream's ledger head it was
read at (`EventStore::last_position` of `GenerationIngested`, the revision of the stream's newest
entry, a head read and never a replay), and answers only while the store's head is still that one.
No store appends a derived event (THE STORE REFUSES), so an entry is the only recording that moves
an identity's latest generation and a head that stands proves no identity moved; the port's one
per-identity head is the group lookup itself, the read the memo spares, so the head it checks is the
stream's. The sink reads that head before each batch's lookup; a head that moved, as another
process's entry moves it, empties the memo, so the next batch of every identity asks the group
lookup. After its own append the sink reads the stream's entries above the memo's head
(`read_stream_typed` of `GenerationIngested`) and follows the head to its entry only when they are
that entry alone, else it empties the memo. Where the head cannot be read the memo answers nothing
and the group lookup answers, so a stale memo never leaves a recording unmade. The memo keeps a
lookup's answer when a later read of the graph's side fails (SINK OUTCOMES row 2). `ReplayKeys`
keeps only its plain key set (`seeded`, `insert`, `contains`) for lifecycle keyed emits, its seed
excluding `retention::PERCEPTION_TYPES` (criterion 7's); its generations map, `install`, `forget`
and `Ticket` go. A revert A, B, A records three entries, two under one replay key, and ends on A's
facts; no reader treats a replay key as unique. ONE function, `ingest::entry_of_batch(root, keyed,
excluded, logged, graph, hash)` (`crates/rigger-grounder/src/ingest.rs`, under `cfg(feature =
"symbols")` as the half functions are), criterion 10's at its first production caller, the run's
sink, `ingest_tree` calling it from criterion 12, its `logged` the sink's log-side lookup (identity
to the latest generation or a failure), answers SINK OUTCOMES rows 1 to 11: it cuts the identity,
calls the lookup, reads `Projection::rebuild_owed` and `Projection::current_generation` and calls
`batch_is_current`, answering nothing to record, the entry with its extraction, or the named
failure. For a batch that is not current it reads the bytes once under the ONE ROOT, extracts them
through its half's `(path, bytes, excluded)` function (THE REBUILD block) and keys that extraction
by `key_batch`; what it records is that extraction, never the walk's batch, and only for an
extraction it records does it hash the bytes, through the ONE HASH FUNCTION, bytes to object id,
which production binds to `git hash-object --stdin` in the ONE ROOT (`worktree::hash_blob`,
`crates/rigger-worktree-git`: one process per call, waited to its exit, never written, no filter,
outside a repository too, since no SHA-1 crate is a direct dependency) and a test binds as it needs.
The hash outside a repository is proved once, at `worktree::hash_blob`'s own adapter test in
`crates/rigger-worktree-git/src/worktree.rs`'s tests module, and `ingest_tree`'s binding of it,
which `rigger graph build` reaches walking `.` outside a repository, is covered by that test. Both
sinks call it and keep no identity cut, only what differs: the log-side lookup they hand it (the
run's memo, the build's store read), the run's memo update, the run-id stamp, the ledger-form append
and the build line's N. The conductor takes the hash function as a new `Deps` field, `hash_blob`, so
it spawns no process, and the migration from `reset_derived`, which takes it as a parameter that
`cmd_reset` binds to `worktree::hash_blob`, its type the one alias `HashBlob` of
`src/cli/hygiene.rs`, the (root, bytes) shape of that function, since neither the domain nor the
worktree crate declares a type for it (the sinks take the root-bound form, `ingest::HashBlob`, which
the light lane does not compile), so a test of `src/cli/hygiene.rs`'s tests module hands it a
failing one (`tests/cli.rs` drives the binary and cannot). Recording requires the `git` binary: a
hash that cannot start is the failing-hash row of SINK OUTCOMES and fails the emit, while the
rebuild skips source 1 only outside a repository or when its batch process cannot start, so a
carried store still rebuilds. THE WALK HANDS EACH BATCH WITH ITS FLAG: `BatchSink` becomes
`FnMut(&[(String, &Event)], bool)`, `key_batch` takes the flag, the code half's two functions answer
each batch's flag by whether `walk_exclusions` names its identity, and the `gd` and `gw` walks hand
`false`; a sink never computes the flag or loads an index. That handoff is criterion 9's. The group
lookup answers a ledger entry on both backends, and `latest_generation`'s type-first check admits
the ledger type, criterion 8's. Criteria 10 and 12 own the sinks, the run's and `rigger graph
build`'s; criterion 7 owns no entry in `read_current_run`'s slice and none in the seeded key set,
and criterion 12 N on the graph build line.

**THE LEDGER ANSWERS THE INDEX-LAG ADVISORY.** `rigger validate`'s graph index-lag advisory
(`read_graph_index_lag`, `src/cli/validate.rs`) reads the run stream through criterion 6's
`ingest::perceived_generations` (THE REBUILD block), never the whole stream, and reads `graph.db` as
the read-only surfaces do (no open when the file is absent, as `graph_rebuild_owed_note` checks,
else `Projector::open`, which writes nothing to an owed file); with `graph.db` absent, owed or
unreadable it compares against the log side alone, as today. `read_graph_index_lag` takes the store
as `&dyn EventStore`, which its one caller, `cmd_validate`, opens through `with_project_store` and
hands in, so criterion 13 asserts on a recording store in `src/cli/validate.rs`'s tests module that
the advisory makes ONE typed read and no whole-stream read. `graph_index_lag` takes the generations
that read answers and names a sampled file when the generation of its current bytes through its
half's `(path, bytes, excluded)` function, with `excluded` whether `walk_exclusions` names the
identity, never taken from an entry, differs from that generation or from `graph.db`'s current
generation (`Projection::current_generation`), the two sides the sinks' check reads; a generation
hashes the whole batch, so equal generations are equal key sets. So a file whose current generation
is not its latest recording's is named. `graph_index_lag_sample` stays `gc` only, its light-lane
stub empty. The line names `rigger graph build` as its fix, the verb that re-perceives the tree,
recording each lagging file's entry and folding it through `ingest::entry_of_batch`, the function
the integration reindex calls too; `rigger reindex` refreshes the symbols index alone and moves
neither side the advisory compares. A sampled `gc` path outside the walk's scope is never a
candidate (the sample keeps a recorded path only when `grounder::tree_bytes` hands it bytes), so
criterion 13's clause for it holds at the binary by construction, and
`graph_index_lag_extracts_no_bytes_for_a_path_outside_the_walk_scope` pins it in the grounder crate.
`graph_index_lag_sample` draws its identities from the generations `perceived_generations` answers
and calls `project_scoped_latest_generations` no more. The keys part of
`project_scoped_latest_generations`'s answer then has no production reader, so criterion 13 removes
it, and `reference_replay_keys` (`tests/common/fixtures/ingest.rs`) collects its keys test-side from
criterion 13 on.

**MIGRATION CONVERTS THE BACKLOG IN PLACE.** `rigger reset --derived` becomes the one-time migration
and refuses in this order: a live writer (`refuse_derived_reset_if_live`), the one refusal
`--force-live` waives; then, taking the rebuild lock (`Projector::lock_rebuild`) before it reads
anything more, a lock it cannot take (`REBUILD_IN_PROGRESS`); then, holding it, a rebuild left
unfinished, naming it and `rigger setup`, a check that opens no graph file that does not stand;
then, when `graph.db` stands, an owed `graph.db`, as today. The lock is a local of `reset_derived`,
held across its one transaction; `cmd_reset` runs `--derived` after every other mode of the
invocation has returned. Criterion 15 lands the rebuild lock, the third refusal and the reworded
`REBUILD_IN_PROGRESS` on the `--derived` verb as it stands, the prune, and criterion 16 turns that
verb's work into the migration, keeping them. ONE public function, criterion 15's, answers the
third: `Projector::rebuild_unfinished(held: &RebuildLock) -> Result<bool, Error>`, beside
`Projector::rebuild`, unchanged, is true when the graph file stands and holds the rebuild cursor
whose tail `Projector::rebuild` finishes or has beside it the shadow it resumes, and false when no
graph file stands, a shadow beside a deleted `graph.db` being the forbidden state OUT OF SCOPE
names, read through the private `shadow_of` and `rebuild_tail_owed` that `Projector::rebuild` reads,
so each keeps one spelling; it takes the held lock, so no caller asks without one, and opens no
graph file that does not stand. `REBUILD_IN_PROGRESS`, the one spelling of the second, reads
"graph.db.lock is held by another `rigger setup` or `rigger reset`", true of every holder. A refusal
appends nothing, folds nothing and writes no row and no projection to `graph.db`, its owed refusal's
open bringing the schema up as today; opening a standing `graph.db`, for the cursor or the owed
check, folds a write-ahead log a dead process left as every open does, a checkpoint that changes
nothing the graph holds. These claims and the order above are of the `--derived` mode's own work:
the identity migration `cmd_reset` runs on a sqlite store before every mode (`migrate_identity_at`,
which can rename streams, append and fold its `DecisionMade` and open `graph.db`) precedes the
live-writer refusal and the rebuild lock, as today, and is outside them. In ONE sqlite transaction
(`Store::shed_derived`), opened immediate with its selection read inside it, as
`prune_derived_index` opens its own, over the project's run stream, the one stream a sink ever
appended a derived event to, for every identity whose latest recording (a derived row or a ledger
entry, keyed alike) is a derived row, it rewrites IN PLACE the lowest-position row
`plan_derived_prune` keeps for the identity above its latest ledger entry (the first row of its
latest batch when that batch was recorded whole), so the identity's latest recording after the
migration is the generation it was before; a kept row below that entry, left by a batch partly
recorded again over it, is deleted with the rest. The rule is the lowest kept row, not the latest,
because criterion 16 places a whole or spanning latest batch at its first kept row, and it is that
row whenever no entry stands inside the batch: position, stream, id, revision and recorded-time
stay; type, data and meta become the identity's `GenerationIngested` (that generation, the blob id
of the tree's file at that path, `excluded` whether `walk_exclusions` names the identity, its group
and replay key, whose `#<n>` counts the distinct replay keys of that generation among the identity's
derived rows; no reader compares `n`). The caller hands `Store::shed_derived` the blob and
`excluded` as a TOTAL function from identity to (blob, excluded), computed before the transaction
under the ONE ROOT: it hashes, through the one hash function, the bytes `grounder::tree_bytes` hands
for each file of `Store::count_derived`'s set; a path it hands none for, an identity it does not
know and one recorded after that read get no blob. A failed hash fails the verb before the
transaction opens. Measured on a copy: 597 of this store's 688 identities name a regular file,
hashed in 0.91 s, 1.5 ms each. It then deletes every remaining row of a derived type in that stream,
keyed or unkeyed; a keyed row whose replay key does not parse is counted with the unkeyed rows
(Global constraints). SHED has ONE definition, the Global constraint's. `Store::shed_derived` hands
`plan_derived_prune` no re-asserting type, so the selection computes no carry and resolves no name
for it; the alias definitions the selection replays for its live-selection caller are read under the
migration's write lock and not consulted, a cost the measured hold below includes and this spec
accepts to keep one selection, and nothing the migration writes depends on an alias. The same
selection backs the read-only count of ONE set: `Store::count_derived` answers the count of derived
events shed, the count of those unkeyed, and the SET of file identities (keyed `<prefix>/<file>`
identities, so a file holding a `gc` and a `gd` batch is two) holding one; the migration's "rows
shed" is the first count, the menu line and the bloat advisory print the set's size, the migration
reads its members, and a store "holds a derived event" when the first count is not zero.
`Store::shed_derived` and `Store::count_derived` select by the DERIVED list directly, never
`class_of`, and `read_live_selection` keeps its injected `ContentIdentity`. EARLIEST SURVIVING
RECORDING: in the same transaction, every identity that sheds derived rows ends with its earliest
surviving recording carrying the identity's earliest recorded valid-time, the rewritten row when no
ledger entry of the identity precedes it and otherwise its earliest ledger entry, re-dated in place.
Identity and generation are cut from each row's replay key (`derived_key_parts`), never its group:
42,563 keyed derived rows here carry no `META_GROUP`. The rewrite keeps every column a uniqueness
rule covers (the primary key, `UNIQUE(stream, revision)`). A spec 101 rebuild records in `applied`
only the positions `plan_derived_prune` keeps, so an incremental graph and a rebuilt one both hold
the rewritten row applied; a batch is one append with no knowledge event inside it, so the entry
stands on the same side of every alias and knowledge event as the batch it replaces (its dating is
an instance of WHAT A REBUILD REPRODUCES). A latest generation whose kept rows span appends folds
whole at the lowest kept position, its names resolving through the aliases defined before that
position where the incremental graph resolved the later rows through aliases defined between the
appends, and that is accepted (WHAT A REBUILD REPRODUCES). On this store: 688 rows rewritten and
228,812 deleted, whose sum, 229,500, is the count of rows shed, with the write lock held 1.9 s on a
copy, inside the 5000 ms busy timeout an appender waits. A stream whose tail is derived rows ends at
a lower revision, and its next appends re-issue revisions, never a position; nothing holds a
revision across the migration: the live-writer refusal keeps a run out, `record_result_if_absent`
re-pins its expected revision on a conflict, `read_run` reads its boundary within one call, no
production code subscribes to a stream, and `--force-live` hands this risk to the operator, as its
refusal says. A crash rolls the transaction back and a rerun starts over; a rerun after success says
there is no derived event to shed, and still calls `Store::reclaim_space` and prints its
`reclamation_lines`. An out-of-line test module's entry records `excluded: true`; one whose
generation the tree's flag no longer reproduces folds nothing at a rebuild until the next ingest. An
unkeyed derived event names no identity (`derived_key_parts`), so no entry re-asserts it (this store
holds none), as does a keyed row whose replay key does not parse (`plan_derived_prune` keeps it as
its own identity with no generation, so its facts stand live until the first rebuild). The migration
reports the entries converted (which can be fewer than the file identities), the rows shed and, on
its own line, the unkeyed rows shed, then reclaims space (`Store::reclaim_space`) and reports the
bytes reclaimed, its lines for the `Reclamation` ONE pure function of it, `reclamation_lines`
(`src/cli/hygiene.rs`), criterion 16's, rendering each state `derived_prune_report` tells apart
today over the four reclamation fields. `prune_derived_index` and its compaction (`PrunedDerived`,
its per-type report but not the plan's `removed_per_type`, its injection seam) go; the live
selection a rebuild folds (`read_live_selection`) is unchanged, so an unmigrated store still folds
each identity's latest derived generation. No production reader resolves a graph `source` position
back to an event row: `recency_by_own_edge` ranks by it and the explain provenance prints it (`event
#N`), so a live fact naming a shed position reads as before. A console tab meets nothing: its
provider reads the current run, so `serve_console_stream`'s floor guard never fires. Sqlite only, as
today. The `--derived` text, the passages DOCUMENT EDITS gives criterion 16, describes a one-time
migration that converts each file's latest batch into a ledger entry and leaves no derived event
behind.

**RECLAMATION STAGES IN MEMORY.** `Store::reclaim_space`, split out of `prune_derived_index`'s
post-commit reclamation (`compact_in_place`, its injectable compacting step, its on-disk after, its
failure report), is the only reclamation; it reports against the on-disk size (main file plus
`-wal`) its caller measured BEFORE its transaction opened the bytes the log lost across the command,
saturating at zero, as `the_reclamation_the_command_reports_is_the_space_the_file_actually_lost`
pins, in a `Reclamation` carrying `PrunedDerived`'s four reclamation fields, public as they are
today so a caller outside the crate can build one in a test; criterion 14 owns it, and
`prune_derived_index` calls it until criterion 16 removes that function. From criterion 14
`PrunedDerived` holds that `Reclamation` in place of its four fields until criterion 16 deletes
`PrunedDerived`, so no tree spells the four fields twice. The caller measures that before-size
through `Store::bytes_on_disk()`, today's private `bytes_on_disk` over the connection's own file
made ONE pub method with the same answer (none for a database with no file behind it), criterion
14's at its first production caller, `prune_derived_index`. It sets `PRAGMA temp_store = MEMORY` on
the `Store`'s own connection before its `VACUUM`, left there for that connection's life, so the copy
SQLite stages is held in the process's memory, never in its temporary directory (`SQLITE_TMPDIR`,
else `TMPDIR`, else `/var/tmp`, possibly a small partition): no setting is process-global and a
crash leaves nothing to reap. It rewrites only a file that holds free pages, so a rerun reclaims
what a skipped reclamation left. Measured on a copy after the migration's delete, with neither
temporary directory variable naming a directory: 1.44 s, 581 MB to 445 MB, 454 MB more peak memory,
no temporary file. Without that memory the `VACUUM` rolls back, the deletes stay, and the verb
prints the failure beside its counts. `rigger reset --derived` calls it holding its probe's step
lock under dead live-writer facts (`LiveWriterFacts::reasons` empty), so two reclaimers never meet;
`--force-live` skips the probe and lock as today, at the operator's risk, and an appender waits for
the `VACUUM`'s write lock inside the 5000 ms busy timeout.

**THE OPERATOR IS TOLD WHAT THE MIGRATION SHEDS.** The bare `rigger reset` menu's `--derived` line
(`derived_menu_line`) reads `Store::count_derived`, never drifting from what `--derived` deletes: on
a store holding derived events it names the "M derived events of N file identities", worded by
`derived_count_phrase` (`src/cli/hygiene.rs`, criterion 17's), and the flag, true for a store whose
rows are all unkeyed (N zero); on a store holding none it says there is no derived event to shed; a
server-backed store's line says the migration does not run there, without the word compaction.
`count_derived_duplicates` and `DerivedPreview` go, criterion 17's, with `removed_per_type`, the
prune plan's private per-type count (`sqlite.rs`) that fed both `PrunedDerived`'s per-type report
and the preview, whose last reader goes here, criterion 16's report having gone first. `rigger
validate`'s log-bloat advisory (`bloat_advisory_for`) reads the same count: a store holding any
derived event is named with `derived_count_phrase` and `rigger reset --derived` as the migration; a
store holding none, or not sqlite, prints nothing, as today. `measure_derived_duplication`,
`DerivedDuplication` and `BLOAT_DUPLICATION_THRESHOLD` go, criterion 18's. The plan's `superseded`
field loses its last reader with `count_derived_duplicates` and goes with it, criterion 17's, and
its `rows` field with `measure_derived_duplication`, criterion 18's: the light lane's `cargo clippy
--workspace --all-targets --no-default-features -- -D warnings` (`.rigger/gates/lanes.sh`) fails on
a field never read.

**THE STORE REFUSES A DERIVED APPEND.** Both adapters' `append` (`sqlite.rs` and `kurrentdb.rs`
under `crates/rigger-store-sqlite/src/eventstore/`) refuse a batch holding any DERIVED event,
through `retention::class_of`, naming the type and writing no event of the batch. The refusal lives
at the adapters, the seam every writer passes; a check in a caller, a wrapper or a test double is
NOT this refusal. A contract-suite test of a store property is re-expressed over ledger entries on
both backends (TEST DISPOSITIONS). ONE PRE-LEDGER ROW INSERTER inserts rows written before this spec
into a store file with raw SQL, on sqlite alone: it lives in `tests/common/fixtures/sqlite.rs`, the
raw-SQL file fixtures the store crate's `test_support` already compiles in through its `#[path]`
include and the root tests reach through `tests/common`, each over its existing `rusqlite` edge, so
no manifest gains a dependency edge. The tests still appending a derived event through a store at
criterion 19 are the root tests' and the store crate's own, so criterion 19 is the first unit
needing the inserter and builds it, and no KurrentDB test seeds pre-ledger rows. The shared
fixtures `tests/common/fixtures/events.rs` and `ledger.rs` also compile into the conductor, driver
and dash crates, which hold no `rusqlite` edge, so their seeders express the same sink state through
ledger entries: `seed_one_shot_fixture` records entries, and the conductor's memo test seeds a graph
holding each generation beside entries on another stream, where the run stream's group lookup
answers none.

**THE LIGHT LANE, decided here so no unit has to.** `--no-default-features` compiles no extraction
and no symbols index. Criteria 1, 2, 7, 8, 14, 15, 17, 18 and 19 assert nothing lane-dependent.
Criterion 3's sampling clause and criteria 4 and 9 run in the default lane only, `tree_bytes`' rule,
which is ungated, in both. Criterion 5's resolved folds, resolution sources, equality and dating run
in the default lane only; the light lane asserts each hand-built entry folding nothing and the hole
paid, and criterion 6's the report's number, its line there carrying no note, since no extraction
compiles to keep the note's promise, an entry recorded at each counted identity's next default-lane
ingest and never a restoration of facts (THE REBUILD block); there `rebuild_owed_graph` binds
`ingest::resolve_entry`'s `not(feature = "symbols")` stub, of the same signature, the root included,
and beside the other light-lane stubs, answering unresolved for every entry and starting no batch
process, criterion 5's. The `walk_exclusions` light-lane stub lands with criterion 4 as the cfg twin
of a referenced function, which UNIT ORDER's rule does not count as a function a criterion adds; its
first light-lane caller is criterion 16's migration. Criteria 10, 11 and 12's sink assertions run in
the default lane only, since the light lane compiles the run's sink out and `rigger graph build`
walks nothing, the extraction arm of that build's sink (`ingest_tree`'s call of
`ingest::entry_of_batch`, with the hash function bound there to `worktree::hash_blob`, so its
signature carries no parameter the light lane leaves unused) sitting behind `cfg(feature =
"symbols")` as the run's sink does and the light-lane `ingest_project_batched` handing it no batch;
the light lane asserts that build recording no entry and no derived event and exiting 0. Criterion
13's named and unnamed files run in the default lane only; the light lane asserts the stub that
samples nothing and names no file, and the reference test over ledger entries. Criterion 16's
`excluded: true` and rebuild-equality clauses run in the default lane only; the light lane asserts
every other clause, the refusals among them, which compile in both lanes as the owed check does,
each entry's `excluded: false` and the rebuild of the migrated store printing the report's number;
that clause observes criterion 6's number, and the default lane's rebuild-equality clause criterion
5's rebuild, as fixtures of the migrated store, and neither owns any part of their rule.

**CONSTRAINTS WALK, decided.**
- *Concurrent ingest:* a step and a `rigger graph build` can record one generation twice; the later
  fold is a re-recording, and two of different generations can fold out of log order; both are
  instances of WHAT A REBUILD REPRODUCES. A `rigger graph build` or offline graph pass, which the
  live-writer refusal does not keep out, appends with `ExpectedRevision::Any` before or after the
  migration's one transaction, an entry it records being the identity's latest recording.
- *Crash-resume:* a rebuild a dead process left unfinished is MIGRATION's third refusal.
- *Cold start:* nothing is carried in memory between processes; the sinks' check asks the store's
  group lookup and `graph.db`. On a store with no recording of an identity its batch is not current,
  so the first ingest reads, extracts and hashes every in-scope file once in the sink, beside the
  walk's index lowering, serially, with no worker pool; after it a file is read and extracted again
  once per new generation, a file whose index lowering lags its bytes once per walk until a reindex,
  and hashed only when an entry is recorded. The hashing is the measured 1.5 ms a file (0.91 s for
  597 files, MIGRATION), the extraction costs one more extraction of the tree, and each walk reads
  `Projection::current_generation`, one primary-key read, for each file, beside the log side's group
  lookup, which the run's sink memoizes per process behind one read of the ledger head per batch.
- *Rebuild cost:* the rebuild folds every entry of the stream, superseded ones included, each at
  most two extractions (source 1, then source 2), so its cost grows with history until spec 108
  consolidates; on this store after the migration that is 688 entries (MIGRATION), one extraction of
  the tree at the cost the cold-start bullet gives, and a second only for an entry whose blob's
  extraction misses its recorded generation. Accepted here.
- *Existing data:* an older store keeps its derived events readable, folds them as spec 101 decided
  and answers the sinks' check from its grouped derived rows until `rigger reset --derived` migrates
  it; an identity whose derived rows carry no group answers no generation at the group lookup, so
  its first walk records one entry that folds as a re-recording (SINK OUTCOMES rows 9 then 13); a
  `graph.db` built before this spec needs no rebuild, since no fold rule or projection version
  changes. A KurrentDB store's derived backlog is never shed (OUT OF SCOPE): the derived events
  already in a KurrentDB stream stay there, so every later read of that stream pays for them, the
  `PERCEPTION_TYPES` read of the rebuild's report and the index-lag advisory reading them and every
  other read skipping them, a permanent cost. The Design bounds it, since no sink appends a derived
  event and the store refuses one (THE STORE REFUSES), so the backlog never grows, and accepts it;
  the sqlite-only bloat advisory never names it.
- *Output streams:* the rebuild's report, the migration's counts and the menu line print on standard
  output, as today, and `rigger graph build`'s `graph build: ingested N code-ingest event(s)` line
  keeps its stream and its meaning, N counting the batch events of each entry SINK OUTCOMES counts
  in N, beside the existing fold-loss clause; and the bloat and index-lag advisories on standard
  error; no line reaches `rigger step`'s standard output.

**STATE PLACEMENT.** The ledger is the log (`GenerationIngested`); the class of a type is code
(`retention`); `graph.db` is a projection. An in-memory set of ingested generations, a `.rigger/`
marker or a cache of resolved blobs is NOT an implementation of any of them; `LoggedGenerations` is
a per-process memo of the log's latest generation, not the ledger.

**OUT OF SCOPE.** `progress.db`, streams other than the run stream, the superseded ledger entries
(each stays live and is re-extracted at every rebuild), the episodic events, which this spec
classifies and leaves live, migration on KurrentDB, a deleted `graph.db` (forbidden by
`crates/rigger-domain/src/docs.rs`, unchanged here) and everything of spec 108.

## Notes (non-criteria)

`GenerationIngested { prefix, file, generation, blob, excluded }`: `prefix` is `gc`, `gd` or `gw`;
`blob` is the object id as git prints it for the repository's object format, or empty for an entry
with no blob, recorded when the path held no file; `excluded` is the walk's flag, whether
`walk_exclusions` names the identity, which changes only a parsed `gc` batch. Its group is
`<prefix>/<file>` and its replay key `<prefix>/<file>@<generation>#<n>`, both built by
`GenerationIngested::event(n)`.

SINK OUTCOMES. What either sink does with one batch the walk hands it, in the order it decides, rows
1 to 11 answered for both by `ingest::entry_of_batch`; "looked up" is the log's latest generation of
the identity as the sink read it, and the memo column is the run's `LoggedGenerations` (`rigger
graph build` keeps none and asks the store each batch).

| # | Case | Entry recorded | Fold | Emit | In graph build's N | Memo afterwards |
|---|---|---|---|---|---|---|
| 1 | the batch's key names no identity (none from `key_batch` does) | none | none | fails, naming the key | no | unchanged |
| 2 | the log side's group lookup, `Projection::rebuild_owed` or `Projection::current_generation` fails | none | none | fails, naming the failed read | no | looked up when the lookup answered and a read of the graph's side failed, else unchanged |
| 3 | CURRENT: the log and the graph both hold the batch's generation, a lagging lowering at that generation included (criterion 13's advisory names it) | none | none | succeeds | no | looked up |
| 4 | the graph owes its rebuild and the log side holds the batch's generation | none | none | succeeds | no | looked up |
| 5 | not current; the read of the bytes fails for a reason other than absence | none | none | fails, naming the read | no | looked up |
| 6 | not current; the sink's own extraction is itself current (a lagging lowering whose bytes extract to the generation both sides hold) | none | none | succeeds | no | looked up |
| 7 | not current; the own extraction of a `gd` or `gw` file is the empty batch (deleted, truncated or made unparsable since the walk) | none | none: the identity keeps what its last fold left | succeeds | no | looked up |
| 8 | not current; hashing the bytes of an extraction it will record fails, a hash process that cannot start included | none | none | fails, naming the hash | no | looked up |
| 9 | not current; bytes read and extracted to a batch neither empty nor current, a lagging lowering whose generation both sides do not hold included | the extraction's generation, the bytes' blob, the walk's flag | by the graph's answer, rows 12 to 15 | succeeds unless the append fails (row 16) | by its fold's row | the entry's generation unless the append fails |
| 10 | not current; a `gc` path holding no file (an absent path reads as no bytes) | the generation of `gc`'s batch for no bytes, no blob, the walk's flag | by the graph's answer, rows 12 to 15 | succeeds unless the append fails (row 16) | by its fold's row | the entry's generation unless the append fails |
| 11 | not current; a `gc` path outside the walk's scope, handed no bytes (the `gd` walk never hands one) | the generation of `gc`'s batch for no bytes, no blob, the walk's flag | by the graph's answer, rows 12 to 15 | succeeds unless the append fails (row 16) | by its fold's row | the entry's generation unless the append fails |
| 12 | an appended entry the graph folds (outcome (c), resolved) | the entry of rows 9 to 11 (its generation, its blob or none, the walk's flag) | the extraction folded at the entry's position | succeeds | yes | the entry's generation |
| 13 | an appended entry that is a re-recording (THE ENTRY block's outcome (b)) | the entry of rows 9 to 11 (its generation, its blob or none, the walk's flag) | the `applied` row only | succeeds | no | the entry's generation |
| 14 | an appended entry the ledger form refuses before folding (the graph owes its rebuild) | the entry of rows 9 to 11 (its generation, its blob or none, the walk's flag) | none: refused, `graph.db` still owing | succeeds at both sinks; the run's sink names the debt through the folding store's injected log (`Deps::log`), as `append_and_fold_batch` says every lost fold, and `rigger graph build`'s line names it in its fold-loss clause, which names the build's first lost fold | yes | the entry's generation |
| 15 | an appended entry whose ledger fold fails | the entry of rows 9 to 11 (its generation, its blob or none, the walk's flag) | none: failed, `graph.db` marked owing (`mark_lost_fold`) | succeeds at both sinks; the run's sink names the failure through the folding store's injected log (`Deps::log`), as `append_and_fold_batch` says every lost fold, and `rigger graph build`'s line names it in its fold-loss clause, which names the build's first lost fold | yes | the entry's generation |
| 16 | the append fails | none | none | fails, naming the store's error | no | looked up |

In the light lane neither sink walks anything (THE LIGHT LANE), so no row is reached. Criterion 10's
continuation names a fixture for every row but 15 and 16, row 13's the identity whose pre-ledger
derived rows carry no group, whose first walk records an entry that folds as a re-recording
(CONSTRAINTS WALK), the emit succeeding, the run's sink's fixture a root periphery test over
`conductor::run` with a sqlite store, seeded through `keyed_derived_event` until criterion 12, its
test-side successor until criterion 19 and the pre-ledger row inserter after it; criterion 11
asserts that fixture's memo taking the entry's generation, shown by a second walk of the identity in
the same process making no group lookup (`CountedRead::LatestInGroup`), and criterion 12 N not
counting such an entry by the outcome the ledger form reports; rows 15 and 16 are asserted by the
lost-fold and refused-append tests criterion 10 re-expresses (TEST DISPOSITIONS); criteria 7 and
12's clauses for `read_current_run`'s slice, the seeded key set and N assert the things PERCEPTION's
last sentence names.

TEST DISPOSITIONS. Each existing test whose assertion the spec changes or re-homes, the criterion
whose change breaks it FIRST and so moves it, and its disposition ("helper" is the one pre-ledger
row inserter, THE STORE REFUSES block):

| Test file or named group | First broken by | Disposition |
|---|---|---|
| `crates/rigger-conductor/src/replay_keys.rs`: the generations map, `install`, `forget`, `tracked`, `Ticket`, and each conductor test's assertion through `tracked` | 10 | deleted: in-memory first-sight seeding of derived keys dies with the map |
| conductor tests of `emit_keyed_batch`, `ingest_project_batches`, `ingest_files_into_graph` reading derived events back | 10 | re-expressed over ledger entries |
| `reference_replay_keys` (`tests/common/fixtures/ingest.rs`) and its callers | 6 | passes the derived types to the new parameter until criterion 13 removes the parameter, so its keys stay the derived reference its callers assert; read with `PERCEPTION_TYPES`, an identity whose latest recording is an entry answers the replay keys of that generation's recordings, the entry's own among them |
| `crates/rigger-domain/src/ingest.rs` tests of `batch_is_latest_recorded` | 12 | re-expressed over the pure `batch_is_current` |
| `keyed_derived_event`'s tests (`crates/rigger-domain/src/ingest.rs`) and its test callers that still append a derived event through a store | 12 | its tests deleted with it; the callers build through a test-side successor in `tests/common/fixtures/events.rs`, which 19 deletes with the last of them; those standing on it until 19 are the root tests' and the store crate's own, the conductor's two (`a_first_sight_lookup_racing_a_newer_generation_leaves_the_process_on_the_stored_generation`, `a_generation_whose_append_is_refused_leaves_the_process_on_the_recorded_generation`) re-expressed over ledger entries at 10 with the `emit_keyed_batch` group and `rigger-graph-sqlite` holding none |
| the test-side entry builder of `tests/common/fixtures/fold.rs` the criteria before 10 build hand-built entries through | 10 | moved into `retention` as `GenerationIngested::event`, its callers calling the constructor; each crate including that file names `retention` at its root, as it names `eventstore` |
| `tests/dedup_seeding_periphery.rs` | 10 | re-expressed over ledger entries, sqlite |
| the `rigger graph build` tests of `tests/cli.rs` reading derived events back | 12 | re-expressed over ledger entries, sqlite |
| `tests/group_lookup_periphery.rs` but its namespace test, `tests/change_path_revert_periphery.rs` and `a_graph_build_whose_fold_is_lost_to_a_lock_says_so_and_the_next_build_refuses`, each seeded through a sink | 10, or 12 for one seeded through `rigger graph build` | re-expressed over ledger entries, sqlite |
| `graph_index_lag*` tests (`crates/rigger-grounder/src/ingest.rs`, `src/cli/validate.rs`) and the index-lag tests of `tests/validate_advisories.rs` seeded through `seed_graph_generation` (`validate_warns_of_graph_index_lag_and_names_reindex`, now `validate_warns_of_graph_index_lag_and_names_graph_build`, `validate_is_silent_on_graph_index_lag_when_the_graph_matches_the_tree`), which seeds by a plain store append that folds nothing | 13 | re-expressed over ledger entries and the graph's current generation, the `tests/validate_advisories.rs` seeds over a folded entry or an absent `graph.db` |
| `latest_generation_answers_what_the_reference_answers_on_the_same_log` (contract suite) | 13 | re-expressed over ledger entries, both backends |
| `a_compaction_that_fails_after_the_commit_still_reports_what_was_deleted`, `a_rerun_reclaims_the_space_a_failed_reclamation_left_behind` (`sqlite.rs`) | 14 | moved onto `Store::reclaim_space`'s injectable step |
| `discipline_names_reset_derived_as_the_event_logs_own_prune` (`docs.rs`), `the_committed_operator_documents_ship_the_derived_prunes_guidance` | 14 | staging sentences re-expressed as held in memory; the `--derived` wording re-expressed by 16 |
| kept properties: `tests/reset_derived_live_writer_guard_periphery.rs` (both refusals), `reset_derived_on_a_backend_that_cannot_compact_fails_loudly_naming_the_backend_it_needs` (sqlite only), `the_prune_reaches_only_the_namespace_it_was_handed_and_matches_that_prefix_literally`, `reset_accepts_each_mode_at_most_once_and_composes_the_two_in_either_order`, `each_reset_mode_sheds_only_its_own_accumulation_and_composing_them_does_exactly_both`, `the_reclamation_the_command_reports_is_the_space_the_file_actually_lost`, `reset_build_cache_composes_with_runs_and_derived_in_either_order` (`tests/reset_build_cache_periphery.rs`), `a_reset_from_a_nested_worktree_migrates_and_compacts_the_store_it_walked_up_to` (asserting each blob read and hashed under `tree_root(store_dir)`, not the working directory's top level), the console floor tests of `tests/dash_console_stream_periphery.rs` | 16 | re-homed onto `rigger reset --derived`, each assertion on the prune's report or its server-backed refusal, `assert_compacted`'s among them, re-expressed over the migration's, the console tests over a hand-built gap |
| the tests of `src/cli/hygiene.rs` reading `derived_prune_report` through `report_of`: `a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts`, `a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written`, `a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation`, `a_prune_that_shed_rows_explains_the_duplication_a_deduplicated_log_still_accumulates`, `an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined` | 16 | each assertion on a reclamation state re-expressed over `reclamation_lines` with a hand-built `Reclamation`, each on the per-type count or its explanation deleted with that report, so the fourth goes whole |
| the compaction's selection, carries and per-type report (the prune tests of `sqlite.rs`, `tests/compaction_generations_periphery.rs` but its tests calling neither `prune_derived_index` nor `count_derived_duplicates`, directly or through a helper (its rebuild, live-selection, key-parts, setup, owed-graph and rebuild-lock tests, their lock text DOCUMENT EDITS', their assertions on the prune's report re-expressed here over the migration's and their derived seeding the last row's), `tests/reset_derived_compaction.rs`, `tests/reset_derived_compaction_periphery.rs` but its nested-worktree test), every preview- or measurement-versus-prune comparison (`measure_derived_duplication_counts_superseded_generations_as_the_prune_selects_them`, `bare_reset_previews_superseded_generations_and_the_real_prune_removes_exactly_that`) and the menu-agreement tests (`bare_reset_on_a_populated_store_reports_measured_counts_matching_a_real_prune_and_mutates_nothing`, `reset_composes_derived_with_runs_bare_reset_previews_it_and_an_unknown_mode_still_refuses`, the three `count_derived_duplicates_*` of `tests/reset_menu_previews_periphery.rs`) | 16 | deleted but for a test another row names, which that row disposes: `prune_derived_index`'s application of the plan and the preview's readers die, `plan_derived_prune` staying the live selection's and the migration's one selection; `read_live_selection`'s tests stay |
| the other menu tests of `tests/reset_menu.rs`, `bare_reset_previews_the_migrated_stores_real_counts_when_history_predates_the_minted_project_identity` (`tests/reset_menu_identity_migration_periphery.rs`) and the two `derived_menu_line` tests of `src/cli/hygiene.rs` | 17 | re-expressed over `Store::count_derived`, the server-backed line's over the wording THE OPERATOR IS TOLD gives it |
| the log-bloat tests of `tests/validate_advisories.rs`, `bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it` (`src/cli/validate.rs`) and the `measure_derived_duplication_*` tests of `sqlite.rs` calling neither `prune_derived_index` nor `count_derived_duplicates` | 18 | re-expressed over `Store::count_derived`, the `sqlite.rs` tests deleted with the function |
| contract-suite tests of a store property on derived fixtures | 19 | re-expressed over ledger entries, both backends |
| `the_group_lookup_answers_each_project_namespace_only_its_own_recording`, the console tests' derived fixtures | 19 | record a `GenerationIngested` or another non-derived type |
| every other test appending a derived event through a store (root and the store crate) | 19 | seeded by the helper, sqlite; a seeder in a fixture the conductor, driver or dash crate includes expresses the same sink state through ledger entries instead, those crates holding no `rusqlite` edge |

Clauses proved outside the test their criterion names. The hash outside a repository is proved at
`hash_blob_outside_a_repository_answers_the_object_id_git_hash_object_gives`
(`crates/rigger-worktree-git/src/worktree.rs`), which covers `ingest_tree`'s binding of
`worktree::hash_blob`. Criterion 15's lock scope is
`a_second_rebuild_lock_taken_while_the_migration_runs_is_refused_with_the_lock_line`
(`src/cli/hygiene.rs`): a hash function handed to `reset_derived` takes `graph.db.lock` again while
the migration runs and is refused with the lock line, red when the lock is dropped after the
unfinished check. Three clauses of criterion 16, the test module's entry recording `excluded: true`,
the rebuild of the migrated store reaching the live facts and the current generations it held
before, are asserted by `reset_derived_converts_each_latest_derived_batch_into_its_entry_in_place`
(`tests/cli.rs`) in the symbols lane only, as THE LIGHT LANE decides; the light lane asserts
`excluded: false` and the rebuild's report number. The memo's proof of currency is
`a_revert_another_process_staled_is_recorded_over_a_graph_that_owes_its_rebuild`, its twin over a
graph rebuilt at that generation and the tests of
`crates/rigger-conductor/src/logged_generations.rs`; the read-count tests of criterion 11 and
`a_step_that_ingests_seeds_each_identity_by_group_lookup_and_appends_only_what_moved` count the
group lookups among the sink's reads, the step test pinning one ledger-head read per batch and one
read of the entries above the head per entry appended. The rewrite above an identity's latest entry
is `shed_derived_rewrites_the_lowest_kept_row_above_the_identitys_latest_ledger_entry`
(`sqlite.rs`).

DOCUMENT EDITS. Each passage the spec makes false, the criterion that rewrites it and the tests that
pin its text:

| Passage | Rewritten by | Pinned by |
|---|---|---|
| `docs/architecture-addendum-context-management.md` section 2.1, its non-goal "Do NOT prune the event log" and its "the whole graph ... is rebuildable from the log" | 1 (it states the target) | none |
| `crates/rigger-domain/src/docs.rs` and `skills/rigger-reset-store/SKILL.md` saying `graph.db` regenerates from `events.db` alone | 10 | none pins the wording; the docs-drift gate holds the skill equal to `docs.rs`'s render (`validate_docs_drift_gate_covers_each_per_operation_skill`) |
| where the vacuum copy is staged (WHAT IT COSTS TO RUN): `docs.rs`, `skills/using-rigger/SKILL.md`, `docs/handbook/using-rigger.md` | 14 | the two document tests of TEST DISPOSITIONS |
| the `--derived` text: usage in `src/main.rs`, the `reset_modes` flag list, the `rigger-reset-store` skill (procedure and anti-move), "Event log hygiene" of the using-rigger skill and handbook, the `--derived` guidance in `docs.rs`, `live_writer_refusal` and `cmd_reset`'s server-backed refusal, which says the migration does not run there and no longer advises pruning the server store; each still says what it says today of the live-writer refusal: the liveness it reads and names, `--force-live`, the refused command and the corruption it risks | 16 | the same two document tests and the refusal tests of TEST DISPOSITIONS |
| `REBUILD_IN_PROGRESS`, its doc comment and `Projector::lock_rebuild`'s (`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`), and the `--runs` usage clause "unless a rebuild in progress holds graph.db.lock" (`src/main.rs`), each made true of every holder of `graph.db.lock` | 15 | `tests/compaction_generations_periphery.rs`, which spells the setup refusal's text and the usage clause (every other use compares against the constant); `docs/audit/2026-09-simplification-audit.md` and `docs/audit/duplication-catalog.json` quote the usage |
| the comment of `serve_console_stream`'s floor guard citing `prune_derived_index` | 16 | none |
| the `FoldAccess` doc comment (`crates/rigger-domain/src/contextgraph.rs`) saying every fold's outcome is reported, rewritten to name `apply_generation`'s convention | 2 (it states the target) | none |
| the `ContentIdentity` policy comment (`crates/rigger-domain/src/eventstore.rs`, "never vocabulary the store owns"), rewritten to say the injected policy configures the live selection a rebuild folds while the migration and the refusal read `retention`'s class table | 16 (it states the target) | none |

`retention::EPISODIC_TYPES`: `SpawnRequested`, `GateVerdict`, `GatePromoted`, `GateDemoted`,
`UnitProposed`, `BlastRadiusComputed`, `ScopeCreep`, `FileTouched`, `SpecDefect`, `ManualReview`,
`DeferredGateFailed`, `TaskAborted`, `BudgetExhausted`, `AgentProgress`, `SpawnLaunched`,
`StopFailure`. The last three are recorded in `progress.db` and classified only for completeness.

## Global constraints

- Hyphens, never em dashes, in every added line.
- One new event type and no others; no new dependency.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).
- A KEYED derived event is shed only by the migration, in the migration's one transaction, which
  rewrites the identity's latest batch into a ledger entry when that batch is derived, because the
  tree re-derives it; an UNKEYED derived event, or a keyed one whose replay key does not parse,
  names no identity and is shed and counted by the migration. SHED is every derived row of the
  stream, the rows the migration rewrites into entries counted as shed.
- Every rebuild is deterministic: the same rows, the same tree, the same object database and the
  same answer to whether the batch process starts yield the same graph.
- The gates cannot see the KurrentDB half of criteria 8, 13 and 19 where the contract suite's
  container is unreachable, the scope of `reset_derived`'s rebuild lock across the migration's
  transaction, which no test observes without a race past the in-crate test that the lock is held
  while the migration hashes the tree, or, where the gate host runs as uid 0, THE READ FAULT, under
  which SINK OUTCOMES row 5's test and `tree_bytes`' read-rule test assert nothing; the adjudicator
  demands that run's evidence, the read fault's from a run under a non-root uid, and reads that
  scope in the diff.

## Done when

- [ ] a test proves THE CLASSES ARE ONE TABLE: every event type a `TYPE_` constant under `src/` or `crates/` declares sits in exactly one class list, and folding each episodic type leaves the live projection unchanged,
  asserted by a source scan under `tests/` and a fold test in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, with no type of the four cross-run lists
  classified episodic. This criterion OWNS `retention` with its feature gate and lists, the move of
  the four conductor type constants, the `GenerationIngested` constant and payload type,
  `PERCEPTION_TYPES` and the passages DOCUMENT EDITS gives it; the parse is criterion 2's, the
  constructor criterion 10's and `class_of` with what a class causes criterion 19's, NOT this one's.
- [ ] a test proves THE ENTRY AND ITS BATCH FOLD AS ONE: `apply_generation` folds a resolved batch at the entry's position and installs its generation, installs none for an unresolved one, and changes no fact for a re-recording,
  asserted at the graph seam in `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` with
  hand-built entries and no sink, with a resolved batch changing the degree of a community's member
  relabelling the community when `apply_generation` returns, read from the community node's `label`
  attribute, an unresolved entry leaving the identity's facts and current generation as they stood,
  an entry whose position the `applied` ledger holds folding nothing and never calling `batch`,
  `Projection::current_generation` answering each state, `apply_generation`'s answer naming the
  outcome for each of the three, entries built by the test-side builder, the generic fold refusing a
  `GenerationIngested` naming `apply_generation` and marking the graph owed, `apply_generation`
  refusing on a graph that owes its rebuild, and a failed `apply_generation` marking the file owed,
  each writing no `applied` row. This criterion OWNS `apply_generation` with its three outcomes, the
  enum naming them, and its two guards, that a failed or refused fold writes no `applied` row,
  `Projection::current_generation`, the generic fold's refusal of the type, `fold`'s asserter
  parameter and the payload's parse; the rebuild, with `fold_source`'s routing of an entry and
  `check_fold_payload`'s ledger arm, is criterion 5's and the sinks with the ledger form of
  `FoldingStore` criterion 10's, NOT this one's.
- [ ] a test proves THE TREE IS READ BY ONE RULE: `grounder::tree_bytes` hands the bytes of a regular, readable file inside the walk's scope under the root it is given, and none for any other path,
  asserted in the grounder crate's tests, with a `gc` path under a hidden directory and one a
  committed `.gitignore` names handed none, a path relative to the root it is given read from that
  root and not the working directory, the workflow definition's path admitted by
  `grounder::in_walk_scope` for `gw`, a file of THE READ FAULT handed none, and
  `graph_index_lag_sample` over derived events recording a `gc` path outside the walk's scope and
  one THE READ FAULT makes unreadable sampling neither. This criterion OWNS
  `grounder::in_walk_scope`, `grounder::tree_bytes` with its read-rule test,
  `tests/common/fixtures/read_fault.rs`, `workflow_doc`'s move and the sample's candidate test
  through `tree_bytes`; the rebuild's and the sinks' reads are criteria 5 and 10's and the
  advisory's two-sided comparison criterion 13's, NOT this one's.
- [ ] a test proves THE EXTRACTION READS BYTES: each half's extraction split out of its disk read answers, for a file's bytes, the batch the walk lowers today from that file,
  asserted in the tests of the module owning each split over a fixture tree holding a `gc` source
  file, an out-of-line test module, a `gd` document and the workflow definition, with
  `events::lower_file` answering the batch `project_batches_paced` and `file_batches` lower,
  `ingest::walk_exclusions` naming the out-of-line test module's `gc` identity and no identity under
  another prefix, the `gd` bytes form answering `file_batch`'s batch, the `gw` bytes form answering
  `load_workflow`'s parse and `ingest::batch_generation` answering the generation `key_batch` keys,
  the walk's keyed batches unchanged. This criterion OWNS `events::lower_file`,
  `ingest::walk_exclusions` with its light-lane stub, the `gd` and `gw` bytes forms with
  `file_batch` and `load_workflow` calling them, and `ingest::batch_generation` with `key_batch`
  calling it; the three `(path, bytes, excluded)` functions are criterion 5's and the walk's flag
  criterion 9's, NOT this one's.
- [ ] a test proves THE GRAPH REBUILDS FROM LEDGER AND TREE: a `graph.db` rebuilt by `rigger setup` from entries that all resolve, recorded by one process, equals the one their incremental folds built on spec 101's comparison surface,
  asserted in `tests/ledger_rebuild.rs`, with the incremental side folded from hand-built entries
  through `Projection::apply_generation`, over generations that drop a design link and a code
  entity, a code structural edge dated at the latest entry that folded its file's batch, with an
  entry whose blob git does not hold resolved from a tree file that extracts to its generation, a
  deleted `gc` file's entry with no blob resolved from no bytes, a tree that is not a git repository
  resolving from its files and `rigger setup` run in a repository subdirectory holding the store's
  `.rigger/` resolving from the repository's top level and not the working directory; and, over a
  log holding an entry no source resolves, the rebuild reaching the same live facts and current
  generations for every identity whose latest entry resolves, a superseded unresolved entry folding
  nothing, an entry appended through a plain append (the generic fold's refusal, which makes the
  graph owe its rebuild) paid by `rigger setup` through the ledger fold, an entry whose payload does
  not parse passed over by a rebuild that completes, `Projector::rebuild` handed a re-extracting
  function answering a batch event whose payload the fold rejects failing rather than passing it
  over, a workflow definition's entry whose blob git does not hold resolved from the tree's
  `.rigger/workflow.yml`, and a batch process that dies mid-pass (a loose object whose body is
  truncated) failing the rebuild, and the pass resumed once the object is restored equaling a single
  pass. This criterion OWNS re-extraction, the three `(path, bytes, excluded)` functions,
  `ingest::resolve_entry` with its light-lane stub, `worktree::BlobBatch`, `tree_root`,
  `fold_source`'s routing of an entry, `check_fold_payload`'s ledger arm, resolution by generation
  and its fallback outside a repository, the paying of a fold's hole and the rule of WHAT A REBUILD
  REPRODUCES; the fold rule is criterion 2's, `grounder::tree_bytes` criterion 3's, the split forms,
  `walk_exclusions` and `ingest::batch_generation` criterion 4's, the report's number criterion 6's,
  the sinks with `ingest::entry_of_batch` and the restoration they make criteria 10 and 12's and the
  migration criterion 16's, NOT this one's.
- [ ] a test proves THE REBUILD REPORTS WHAT THE NEXT INGEST RECORDS: `rigger setup`'s rebuild prints the number of identities whose current generation in `graph.db` is not their latest recording's and whose file the tree holds,
  asserted in `tests/ledger_rebuild.rs`, counting the identity whose current generation is not its
  latest recording's and whose file the tree holds, and not one whose file is gone or whose `gc`
  path outside the walk's scope holds a regular readable file, the same after a rebuild interrupted
  and resumed, the default lane's line carrying the note, a zero printed without it and the light
  lane's line carrying none. This criterion OWNS the report's number with
  `ingest::perceived_generations`, its read and the type-list parameter it adds to
  `project_scoped_latest_generations`, and the report's line; the rebuild is criterion 5's,
  `grounder::tree_bytes` criterion 3's and the parameter's removal criterion 13's, NOT this one's.
- [ ] a test proves READERS SKIP PERCEPTION: a `GenerationIngested` in the run stream is absent from `read_current_run`'s slice, with a `RunStarted` before it and with none, and from the seeded key set,
  asserted in `crates/rigger-domain/src/run/read.rs`'s tests and the conductor's `replay_keys.rs`
  tests over hand-built entries, with every derived type absent as before. This criterion OWNS both
  of `read_run`'s reads taking `retention::PERCEPTION_TYPES` and `ReplayKeys`' seed excluding them;
  the sinks that record entries are criteria 10 and 12's and what remains of `ReplayKeys` criterion
  10's, NOT this one's.
- [ ] a test proves THE GROUP LOOKUP ANSWERS A LEDGER ENTRY: `EventStore::latest_in_group` answers a `GenerationIngested` recorded under its group on both backends, and `ingest::latest_generation` answers that entry's generation,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`) and
  `crates/rigger-domain/src/ingest.rs`'s tests over hand-built entries, with an identity whose
  latest recording is a derived row answered as before. This criterion OWNS the group lookup's and
  `latest_generation`'s reading of the ledger type with the contract case; the sinks that record
  entries are criteria 10 and 12's, NOT this one's.
- [ ] a test proves THE WALK HANDS EACH BATCH WITH ITS FLAG: every walk hands an out-of-line test module's batch with its flag set and every other batch with it clear,
  asserted in the grounder crate's tests through a recording `BatchSink` at the code half's
  whole-tree walk and its integration reindex and at the `gd` and `gw` walks, with `key_batch`
  handed each batch's flag and both sinks, `graph_index_lag`'s closure and the test seeders taking
  the flag and ignoring it, what they record unchanged. This criterion OWNS `BatchSink`'s flag,
  `key_batch`'s flag parameter, the code half's two functions answering the flag through
  `walk_exclusions` and each caller it adapts; `walk_exclusions` is criterion 4's and the sinks' use
  of the flag criteria 10 and 12's, NOT this one's.
- [ ] a test proves THE RUN'S SINK RECORDS PERCEPTION AS A LEDGER ENTRY: `RunCtx::emit_keyed_batch` handed a file changed to a non-empty extraction records one `GenerationIngested` from the bytes it read, and no derived event,
  asserted through a recording store at `RunCtx::emit_keyed_batch`, with the entry's generation and
  blob those of the bytes it read, an unchanged file recording nothing, a file whose generation the
  log holds and `graph.db` does not recorded again, a deleted `gc` file and a `gc` path outside the
  walk's scope each recording an entry with no blob, an out-of-line test module's file recording an
  entry whose generation is its boundary batch's and whose flag is set at a whole-tree walk and at
  an integration reindex naming it, an index lowering that lags the file's bytes at a generation
  other than the one both sides hold recording the bytes' generation, a tree that is not a git
  repository recording the blob id `git hash-object` gives, a batch whose key names no identity
  failing the emit and recording nothing, a failing hash function, a read failing for a reason other
  than absence (THE READ FAULT) and a failing read of the log side and one of the graph side each
  doing the same, a lagging lowering whose bytes extract to the generation both sides hold and a
  `gd` file emptied after the walk each recording nothing with the emit succeeding, a graph that
  owes its rebuild recording one entry per generation, an identity whose pre-ledger derived rows
  carry no group recording at its first walk an entry that folds as a re-recording, in the root
  periphery test SINK OUTCOMES names, the ledger form writing one `applied` row per entry, the
  constructor's event parsing back to its five fields under its group and replay key, and a revert
  A, B, A recording three entries and leaving A's facts. This criterion OWNS the run's sink's write
  path, `ingest::entry_of_batch` with SINK OUTCOMES row 5's test, the pure
  `ingest::batch_is_current`, `GenerationIngested::event`, the hash function with
  `worktree::hash_blob` and `Deps::hash_blob`, the ledger form of `FoldingStore` with `Fold::settle`
  made public, what remains of `ReplayKeys`, the passage DOCUMENT EDITS gives it and the moves TEST
  DISPOSITIONS gives it; the fold rule is criterion 2's, the extraction criteria 4 and 5's, the
  readers' exclusions criterion 7's, the group lookup's ledger reading criterion 8's, the walk's
  flag criterion 9's, the memo criterion 11's, `rigger graph build`'s sink criterion 12's and the
  refusal and the pre-ledger row inserter criterion 19's, NOT this one's.
- [ ] a test proves THE RUN'S SINK MEMOIZES THE LOG SIDE: within one process the run's sink asks the store's group lookup for an identity once and answers its later batches from `LoggedGenerations`,
  asserted at the run's sink counted through `CountedRead::LatestInGroup`, with a current batch
  handed twice making one group lookup, a batch whose group lookup failed handed again making a
  second, the identity of SINK OUTCOMES row 13's fixture taking its entry's generation in the memo
  so a second walk of it in the same process makes no group lookup, and a long-lived run restoring
  an identity a rebuild left behind at the next integration reindex naming its file. This criterion
  OWNS `LoggedGenerations` and the memo's updates; the run's sink's write path is criterion 10's,
  NOT this one's.
- [ ] a test proves `rigger graph build` RECORDS PERCEPTION AS A LEDGER ENTRY: `ingest_tree` handed a file changed to a non-empty extraction records one `GenerationIngested` from the bytes it read, and no derived event,
  asserted through a recording store at `ingest_tree` and in `tests/cli.rs`, with the entry's
  generation and blob those of the bytes it read, an unchanged file recording nothing, an identity a
  rebuild left behind restored by the next `rigger graph build`, an identity whose pre-ledger
  derived rows carry no group recording an entry that folds as a re-recording and is not counted in
  N, the `graph build` line's N counting each SINK OUTCOMES row that counts, an entry the ledger
  form refuses on a graph that owes its rebuild named in the line's fold-loss clause, and the light
  lane's build recording no entry and no derived event and exiting 0. This criterion OWNS
  `ingest_tree`'s write path with its hash binding, the graph build line's count, the removal of
  `batch_is_latest_recorded` and `keyed_derived_event` with the test-side successor and the moves
  TEST DISPOSITIONS gives it; `ingest::entry_of_batch` and the rows it answers are criterion 10's,
  the index-lag advisory criterion 13's and the refusal criterion 19's, NOT this one's.
- [ ] a test proves THE LEDGER ANSWERS THE INDEX-LAG ADVISORY: `rigger validate` names a sampled file whose current bytes extract to a generation other than its latest entry's or other than `graph.db`'s current one,
  and names no file whose bytes extract to the generation both hold, asserted in `tests/cli.rs`,
  with a file named when `rigger validate` runs in a repository subdirectory holding the store's
  `.rigger/`, whose ONE ROOT is the repository's top level and not the working directory, an
  out-of-line test module whose entries record its boundary batch not named, a sampled `gc` path
  outside the walk's scope holding a regular readable file whose entry records the generation of
  `gc`'s batch for no bytes not named, a `graph.db` that owes its rebuild compared from the log side
  alone, and the light lane's stub sampling nothing; and, in `src/cli/validate.rs`'s tests,
  `read_graph_index_lag` over a recording store making ONE `read_stream_typed` read with
  `TypeSelection::Only` of `retention::PERCEPTION_TYPES` and no whole-stream read. This criterion
  OWNS `graph_index_lag`'s two-sided comparison, validate's call of `ingest::perceived_generations`,
  the removal of the keys part and of the type-list parameter, whose only production value from
  criterion 13 on is `PERCEPTION_TYPES` (`perceived_generations` stays its caller), and the moves
  TEST DISPOSITIONS gives it; the `(path, bytes, excluded)` functions, `walk_exclusions`,
  `grounder::tree_bytes`, `ingest::perceived_generations` and the type-list parameter until
  criterion 13 are criteria 5, 4, 3 and 6's, the sinks' entries criteria 10 and 12's,
  `Projection::current_generation` criterion 2's and the bloat advisory criterion 18's, NOT this
  one's.
- [ ] a test proves RECLAMATION STAGES IN MEMORY: `Store::reclaim_space` on a sqlite store holding free pages rewrites the file smaller and reports the bytes reclaimed, with its own connection reporting `temp_store` as memory,
  asserted in `crates/rigger-store-sqlite/src/eventstore/sqlite.rs`, with the bytes reported against
  a before-size its caller hands it, a failing compacting step reported and its rerun reclaiming, a
  file holding no free pages left unrewritten and `temp_store` read back on the `Store`'s connection
  after the call. This criterion OWNS `Store::reclaim_space` with `Reclamation`,
  `Store::bytes_on_disk`, `prune_derived_index`'s call of it, the staging passage DOCUMENT EDITS
  gives it and the moves TEST DISPOSITIONS gives it; the verb that calls it is criterion 16's, NOT
  this one's.
- [ ] a test proves THE DERIVED RESET REFUSES AN UNFINISHED REBUILD: `rigger reset --derived` refuses, naming `rigger setup` and changing nothing, while a rebuild left unfinished stands, and refuses a held rebuild lock,
  asserted in `tests/cli.rs` with a regular file standing at `graph.db`'s shadow path and the
  rebuild lock held by the test, the second refused with the reworded `REBUILD_IN_PROGRESS` text,
  and, in `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`'s tests,
  `Projector::rebuild_unfinished` over a standing shadow, a graph file holding the cursor, neither
  and no graph file. This criterion OWNS `Projector::rebuild_unfinished`, the rebuild lock
  `reset_derived` holds, the reworded `REBUILD_IN_PROGRESS` and the passage DOCUMENT EDITS gives it;
  the migration is criterion 16's, NOT this one's.
- [ ] a test proves MIGRATION CONVERTS IN PLACE: `rigger reset --derived` rewrites the first row of each identity's latest batch into its entry when that batch is derived, deletes every other derived row and reports the counts,
  over three generations of a file, a latest generation recorded twice, a deleted file, an identity
  whose latest recording is already a ledger entry above derived rows, an out-of-line test module, a
  path that is not a regular file, a `gc` path outside the walk's scope holding a regular readable
  file recording an entry with no blob, an unkeyed derived event and a keyed one whose replay key
  does not parse, a latest generation whose kept rows stand in two appended batches with an alias
  between, and the unkeyed count on its own line, asserted in `tests/cli.rs`, with each rewritten
  entry at the first position of its identity's latest derived batch, every identity's earliest
  surviving recording dated at its earliest recorded valid-time, the test module's entry recording
  `excluded: true`, no projection or ledger row written to `graph.db`, a rebuild of the migrated
  store reaching the live facts the recordings whose replay key parses asserted, save the spanning
  generation's names resolved through the aliases defined before its lowest kept row, the unparsable
  key's facts absent after it, and the current generations the store held before for the files the
  tree holds unchanged, the bytes reclaimed reported, nothing to shed when run again while still
  reclaiming, and, in `src/cli/hygiene.rs`'s tests, `reset_derived` handed a failing hash function
  failing before any row changes and `reclamation_lines` over hand-built `Reclamation`s, one per
  state. This criterion OWNS the migration, `reclamation_lines`, `Store::shed_derived`,
  `Store::count_derived`, the deletion of `prune_derived_index`, the passages DOCUMENT EDITS gives
  it and the moves TEST DISPOSITIONS gives it; the reclamation and `Store::bytes_on_disk`, through
  which `reset_derived` measures its before-size, criterion 14's, the refusals of an unfinished
  rebuild and a held lock criterion 15's, the menu line and the bloat advisory criteria 17 and 18's,
  the rebuild, `walk_exclusions` and `grounder::tree_bytes` criteria 5, 4 and 3's and the pre-ledger
  row inserter criterion 19's, NOT this one's.
- [ ] a test proves THE RESET MENU PREVIEWS THE MIGRATION: bare `rigger reset` on a store holding derived events prints the count of events and file identities `rigger reset --derived` then sheds, and says none is left to shed once it has,
  asserted in `tests/cli.rs`. This criterion OWNS the menu's `--derived` line, its server-backed
  wording included, `derived_count_phrase`, the removal of `count_derived_duplicates`,
  `DerivedPreview` and `removed_per_type` and the moves TEST DISPOSITIONS gives it; the count and
  the migration are criterion 16's, the bloat advisory criterion 18's, NOT this one's.
- [ ] a test proves THE BLOAT ADVISORY NAMES THE MIGRATION: `rigger validate` on a store holding any derived event prints one warning naming the events and file identities left and `rigger reset --derived`, and prints none once migrated,
  asserted in `tests/cli.rs`. This criterion OWNS `bloat_advisory_for`'s new reading, the removal of
  `measure_derived_duplication`, `DerivedDuplication` and `BLOAT_DUPLICATION_THRESHOLD` and the
  moves TEST DISPOSITIONS gives it; the count and the migration are criterion 16's,
  `derived_count_phrase` criterion 17's and the index-lag advisory criterion 13's, NOT this one's.
- [ ] a test proves THE STORE REFUSES A DERIVED APPEND: an append whose batch holds any derived event is refused naming the type and writes no event of the batch, on both backends,
  asserted in the backend-agnostic contract suite
  (`crates/rigger-store-sqlite/src/eventstore/contract.rs`), with a type no list names accepted.
  This criterion OWNS the refusal with `class_of`, the re-expression over ledger entries of the
  contract-suite tests about the store but the reference test, the one pre-ledger row inserter and
  the moves TEST DISPOSITIONS gives it; the sinks that stop emitting are criteria 10 and 12's, NOT
  this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This criterion OWNS only the lanes over the integrated result.
