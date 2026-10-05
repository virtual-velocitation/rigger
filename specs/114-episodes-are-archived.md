# 114 - Episodes are archived: a finished run's mechanics move to git

**Goal:** rigger is a hive mind: its graph holds the project's whole understanding and serves each
persona the slice it needs, and the append-only log is only the persistence underneath. Once spec
107 has made the log stop holding what the hive merely perceived, it still holds how every finished
run spent itself (its episodes). This spec moves a finished run's episodes to git, where they stay
readable on demand, so the live log keeps what the hive decided, learned and did and the current
run's mechanics. Measured on the 2026-10-05 store (288,636 events, 430 MB of payload, a 580 MB
file):

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
constant, `Store::reclaim_space` and the `--derived` migration
(`crates/rigger-store-sqlite/src/eventstore/sqlite.rs`), `FoldingStore`
(`crates/rigger-grounder/src/ingest.rs`) and `read_graph_index_lag`'s typed read
(`src/cli/validate.rs`); no criterion here has an edge into spec 107. Criterion 1 needs nothing.
Criteria 2 and 3 need 1. Criterion 4 needs 1, 2 and 3. Criteria 5 and 6 need 4, whose
`ArchiveOutcome` both read. Criterion 7 needs 5 and 6. Criterion 8 needs 7, and criterion 9 needs 8,
since 8 and 9 rewrite the `rigger-reset-store` skill in turn. Criterion 10 needs all nine. The spec
is launched on rigger-run once spec 107 has landed there. A test that fails because of a unit's
change is that unit's to move, whichever criterion owns the surface it asserts. A Design sentence
about a later criterion's behaviour describes the integrated result, and an earlier unit's tests
reach it only through fixtures. On a tree holding 1 and not 2, the `RunArchive` write issues
`create` alone, so a span whose ref exists fails with git's error and stays pending. A production
path archives only from criterion 7 on, after criterion 3 moved `rigger replay` and `rigger stats
--all` onto `read_history`, so no tree archives a span its readers cannot read.

