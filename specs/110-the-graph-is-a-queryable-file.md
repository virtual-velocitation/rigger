# 110 - The graph is a queryable file

**Goal:** the graph is the hive's memory: it holds the project's whole understanding and each
persona is served the slice it needs from it. Today it lives in an embedded SQL database
(`.rigger/graph.db`, `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`, 8,307
lines) behind a six-method port (`Projection`, `crates/rigger-domain/src/contextgraph.rs:531`). Every process
that asks the graph a question opens the engine; nothing holds the graph resident for the
process that folds it thousands of times per run. Once the code store lands, graph.db holds records only: 19,369 live nodes today, of which
12,426 are code entities that move to the code store. This spec ships section 5.5, 5.7's snapshot
operation and 5.8 of docs/architecture-addendum-the-owned-store.md: the graph is a
memory-resident structure in the owner and a memory-mapped, queryable file for every other
process, and the last SQL dependency leaves the build.

## Design

THE BASE SNAPSHOT, decided: `.rigger/store/graph/base.gsnap`, format-tagged, compressed
sparse row: header with section offsets; node table sorted by id (id offset, kind, payload
offset, first-out, first-in); out-edge and in-edge arrays; edge table (relation, dst, src,
`valid_from`, `valid_to`, tier, source position); alias table sorted by alias; the applied
position set; string pool. The base holds the record graph only: no code node or edge is
resident. A reader answers `subgraph`, `resolve`, `locate` and `calls` by binary search in the sorted
tables and walks of the adjacency arrays, mapping the file read-only and never copying a
section into a heap structure.

DELTAS, decided: `.rigger/store/graph/deltas/<position>.gdelta`, one per applied batch, holding
the batch's node upserts, edge inserts and `valid_to` marks, format-tagged and CRC-checked.
A reader applies every delta named by the manifest after the base, in position order. The
owner rewrites the base when deltas exceed 8 MB or 10,000 files, and at unmount, then removes
the folded deltas through the manifest.

THE RESIDENT GRAPH, decided: the mount owner holds the same CSR layout in memory with an
append-only overflow region for the current session's upserts, so `apply` is O(batch) and
reads are the same code path as the mapped file. `apply_batch` writes the delta file before
mutating memory; a crash between the two costs nothing because the delta is the record.

REBUILD, decided: `rigger graph rebuild` and the once-per-mount background verify construct
the record graph from the record journal alone into a shadow CSR, and advance a shadow code
snapshot from empty to the stamp's commit; the verify compares the CSR with the loaded
projection section by section and the snapshot with the stored one; on mismatch
the rebuild is published as the new base, a `LessonLearned` names the differing section, and
`rigger status` drops the PROVISIONAL label it carried until the verify finished.

MIGRATION, decided: the first open of a project with `graph.db` and no `base.gsnap` writes
the base from the SQL projection through the existing port, renames `graph.db` to
`graph.db.bak-<stamp>-presnapshot`, and schedules the verify. `rusqlite` and its bundled
library are removed from `Cargo.toml`; the `store` feature builds for the pure lane.

CONSTRAINTS WALK: empty graph - a base with empty sections. Repeated apply of one batch -
idempotent through the applied position set, which replaces the `applied` table. Crash after delta,
before memory - the delta replays on open. Crash mid-base-rewrite - the manifest names the
old base and the deltas; the temp base is garbage. Two readers, one owner - readers map
immutable files; the owner's rename is atomic. Cold start with no owner - a client reads
base plus deltas and never writes. Rollback of a landing - the graph's `valid_to` marks are
records in deltas like any other.

## Notes (non-criteria)

Section layout and endianness are pinned in `src/store/graph/format.rs` with a golden
fixture; its size goals are the record graph's. The mapped reader is generic over a
byte source so the resident graph and the file share one query implementation.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency; `rusqlite` removed.
- A published base or delta is never modified; the manifest is the only mutable reference.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE BASE FORMAT: a graph written as a base round-trips every node, edge,
  validity interval, alias and source key through the mapped reader, pinned by a golden
  fixture of the byte layout. This criterion OWNS the base format; deltas are criterion 2's,
  NOT this one's.
- [ ] a test proves DELTAS FOLD: applying twenty batches writes twenty deltas that a reader
  folds onto the base to the same answers the owner's resident graph gives, and the owner's
  base rewrite at the threshold removes exactly the folded deltas. This criterion OWNS the
  delta format, its threshold and the rewrite.
- [ ] a test proves CLIENTS MAP, NEVER LOAD: `rigger peers` against a base of 20,000 nodes answers a
  neighborhood touching under 4 MB of pages and under 16 MB of heap, asserted by a counting
  allocator and page residency. This criterion OWNS the client read path.
- [ ] a test proves REBUILD WINS: a base with one edge altered is detected by the verify, the
  rebuild is published, a `LessonLearned` names the section, and `rigger status` drops
  PROVISIONAL only after the verify completes. This criterion OWNS the rebuild and the label.
- [ ] a test proves MIGRATION AND REMOVAL: a project with `graph.db` opens into a base plus a
  backup the reaper skips; `rusqlite` is absent from `Cargo.lock`; the `store` feature builds
  in the pure lane. This criterion OWNS the migration and the dependency removal.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
