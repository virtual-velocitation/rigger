# 101 - A one-shot command folds only the run it serves

**Goal:** every `rigger` invocation that serves one run reads that run, not the whole
history of the project. Measured on the 2026-09-15 store (2,075,706 events, 1.24 GB):
`rigger status` peaks at 2.7 GB resident, `rigger peers` at 5.3 GB (its sidecar replays from
position 0, `src/main.rs:9163`), and one `rigger step` at 9.8 GB (the kernel's out-of-memory
report of that day) - while the run those commands served spans 67,456 events
(positions 3,146,193 to 3,213,649). 97% of the stream is derived graph ingest: 1,801,003
`EdgeInferred`, 125,861 `CodeEntityExtracted`, 101,354 `DocLinkExtracted`, and of the
edges ~1.62 million are superseded generations of files that were later re-ingested
(`gc/<file>@<hash>#<n>` keys: a file edit re-records every edge of the file under a new
generation). `src/conductor.rs` calls `read_stream(STREAM, 0, Direction::Forward)` at 108
sites, `src/main.rs` at 36, `src/run.rs` at 22. Five agents each calling rigger a few times a
minute put 15 to 25 GB of baseline pressure on a 62 GB machine before a single cargo build.

## Design

**THE FOLD HAS A BOUNDARY, decided here so no unit has to.** The current run begins at the
stream position of its `RunStarted` event (the `runscope` boundary that
`runscope::current_run` already applies - after reading everything). The boundary is a store
port query, `last_position(stream, event_type)`, implemented on the embedded sqlite store as
an indexed lookup and on the server-backed store as a backward read that stops at the first
match. No caller derives the boundary by scanning forward from 0.

**THREE READ CLASSES.** (i) The run's own events, from the boundary forward, are read and
folded whole - they are the run. (ii) Carried-over knowledge - `LessonLearned`,
`DecisionMade`, `ReviewFinding` and the playbook events the fold consults across runs - is
read BY TYPE over the whole stream through the store's type index (`read_stream_typed`),
so its cost is bounded by its own count (about 22,000 events today), never by the derived
types. (iii) The derived ingest types (`ingest::DERIVED_INDEX_TYPES`) are NEVER materialized
by a one-shot command: `graph.db` is their fold, and the only question a command asks of
them - a file's latest recorded generation (`ingest::project_scoped_latest_generations`,
`project_scoped_replay_keys`) - is answered by a store query over the `replay_key` meta
column grouped by file identity. An in-memory scan of every derived event to find the latest
key per file is NOT an implementation of this design.

**COMPACTION SHEDS SUPERSEDED GENERATIONS.** `rigger reset --derived` today keeps the latest
recording per exact replay key (13 duplicates on this store) and leaves every superseded
generation in place. It keeps, per `<prefix>/<file>` identity, only the recordings of the
LATEST generation, carrying the earliest valid-time onto a kept recording exactly as the
reasserting-types rule already does. Correctness is rebuild-identical: `graph.db` rebuilt
from the compacted log equals `graph.db` rebuilt from the full log, byte for byte. A file
reverted to an earlier content re-emits its batch (that is already how the walk keys), so
no shed generation is ever needed again.

**THE LIVE-WRITER GUARD READS LIVENESS.** `refuse_derived_reset_if_live` treats a
non-terminal unit as a live writer; a run whose driver died leaves units non-terminal
forever and the only way past is `--force-live`, so the run whose bloat most needs the
compaction is the one that refuses it. A run is live when a step lock is held, when a spawn's
liveness marker is younger than the spawn wall-clock bound, or when a registry instance
heartbeat is younger than `registry::DEFAULT_IDLE_MS`. Unit terminality is not a liveness
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
- A backend that cannot answer a typed or boundary query natively answers it by a bounded
  backward read; it never falls back to a forward scan from 0.

## Done when

- [ ] a test proves THE BOUNDARY IS A QUERY: `Store::last_position(stream, "RunStarted")`
  returns the current run's boundary on both backends (the sqlite store through an indexed
  lookup, the server-backed store through a backward read that stops at the first match),
  pinned at the store port trait with the double asserting no forward read from 0 ever
  happened. This criterion OWNS the boundary lookup; what reads from it is criterion 2's,
  NOT this one's.
- [ ] a test proves ONE-SHOT COMMANDS READ FROM THE BOUNDARY: `rigger status`, `rigger step`,
  `rigger watch`, the dash snapshot and the sidecar behind `rigger peers` and the MCP tools
  read the run's own events from the boundary and the carried-over knowledge by type, so a
  fixture stream with 200,000 derived events and two superseded runs before the boundary
  costs exactly the run's events plus the typed carry-over, asserted through the counting
  store double. This criterion OWNS the read position of every one-shot command; the
  boundary lookup is criterion 1's and the derived-generation question is criterion 3's,
  NOT this one's; cross-run commands are excluded.
- [ ] a test proves THE LATEST GENERATION IS A QUERY: `project_scoped_latest_generations`
  and every consumer of `project_scoped_replay_keys` answer from a store query over the
  `replay_key` meta grouped by file identity, and no one-shot command materializes a
  `DERIVED_INDEX_TYPES` event, pinned by the counting double reporting zero derived events
  read across a `rigger step` that reindexes a changed file. This criterion OWNS the derived
  read path; the compaction of those events is criterion 4's, NOT this one's.
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