**THE INVARIANT, amended here so no unit has to.** Section 2.1 of
`docs/architecture-addendum-context-management.md`, as spec 107 left it, gains: git retains a
finished run's EPISODES; and its sentence on `reset --runs` now says the verb archives earlier spans
to git before it prunes the graph. Criterion 1's unit edits those passages. `RunArchived` is
KNOWLEDGE: criterion 1 adds its `TYPE_` constant to `retention::KNOWLEDGE_TYPES`, which spec 107's
source scan then requires.

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
the prelude's 2,929 rows (63 MB) and 0.07 s for the largest run's 2,019 rows (12 MB), so an appender
that arrives meanwhile, a worker's `rigger result` included, waits inside the 5000 ms busy timeout
and lands. A SPAN DESCRIPTOR (the ref, the group, the `run` field) names each span, and
`archive_run` and both readers are span-kind-agnostic over it (criteria 1 and 3). THE DESCRIPTOR
LIST, criterion 1's, names the prelude and every run in position order: the readers enumerate it,
and `archive_pending` archives every pending span of it, the prelude first. Within a store a span is
identified by the position of its `RunStarted`, which the log guarantees unique, never by its run
id, which it does not; refs are repository-wide, so a run's ref is
`refs/rigger/archive/<store>/run/<position>`, at the fixed width Notes gives so the refs list in run
order, and the prelude's `refs/rigger/archive/<store>/prelude`, where `<store>` is the event id of
the store's first `RunStarted`, a knowledge event this spec never deletes, so a recreated store or a
second store in one repository gets its own namespace. A descriptor is built only from a lowercase
UUID, as `Event::new` mints for every `RunStarted` (the log stores an id as handed, and another can
carry `/` or `..` and name a ref outside the namespace); any other id yields none, which
`archive_pending` returns as a permanent skip naming it. The group stays keyed by position alone
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
stream's running revision maximum). Two whole-stream reads change: `cmd_replay` and `stats_lines
--all` read through `read_history` (criterion 3); `read_graph_index_lag` already reads by type (spec
107). A console tab meets nothing either: its provider holds no earlier span's row. `RunArchive` is
declared in the domain beside `EventStore`; its one adapter, in `crates/rigger-worktree-git`, runs
`git hash-object -w --stdin`, `git update-ref --stdin` and `git cat-file blob`. Git compresses the
blob, so no compression crate is added. Git is the retention system: the ref is local until the
operator pushes it, and no blob rigger wrote ever loses its last ref by rigger's hand. The rebuild
(spec 107's `rebuild_owed_graph`) reads no archive, since an episodic event changes neither the live
projection nor the fold state (spec 107's fold test), the `applied` rows of archived positions are
outside spec 101's comparison surface, and an archive delete landing between the rebuild's position
read (`read_live_positions`) and its selection read (`read_live_selection`) removes only positions
whose fold changes no fact, so the rebuild needs no lock beyond `graph.db.lock`. BACKEND SCOPE: the
sqlite store deletes; the KurrentDB adapter answers each of the three archive port methods
(`delete_archived` and the two reads below) with an unsupported-operation error, and `read_history`
calls none of them, so on KurrentDB `rigger replay` and `rigger stats --all` answer as today
(criterion 3). Every `EventStore` implementation (both adapters, `Namespaced`, `FoldingStore`, the
test doubles) gains the three methods in criterion 1's unit.

**FINDING A PENDING RUN READS THE INDEX, NOT THE BACKLOG.** A span is a range of log POSITIONS: from
its `RunStarted`'s position up to the next `RunStarted`'s (the prelude: from the stream's start up
to the first `RunStarted`'s), whatever revision a row carries. Two new port reads, both criterion
1's and both anchored on position exactly as `read_stream_typed` is:
`EventStore::first_of_types(stream, from, types)` answers the position of the oldest event at or
after log position `from` whose type is named, never its data (on sqlite one seek of
`idx_events_stream_type` per named type), and `EventStore::read_span_typed(stream, from, to,
selection)` reads the events at log positions from `from` below `to` that `selection` admits, in
position order. `archive_pending` takes the current run's boundary from `last_position(RunStarted)`
(none: nothing is pending), anchored on the position of the event at that revision as
`read_stream_typed` anchors `from`, and the first run's from `first_of_types(0, [RunStarted])`, then
the oldest episodic event from position 0: none, or one at or past the current boundary's position,
means nothing is pending; one below the first boundary makes the prelude pending; otherwise it steps
`first_of_types(.., [RunStarted])` forward to the run whose span holds that event. So a row is
archived only when it lies below the current boundary in position order: a current-run row a stale
writer reissued at a low revision (`Error::OutOfOrder`) lies above the boundary and in no earlier
span. Only when something is pending does it read the first `RunStarted`'s id for the ref's store
component. It reads each span alone through `read_span_typed` with `TypeSelection::Except` of the
derived types, archives it or names why not, and asks again from the span's upper boundary. A
trigger with nothing pending costs three port reads, each index-only, and a backlog is held one span
at a time.

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
position above every one it has used, `AUTOINCREMENT`, and spec 107's migration rewrites rows in
place into knowledge), so a span's episodic rows are all live or all gone. The prelude resumes as a
run does.

**THE ARCHIVE IS READ ON DEMAND.** `archive::read_archived(ref, blob)` resolves the ref, checks that
it names `blob`, parses the rows and yields the events in position order; a missing ref or a
different blob is an error naming the ref. Criterion 1 owns it: `archive_run`'s read-back hands it
the ref and blob it is about to record, `read_history` those the span's latest `RunArchived`
records. One domain function, `archive::read_history` (criterion 3's), over `&dyn EventStore` and
`&dyn RunArchive`, serves both readers the spans of the descriptor list they name. Its one read is
the forward `read_stream` from revision 0 both commands make today (`cmd_replay` through
`Namespaced::read_stream`, `stats_lines` through `read_project_stream`). A span's `RunArchived` is
appended above every row of the span, so that read reaches a span's live rows before its
`RunArchived`, the log-prefix argument `read_run` makes, and an archive whose delete lands during
the read appended its `RunArchived` before that delete, so the read meets the span's rows, its
`RunArchived` or both. It is TOTAL over a span's three states: a span holding an episodic row in
that read is served from its live rows and its ref is not resolved; a span with none and a
`RunArchived` is served from `read_archived` of its latest one, its archived events interleaved by
position with its live knowledge rows; a span with none and no `RunArchived` (a run of knowledge
only, a current run just started, a prelude with no episodes) is served from its live rows alone,
resolving no ref and printing no missing-ref line or refusal. No span is read from both sides, and a
pending span (interrupted after its `RunArchived`, or whose ref was deleted by hand) reads complete
with no missing-ref error. `rigger replay <run>` (`cmd_replay`, `src/cli/mod.rs`) resolves the run
id to its `RunStarted`, the first match as `baseline_run_slice` takes it today, reads that span
through `read_history` before it slices the baseline, and refuses naming the ref when it is missing,
on standard error with a failing exit; a later `RunStarted` sharing that run id is not addressed by
id, and no run id names the prelude, as today. `rigger stats --all` (`stats_lines`) reads every span
of the descriptor list through `read_history` and prints one line naming each missing ref on
standard error, so its standard output keeps its bytes. Neither command folds a `RunArchived` or a
`GenerationIngested`: `metrics::project` (`crates/rigger-domain/src/metrics.rs`), which both fold,
ignores every type it does not name, and `replay_trajectory` keeps only `SpawnResult` and
`GateVerdict`, so the `RunArchived` an archive appends to the current run changes no line of their
output. No step, one-shot command or rebuild reads an archive.

**ARCHIVING RUNS AT TWO TRIGGERS.** One function, `archive_pending`
(`crates/rigger-driver/src/archiving.rs`), finds the pending spans as above and hands each to
`archive::archive_run` in position order. It takes its store only wired through `FoldingStore`, by
its parameter type, so an unfolded `RunArchived` cannot be written: each one writes its `applied`
row, and the graph prune `reset --runs` runs after it meets no hole (`Projector::prune` refuses a
graph that owes its rebuild). It returns an `ArchiveOutcome` (criterion 4's, every variant), which
is DATA: the totals, each span named (a failed write or read-back with git's error, a blob moved
aside), a permanent skip (a store id no descriptor is built from, a store that cannot delete, a
project outside a git repository) or the transient skip of a held lock. WHETHER THIS PROJECT CAN
ARCHIVE is one decision function in `src/cli/run.rs`, criterion 5's, which yields nothing but the
two PERMANENT skips: a store that cannot delete (any but sqlite), where `delete_archived` would fail
at every trigger, and a project outside a git repository, where `git hash-object -w` would. ONE
render function beside it, criterion 6's, turns an outcome into lines in two cadences: the DRIVER
cadence prints one line per named span and nothing else (a permanent skip is the project's property,
and the next trigger retries a held lock), and every driver writes it to STANDARD ERROR through the
`stderr_line` its `Deps::log` already carries (`cmd_step`, `run_cli` and `run_workflow` in
`src/cli/run.rs`), never to standard output, which for `rigger step` is the JSON its courier parses;
the RESET cadence prints every outcome, each skip once, and the totals, on standard output as
`reset_runs` prints today; a skip is reported only when there was something to skip. Every caller
reaches the archive through one wiring helper, `run_archiving(held)` in `src/cli/run.rs`, criterion
7's whole: it calls the decision, pairs `archive_pending` with the git adapter and applies the LOCK
RULE: handed the step lock its caller holds, it archives; handed none, it takes the lock without
waiting around the archive alone, and returns the transient skip when another process holds it. The
step lock alone serializes archivers; without it two could each find no `RunArchived` for a span and
each append one. The conductor's trigger: at the start of every `conductor::run`, once
`run_store::ensure_started` or the driver's own pinned or fresh mint has fixed the current run, it
calls the archiving handle `Deps` carries and hands the outcome back; unwired, it archives nothing.
`rigger step` (`cmd_step`) wires the helper with the step lock it holds for the whole step; `rigger
run` (`run_cli`), `rigger serve` and `rigger workflow` (both `run_workflow`) wire it with no lock,
each printing the outcome in the driver cadence; the replay's isolated re-drive and the canary, on
isolated stores, wire none. `rigger reset --runs` (`reset_runs`) runs, in order: its removal of a
stale pruned copy and `open_graph`, which refuses a `graph.db` that owes its rebuild, as today, so
an owed graph archives nothing; its probe (`live_writer_facts` over the current run's slice), which
takes the step lock and keeps it to the end; the helper, handed that lock, its outcome printed in
the reset cadence; its ONE whole-stream closure read, as `reset_runs` documents it, now after the
archive, when the stream no longer holds the backlog; `close_landed_units` over the current run's
slice; the graph prune and the graph file's compaction as today; then `Store::reclaim_space` (spec
107), only while the probe still holds the lock and its live-writer facts are dead, since an
appender that outlasts the `VACUUM`'s busy timeout fails and does not retry, which would lose a live
run's `rigger result`, else one line saying reclamation was skipped and why, the archive and its
delete standing. It prints one line with the bytes reclaimed; on a store that is not sqlite the
archive is a permanent skip and the reclamation prints its own skip line. Measured on a copy of this
store after the migration's and the archive's deletes, the reclamation took 0.44 s, 580 MB to 92 MB,
with 94 MB more peak memory. The conductor trigger never reclaims: a later append reuses the freed
pages. A driver that dies holding the lock releases it with its process, and the next trigger
completes the interrupted span. The first trigger on an older store pays the whole backlog once,
about 9 s for this store, so no budget knob is added; the documented pre-run `rigger reset --runs`
normally pays it. The `--runs` text says the same: its usage text in `src/main.rs`, the flag list
`reset_modes` prints, the `rigger-reset-store` skill and the `--runs` guidance in
`crates/rigger-domain/src/docs.rs` describe the archive, its reclamation and skip and the lock rule.
The bare `rigger reset` menu's `--runs` line keeps previewing the graph prune alone; archiving is
not previewed.

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
the span count. Its lines go to standard error, beside validate's other advisories. A span whose
blob git holds prints one advisory line: the span (its run id and position, or `prelude`), the full
ref, the recorded blob id and the restore command: `git update-ref <ref> <blob>` for a missing ref,
and for a ref naming another blob `<old>` the two instructions the `RunArchive` write issues,
`printf 'update refs/rigger/aside/<old> <old>\nupdate <ref> <blob> <old>\n' | git update-ref
--stdin`. Obeying either reaches a ref naming the recorded blob, which is silent, and leaves no blob
unreachable; a ref moved meanwhile makes git refuse and write nothing. The spans whose blob git no
longer holds are named together on ONE line, their count, positions and full refs, as lost for good,
with no command, a line accepted to repeat since the log still records archives nothing can restore.
The advisory never fails validate; criterion 9 adds both methods to the port and its git adapter. No
`RunArchived`, a project outside a git repository and a KurrentDB store print nothing; the advisory
writes and carries nothing, so a repeated validate prints the same lines. The `rigger-reset-store`
skill names the advisory and its lines.

**CONSTRAINTS WALK, decided.**
- *Cold start:* nothing is carried in memory between processes; pending spans are found from the
  type index.
- *Existing data:* a store that predates this spec holds every earlier span live; its first trigger
  pays the backlog once (ARCHIVING RUNS AT TWO TRIGGERS), and a store spec 107 has not migrated
  archives its spans all the same, since a span's read excludes the derived types.
- *Concurrent:* two archivers are serialized by the step lock, and an appender waits inside the busy
  timeout while a delete holds the write lock.
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
deleted `graph.db` (forbidden by `crates/rigger-domain/src/docs.rs`, unchanged here) and everything
of spec 108.

## Notes (non-criteria)

`RunArchived { run, ref, blob, events, bytes, first, last }`: `run` is the run id, or empty for the
prelude, which no run owns; `ref` is `refs/rigger/archive/<store>/run/<position>` or
`refs/rigger/archive/<store>/prelude`, where `<store>` is the event id of the store's first
`RunStarted` and `<position>` is the global position (`Position`, a `u64`) of the span's
`RunStarted`, written in decimal zero-padded to 20 digits, the width of the largest `u64`; `events`
and `bytes` count what the blob holds; `first` and `last` are the lowest and highest archived
positions. It carries `META_GROUP` `archive/run/<position>` or `archive/prelude` and the replay key
`<group>@<blob>#<events>`, the key form of spec 107's ledger entry.

