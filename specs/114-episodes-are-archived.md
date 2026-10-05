# 114 - Episodes are archived: a finished run's mechanics move to git

**Goal:** rigger is a hive mind: its graph holds the project's whole understanding and serves each
persona the slice it needs, and the append-only log is only the persistence underneath. Once spec
107 has made the log stop holding what the hive merely perceived, it still holds how every finished
run spent itself (its episodes). This spec moves a finished run's episodes to git, readable on
demand, so the live log keeps what the hive decided, learned and did and the current run's
mechanics. Measured on the 2026-10-05 store (288,636 events, 430 MB of payload, a 580 MB file):

- A run's mechanics, the episodic types (`retention::EPISODIC_TYPES`, spec 107), are 21,386 events
  and 348 MB, 81% of the payload; `SpawnRequested` alone is 7,242 events and 343 MB of prompts.
  17,988 of them (278 MB) belong to the 153 runs before the current one and 2,929 (63 MB) to the run
  stream before its first `RunStarted`, and nothing removes them: `rigger reset --runs`
  (`reset_runs`, `src/cli/hygiene.rs`) prunes `graph.db` and deletes no event.
- Serialized, that backlog is 349.9 MB: git writes it at 1.5 s and reads it back at 0.5 s per 100
  MB, and deleting the largest span holds the store's write lock 0.27 s.

## Design

