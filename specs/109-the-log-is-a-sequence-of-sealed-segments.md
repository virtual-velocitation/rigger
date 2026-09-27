# 109 - The log is a sequence of sealed segments

**Goal:** rigger is a hive mind: its graph holds the project's whole understanding and serves
each persona the slice it needs, and the append-only log is only the persistence underneath.
That log is stored today by an embedded SQL engine (`rusqlite` with the bundled
C library, `Cargo.toml:150`) behind a five-method port (`EventStore`, `crates/rigger-domain/src/eventstore.rs:499`).
The engine is the one C dependency in the build, it knows nothing of runs, and it is the
reason the `store` lane cannot join the pure `core` lane. This project's log holds 2,497,006
events in 1.44 GB, of which 149 runs' mechanics and 17,424 knowledge events are the only
content the log is truth for once specs 101 and 107 land. This spec ships sections 5.1-5.4,
5.6, 5.8 and 5.9 of docs/architecture-addendum-the-owned-store.md: the log becomes a
sequence of immutable per-run segment files written by rigger itself, and the SQL log backend
is deleted.

## Design

THE RECORD, decided: `len u32 | pos u64 | kind u8 | payload | crc32`, little-endian, payload
the event's JSON exactly as the port receives it today, `kind` 0 = event, 1 = ledger, 2 =
progress. CRC-32 (IEEE) is table-driven in `src/store/crc.rs`; no crate.

THE SEGMENT, decided: `.rigger/store/segments/<first-position, 15 digits>.seg`. One segment
is active; it seals when the run that opened it reaches a terminal state or at 64 MB. Sealing
appends a footer: position range, run id, per-type position list, per-stream live-record
count, per-file latest ledger generation, record count, digest, and a trailer
`footer_len u32 | magic`. A segment without a trailer is active or torn. No segment
spans two runs; a run may span several.

THE MANIFEST, decided: `.rigger/store/MANIFEST`, format-tagged, lists every sealed segment
(name, range, run, digest), the active segment, the ledger segment and the format version;
replaced by temp + fsync + rename, only by the mount owner (spec 111) or, before spec 111
lands, by whichever process holds the `OWNER` lock for the operation. A file the manifest
does not name is garbage.

APPENDING, decided: any process appends by taking `.rigger/store/JOURNAL.lock` exclusively,
reading the active segment's length, writing the batch, fsyncing, releasing. The lock is held
for one batch only. Positions are contiguous `u64` across segments as today.

READING, decided: `read_stream(from)` maps only the segments whose range intersects;
`read_stream_typed(type)` walks footers' type lists and scans the active segment;
`last_position(stream, type)` walks footers backward and stops at the first non-empty list;
spec 101's latest-generation question is answered from the footers' per-file maps. No read
path touches a record outside the answer except within the active segment.

REVISION IS COMPUTED, decided: a record's revision is its rank among the live records of its
stream, derived from the footers' per-stream counts plus the active segment; it is never
stored. `ExpectedRevision` compares against that rank under the JOURNAL lock. Compaction and
archive therefore cannot leave a hole, and the revision-hole detection and two-phase repair in
`rigger validate` (`docs/architecture.md:580-617`) are removed with the SQL backend.

