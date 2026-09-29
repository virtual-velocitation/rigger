# 101 - A one-shot command folds only the run it serves

**Goal:** every `rigger` invocation that serves one run reads that run, not the whole
history of the project. Measured on the 2026-09-15 store (2,075,706 events, 1.24 GB):
`rigger status` peaks at 2.7 GB resident, `rigger peers` at 5.3 GB (its sidecar replays from
position 0, `src/cli/observe.rs:531`), and one `rigger step` at 9.8 GB (the kernel's out-of-memory
report of that day) - while the run those commands served spans 67,456 events
(positions 3,146,193 to 3,213,649). 97% of the stream is derived graph ingest: 1,801,003
`EdgeInferred`, 125,861 `CodeEntityExtracted`, 101,354 `DocLinkExtracted`, and of the
edges ~1.62 million are superseded generations of files that were later re-ingested
(`gc/<file>@<hash>#<n>` keys: a file edit re-records every edge of the file under a new
generation). On the 2026-09-28 tree `read_stream(STREAM, 0, Direction::Forward)` sits at 104 sites in
`crates/rigger-conductor/src/conductor.rs`, 28 in `crates/rigger-driver/src/driver/replay.rs`, 16 in
`crates/rigger-store-sqlite/src/run_store.rs` and 14 in `src/cli/run.rs`, 206 across the workspace. Five agents each calling rigger a few times a
minute put 15 to 25 GB of baseline pressure on a 62 GB machine before a single cargo build.

## Design

**THE FOLD HAS A BOUNDARY, decided here so no unit has to.** The current run begins at the
stream position of its `RunStarted` event (the `runscope` boundary that
`runscope::current_run` (`crates/rigger-domain/src/run.rs:156`) already applies - after reading
everything). The boundary is a new `EventStore` port method (`crates/rigger-domain/src/eventstore.rs:499`),
`last_position(stream, event_type)`, implemented on the embedded sqlite store
(`crates/rigger-store-sqlite/src/eventstore/sqlite.rs:779`) as an indexed lookup and on the
server-backed KurrentDB store (`crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs:354`) as a
backward read that stops at the first match. No caller derives the boundary by scanning forward from 0.

**THREE READ CLASSES.** (i) The run's own events, from the boundary forward, are read and
folded whole - they are the run. (ii) The typed carry-over has two parts, both read BY TYPE over
the whole stream through a new `EventStore::read_stream_typed` port method backed by a type
index; `Filter` carries only a `stream_prefix` today, so no by-type read exists to reuse. (a) The
knowledge types: `LessonLearned`, `DecisionMade`, `ReviewFinding` and the playbook events the fold
consults across runs. (b) The criterion-adoption lifecycle types: `RunStarted`, `UnitStarted`,
`UnitIntegrated`, `UnitFailed` and `UnitStatus` of every run, read `Only` by type and ONLY by a
step that starts a criterion unit in a repo, because criterion adoption consults every prior
run's outcome by contract; a step that starts no criterion unit, and every other one-shot
command, performs no adoption read. Each part is one constant list declared once, side by side,
in the domain (no second spelling anywhere), read as `Only(list)`, never `Except(derived)` and
never a whole-stream read; each is bounded by its own count (the knowledge types about 22,000
events today), never by the derived types, and neither ever materializes a derived event. "The
typed carry-over" in criterion 2's cost bound means both parts: a repo step that does not ingest
costs exactly the run's events plus the knowledge types plus the adoption lifecycle types, and
nothing else. So the counting-double step test covers a stage with a repo and a criterion id and
asserts the adoption read through the double, and the binary poisoned-log step test runs a REPO
step against a log whose superseded runs' events outside both parts and every derived event are
undecodable, so a step that materializes one fails while the adoption read of the superseded
runs' lifecycle events passes. (iii) The derived ingest types (`ingest::DERIVED_INDEX_TYPES`, `crates/rigger-domain/src/ingest.rs:36`) are NEVER materialized
by a one-shot command: `graph.db` is their fold, and the only question a command asks of
them - a file's latest recorded generation (today `ingest::project_scoped_latest_generations`,
`crates/rigger-domain/src/ingest.rs:174`, over a whole-stream read) - is answered per identity by
the group lookup decided below (THE LATEST GENERATION IS A GROUP LOOKUP). An in-memory scan of
every derived event to find the latest key per file is NOT an implementation of this design.