**UNIT ORDER AND BASE, decided here.** The base is a tree where spec 107 has landed, so its symbols
are existing code: `retention` with `EPISODIC_TYPES`, `KNOWLEDGE_TYPES` and `class_of`
(`crates/rigger-domain/src/retention.rs`) and the source scan that classifies every `TYPE_`
constant; `Store::reclaim_space`, which reports against a before-size its caller hands it, and the
`--derived` migration (`crates/rigger-store-sqlite/src/eventstore/sqlite.rs`); `FoldingStore` and
`ingest::folding_into` (`crates/rigger-grounder/src/ingest.rs`); `tree_root(store_dir)`
(`src/cli/mod.rs`, spec 107's ONE ROOT); and `read_graph_index_lag`'s typed read
(`src/cli/validate.rs`); no criterion here has an edge into spec 107. Criterion 1 needs nothing.
Criteria 2 and 3 need 1. Criterion 4 needs 1 and 2. Criteria 5 and 6 need 4, whose `ArchiveOutcome`
both read. Criterion 7 needs 3, 5 and 6. Criterion 8 needs 7, criterion 9 needs 8, and criterion 10
needs 9, since 7 to 10 rewrite the `rigger-reset-store` skill in turn. Criterion 11 needs all ten.
The spec is launched on rigger-run once spec 107 has landed there. A test a unit's change breaks is
that unit's to move, and TEST HOMES, TEST DISPOSITIONS and DOCUMENT EDITS (Notes) give each
criterion's test files, each existing test it moves and each passage it rewrites. A Design sentence
about a later criterion's behaviour describes the integrated result, as criterion 1's architecture
passages do (they state the target), and an earlier unit's tests reach it only through fixtures. On
a tree holding 1 and not 2 the `RunArchive` write already looks its ref up through
`RunArchive::list` and issues `create` for a missing ref, an existing ref failing with a named
error, the state criterion 2 replaces; `archive_run` there appends its `RunArchived` unconditionally
after the `create`, and the skip of a `RunArchived` already naming the blob is criterion 2's. Every
symbol a criterion adds has a production caller at that criterion's landing or is a `pub` item of a
library crate, so no interim tree raises `dead_code`: the decision and the render are library items
`run_archiving` calls from criterion 7 on. `archive` is declared in
`crates/rigger-domain/src/lib.rs` under `#[cfg(any(feature = "store", not(feature = "core")))]`, as
`playbooks` and `review` are, and the git adapter's module under the gate its crate's `worktree`
module carries. A production path archives only from criterion 7 on, after criterion 3 moved `rigger
replay` and `rigger stats --all` onto `read_history`, so no tree archives a span its readers cannot
read.

**THE INVARIANT, amended here so no unit has to.** Section 2.1 of
`docs/architecture-addendum-context-management.md` gains: git retains a finished run's EPISODES; and
its sentence on `reset --runs` now says the verb archives earlier spans before its prune. Criterion
1's unit edits those passages and the others DOCUMENT EDITS gives it. `RunArchived` is KNOWLEDGE:
criterion 1 adds its `TYPE_` constant to `retention::KNOWLEDGE_TYPES`, which spec 107's source scan
then requires, and writes on `EPISODIC_TYPES`' doc comment the rule of the Global constraint on the
class table.

**A FINISHED RUN'S EPISODES MOVE TO GIT.** A run is the span of the run stream from its `RunStarted`
to the next `RunStarted`, the boundary `run::current_run` applies. The PRELUDE is the span below the
first `RunStarted`, which belongs to no run and is never the current run while a `RunStarted`
exists. A span (a run or the prelude) is PENDING when it holds a live episodic event and a
`RunStarted` above it exists, whatever its `RunArchived` state, so a span interrupted after its
`RunArchived` stays pending; a store with no `RunStarted` has no pending span. EVERY PENDING SPAN IS
ARCHIVABLE, with no liveness condition: once the next `RunStarted` is recorded, nothing a spawn or
driver of an earlier span can still do reads that span's episodic rows from the live log, since each
such path (`rigger result`, `prompt`, `scratch`, `resume-unit`, `status`, the hooks, the dash and
`conductor::run`) reads the current run through `read_current_run`, or `progress.db`. A
still-running spawn of an archived span therefore meets what it meets today: `rigger prompt` finds
no request of it in the current run, and its late `rigger result` is recorded in the current run as
an orphan (`result_advisories`). One domain use case, `archive::archive_run`
(`crates/rigger-domain/src/archive.rs`), archives one span as one read hands it, in this order: it
serializes the span's episodic events in position order (Notes), writes the bytes as a git blob
under the span's ref through a new `RunArchive` port, reads the bytes back through
`RunArchive::read` and compares them byte for byte with what it wrote, appends one `RunArchived`
(Notes), then deletes exactly those log positions in one transaction through a new port method
`EventStore::delete_archived(stream, positions)`. The delete holds the store's write lock for that
transaction, 0.27 s at most on this store (Goal), so an appender that arrives meanwhile, a worker's
`rigger result` included, waits inside the 5000 ms busy timeout and lands. THE SPAN DESCRIPTOR (the
ref, the group, the `run` field) is one pure constructor, criterion 1's, over the store digest (none
while the store has no `RunStarted`) and the span's `RunStarted` row (none for the prelude);
`archive_run` and both readers are span-kind-agnostic over it. THE DESCRIPTOR LIST always opens with
the prelude's descriptor, whose ref is formed only when a first `RunStarted` exists, followed by the
constructor mapped over the `RunStarted` rows in position order; on a store with no `RunStarted` it
is the prelude alone, served from its live rows, so `rigger stats --all` folds the whole stream as
`stats_lines` does today. `read_history` maps the constructor over the `RunStarted` rows of its one
read; `archive_pending` never builds the list (FINDING A PENDING SPAN). Within a store a span is
identified by the position of its `RunStarted`, which the log guarantees unique, never by its run
id, which it does not; refs are repository-wide, so a run's ref is
`refs/rigger/archive/<store>/run/<position>` and the prelude's
`refs/rigger/archive/<store>/prelude`, where `<store>` is the event id of the store's first
`RunStarted`, a knowledge event this spec never deletes, hashed by the domain's one stable hash
(`playbooks::fnv1a_64`) and written as 16 lowercase hex digits as `review::critique_hash` renders
it. A position is rendered ONE way, decimal zero-padded to 20 digits (Notes), in the ref, the group
and the replay key alike, so the refs list in run order. So every id yields a ref-safe component and
every store its descriptor list, a recreated store or a second store in one repository gets its own
namespace and a copied store shares one; a repository holds a handful of stores, so a collision at
64 bits is not a concern. The group (`archive/run/<position>`, `archive/prelude`) needs no store
component, since a group lives in one store's log. Knowledge events stay. A span holding no episodic
event is not pending: it writes no ref and no event. ONE FAILURE RULE: any error of git, of the
store or of a lock take other than busy, while finding or archiving a span, is the outcome's ONE
failure. It names the span when one is known and none otherwise, nothing is deleted that was not
already, the span stays pending and `archive_pending` STOPS there, the spans above it waiting for
the next trigger, so a broken store or repository costs at most one span read per trigger, never the
whole backlog. A driver always proceeds to its `conductor::run`, and `rigger reset --runs` prints
the failure and continues with its remaining steps in its order. A blob moved aside is no failure
and does not stop the loop; it is recorded in the outcome as soon as its ref write succeeds,
whatever then happens to the span, so a failed span that moved one prints both lines; a move-aside
whose process died before printing is not reported again, its aside ref standing (`git for-each-ref
refs/rigger/aside` lists it). Archiving changes no fold: no step or one-shot fold reads an earlier
run's episodic event (spec 101). THE READER AUDIT: every reader through `run::read::read_run` or
`read_current_run`, and every typed read of the run stream (`MINT_DECISION_TYPES`, `ADOPTION_TYPES`,
`DecisionMade`, `ReviewFinding`, `SpawnResult` from a current-run revision), reads knowledge types
or the current run; the critique store, the canary stream and `progress.db` are other stores.
`cmd_playbooks`, `stats_lines` without `--all`, `read_model_drift`, `reset_menu`,
`refuse_derived_reset_if_live`, `read_run_units_or_why`, `read_order_signatures` and `reset_runs`'
closure read materialize the whole run stream and use only knowledge types or the current run, so
archiving shrinks what they read and changes none of their answers but one: `read_order_signatures`
flags rows whose revision does not exceed the stream's running maximum, so an order signature whose
high-revision row was archived stops being reported, which is accepted. `read_model_drift`'s fold
reads `META_MODEL_RESOLVED`, which rides only `UnitStatus` events (the `emit_keyed_meta` sites of
`crates/rigger-conductor/src/conductor.rs`), a KNOWLEDGE type, so `metrics::model_drift` is
unaffected. Two whole-stream reads change: `cmd_replay` and `stats_lines --all` read through
`read_history` (criterion 3); `read_graph_index_lag` already reads by type (spec 107). A console tab
meets nothing either: its provider holds no earlier span's row. ONE DIRECTORY: everything the
archive touches derives from the store's `.rigger` directory: the step lock is taken there, the git
adapter runs every command with `git -C` at spec 107's `tree_root(store_dir)`, and the decision's
repository input is whether that root is a git top-level (`git_repo_at` answering one). `RunArchive`
is declared in the domain beside `EventStore`; its one adapter, in `crates/rigger-worktree-git`,
runs exactly these: `git hash-object -w --stdin` and `git update-ref --stdin` for the write, `git
for-each-ref` for `RunArchive::list(pattern)` (the write's lookup of its ref and validate's listing
of the namespace), `git cat-file --batch` for `RunArchive::read(blob)` and `git cat-file
--batch-check` for `RunArchive::holds` (criterion 10's), the last two with `GIT_NO_LAZY_FETCH=1`, so
held means held in this repository's object database and a partial clone starts no fetch. `read`
answers three ways in one process: the bytes, NOT HELD (git prints `<id> missing`) or an error. Git
compresses the blob, so no compression crate is added. Git is the retention system: the ref is local
until the operator pushes it, and no blob rigger wrote ever loses its last ref by rigger's hand. The
rebuild (spec 107's `rebuild_owed_graph`) reads no archive, since an episodic event changes neither
the live projection nor the fold state (spec 107's classes criterion), the `applied` rows of
archived positions are outside spec 101's comparison surface, and an archive delete landing between
the rebuild's position read (`read_live_positions`) and its selection read (`read_live_selection`)
removes only positions whose fold changes no fact, so the rebuild needs no lock beyond
`graph.db.lock`. BACKEND SCOPE: the sqlite store deletes; the KurrentDB adapter answers each of the
three archive port methods (`delete_archived` and the two reads below) with the port's existing
backend error (`Error::Backend`) naming the method, with no new error variant; production never
reaches them, since the decision skips a store that is not sqlite first, and `read_history` calls
none of them, so on KurrentDB `rigger replay` and `rigger stats --all` answer as today (criterion
3). Every `EventStore` implementation (both adapters, `Namespaced`, `FoldingStore`, the test
doubles) gains the three methods in criterion 1's unit.

**FINDING A PENDING SPAN READS THE INDEX, NOT THE BACKLOG.** A span is a range of log POSITIONS:
from its `RunStarted`'s position up to the next `RunStarted`'s (the prelude: from the stream's start
up to the first `RunStarted`'s), whatever revision a row carries. Two new port reads, both criterion
1's and both anchored on position exactly as `read_stream_typed` is:
`EventStore::first_of_types(stream, from, types)` answers the position of the oldest event at or
after log position `from` whose type is named, never its data (on sqlite one seek of
`idx_events_stream_type` per named type), and `EventStore::read_span_typed(stream, from, to,
selection)` reads the events at log positions from `from` below `to` that `selection` admits, in
position order. `archive_pending` decides in positions only, by three reads: B = `first_of_types(0,
[RunStarted])` (none: nothing is pending); E = `first_of_types(0, episodic types)` (none: nothing is
pending); N = `first_of_types(E, [RunStarted])` (none: E lies in the current run and nothing is
pending). Otherwise E's span is pending and ends at N: it is the prelude when E lies below B, and
otherwise the run whose `RunStarted` is the last at or below E, which `first_of_types(..,
[RunStarted])` steps to forward, resuming from the previous span's end and never from B again. So a
row is archived only when it lies below the current boundary in position order: a current-run row a
stale writer reissued at a low revision (`Error::OutOfOrder`) lies above the boundary and in no
earlier span. It reads the first `RunStarted` (`read_span_typed` at B) for the store digest, reads
E's span alone through `read_span_typed` with `TypeSelection::Except` of the derived types, builds
that span's descriptor from the `RunStarted` row that read returns (the prelude's from none),
archives it or records the failure, and asks again with E taken from N. A trigger with nothing
pending costs at most three port reads, each index-only, and a backlog is held one span at a time.

**AN INTERRUPTED ARCHIVE COMPLETES.** The serialization is a pure function of the rows, so the next
archive of an interrupted span re-serializes its live episodic events to the same blob. The
`RunArchive` write reads the ref's present value through `RunArchive::list` handed the exact ref and
issues one `git update-ref --stdin` transaction by its state: a missing ref gets `create <ref>
<blob>`; a ref already naming that blob is left as it stands; a ref naming another blob `<old>` (a
diverged copy, CONSTRAINTS WALK) gets `update refs/rigger/aside/<old> <old>` then `update <ref>
<blob> <old>`, which keeps `<old>` reachable, is a no-op on an aside ref already naming it, and
applies neither line when the ref moved meanwhile (git exits 128, naming the ref's value and
`<old>`). The archive then proceeds as for any span (read-back, `RunArchived`, delete), its result
naming the span, the ref, the blob moved aside and its aside ref. A `RunArchived` carries
`META_GROUP` (`archive/run/<position>` or `archive/prelude`) and a replay key naming its blob
(Notes), so the group lookup answers the span's latest `RunArchived` and its blob without reading
the stream: one naming the same blob is not appended again (criterion 2's skip), and one naming
another blob is followed by a new one, which readers and validate take as the span's. A
`RunArchived` belongs, as an archive record, to the span its group names, and as a live knowledge
row to the span its own position lies in. The delete is one transaction and no row enters a span
below the current boundary (sqlite gives each append a position above every one it has used,
`AUTOINCREMENT`, and spec 107's migration rewrites rows in place into knowledge), so while the class
table stands (Global constraints) a span's episodic rows are all live or all gone. A class table
changed without the spec that constraint requires loses nothing: the earlier blob is kept under its
aside ref and the move is printed. The prelude resumes as a run does.

**THE ARCHIVE IS READ ON DEMAND.** `archive::read_archived(blob)` reads the blob through
`RunArchive::read`, parses the rows and yields the events in position order; NOT HELD is the LOST
SPAN, and any other git failure, and bytes that do not parse, are an error naming git's or the
parser's message. Criterion 1 owns it; `read_history` hands it the blob the span's latest
`RunArchived` records. The ref is the retention anchor and nothing else: no reader resolves it, and
`rigger validate` alone watches it. One domain function, `archive::read_history` (criterion 3's),
over `&dyn EventStore` and `&dyn RunArchive`, serves both readers the spans of the descriptor list.
Its one read is the forward `read_stream` from revision 0 both commands make today (`cmd_replay`
through `Namespaced::read_stream`, `stats_lines` through `read_project_stream`), and each command
builds the adapter from the store directory it reads the store from (ONE DIRECTORY). On sqlite that
read is one statement (`read_forward` prepares one SELECT and steps it) on a connection
`crate::sqlite::open_connection` opens in WAL mode, so it reads one snapshot of the log, fixed when
the statement starts, whatever an archiver commits meanwhile; since an archive appends its
`RunArchived` before its one-transaction delete, the read holds all of a span's episodic rows or
none of them, with or without the span's `RunArchived`, and on KurrentDB nothing is archived, so no
span changes under the read. The read is ordered by revision, not position, and the span assignment
needs no order: `read_history` assigns each row to its span by position range, against the
`RunStarted` positions in the same read, so a row a stale writer reissued at a low revision counts
in the span its position names, and it yields every span's events in position order in all three
states. It is TOTAL over a span's three states: a span holding an episodic row in that read is
served from its live rows and its ref is not resolved; a span with none and a `RunArchived` is
served from `read_archived` of its latest one, its archived events interleaved by position with its
live knowledge rows; a span with none and no `RunArchived` (a run of knowledge only, a current run
just started, a prelude with no episodes) is served from its live rows alone, resolving no ref and
printing no lost-span line or refusal. No span is read from both sides, and a pending span reads
complete. `rigger replay <run>` (`cmd_replay`, `src/cli/mod.rs`) resolves the run id to its
`RunStarted`, the first match as `baseline_run_slice` takes it today, and `latest` to the last
descriptor of the list (the prelude on a store with no `RunStarted`, as `current_run` answers
today), always served from live rows; it reads that span through `read_history` before it slices the
baseline, refuses a lost span naming its position, ref and blob on standard error with a failing
exit, and fails naming the error on any other read failure; a later `RunStarted` sharing that run id
is not addressed by id, and no run id names the prelude, as today. `rigger stats --all`
(`stats_lines`) reads every span of the descriptor list through `read_history`, folds every span it
can read, prints the aggregate on standard output, names each lost span (its position, ref and blob)
on standard error and exits 0, the state validate's not-held line reports (VALIDATE), and fails
naming the error on any other read failure. A missing or repointed ref whose blob git still holds
changes nothing either command prints. Neither command folds a `RunArchived` or a
`GenerationIngested`: `metrics::project` (`crates/rigger-domain/src/metrics.rs`), which both fold,
ignores every type it does not name, and `replay_trajectory` keeps only `SpawnResult` and
`GateVerdict`, so the `RunArchived` an archive appends to the current run changes no line of their
output. No step, one-shot command or rebuild reads an archive.

**ARCHIVING RUNS AT TWO TRIGGERS.** One domain function, `archive::archive_pending`
(`crates/rigger-domain/src/archive.rs`, over `&dyn EventStore` and `&dyn RunArchive`), finds the
pending spans as above and hands each to `archive_run` in position order, the prelude first. It
returns an `ArchiveOutcome`, declared beside it (criterion 4's, every variant and the totals'
values), which is DATA: the totals, at most ONE failure (ONE FAILURE RULE), each blob moved aside,
and at most ONE skip, permanent or the transient skip of a held lock. WHETHER THIS PROJECT CAN
ARCHIVE is one decision, `archive::archive_skip(store_is_sqlite, in_git_repository)`, a `pub`
function of `crates/rigger-domain/src/archive.rs`, criterion 5's, which never calls the store and
answers exactly one PERMANENT skip or none, the store judged first: a store that is not sqlite,
whose adapter cannot delete, answers its skip whatever the repository; a sqlite store outside a git
repository, where `git hash-object -w` would fail, answers the repository's. Its inputs are facts
its caller already has: whether the store selection the command resolved is sqlite
(`StoreSelection::is_sqlite`, `src/cli/mod.rs`, a private type of the binary, so the decision takes
its answer as a flag and lives in the library) and the ONE DIRECTORY's repository answer. ONE render
function, `archive::render(outcome, cadence)`, a `pub` function beside `ArchiveOutcome`, criterion
6's, turns an outcome into lines in two cadences: the DRIVER cadence prints one line for the failure
and one per blob moved aside, and nothing else (a permanent skip is the project's property, and the
next trigger retries a held lock), and every driver writes it to STANDARD ERROR through
`stderr_line`, never to standard output, which for `rigger step` is the JSON its courier parses; the
RESET cadence prints every outcome, its one skip, its failure and the totals. Every caller reaches
the archive through one wiring helper, criterion 7's whole: `run_archiving(store_dir: &Path, held:
Option<&HeldLock>, store: &dyn EventStore, graph: &dyn Projection, store_is_sqlite: bool) ->
ArchiveOutcome` in `src/cli/run.rs`. In a fixed order it (1) calls the decision, returning a
permanent skip before any lock is touched, so a permanent skip outranks the held-lock skip; (2) uses
the lock handed, else takes one without waiting in `store_dir`, a busy take answering the transient
skip; (3) hands `archive_pending` the git adapter at `tree_root(store_dir)` and the store wired
exactly as the driver's own appends are, `ingest::folding_into` over the graph its caller opened
with `open_graph`, so each `RunArchived` is folded and writes its `applied` row. A fold that is lost
is the folding store's reported loss, on that store's own stream under every caller, and leaves the
graph owing its rebuild as any lost fold does (spec 101); the delete proceeds, the prune and
compaction of `rigger reset --runs` then run over the marked file as today, and the next `rigger
setup` rebuilds it. The step lock alone serializes archivers; without it two could each find no
`RunArchived` for a span and each append one. THE DRIVERS: `rigger step` (`cmd_step`), `rigger run`
under both its drivers (`run_cli` for `--driver cli`, `run_workflow` for `--driver workflow`) and
`rigger serve` (`cmd_serve`, through `run_workflow`) call `run_archiving` immediately before each
`conductor::run` call they make, `cmd_step` with the step lock it holds for the whole step and the
others with none, each handing the cwd-relative `.rigger` its store is opened from (`RIGGER_DIR`),
and write the outcome in the driver cadence; `rigger workflow` (`cmd_workflow`) launches a script
that spawns `rigger serve` and calls neither. `conductor::run`, `Deps` and `crates/rigger-conductor`
are untouched by this spec. A run superseded by a `RunStarted` minted inside a pass is archived at
the next pass, and nothing depends on the difference. The replay's isolated re-drive (`cmd_replay`)
calls `conductor::run` over an isolated store and never archives; the canary
(`canary_store::run_canary`) calls no `conductor::run`. `rigger reset --runs` (`reset_runs`) runs,
in order: its removal of a stale pruned copy and `open_graph`, which refuses a `graph.db` that owes
its rebuild, as today, so an owed graph archives nothing; its probe (`live_writer_facts` over the
current run's slice), which takes the step lock in `StoreLocation::dir` and keeps it to the end; on
sqlite, one measurement of the store's on-disk size (main file plus `-wal`); the helper, ALWAYS
called, handed `StoreLocation::dir` and the probe's lock or none, so with none the helper's own take
decides (it archives if the step ended meanwhile, else answers the held-lock skip), its outcome
printed in the reset cadence on standard output; its ONE whole-stream closure read, as `reset_runs`
documents it, now after the archive, when the stream no longer holds the backlog;
`close_landed_units` over the current run's slice; the graph prune and the graph file's compaction
as today; then, on sqlite only, the RECLAMATION STEP. The drivers' `RIGGER_DIR` and reset's
`StoreLocation::dir` name the same `step.lock` for one store today, the `.rigger` holding its
`events.db` (`acquire_step_lock`). The reclamation step is ONE function, criterion 9's, over the
sqlite `Store`, the before-size and the probe's facts, returning its line: it calls
`Store::reclaim_space` with that before-size only while the probe still holds the lock and its
live-writer facts are dead, since an appender that outlasts the `VACUUM`'s busy timeout fails and
does not retry, which would lose a live run's `rigger result`, and prints the before-size less the
size after, what the whole verb gave back; else it prints one line saying reclamation was skipped
and why, the archive and its delete standing. The facts are the probe's, taken once: a driver that
registers after the probe is accepted, its appends waiting inside the busy timeout as beside the
delete. A store that is not sqlite prints only the archive's permanent skip. On a copy of this store
after both specs' deletes it took 0.44 s, 580 MB to 92 MB. A driver never reclaims: a later append
reuses the freed pages. A driver that dies holding the lock releases it with its process, and the
next trigger completes the interrupted span. The first trigger on an older store pays the whole
backlog once, about 9 s for this store, so no budget knob is added; the documented pre-run `rigger
reset --runs` normally pays it. The `--runs` text (DOCUMENT EDITS) describes the archive, its
reclamation and the lock rule. The bare `rigger reset` menu's `--runs` line keeps previewing the
graph prune alone; archiving is not previewed.

**VALIDATE NAMES A LOST ARCHIVE REF.** A deleted archive ref is recoverable while git still holds
its blob: the `RunArchived` records the blob id, so one `git update-ref` restores the ref, and git
prunes an unreachable object once its grace period passes. `rigger validate` runs before every
launch, so its report is the notice that arrives inside that window. The advisory is one function
over `&dyn EventStore` and `&dyn RunArchive`, criterion 10's, which `cmd_validate` wires as spec 107
wires `read_graph_index_lag`: the store it opens through `with_project_store` and the adapter built
from that store's `.rigger` directory. It reads every `RunArchived` through one
`EventStore::read_stream_typed` of that type; with none it reads nothing further and starts no git
process. Otherwise it keeps each span's latest, lists the refs under its own store's namespace,
`refs/rigger/archive/<store>/`, with their blob ids through `RunArchive::list` handed the namespace
(one `git for-each-ref` process), and compares the two as sets. For the spans whose latest
`RunArchived` names a ref that is missing or names another blob, the prelude's included, and whose
recorded `first` position holds no live episodic row (one `first_of_types` from `first` each; a
pending span's ref is rewritten by the next trigger), it asks git whether it holds each recorded
blob through `RunArchive::holds` (one `git cat-file --batch-check` process over all of them), so the
advisory starts at most two git processes whatever the span count. Its lines go to standard error,
beside validate's other advisories. A span whose blob git holds prints one advisory line: the span
(its run id and position, or `prelude`), the full ref, the recorded blob id and the restore command:
`git update-ref <ref> <blob> ''` for a missing ref, whose empty old value makes git refuse when the
ref exists meanwhile, and for a ref naming another blob `<old>` the two instructions the
`RunArchive` write issues, `printf 'update refs/rigger/aside/<old> <old>\nupdate <ref> <blob>
<old>\n' | git update-ref --stdin`. Obeying either reaches a ref naming the recorded blob, which is
silent, and leaves no blob unreachable; a ref moved meanwhile makes git refuse and write nothing.
The spans whose blob this repository does not hold are named together on ONE line, their count,
positions and full refs, saying this repository does not hold their blobs and naming the namespace a
remote may hold them under with the fetch refspec that restores them,
`refs/rigger/archive/<store>/*:refs/rigger/archive/<store>/*`, a line that repeats while they stay
missing. The advisory never fails validate; criterion 10 adds `RunArchive::holds` to the port and
its git adapter. A project outside a git repository and a KurrentDB store print nothing; the
advisory writes and carries nothing, so a repeated validate prints the same lines. The
`rigger-reset-store` skill names it.

**CONSTRAINTS WALK, decided.**
- *Cold start:* no state is kept in memory; pending spans are found from the type index.
- *Existing data:* a store that predates this spec holds every earlier span live; its first trigger
  pays the backlog once (ARCHIVING RUNS AT TWO TRIGGERS), and a store spec 107 has not migrated
  archives its spans all the same, since a span's read excludes the derived types. The archive holds
  what the port reads, so a row whose stored metadata is not a JSON object of strings archives with
  the empty metadata every reader already sees.
- *Recreated store:* a store recreated in the same repository, or a second store beside it, has
  another first `RunStarted` and so another namespace; the old store's refs stay in git untouched
  and unreferenced, which is the retention rule.
- *Copied store:* a copied or restored `events.db` shares its namespace; a diverged timeline's span
  finds its ref naming the earlier timeline's blob and moves it aside, and the abandoned copy's
  validate then reports that ref. Two diverged copies in one repository are kept consistent no
  further: nothing is lost, each older blob staying under its aside ref.

**STATE PLACEMENT.** The archive index is the log (`RunArchived`); the archived bytes are git;
`graph.db` is a projection. A sidecar file listing archived runs, a `.rigger/` marker or a cache of
resolved blobs is NOT an implementation of either.

**OUT OF SCOPE.** `progress.db`, streams other than the run stream, archiving on KurrentDB, a
deleted `graph.db` (forbidden by `docs.rs`, unchanged here), a store archived under another class
table (Global constraints) and everything of spec 108.

## Notes (non-criteria)

`RunArchived { run, ref, blob, events, bytes, first, last }`: `run` is the run id, or empty for the
prelude, which no run owns; `ref` is `refs/rigger/archive/<store>/run/<position>` or
`refs/rigger/archive/<store>/prelude`, where `<store>` is the 16-hex-digit digest of the event id of
the store's first `RunStarted`; `events` and `bytes` count what the blob holds; `first` and `last`
are the lowest and highest archived positions. It carries `META_GROUP` `archive/run/<position>` or
`archive/prelude` and the replay key `<group>@<blob>#<events>`, the key form of spec 107's ledger
entry. `<position>` is the global position (`Position`, a `u64`) of the span's `RunStarted`, written
in decimal zero-padded to 20 digits, the width of the largest `u64`, in the ref, the group and the
replay key alike.

The archive blob holds one JSON object per archived row, in position order, keys in this order:
`position`, `stream`, `type`, `id`, `data`, `meta`, `valid_from`, `recorded_at`, `revision`. `data`
is the row's bytes as a JSON string when they are UTF-8, else lowercase hex under `data_hex`; `meta`
is its JSON object; times are integer nanoseconds since the epoch.

TEST HOMES. Where each criterion's assertions live, by what each crate can see:
`crates/rigger-domain` has no dev-dependency and gains none, so its archive tests use doubles of
both ports and nothing else; an assertion over a fixture git repository lives where the git adapter
is visible, and one over the sqlite store and git where both are.

| Criterion | Test files |
|---|---|
| 1 | `crates/rigger-domain/src/archive.rs` (doubles: the descriptor list, a span with no episode, a read-back not held or failing); `tests/episode_archive.rs`, new (sqlite and a fixture git repository: a run and the prelude archived, two stores in one repository, an id that is not a UUID, an existing ref's named error); the adapter's tests in `crates/rigger-worktree-git` (`create`, `list`, `read`'s three answers); `assert_archive_contract` in `contract.rs`, a second contract entry point the `sqlite.rs` and `namespace.rs` suites run (the three port methods over port-written rows); `sqlite.rs`'s own tests (the reissued-row anchoring in raw SQL, beside `a_typed_read_hands_back_a_reissued_row_where_the_log_recorded_it`); `kurrentdb.rs`'s `passes_the_contract` (the three `Error::Backend` answers) |
| 2 | `tests/episode_archive.rs` (a store double failing on command over a fixture git repository); the adapter's tests (the write's two existing-ref states) |
| 3 | `archive.rs` (`read_history` over doubles); `tests/cli.rs` (both commands) |
| 4 | `archive.rs` (the outcome, the stop rule and the three reads, on a recording double); `tests/episode_archive.rs` (every pending span over sqlite and a fixture git repository) |
| 5, 6 | `archive.rs`, with no double and over hand-built outcomes |
| 7 | `src/cli/run.rs`; `tests/cli.rs` through `rigger step` |
| 8 | `tests/cli.rs` |
| 9 | `src/cli/hygiene.rs` (the reclamation function over a sqlite `Store`); `tests/cli.rs` |
| 10 | `src/cli/validate.rs` (the advisory over doubles of both ports); `tests/cli.rs` |

TEST DISPOSITIONS. Each existing test this spec breaks, the criterion whose change breaks it first
and what it pins afterwards. No other existing test asserts that the run stream keeps an earlier
run's episodic row after a driver step or a reset.

| Test | First broken by | Disposition |
|---|---|---|
| `reset_runs_alone_migrates_a_legacy_store_and_its_report_says_what_that_wrote` (`tests/reset_derived_compaction_periphery.rs`): its "deletes no event" report needle and its every-row-survives check | 8 | the needle pins the rewritten report line; survival is restated over the knowledge rows, earlier spans' episodic rows archived |
| `each_reset_mode_sheds_only_its_own_accumulation_and_composing_them_does_exactly_both` (same file), its `--runs` half as spec 107's re-homing leaves it | 8 | `--runs` deletes no event but the archived episodic rows of earlier spans |
| `reset_runs_closes_a_dead_landed_run_whatever_a_prior_run_left_unanswered` (`tests/reset_runs_closes_landed_run_periphery.rs`): `assert_prefix_kept` and "exactly one terminal event" | 8 | the prefix is restated over the knowledge rows, and the `RunArchived` the reset appends is expected beside the `UnitIntegrated` |
| `discipline_names_reset_derived_as_the_event_logs_own_prune` (`docs.rs`), as spec 107 leaves it | 8 | pins the discipline naming both verbs that delete events |

DOCUMENT EDITS. Each passage this spec makes false, the criterion whose change makes it false and
rewrites it, and the tests pinning its text;
`committed_registry_docs_are_in_sync_with_a_fresh_render` (`src/cli/mod.rs`) pins every rendered
skill, so each criterion commits its re-render with its edit.

| Passage | Rewritten by | Pinned by |
|---|---|---|
| `docs/architecture-addendum-context-management.md` section 2.1 and its append-only log box; `docs/architecture.md` R2, the append-only event log (they state the target, UNIT ORDER) | 1 | none |
| `README.md`: "You never edit or delete an event" | 7 | none |
| the `rigger-reset-store` skill (`crates/rigger-domain/src/docs.rs`): "only one of them holds anything durable", "every decision, finding, gate verdict ... ever recorded" and the anti-move "The event log is append-only truth" | 7 | the render test |
| `reset_runs`' printed report line ("this prune deletes no event from the log ...") and its doc comment (`src/cli/hygiene.rs`) | 8 | `reset_runs_alone_migrates_a_legacy_store_and_its_report_says_what_that_wrote` |
| the `--runs` text on the archive and the lock rule: usage in `src/main.rs`, the `reset_modes` flag list, the reset-store skill's procedure, the `--runs` guidance in `docs.rs` and the discipline's "has its own prune" and "each prunes its own accumulation" as spec 107 leaves them | 8 | the render test; `discipline_names_reset_derived_as_the_event_logs_own_prune` |
| the same `--runs` text on reclamation | 9 | the render test |
| the reset-store skill's archive-ref advisory text | 10 | the render test |

`RunArchived` is the spec's one new event type; its point is where a finished run's episodes live.

## Global constraints

- Hyphens, never em dashes, in every added line.
- One new event type and no others; no new dependency.
- Every lane green: fmt, and clippy and tests on the default lane and on the two lanes
  `.rigger/gates/lanes.sh` runs, `no-default` (`--workspace --no-default-features`) and `core`
  (`--no-default-features --features core` over every member declaring a `core` feature).
- No EPISODIC event leaves the live store before its bytes are read back from git, on a first pass
  and on a resume alike.

- THE CLASS TABLE IS FIXED for this spec: `retention::EPISODIC_TYPES` gains or loses a type only by
  a spec that says what that type's rows in already-archived spans become; this spec decides nothing
  for a store archived under another table.
- Every archive is deterministic: the same rows yield the same bytes.
- The gates cannot see the KurrentDB half of criterion 1 where the contract suite's container is
  unreachable; the adjudicator demands that run's evidence.

## Done when

- [ ] a test proves A SPAN'S EPISODES ARE ARCHIVED: archiving a run's span writes its episodic events under `refs/rigger/archive/<store>/run/<position>`, records one `RunArchived` and deletes exactly those events, its knowledge events staying,
  with the prelude's span archived the same way under `refs/rigger/archive/<store>/prelude` with an
  empty `run`, the descriptor list opening with the prelude and naming every run in position order
  and a store where no run has started listing the prelude alone, a span holding no episodic event
  writing no ref and no event, a read-back answering not held or an error deleting nothing, a second
  store in the same repository archiving the same positions under its own namespace, a first run
  start whose id is not a UUID archiving under its digest and a ref that already exists failing the
  write with a named error, each in the file TEST HOMES (Notes) gives it; and `first_of_types`,
  `read_span_typed` and `delete_archived` proved by the archive contract cases the sqlite and
  `Namespaced` suites run, anchored on position over a row reissued at a low revision, and answering
  `Error::Backend` naming the method on KurrentDB. This criterion OWNS `RunArchived` with its
  constant, its `KNOWLEDGE_TYPES` row, its shape, group and replay key, the addendum amendment and
  the architecture passages DOCUMENT EDITS gives it, the class table's rule on `EPISODIC_TYPES`' doc
  comment, the span-kind-agnostic use case, the span descriptor's constructor, its digest and the
  descriptor list, `read_archived`, the `RunArchive` port with `read` and `list` and its git
  adapter's `create` write, `EventStore::delete_archived`, `first_of_types` and `read_span_typed`;
  the write to an existing ref, the skip of a `RunArchived` already naming the blob and the resume
  are criterion 2's, `holds` criterion 10's and every trigger criteria 4, 7 and 8's, NOT this one's.
- [ ] a test proves AN INTERRUPTED ARCHIVE COMPLETES: an archive stopped after its ref was written, and one stopped after its `RunArchived` was recorded, are each completed by the next archive of the span after a read-back of the blob,
  with no second ref, no second archive event and every episodic event of the span gone, a span
  whose ref names another blob archived after that blob is kept under `refs/rigger/aside/<blob>`
  with its result naming the ref, both blobs and the aside ref, and an aside ref already naming that
  blob left as it is, driven through a store double that fails on command over a fixture git
  repository in the files TEST HOMES gives it. This criterion OWNS the resume, the skip of a
  `RunArchived` already naming the blob and the write's two existing-ref states; the first pass is
  criterion 1's and every line criterion 6's, NOT this one's.
- [ ] a test proves THE ARCHIVE READS BACK: `rigger replay <run>` and `rigger stats --all` print, over a store whose earlier spans are archived, the bytes of standard output they print over that store before archiving,
  with the archive event standing in the current run, the prelude archived, a span archived and not
  yet deleted, a missing ref and a repointed ref whose blob git holds printing those bytes too, a
  run of knowledge only and a current run just started served from their live rows, a store where no
  run has started folded whole by `--all` and answered as `latest` by replay, replay of a span whose
  blob this repository does not hold refusing on standard error with a failing exit, `--all` over
  such a span folding the others, naming it on standard error and exiting 0, and a blob whose bytes
  do not parse or whose read fails failing both commands naming the error; and `read_history`
  serving once, from its live rows, a span a store double archives mid-read after returning its
  rows, and calling no archive port method over a store double that answers each with a backend
  error, with every span's events yielded in position order in all three states, each in the file
  TEST HOMES gives it. This criterion OWNS `read_history`, its read, its three states and both
  commands' calls of it; `read_archived` and the descriptor list are criterion 1's and validate's
  advisory criterion 10's, NOT this one's.
- [ ] a test proves EVERY PENDING SPAN IS FOUND AND ARCHIVED: `archive_pending` archives every pending span of the run stream, the prelude first, and leaves the current run's events live,
  with the knowledge below the first run boundary staying, a store where no run has started
  archiving nothing, a run and a prelude interrupted after their archive event completed, a
  current-run row at a reissued low revision left live, a span whose archive event the store fails
  to append named with its error and stopping the loop with the spans above it left pending, a read
  failing before any span is known recorded as the one failure naming no span, a span that moved a
  blob aside and then failed recording both, and a store with nothing pending answered after at most
  three port reads, each index-only, counted on a recording store, each case asserted on the outcome
  as data in the file TEST HOMES gives it. This criterion OWNS `archive_pending` and
  `ArchiveOutcome` with every variant and the totals' values; the descriptor list is criterion 1's,
  the decision criterion 5's, the render criterion 6's, `run_archiving` and the drivers criterion
  7's and the reset triggers criteria 8 and 9's, NOT this one's.
- [ ] a test proves A PROJECT THAT CANNOT ARCHIVE IS SKIPPED: the decision answers exactly one permanent skip naming why, the store judged first, and nothing for a sqlite store in a git repository,
  over the four combinations of its two inputs with no store double: a store that is not sqlite
  answering its skip inside a git repository and outside one, and a sqlite store outside a git
  repository answering the repository's, in the tests of `crates/rigger-domain/src/archive.rs`. This
  criterion OWNS `archive::archive_skip`, which never calls the store and yields nothing but those
  two permanent-skip variants; the variants are criterion 4's, the render criterion 6's and
  `run_archiving`, which derives its inputs and calls it, criterion 7's, NOT this one's.
- [ ] a test proves EVERY OUTCOME RENDERS ONCE PER CADENCE: the render turns an `ArchiveOutcome` into the driver cadence's lines and the reset cadence's,
  the driver cadence one line for the failure and one per blob moved aside and nothing else, and the
  reset cadence every outcome, its one skip, its failure and the totals, over hand-built outcomes
  only in the tests of `crates/rigger-domain/src/archive.rs`, with a failed span and a blob moved
  aside each named once in both cadences, a failure naming no span printed in both, no driver line
  for a permanent skip, a held lock or nothing pending, and each permanent-skip variant and the
  held-lock skip printed once in the reset cadence. This criterion OWNS `archive::render`, its two
  cadences and every line's text, the totals line included; the outcome and the totals' values are
  criterion 4's, the decision criterion 5's and the streams criteria 7 and 8's, NOT this one's.
- [ ] a test proves EVERY DRIVER ARCHIVES UNDER THE STEP LOCK: `run_archiving` handed no lock archives a pending span when the step lock is free and releases it, and returns the transient skip when another process holds it,
  with a store selection that is not sqlite and a fixture directory outside a git repository each
  archiving nothing, taking no lock and calling no archive port method even while another process
  holds the lock, a failed span named while `rigger step` still runs its `conductor::run`, each
  archive event folded into `graph.db` with its `applied` row, `cmd_step` calling it with the lock
  it holds before its `conductor::run` and `run_cli` and `run_workflow` with none, and a span whose
  ref names another blob named once on standard error while `rigger step`'s standard output stays
  the JSON its courier parses, each in the file TEST HOMES gives it. This criterion OWNS
  `run_archiving` whole (the decision's inputs and call, the lock rule, the folding store's wiring,
  the `archive_pending` call), the drivers' calls, the driver cadence's stream and the passages and
  test moves DOCUMENT EDITS and TEST DISPOSITIONS give it; the decision is criterion 5's, the render
  criterion 6's, `archive_pending` criterion 4's and the reset triggers criteria 8 and 9's, NOT this
  one's.
- [ ] a test proves `rigger reset --runs` ARCHIVES: on a store whose earlier runs were never archived, it archives each pending span before its whole-stream closure read and prints the archive's outcome,
  with a `graph.db` that owes its rebuild refused before the probe and archiving nothing, a lost
  fold of an archive event leaving the graph owing its rebuild while the prune and the compaction
  run over it as today and the next `rigger setup` rebuilding it, a probe holding no lock printing
  the held-lock skip while a step runs and archiving once the step has ended, a failed span named
  and the remaining steps run, and a project outside a git repository named once, every line on
  standard output, in `tests/cli.rs`. This criterion OWNS the reset's call of `run_archiving` and
  its place in the reset's order, handing the outcome to the reset cadence on standard output, and
  the passages and test moves DOCUMENT EDITS and TEST DISPOSITIONS give it; the totals' values are
  criterion 4's, the render criterion 6's, `run_archiving` criterion 7's, the reclamation criterion
  9's and the skill's archive-ref advisory text criterion 10's, NOT this one's.
- [ ] a test proves `rigger reset --runs` RECLAIMS: after its archive and its graph prune, it reclaims the store's free pages and prints what the whole verb gave back, the size it measured once before the archive less the size after,
  with the file smaller on disk and, while a live writer holds, reclamation skipped with a line
  saying why, the archive and its delete standing, through the binary in `tests/cli.rs` and over the
  reclamation function handed live and dead facts in the tests of `src/cli/hygiene.rs`. This
  criterion OWNS the before-size measurement, the reclamation function with its
  `Store::reclaim_space` call, its live-writer condition and its two lines, and the passages
  DOCUMENT EDITS gives it; the archive trigger is criterion 8's and `Store::reclaim_space` spec
  107's, NOT this one's.
- [ ] a test proves VALIDATE NAMES A LOST ARCHIVE REF: `rigger validate` on a store with an archived run whose ref was deleted prints one advisory naming the run, the ref, the recorded blob and `git update-ref <ref> <blob> ''`, and still exits 0,
  on standard error, with a ref naming another blob printed with the aside transaction, a pending
  span whose ref was deleted not reported, the prelude's ref covered, spans whose blob this
  repository does not hold named on one line with their full refs and the fetch refspec of their
  namespace, a missing ref of another store in the same repository not reported, a store whose refs
  all match printing nothing and a ref restored by either printed command silent again, through the
  binary in `tests/cli.rs`; and the advisory, driven over doubles of both ports in the tests of
  `src/cli/validate.rs`, starting no git process with no archive event recorded and calling
  `RunArchive::list` once and `RunArchive::holds` at most once whatever the span count. This
  criterion OWNS the advisory function and its wiring in `cmd_validate`, its not-held line,
  `RunArchive::holds` and the passages DOCUMENT EDITS gives it; naming a lost span at read time is
  criterion 3's and the index-lag and bloat advisories are spec 107's, NOT this one's.
- [ ] every lane green: fmt, and clippy and tests on the default lane and on the `no-default` and `core` lanes `.rigger/gates/lanes.sh` runs. This criterion OWNS only the lanes over the integrated result.