The archive blob holds one JSON object per archived row, in position order, keys in this order:
`position`, `stream`, `type`, `id`, `data`, `meta`, `valid_from`, `recorded_at`, `revision`. `data`
is the row's bytes as a JSON string when they are UTF-8, else lowercase hex under `data_hex`; `meta`
is its JSON object; times are integer nanoseconds since the epoch.

The episodic types are spec 107's `retention::EPISODIC_TYPES`, unchanged here.

`RunArchived` is the spec's one new event type; its whole point is where a finished run's episodes
live.

## Global constraints

- Hyphens, never em dashes, in every added line.
- One new event type and no others; no new dependency.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).
- No EPISODIC event leaves the live store before its bytes are read back from git, on a first pass
  and on a resume alike.
- Every archive is deterministic: the same rows yield the same bytes.

## Done when

- [ ] a test proves A SPAN'S EPISODES ARE ARCHIVED: archiving a run's span writes its episodic events under `refs/rigger/archive/<store>/run/<position>`, records one `RunArchived` and deletes exactly those events, its knowledge events staying,
  asserted against `archive::archive_run` over a sqlite store and a fixture git repository, with the
  prelude's span archived the same way under `refs/rigger/archive/<store>/prelude` with an empty
  `run`, the descriptor list naming the prelude and every run in position order, a span holding no
  episodic event writing no ref and no event, a `RunArchive` double failing its write deleting
  nothing, a second store in the same repository archiving the same positions under its own
  namespace, and a first `RunStarted` id that is not a lowercase UUID yielding no span descriptor.
  This criterion OWNS `RunArchived` with its constant, its `KNOWLEDGE_TYPES` row, its shape, group
  and replay key, the addendum amendment, the span-kind-agnostic use case, the span descriptor and
  the descriptor list with their store component and its validation, `read_archived`, the
  `RunArchive` port and its git adapter's `create` write, `EventStore::delete_archived`,
  `first_of_types` and `read_span_typed`, both anchored on position; the write to an existing ref
  and the resume are criterion 2's and every trigger criteria 4, 7 and 8's, NOT this one's.