**THE RUN SLICE EXCLUDES THE DERIVED TYPES, decided here.** Derived events appended during the
run (a `rigger step` that reindexes a changed file) sit after the boundary, so class (i) and
class (iii) meet there: the from-boundary read of class (i) excludes `ingest::DERIVED_INDEX_TYPES`
at the store, never a read-everything-then-drop. The exclusion is carried by the same port method
as class (ii): `read_stream_typed(stream, from, selection)` takes a `TypeSelection`, `Only(types)`
for the carried-over knowledge and `Except(types)` for the run slice, both answered by the one type
index. `Filter`, `read_stream` and `read_all` keep their current meaning. Criterion 2 OWNS this
exclusion as part of the read position of every one-shot command; criterion 3 OWNS only the
latest-generation lookup and relies on criterion 2's exclusion, never re-implementing it.

**SHARED INSTRUMENTS HAVE ONE OWNER, decided here.** The counting store double (it records how
many events each read materializes) is built ONCE by criterion 1's unit as shared test
infrastructure under `tests/common/` and reused by criteria 2 and 3, which add no second double
(each adds only the delegation of its own new port method to it). The port method
`EventStore::last_position` and its two adapters belong to criterion 1;
`EventStore::read_stream_typed`, `TypeSelection`, the type index on both backends and the
ingest-gated seed read of a step belong to criterion 2; `EventStore::latest_in_group`,
`eventstore::META_GROUP`, the group index on both backends, the keyed derived-event helper and
both ingest sinks' first-sight seeding belong to criterion 3. Every `EventStore` implementation
(adapter, the `Namespaced` wrapper, test double) gains a new port method in the unit that adds
it. Criteria 2 and 3 depend on criterion 1's double and criterion 3 depends on criterion 2, so
the units run in the order 1, 2, 3.

