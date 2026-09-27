# 107 - Perception is re-derived and episodes are archived: the log keeps knowledge

**Goal:** rigger is a hive mind: its graph holds the project's whole understanding and serves
each persona the slice it needs, and the append-only log is only the persistence underneath.
That log keeps what the hive decided, learned and did; it stops holding what the hive merely
perceived or how it spent a run. Measured on the 2026-09-26 store (2,497,006
events, 609 MB payload, 1.4 GB file): 2,443,614 events (97.9%) are the derived index -
`EdgeInferred` 2,101,788, `DocLinkExtracted` 206,363, `CodeEntityExtracted` 133,591,
`DocConceptExtracted` 1,872 - re-derivable from the tree and already folded into `graph.db` at emit time
(`ingest::append_and_fold_batch`, `src/ingest.rs:46`; `RunCtx::emit_keyed_batch`,
`src/conductor.rs:3261`), so the log's copy has no production reader: `graph.db` is a persisted
incremental projection, no command rebuilds it from the log (only tests do,
`src/contextgraph/sqlite.rs:7177`), and `src/docs.rs:728` forbids deleting it. They are
appended on every step (`conductor.rs:10557`), on every landed merge (`conductor.rs:9504`) and
by `rigger graph build` (`src/main.rs:4737`); spec 60's guard against re-accumulation is a dedup
inside the two ingest sinks (`project_scoped_replay_keys`, `src/ingest.rs:518`), and the store
itself accepts any derived append. Spec 101 stops READING them;
nothing stops WRITING them. 35,334 events are a run's own
mechanics and carry 333 MB, 55% of every byte: `SpawnRequested` alone is 6,729 events and
302 MB of prompts (`SpawnRequest::prompt`, `crates/rigger-domain/src/spawn.rs:290`, parked whole by
`spawn_store::park_in_run`, `src/spawn_store.rs:31`). The knowledge the hive keeps across runs - `DecisionMade`, `LessonLearned`,
`ReviewFinding` - is 17,424 events and 23 MB. 149 runs are recorded; the current one spans
90,008 events and 12 MB. Every courier replays the whole file: `rigger status` peaks at 3.2 GB
resident in 4.6 s, `rigger step` at 8.3 GB, and on 2026-09-24 15:13 the kernel's
out-of-memory killer chose that step as the largest process on the machine. `rigger reset
--runs` (`src/main.rs:9154`) prunes the GRAPH of dead runs (`superseded_graph_nodes`,
`main.rs:9231`) and leaves the log untouched, so no run's mechanics have ever left the live
store; `read_stream` (`src/eventstore/sqlite.rs:897`) materializes the whole stream on every
call and nearly every caller passes position 0.

## Design

**THE INVARIANT, amended here so no unit has to.** Context-management addendum section 2.1
reads "the event log is the source of truth; the graph is a rebuildable projection". It now
reads: the log is the source of truth for KNOWLEDGE and ACTS; the tree is the source of truth
for STRUCTURE; the graph is rebuildable from the two together. The addendum is edited in the
same unit that lands criterion 1 and says why in one paragraph.

**THREE CLASSES OF EVENT, decided by type.** KNOWLEDGE is the typed carry-over set spec 101
reads across runs (`DecisionMade`, `LessonLearned`, `ReviewFinding`, `ReviewVerdict`,
`GatePromoted`, `GateDemoted`) plus `RunStarted`. DERIVED is `ingest::DERIVED_INDEX_TYPES`
plus the offline pass results `CommunityAssigned`, `ConceptDerived`, `ConceptRealized`.
EPISODIC is every other type. The three sets are code-owned constants beside
`DERIVED_INDEX_TYPES` and are exhaustive: a type in none of them fails the store's append
seam loudly, so a future event type is classified the day it is added.