ONE HANDLE PER ROOT, decided: the `progress` stream lives in the same journal as kind-2
records, excluded from replay by kind; a command opens one store per project root, resolved
by `require_store_dir` (`src/main.rs:2068`), and the three-handle open sites
(`src/main.rs:3911-3928`, the MCP server's cwd-resolved graph at `:13618-13625`) collapse to
that one resolution.

SUBSCRIPTION, decided: `subscribe_all`/`subscribe_stream` watch the active segment's length
(inotify through `std`-only polling of metadata at 250 ms; no crate) and deliver records from
the last delivered position. The manifest is re-read when its inode changes.

RECOVERY, decided: on open, the active segment is truncated to its last record whose CRC
verifies; sealed digests are verified on first map and a mismatch is a hard error naming the
file. A torn footer makes the segment active again and the seal is redone.

MIGRATION, decided: `rigger store migrate` reads the SQL log through the existing port from
position 0, writes segments split at each `RunStarted`, seals every run but the live one,
writes the manifest, renames `events.db` to `events.db.bak-<stamp>-presegments` (which the
reaper never deletes, spec 77 GC4) and records a `DecisionMade` naming the counts. The first
open of a project that has `events.db` and no `MANIFEST` runs the migration after taking a
backup. `progress.db` migrates the same way into kind-2 records of the live run's segment,
and `progress_store.rs` reads and writes kind-2 records through the same engine.

BACKEND SELECTION, decided: `[store] backend = "segments"` is the default once this spec
lands; the server-backed backend keeps its config value; the `sqlite` value is removed and
`src/eventstore/sqlite.rs` is deleted together with the SQL progress store. The graph backend
(spec 110) is untouched by this spec.

CONSTRAINTS WALK: empty store - manifest with no segments, first append opens segment 0.
Crash mid-batch - torn tail truncated, batch absent. Crash mid-seal - trailer missing, segment
re-sealed on open. Crash mid-manifest rename - old manifest wins, new segment garbage-collected.
Two processes appending - serialized by the lock; positions contiguous. Reader during append -
reads up to the last complete record. Cold start - open is footers plus the active segment.
Archive (spec 107) - replaces sealed segments through the manifest, never touches the lock.

## Notes (non-criteria)

The store module layout is `src/store/{crc,record,segment,manifest,journal,reader,watch}.rs`
behind the two existing ports; nothing above the ports changes. Footer type lists are
`(type_name, [pos])` sorted by type; a segment of 90,000 events (this run) has a footer near
400 KB.

## Global constraints

- Hyphens, never em dashes. Both feature lanes and the core lane green (fmt, clippy -D
  warnings, test); no-os-kill and reap audits green.
- No new event type; no new crate dependency; the `rusqlite` dependency is removed only when
  spec 110 removes its last user.
- Positions are never renumbered; a record's bytes are never rewritten after its batch fsyncs.
- The operator's installed rigger binary is never replaced or modified by any unit.

## Done when

- [ ] a test proves THE RECORD AND SEGMENT FORMAT: a batch appended to an empty store
  produces one active segment whose records round-trip through the reader with verified
  CRCs, sealing writes a footer whose type lists, stream counts and digest match the
  records, and the engine passes `eventstore::contract::assert_contract` unchanged. This
  criterion OWNS the byte formats of record, footer and trailer and the port conformance;
  the manifest is criterion 2's, NOT this one's.
- [ ] a test proves THE MANIFEST IS THE VIEW: a reader opens only files the manifest names, a
  manifest replaced by rename is seen whole or not at all, and a segment left unnamed by a
  crashed seal is reported as garbage and reaped by the owner. This criterion OWNS the
  manifest's contents and its replacement.
- [ ] a test proves APPENDS ARE SERIALIZED: 64 processes appending batches concurrently
  produce contiguous positions with no interleaved bytes and the lock is never held across a
  read, pinned by a lock-hold timer in the journal seam. This criterion OWNS the JOURNAL
  lock's discipline.
- [ ] a test proves READS COST THEIR ANSWER: `read_stream_typed`, `last_position` and the
  latest-generation query over a store of 200,000 records in ten sealed segments touch only
  footers plus the matching records, asserted through a counting reader double, and a
  stream's revisions stay contiguous across a compaction that removes its records. This
  criterion OWNS the read paths and the computed revision; the port contract itself is spec
  101's.
- [ ] a test proves RECOVERY: a segment cut mid-record reopens to its last valid record, a
  segment cut mid-footer becomes active again, and a sealed segment with a flipped byte
  fails to map with an error naming the file. This criterion OWNS torn-tail and digest
  handling.
- [ ] a test proves SUBSCRIPTION: a subscriber sees a foreign process's append within 500 ms
  and misses none across a seal. This criterion OWNS the watch.
- [ ] a test proves MIGRATION: `rigger store migrate` on a fixture SQL log with three runs
  writes three sealed segments and one active, a manifest, a backup the reaper skips, and a
  `DecisionMade` with the counts; a re-run is a no-op; the progress store migrates to kind-2
  records. This criterion OWNS the migration and the backend default; deleting the SQL
  backend is criterion 8's, NOT this one's.
- [ ] a test proves THE SQL LOG IS GONE: `src/eventstore/sqlite.rs` and the SQL progress store
  no longer exist, the `sqlite` backend value is rejected with a message naming the
  migration, and the simplification audit lists no dead store code. This criterion OWNS the
  removal.
- [ ] both feature lanes and the core lane green (fmt, clippy, test).