- [ ] a test proves AN INTERRUPTED ARCHIVE COMPLETES: an archive stopped after its ref was written, and one stopped after its `RunArchived` was recorded, are each completed by the next archive of the span after a read-back of the blob,
  with no second ref, no second `RunArchived` and every episodic event of the span gone, a span
  whose ref names another blob archived after that blob is kept under `refs/rigger/aside/<blob>`
  with its result naming the ref, both blobs and the aside ref and an aside ref already naming that
  blob left as it is, asserted against `archive::archive_run` with a store double that fails on
  command and a fixture git repository. This criterion OWNS the resume and the write's two
  existing-ref states; the first pass is criterion 1's and every line criterion 6's, NOT this one's.
- [ ] a test proves THE ARCHIVE READS BACK: `rigger replay <run>` and `rigger stats --all` print, over a store whose earlier spans are archived, the bytes of standard output they print over that store before archiving,
  with the `RunArchived` standing in the current run, the prelude archived, a span archived and not
  yet deleted printing those bytes too, a run of knowledge only and a current run just started
  served from their live rows with no ref resolved and no missing-ref line, and each naming on
  standard error the missing ref of a span whose rows are gone, asserted in `tests/cli.rs`; and
  `read_history` serving once, from its live rows, a span a store double archives mid-read after
  returning its rows, a pending span whose ref was deleted with no missing-ref error, and no archive
  port method called over a store double that answers each with the unsupported-operation error,
  asserted in `crates/rigger-domain/src/archive.rs`. This criterion OWNS `read_history`, its read,
  its three states and both commands' calls of it; `read_archived` and the descriptor list are
  criterion 1's and validate's archive-ref advisory criterion 9's, NOT this one's.