**PERCEPTION IS A LEDGER ENTRY, NOT A PAYLOAD.** The two fold-at-emit seams,
`ingest::append_and_fold_batch` (`src/ingest.rs:46`) and `RunCtx::emit_keyed_batch`
(`src/conductor.rs:3261`), append ONE event per file generation,
`GenerationIngested { prefix, file, blob, extractor }`, where `blob` is the git blob id of the
content ingested and `extractor` the extraction version, and fold the extracted batch
(`symbols::events::extract_events`, `src/grounder/symbols/events.rs:171`; the design and
workflow extractors) into `graph.db` through the same `apply`, keyed in the `applied` table by
the ledger event's position and carrying the generation's replay key
`<prefix>/<file>@<blob>#<i>` in each edge's `source`. The cold-build sink (`cmd_graph_build`,
`src/main.rs:4670`) goes through the same seam. A
deleted file appends `GenerationIngested { blob: "" }` and supersedes its live structural
edges exactly as a new generation does. The offline passes (`cmd_graph_communities`, `src/main.rs:4774`, whose only event writer is
`community::events`, `src/community.rs:363`; `cmd_graph_concepts`, `main.rs:4872`) append one
`PassRecorded { pass, input_hash, resolution }` and fold their membership edges the same way. No derived payload is appended to the log by any path; the
store's append seam rejects a `DERIVED_INDEX_TYPES` append outright.

**REBUILD READS THE LEDGER AND THE TREE.** `rigger graph rebuild` is new: today `rigger graph
build` (`src/main.rs:4717`) walks the tree and appends, and a log-to-graph rebuild exists only
in tests. `graph.db` rebuilt from an empty file replays the
knowledge and episodic events through `apply`, and for each ledger entry re-extracts the blob
by id (`git cat-file blob`) and applies the result; for each pass record it re-runs the pass.
Determinism is the extractor's and the passes' existing property. Rebuilt equals incremental,
byte for byte, for every generation whose blob is reachable from the repository; a generation
whose blob was never committed and is no longer reachable rebuilds as a ledger entry with no
edges, and the rebuild names how many such generations it met. Live structure is never in
that set: the working tree is re-ingested on the next step and records a reachable blob or a
fresh generation.

**MIGRATION SHEDS THE BACKLOG.** `rigger reset --derived` on a store that still carries
derived events first records a `GenerationIngested` for every file's latest generation (the
`project_scoped_latest_generations` query spec 101 criterion 3 makes cheap), verifies that
`graph.db` holds that generation, then sheds every derived event from the log in one
transaction and reports the count. Spec 101 criterion 4's generation-compaction is the same
code path with a wider shed set; after migration `--derived` is a no-op that says so.

**A FINISHED RUN'S EPISODES MOVE TO GIT.** A run is FINISHED when it is not the current run,
or every unit of the current run is terminal; it is ARCHIVABLE when it is finished and not
live by spec 101's liveness rule (no held step lock, no spawn marker younger than the wall
clock bound, no registry heartbeat younger than the idle window). Archiving writes the run's
EPISODIC events, in position order, as one zstd-compressed JSONL blob under the ref
`refs/rigger/archive/<run-id>` in the project repository, appends
`RunArchived { run_id, ref, events, bytes, digest }` to the log, then deletes exactly those
events from the live store in one transaction. Knowledge events and `RunStarted` stay live.
Archiving runs in two places: `rigger reset --runs` (`reset_runs`, `src/main.rs:9154`, which
keeps its graph prune and gains spec 101's liveness guard - it has none today) and the
conductor at `RunStarted`, before the new run's first ingest, for every archivable predecessor.
`BlastRadiusComputed` (`conductor.rs:10484`) and `FileTouched` (`conductor.rs:9475`) are
episodic and travel with the run.
Git is the retention system: the ref is local until the operator pushes it, and nothing rigger
does deletes a ref.

**THE ARCHIVE IS PART OF THE LOG.** One-shot commands never read an archive (spec 101 already
bounds them to the run and the typed carry-over). `rigger replay`, `rigger stats` and a full
`graph.db` rebuild read archives on demand through one store-port method, `read_archived(run)`,
which resolves the ref named by the `RunArchived` event, verifies the digest, and yields the
events in position order; a missing ref is named, never skipped silently.

**CONSTRAINTS WALK, decided.** Crash between the ref write and the delete: the next archive
finds the ref, verifies the digest, appends nothing new and completes the delete. Repeat: a
run with a `RunArchived` and no live episodic events is a no-op. Empty: a finished run with no
episodic events writes no ref and no event. Concurrent: archiving holds the step lock and
refuses while a run is live, naming what is live. Cold start: a fresh clone has no archives,
no `graph.db` and a log of knowledge and ledger entries; the first step rebuilds the graph
from log and tree. Revert: an operator who deletes an archive ref loses that run's
mechanics and nothing else; `rigger validate` reports a `RunArchived` whose ref is missing.

**STATE PLACEMENT.** The ledger and the archive index are the log (`GenerationIngested`,
`PassRecorded`, `RunArchived`). `graph.db` is a projection. A sidecar file listing archived
runs, an in-memory set of ingested generations, or a `.rigger/` marker is NOT an
implementation of either.

## Notes (non-criteria)

`GenerationIngested`, `PassRecorded` and `RunArchived` are the spec's three new event types;
its whole point is the shape of the log. The archive blob is JSONL of the stored event rows
(position, stream, type, id, data, meta, valid_from, recorded_at, revision), compressed with
zstd; the digest is FNV-1a/64 over the uncompressed bytes, the crate's one stable-hash idiom.
Spec 101 lands first: its boundary and typed reads are what make the live store small enough
to matter, and its `--derived` compaction is the migration's code path. `progress.db`
(`AgentProgress`, `SpawnLaunched`) is its own file with its own lifecycle and is out of scope.

## Global constraints

- Hyphens, never em dashes, in every added line.
- Three new event types and no others; no new dependency except a zstd crate if the tree does
  not already carry one.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).