**THE STEP'S SEED IS PER WALKED IDENTITY, read from the code.** `conductor::run` seeds the
project-scoped half of `replayed_keys` and `replayed_generations` from a whole-stream read
(`crates/rigger-conductor/src/conductor.rs:1560` feeding `project_scoped_latest_generations`
at `:1601`). The only reader of that half is `RunCtx::emit_keyed_batch` (`:2953`), which weighs
one file's batch at a time, and its only callers are the two ingest paths: the whole-tree walk
`ingest_project_batches` (`:10230`, reached at most once per process through
`ingest_project_into_graph`'s guard, `:10214`) and the merge-scoped `ingest_files_into_graph`
(`:10308`). A step therefore needs the latest recorded generation of each identity its walk
emits (every file of the current tree, plus a merge's files on an integration). It never needs
an identity the walk no longer emits, and it cannot be handed a precomputed changed set, because
which files changed is exactly what the answer decides. A step that does not ingest (no graph to
fold into, no repo, the light lane: the condition `ingest_project_batches` checks at `:10231`)
weighs no batch and needs no seed.

**CRITERIA 2 AND 3 SPLIT AT THE INGEST, decided here.** Criterion 2 lands first. It moves every
fold read onto the boundary and the typed carry-over, and it takes the seed's whole-stream read
only in a step that ingests (the condition above), so a step that does not ingest reads no
derived event and criterion 2's assertion is passable while the seed still walks the stream.
Criterion 3 lands second: it replaces that remaining whole-stream read with the group lookup,
which is what makes a step that ingests cost the same reads as one that does not, and it OWNS
that step-wide assertion. Neither unit builds the other's half.

**THE LATEST GENERATION IS A GROUP LOOKUP, decided here so no unit has to.**
- *The stamp.* Every derived event carries, beside its `replay_key`, the metadata entry
  `eventstore::META_GROUP` (`group`) holding its batch identity `<prefix>/<file>`, cut by
  `ingest::derived_key_parts` (the one parser of the key). One `ingest` helper builds a keyed
  derived event with both entries, and both ingest sinks (the run's `emit_keyed_batch` and
  `rigger graph build`'s sink, `src/cli/graph.rs:537`) build their events through it. Metadata
  only: no new event type, and the fold ignores it.
- *The port.* `EventStore::latest_in_group(stream, group)` returns the position, type and
  metadata of the newest event on `stream` stamped with `group`, never its data, so the counting
  double counts it as zero events materialized. `ingest::latest_generation(store, stream,
  identity)` is the one domain reader: type first (a newest match outside `DERIVED_INDEX_TYPES`,
  or one whose key does not parse, answers no generation, the fail-safe direction that
  re-emits), then the generation cut from its `replay_key`.
- *The embedded sqlite store* answers from a partial expression index over the stream and the
  `group` entry of `meta` for the rows that carry one, created with the schema
  (`CREATE INDEX IF NOT EXISTS`); the lookup is one index seek to the highest position. The
  stamp lives in the event row, so it is atomic with the append.
- *The server-backed KurrentDB store* answers from one group stream per identity
  (`rigger-group/<stream>/<group>`) holding KurrentDB link events (`$>`, the server's own link
  type, not a rigger event type). Before an append whose events carry a group, the adapter reads
  the stream's last revision (a backward read of one event), appends to each group's stream a
  link naming the revision that group's first event will take, then appends the events expecting
  that revision; when the caller's expectation is `Any`, a conflict re-reads and re-links, and any
  other expectation's conflict is the caller's as today. The lookup reads the group stream
  backward and answers from the newest link whose resolved event carries that group; a link whose
  revision holds another group's event, or nothing, is skipped. Because the link is written before
  its events, every recorded batch has a link at its exact revision: a crash can leave a dangling
  link, never an unlinked recording, so the newest resolving link names the latest recording and
  a revert can never be suppressed against a stale answer. The adapter's `$all` reads and
  subscriptions skip records whose type begins with `$`, so no link reaches a caller. This is
  chosen over a backward read of the project stream per identity, which is unbounded: proving a
  never-recorded identity absent walks to position 0, and a file last ingested long ago walks
  nearly the whole stream. The KurrentDB half runs only where the contract suite's container is
  reachable, which the gates do not guarantee: the adjudicator demands that run's evidence.
- *The seeding.* Both sinks ask the lookup the first time they meet an identity in a process,
  through one `ingest` helper, before the conductor takes its dedup locks. When the answer
  equals the batch's generation, the sink installs that generation with the batch's keys (a key
  is a pure function of the batch's bytes, so they are the recorded keys) and the batch appends
  nothing; otherwise it seeds nothing and the batch appends. From then on the in-process
  `replayed_generations` governs that identity exactly as spec 86 decided, and
  `ingest_project_into_graph`'s once-per-process guard stays. The upfront whole-stream seed in
  `conductor::run` and the whole-stream read at `src/cli/graph.rs:535` are removed.
- *The upgrade.* Events recorded before the stamp carry no group, so on an existing store the
  first step that ingests finds no generation for any identity and re-emits the live index once
  (the latest generation of every file the walk emits), stamped; every later lookup answers. That
  one re-emission is the upgrade cost on both backends, and criterion 4's exact-key dedup reclaims
  the unstamped copies. No migration rewrites recorded events.
- *The reference.* `project_scoped_latest_generations` and `project_scoped_replay_keys` stay as the
  pure reference over a slice: `rigger validate` (a project-health command that already reads the
  whole stream for its other advisories, out of scope like the cross-run commands) keeps its
  index-lag sample on them, and the lookup's contract test asserts that the lookup answers what
  they answer on the same log, on both backends.

**THE CONSTRAINTS WALK OVER CRITERIA 2 AND 3.**
- *Empty store:* no group is recorded, so every identity the walk emits appends, exactly a first
  ingest today.
- *Repeated step:* an unchanged tree finds every lookup equal to its batch and appends nothing
  (the existing replay-idempotency tests stay green unchanged).
- *Revert:* a file reverted to an earlier generation finds the newer generation as its latest and
  re-emits; the next lookup then answers the reverted generation.
- *Revert and drop under compaction (criterion 4):* a revert to an earlier generation
  re-asserts that generation's facts as the newest generation, so retired facts return live under
  the newer valid-time and the compaction keeps only that newest recording. A generation that
  drops EVERY fact of a file leaves the file's node live only while a live decision, lesson or
  finding edge touches it. An existing `graph.db` folded before the rule is cold-rebuilt from the
  log once by `rigger setup`, and `--derived` refuses to compact it until then. A cross-file
  reference or test proof whose definition a generation drops is demoted, or returned to pending,
  as that definition retires, so a later definition of the name converges it identically in both
  rebuilds. A dropped entity touched only by a community or concept edge retires, with that edge,
  in both rebuilds. A pre-rule `graph.db` is rebuilt only by `rigger setup`, fold-dependent commands
  refuse until then and emits append without folding, and no concurrent open can undo the rebuild.
- *Concurrent step and status:* status reads the boundary and the typed carry-over and takes no
  step lock; the step's lookups read committed rows only. A repo step that adopts a prior
  criterion branch reads the adoption lifecycle types by type and nothing else cross-run. A `rigger graph build` running beside a
  step can record one generation twice, as it can today, and criterion 4's dedup collapses it.
- *Crash-resume:* on sqlite the stamp commits with its event; on KurrentDB a crash leaves at most a
  dangling link, which the lookup skips. A step that crashed mid-walk leaves the files it appended
  recorded, and the next step's lookups answer them.
- *Cold start:* nothing is carried in memory between processes; every process asks the store.

**COMPACTION SHEDS SUPERSEDED GENERATIONS.** `rigger reset --derived` today keeps the latest
recording per exact replay key (13 duplicates on this store) and leaves every superseded
generation in place. It keeps, per `<prefix>/<file>` identity, only the recordings of the
LATEST generation, carrying the earliest valid-time onto a kept recording exactly as the
reasserting-types rule already does. Correctness is rebuild-identical: `graph.db` rebuilt
from the compacted log equals `graph.db` rebuilt from the full log, byte for byte in the live
projection defined in the next block. A file
reverted to an earlier content re-emits its batch (that is already how the walk keys), so
no shed generation is ever needed again.

**A GENERATION SUPERSEDES THE WHOLE PRIOR GENERATION OF ITS FILE, IN BOTH HALVES.** The graph
models the target project's CURRENT state. Today a design-doc generation that drops a link leaves
the prior generation's edge live (`valid_to` null), and a code generation that drops an entity
retires the entity's edges but leaves its node live, so a whole-log rebuild carries facts the tree
no longer makes and the compacted-log rebuild does not. A whole-log rebuild that keeps a dropped
fact live is a defect of the fold, and this spec closes it. When a newer generation of a
`<prefix>/<file>` identity folds, every fact the prior generation asserted for that file that the
newer generation does not re-assert is retired (`valid_to` stamped, never deleted), exactly as the
code half already retires a prior generation's edges: the design half's links
(`DocLinkExtracted`) and concepts (`DocConceptExtracted`) exactly as the code half's edges. A node
that no live generation asserts is retired the same way; only a live decision, lesson or finding
edge (the knowledge edges) still touching it keeps it live. The mechanism is the fold's, at
the single fold authority for each arm (`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`
and the domain rules in `crates/rigger-domain/src/contextgraph.rs`), never a second pass or a
post-fold sweep. This is what makes criterion 4's identity hold whenever a later generation DROPS
a fact, not only when it adds or moves one.
- *Convergences undo with their definition.* Every name-resolution convergence the fold keeps is
  two-way. A reference tier promoted because a definition of its name existed (AMBIGUOUS to
  INFERRED, or its CALLS twin) is demoted again when the last live definition of that name retires,
  and a test proof that landed on a definition returns to the pending state when that definition
  retires, so a later definition of the same name receives it. The projection is a pure function of
  the log, so folding a log and folding its compacted form reach the same state on EVERY future
  event, not only at the point compared. The single authority is the fold's own sites in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`: the definition arm's tier promotion and
  the pending-proof reconcile. No sweep and no second pass.
- *The identity.* Criterion 4 compares the LIVE PROJECTION (the public wire form of
  `Projector::whole()`: every node, every live edge, every column, deterministically ordered) PLUS
  the fold state that decides future folds: the pending proofs, the restored-attribute record of
  retired nodes, and the assertion ledgers restricted to live generations, of `graph.db` rebuilt
  from the compacted log versus from the original log. Everything else in the `graph.db` file
  differs by construction (the applied-position ledger records every folded position, and retired
  history rows are history the compacted log no longer replays) and is excluded, so file bytes are
  not the identity. The test compares those tables row for row, then folds at least one further
  event into both rebuilds and compares again, so a latent divergence cannot pass. Its fixture MUST
  seed at least one generation that drops a design link and one that drops a code entity, besides
  the ordinary add-and-move generations, plus a cross-file reference to a dropped name, a test proof
  consumed by a definition a later generation sheds, and a log mixing unkeyed and keyed recordings
  of the same fact; it MUST NOT except any node or edge from the equality.
- *Existing graph.db files.* The fold rule ships with a projection version recorded in `graph.db`. A
  `graph.db` whose recorded version predates the rule (its generation and assertion ledgers empty or
  absent) is rebuilt cold from the log once, explicitly, by `rigger setup` - the verb every install
  already runs in the project; no folding command ever rebuilds implicitly. The rebuild folds the
  live selection - every non-derived event plus each identity's latest generation, exactly the rows
  the compaction plan keeps, so the rebuild and `rigger reset --derived` agree by construction - and
  never a superseded generation, which criterion 4's identity licenses; its cost is bounded by the
  live projection, not the log's age. It streams the log once, in order, and never materializes the
  run stream in memory or reads it twice. It folds into a fresh shadow graph file beside the live
  one in committed batches, recording the last folded position in the shadow, then stamps the
  projection version and renames the shadow into place in one step: a racing open of either kind
  sees the old file (rebuild still owed) or the complete rebuilt one, never a half-rebuilt file,
  and the live file is never held under a long write transaction. An interrupted rebuild leaves the
  live file untouched, and the next `rigger setup` resumes from the shadow's last committed batch.
  `rigger setup` reports progress as it folds and installs nothing else until the rebuild has
  completed or been refused. `rigger emit`, and every command whose job is to append to the log,
  always appends and never opens `graph.db` first; while the rebuild is owed it skips the
  incremental fold and says so, since the rebuild re-derives every fold from the log. A command
  whose answer depends on the fold (step, run, graph build, the MCP graph and grounding tools) that
  opens a `graph.db` at the old version refuses at once naming `rigger setup`; it never waits,
  never rebuilds and never fails with "database is locked". A read-only open (dash, validate, graph
  inspection) writes nothing: it answers from the projection as it stands and says the rebuild is
  owed. Incremental folding never resumes on a ledger-less file. `rigger reset --derived` refuses
  to compact a store whose `graph.db` is at the old version until the rebuild has happened, and
  says so. Tests: a cold rebuild through `rigger setup` stamps the version; an emit at the old
  version appends and skips the fold, leaving `graph.db` unchanged; a fold-dependent command at the
  old version refuses naming `rigger setup` without writing; a read-only open racing the rebuild
  leaves the rebuilt file intact and a folding open during it refuses rather than failing locked.
  The criterion 4 unit's evidence MUST include the rebuild completing through `rigger setup` on a
  snapshot of a real log with wall time and peak memory recorded, and an interrupt-then-rerun on
  that snapshot completing without refolding the batches already committed. Without this, every
  store folded before this spec keeps facts a pre-upgrade generation asserted, and a compacted such
  store disagrees with every future rebuild.
- *Unkeyed recordings are permanent asserters.* A derived recording without a replay key (written
  before replay keys existed) is an asserter in its own right for the nodes AND edges it folds: a
  keyed generation's retirement never retires a node or edge an unkeyed recording still asserts,
  edges get the same identity-empty asserter record nodes already have, and compaction never selects
  unkeyed rows. The two rebuilds therefore agree on a log that mixes unkeyed and keyed recordings of
  the same fact, which is the shape of every store written before replay keys.
- *Only knowledge holds a node.* The edges that hold a node whose asserting generation retired
  are exactly the knowledge edges, folded from `DecisionMade`, `LessonLearned` and `ReviewFinding`;
  nothing else holds. A graph-derived attachment - the `IN_COMMUNITY` edge from `CommunityAssigned`
  and the `REALIZES` edge from `ConceptRealized` - is not a derived index type, survives
  compaction, and never holds a node: when the last live generation asserting the node retires,
  those edges retire with it (`valid_to` stamped), and the retired node keeps none of the retired
  generation's attributes, the same retraction the code half applies to any superseded generation
  (an early return on an unsettled kind is not this rule). An attachment folded onto a node the
  graph does not hold (the compacted log replays one whose node's generation was shed) is recorded
  retired at fold time, never creates a node and never stays live; a knowledge edge folded onto an
  absent node creates the held node as the whole-log fold leaves it, so the two rebuilds agree node
  for node and edge for edge. The authority is the fold's own arms in
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` (the community arm, the concept arm and
  the node-retirement rule), no sweep and no second pass. Criterion 4's fixture MUST seed a
  community assignment and a concept realization on an entity a later generation drops, and a
  knowledge edge on another dropped entity, and assert both rebuilds agree on every node and edge
  including the retired-node record; the review verifies criterion 4 by a rebuild-identity check
  over a subset of a real log (every non-derived event plus the derived events of a bounded set of
  identities), not by the synthetic fixture alone.
- *Ownership.* Criterion 4's unit owns the fold change, and its blast radius grows to
  `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` and
  `crates/rigger-domain/src/contextgraph.rs`, because the compaction's correctness argument IS
  this agreement; no other criterion touches the fold.

**THE LIVE-WRITER GUARD READS LIVENESS.** `refuse_derived_reset_if_live` (`src/cli/hygiene.rs:610`) treats a
non-terminal unit as a live writer; a run whose driver died leaves units non-terminal
forever and the only way past is `--force-live`, so the run whose bloat most needs the
compaction is the one that refuses it. A run is live when a step lock is held, when a spawn's
liveness marker is younger than the spawn wall-clock bound, or when a registry instance
heartbeat is younger than `registry::DEFAULT_IDLE_MS` (`crates/rigger-store-sqlite/src/registry.rs:32`). Unit terminality is not a liveness
signal. `--force-live` keeps its meaning (skip the check entirely).

**CROSS-RUN COMMANDS ARE OUT OF SCOPE.** `rigger reset --runs`, `rigger stats`,
`rigger replay` and `rigger canary` are cross-run by contract and keep their whole-stream
reads.

## Notes (non-criteria)

The cost is asserted at the store seam, not by measuring resident memory in a test: a
counting store double records how many events each read materializes, and a fixture stream
holding 200,000 synthetic derived events and two superseded runs before the boundary must
cost a one-shot command exactly the run's own events plus the carried-over typed events.

## Global constraints

- Hyphens, never em dashes, in every added line.
- No new event type; no new dependency.
- Both feature lanes green (fmt, clippy, test on default and --no-default-features).
- A backend answers the boundary lookup, the typed read and the group lookup from its own index,
  server-side filter or group stream; where it has none it reads backward to a bound it names
  (the boundary's first match). None falls back to a client-side forward scan from 0, and none
  materializes every derived event to answer one of them.

## Done when

- [ ] a test proves THE BOUNDARY IS A QUERY: `EventStore::last_position(stream, "RunStarted")`
  returns the current run's boundary on both backends (the sqlite store through an indexed
  lookup, the server-backed KurrentDB store through a backward read that stops at the first match),
  pinned at the store port trait with the double asserting no forward read from 0 ever
  happened. This criterion OWNS the boundary lookup; what reads from it is criterion 2's,
  NOT this one's.
- [ ] a test proves ONE-SHOT COMMANDS READ FROM THE BOUNDARY: `rigger status`, `rigger watch`,
  the dash snapshot, the sidecar behind `rigger peers`, the MCP tools and a `rigger step` that
  does not ingest read the run's own events from the boundary with the derived types excluded
  and the carried-over knowledge by type, so a fixture stream with 200,000 derived events and two
  superseded runs before the boundary costs each of them exactly the run's events plus the typed
  carry-over, asserted through the counting store double. This criterion OWNS the read position
  of every one-shot command and the run slice's derived-type exclusion; the boundary lookup is
  criterion 1's and the latest-generation seed of a step that ingests is criterion 3's, NOT this
  one's; cross-run commands are excluded.
- [ ] a test proves THE LATEST GENERATION IS A GROUP LOOKUP: a `rigger step` that ingests a tree
  holding an unchanged, a changed and a reverted file over the same fixture materializes zero
  `DERIVED_INDEX_TYPES` events, costs exactly the run's events plus the typed carry-over, and
  appends exactly the changed and reverted files' batches, asserted through the counting store
  double with the seed answered by `EventStore::latest_in_group` on both backends. This criterion
  OWNS the latest-generation lookup (port method, both adapters, group stamp, both ingest sinks'
  seeding) and the cost of a step that ingests; the read position of everything else is criterion
  2's and the compaction of superseded generations is criterion 4's, NOT this one's.
- [ ] a test proves COMPACTION SHEDS SUPERSEDED GENERATIONS: `rigger reset --derived` on a
  log holding three generations of one file keeps only the latest generation's recordings
  (plus the exact-key dedup it already does), reports the count shed, and `graph.db` rebuilt
  from the compacted log is byte-identical to one rebuilt from the original. This criterion
  OWNS `--derived`'s key semantics; the guard that lets it run is criterion 5's, NOT this
  one's.
- [ ] a test proves THE GUARD READS LIVENESS: `rigger reset --derived` proceeds without
  `--force-live` on a store whose units are non-terminal but whose step lock is free, whose
  spawn markers are all older than the wall-clock bound and whose registry heartbeat is
  older than the idle window, and still refuses (naming what is live) when any one of those
  three is fresh. This criterion OWNS the live-writer refusal's definition.
- [ ] both feature lanes green (fmt, clippy, test on default and --no-default-features).