- [ ] a test proves A CONDUCTOR RUN ARCHIVES ITS PREDECESSORS: a `conductor::run` wired with an archiving handle archives every pending earlier span of the run stream, the prelude first, and leaves the current run's events live,
  asserted through `conductor::run` over a fixture repository, with each `RunArchived` holding its
  `applied` row in `graph.db`, the knowledge below the first `RunStarted` staying, an unwired
  conductor archiving nothing, a store with no `RunStarted` archiving nothing, a run and a prelude
  interrupted after their `RunArchived` completed, a current-run row at a reissued low revision left
  live, a span with no descriptor returned as a permanent skip naming the id, and a trigger with
  nothing pending issuing three port reads, each index-only, counted on a recording store; each case
  is asserted on the outcome as data. This criterion OWNS `archive_pending` and its
  `FoldingStore`-only store parameter, `ArchiveOutcome` with every variant, the `Deps` handle and
  the conductor trigger; the descriptor list and its validation are criterion 1's, the decision
  criterion 5's, the render criterion 6's, `run_archiving` and the drivers' wiring criterion 7's and
  the reset trigger criterion 8's, NOT this one's.
- [ ] a test proves A PROJECT THAT CANNOT ARCHIVE IS SKIPPED: the decision answers the permanent skip naming why for a store that cannot delete and for a project outside a git repository, and nothing for a sqlite store in a git repository,
  asserted in the tests of `src/cli/run.rs` over a store double and fixture directories. This
  criterion OWNS the decision function, which yields nothing but those two permanent-skip variants;
  the variants are criterion 4's, the render criterion 6's and `run_archiving`, which calls the
  decision, criterion 7's, NOT this one's.