- Nothing is deleted from the live store before its bytes are verified readable from git.
- Every archive and rebuild is deterministic: same inputs, byte-identical outputs.

## Done when

- [ ] a test proves PERCEPTION IS A LEDGER ENTRY: ingesting a file appends exactly one
  `GenerationIngested` naming its blob id and zero `DERIVED_INDEX_TYPES` events, while
  `graph.db` holds the file's entities and edges under the generation's replay key, and a
  deleted file appends a blank-blob entry that supersedes its live edges, pinned at the ingest
  emit seam with a counting store double. This criterion OWNS the ingest write path; the
  offline passes are criterion 2's and the rebuild is criterion 3's, NOT this one's.
- [ ] a test proves THE PASSES ARE LEDGERED: the community and concept passes each append one
  `PassRecorded` and write their membership edges to `graph.db` directly, appending no
  `CommunityAssigned`, `ConceptDerived` or `ConceptRealized` event. This criterion OWNS the
  offline-pass write path.
- [ ] a test proves THE GRAPH REBUILDS FROM LOG AND TREE: a `graph.db` rebuilt from an empty
  file over a log of knowledge events and ledger entries, with blobs read from the repository,
  is byte-identical to the incrementally built one for every committed generation, and an
  unreachable superseded generation rebuilds as a ledger entry with no edges and is counted in
  the rebuild's report. This criterion OWNS rebuild identity.
- [ ] a test proves MIGRATION SHEDS THE BACKLOG: `rigger reset --derived` on a store carrying
  three generations of derived events for one file records one `GenerationIngested` for the
  latest, sheds every derived event, reports the count, leaves the live graph unchanged, and
  says it is a no-op when run again. This criterion OWNS the one-time shed; the guard that lets
  it run is spec 101's.
- [ ] a test proves A FINISHED RUN'S EPISODES ARE ARCHIVED: at `RunStarted`, and on
  `rigger reset --runs`, each archivable predecessor's episodic events are written under
  `refs/rigger/archive/<run-id>`, a `RunArchived` naming the ref, count, bytes and digest is
  appended, exactly those events leave the live store, and knowledge events and `RunStarted`
  remain, pinned by the store double showing the live store afterwards holds the current run
  plus knowledge only. This criterion OWNS what is archived and when; reading it back is
  criterion 6's and its failure modes are criterion 7's, NOT this one's.
- [ ] a test proves THE ARCHIVE READS BACK: `read_archived(run)` yields the archived events in
  position order after verifying the digest, `rigger replay` and a full rebuild consume them,
  and a missing ref is reported by name. This criterion OWNS the archive read path.
- [ ] a test proves THE ARCHIVE IS IDEMPOTENT AND CRASH-SAFE: a crash injected between the ref
  write and the live delete leaves a store the next archive completes without a second ref or
  event; an archived run is a no-op; an empty finished run writes nothing; a live run is
  refused by name. This criterion OWNS the constraints walk.
- [ ] a test proves THE STORE REFUSES AN UNCLASSIFIED OR DERIVED APPEND: `EventStore::append`
  (`src/eventstore/mod.rs:545`) rejects, naming the type, any event whose type is in none of
  the three classes or is one of `DERIVED_INDEX_TYPES`, pinned at that seam against the store's
  classification table, where an in-memory or caller-side check is NOT an implementation. This
  criterion OWNS the append-seam refusal; the ingest-sink write path and its dedup are
  criterion 1's, NOT this one's.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