- [ ] a test proves EVERY OUTCOME RENDERS ONCE PER CADENCE: the render turns an `ArchiveOutcome` into the driver cadence's lines, one per named span and nothing else, and the reset cadence's, every outcome, each skip once and the totals,
  asserted in the tests of `src/cli/run.rs` over hand-built outcomes only, with a failed write and a
  blob moved aside each named once in both cadences, no driver line for a permanent skip, a held
  lock or nothing pending, and each permanent-skip variant printed once in the reset cadence. This
  criterion OWNS the render, its two cadences and every line's text; the outcome is criterion 4's,
  the decision criterion 5's and the stream each caller writes to criteria 7 and 8's, NOT this
  one's.
- [ ] a test proves EVERY DRIVER ARCHIVES UNDER THE STEP LOCK: `run_archiving` handed no lock archives a pending span when the step lock is free and releases it, and returns the transient skip when another process holds it,
  asserted in the tests of `src/cli/run.rs` and through `rigger step` in `tests/cli.rs`, with a
  project that cannot archive archiving nothing, `cmd_step` wiring `Deps` through that helper with
  the lock it holds and `run_cli` and `run_workflow` with none, and a span whose ref names another
  blob named once on standard error while `rigger step`'s standard output stays the JSON its courier
  parses. This criterion OWNS `run_archiving` whole (the decision's call, the lock rule, the
  `archive_pending` call), the three drivers' wiring and the driver cadence's stream; the decision
  is criterion 5's, the render criterion 6's, the conductor trigger criterion 4's and the reset
  trigger criterion 8's, NOT this one's.
- [ ] a test proves `rigger reset --runs` ARCHIVES: on a store whose earlier runs were never archived, it archives each pending span before its whole-stream closure read and prints the archived totals,
  with a `graph.db` that owes its rebuild refused before the probe and archiving nothing, the graph
  prune meeting no hole, the file smaller on disk and the bytes reclaimed printed, reclamation
  skipped with a line saying why while a live writer holds, and a project outside a git repository
  named once, every line on standard output, asserted in `tests/cli.rs` as the reset cadence's
  caller, and a store that is not sqlite printing the archive's permanent skip and the reclamation's
  skip line, asserted over a store double in the tests of `src/cli/hygiene.rs`. This criterion OWNS
  the reset trigger, its order with the owed-rebuild refusal first, its reclamation call and lines,
  the reset cadence's stream and the `--runs` text; the render is criterion 6's, `run_archiving`
  criterion 7's, `Store::reclaim_space` spec 107's and the skill's archive-ref advisory text
  criterion 9's, NOT this one's.
- [ ] a test proves VALIDATE NAMES A LOST ARCHIVE REF: `rigger validate` on a store with an archived run whose ref was deleted prints one advisory naming the run, the ref, the recorded blob and `git update-ref`, and still exits 0,
  asserted in `tests/cli.rs` on standard error, with a ref naming another blob printed with the
  aside transaction, a pending span whose ref was deleted not reported, the prelude's ref covered,
  spans whose blob git no longer holds named on one lost-for-good line with their full refs and no
  command, a missing ref of another store in the same repository not reported, a store whose refs
  all match printing nothing and a ref restored by either printed command silent again; and the
  advisory calling `RunArchive::list` once and `RunArchive::holds` at most once whatever the span
  count, counted on a double of the archive port in the tests of `src/cli/validate.rs`. This
  criterion OWNS the advisory, its lost-for-good line, `RunArchive::list`, `RunArchive::holds` and
  the skill's text on them; naming a missing ref at read time is criterion 3's and the index-lag and
  bloat advisories are spec 107's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features). This criterion OWNS only the lanes over the integrated result.
