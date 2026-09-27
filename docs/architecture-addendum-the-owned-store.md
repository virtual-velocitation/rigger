# Architecture addendum: the owned store

> Rigger's memory is a hive mind: the knowledge graph holds the whole understanding of every
> project rigger works on, and each persona is served the slice it needs. This addendum
> specifies the storage engine under that memory. Rigger owns it: a single-node,
> log-structured store written in the tree, with no database engine underneath, designed from
> the start for many projects on one machine and for strict, measured resource use. A
> project's memory is MOUNTED only while the project has workloads; an idle project costs the
> machine nothing but disk. The body describes the target state; the present state appears
> only in Problem and Delivery.

---

## 1. Problem (measured)

Rigger persists its memory in three embedded SQL databases per project: the event log
(`.rigger/events.db`), the knowledge-graph projection (`.rigger/graph.db`) and the agent
progress side-store (`.rigger/progress.db`). Measured on this project's own store on
2026-09-26:

| Quantity | Value |
|---|---|
| events in the log / file size | 2,497,006 / 1.44 GB |
| derived structure events (re-derivable from the tree) | 2,443,614 (97.9%) |
| episodic events of finished runs, of which prompt bodies | 35,334 events / 333 MB, of which 303 MB |
| knowledge events (decisions, lessons, findings) | 17,424 / 23 MB |
| graph projection: nodes / edges / file | 61,950 / 91,694 / 73 MB |
| one point lookup in the projection | ~10 us |
| `rigger status` on an idle project | 4.6 s, 3.2 GB resident |
| one checkin `rigger step` | 8.3 GB resident |

The seconds and gigabytes are not the engine's: every courier replays the whole stream from
position zero. Specs 101, 107 and 108 bound the reads, move perception out of the log and
archive finished runs, and they do so behind the two store ports. What remains after them is
the engine itself, and the engine is wrong in three ways that no read discipline fixes:

- **It is a foreign engine under a log-shaped workload.** Across 10,135 lines of store code
  behind two ports of five and six methods (`EventStore`, `crates/rigger-domain/src/eventstore.rs:499`;
  `Projection`, `crates/rigger-domain/src/contextgraph.rs:531`), the relational surface in use is four joins
  and no trigger, view or full-text index; the log is one table (`crates/rigger-store-sqlite/src/eventstore/sqlite.rs:21`)
  and the progress side-store is a second instance of the same engine over the same schema.
  SQL is being used as a B-tree with transactions and a file format. Its bundled C library
  (`Cargo.toml:150`) is the one C dependency in the build and the one reason the `store` lane
  cannot follow the pure `core` lane. Two of its indexes work only when a query is phrased
  with the exact expression they were built on (`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:787-792`), and
  `reset --derived` can leave per-stream revision holes that a documented two-phase operator
  procedure repairs (`docs/architecture.md:580-617`).
- **It has no notion of a project's lifecycle.** A store is open whenever any process wants
  it: every command opens up to three engine handles at its composition root
  (`src/main.rs:3911-3928`), the dashboard, which is already the machine's multi-project
  reader (`?instance=<id>`, `src/dash.rs:3413`), reopens each attached project's files on
  every HTTP request (`src/main.rs:7270-7377`), and every subscription is a thread waking
  every 25 ms to poll through the store's single mutexed connection
  (`crates/rigger-store-sqlite/src/eventstore/sqlite.rs:42`, `:1001`). Nothing distinguishes a project running three
  agents from a project nobody has touched in a month; both are reached the same way and cost
  the same to read. With two projects on one machine, the memory a courier spends is set by
  the largest project it happens to open. The MCP server even resolves its log through the
  walked-up store root and its graph through the cwd (`src/main.rs:13618-13625`).
- **Its resource use is unbounded by design.** Resident memory is whatever a replay needs.
  The kernel's out-of-memory report of 2026-09-24 chose the largest process on the machine, a
  `rigger step` at 8.5 GB, while eight test children allocated 5.5 GB each. Nothing in rigger
  measured, budgeted or refused any of it.

### Non-goals

- No server, no daemon protocol for data. The store is files under the project's own
  `.rigger/`; every coordination primitive is a held file lock or an immutable file.
- No distribution, replication, sharding or consensus. Rigger runs on one machine.
- No new dependency. The engine, its checksums and its file formats are written in the
  tree; the build loses a dependency and gains none.
- No change to the two store ports' contracts as specs 101, 107 and 108 leave them. The
  engine is a backend; conductor, grounding and dash code do not learn it exists. The
  project namespace on stream names (`eventstore::namespace::Namespaced`,
  `docs/architecture.md:570`) and the server-backed store (`store.backend = "kurrentdb"`)
  stay exactly as they are; the engine replaces the `sqlite` value only.

---

## 2. Invariants

1. **One writer per project, proven by a held lock.** Appends to a project's journal are
   serialized by a file lock any rigger process may take for one append. Structural work
   (sealing, compaction, archive, snapshot, manifest) belongs to the mount owner alone.
2. **Sealed means immutable.** A sealed segment and a published snapshot are never modified.
   Readers map them without coordination; the only mutable files are the active journal's
   tail and the manifest, and the manifest changes by atomic rename.
3. **Every index is a projection.** The graph, the type indexes and the run boundaries are
   rebuildable from the journal, the ledger and the tree. Rebuild wins over any persisted
   projection on mismatch.
4. **A project costs nothing while idle.** No process holds an idle project's files or memory.
   The first workload mounts it; the last workload's end unmounts it after a bounded idle
   window.
5. **Resources are measured, budgeted and refused, never estimated.** Every mounted project
   publishes its resident bytes; admission reads the sum against a machine budget; a refusal
   names the project, the number and the remedy.
6. **A project's store is reachable only through its own root.** Every path derives from
   the project root through the one encoding; no machine-scoped file carries project content.
7. **Formats are versioned and fenced.** Every file the engine writes opens with a format
   tag; an older binary refuses a newer store; a newer binary migrates an older one.

---

## 3. Topology

```
 MACHINE SCOPE (0700, $XDG_STATE_HOME/rigger/machine/)   per-user, content-free
 +---------------------------------------------------------------------------+
 | budget.toml   resident-memory budget, mount cap, idle window (operator)   |
 | mounts/<project-id>.slot   flock-held while mounted; resident_bytes,      |
 |                            pid/start/boot identity, since                 |
 | maintenance.lock   one compaction/consolidation/archive at a time         |
 +---------------------------------------------------------------------------+
        ^ admission reads             ^ publishes                 ^ holds during
        |                             |                            |  structural work
 PROJECT A (mounted)                  |                     PROJECT B (idle)
 +--------------------------------------------------+     +------------------------+
 | owner: rigger run (the supervisor), holds        |     | no process, no memory  |
 |   .rigger/store/OWNER (flock)                    |     | .rigger/store/         |
 |   resident graph  (CSR in memory)                |     |   MANIFEST             |
 |   journal index   (position -> segment, offset)  |     |   segments/*.seg       |
 |   type index      (type -> positions)            |     |   graph/base.gsnap     |
 |   tail subscriber (folds appends as they land)   |     |   graph/deltas/*.gdelta|
 +----------------+---------------------------------+     |   ledger.seg           |
                  | mmap, read-only                        +------------------------+
   +--------------+-----------------------------+                 ^
   | clients: rigger status / peers / mcp /     |   cold read     |
   | hooks / rigger emit / rigger result        |-----------------+
   | (append under the JOURNAL lock; read       |   pages touched, nothing resident
   |  sealed segments + snapshot + deltas)      |
   +--------------------------------------------+
```

Why this shape: every consumer of the store today is a separate process, and after specs
105 and 106 most still are (hooks, MCP tools, one-shot verbs), so a design that needs the
owner to be alive for every read or write would make the owner a database server. Immutable
files plus one lock give every process a correct view with no protocol, and give an idle
project a cost of zero because nothing has to be running for a client to read it.

---

## 4. The data model

The store holds four kinds of content, each with its own lifecycle. The kinds are the ones
specs 107 and 108 define; the engine gives each the physical home its lifecycle wants.

| Kind | Content | Truth | Lifecycle | Physical home |
|---|---|---|---|---|
| knowledge | decisions, lessons, findings, verdicts, gate promotions, run starts | the log | never forgotten; superseded into history on evidence or consolidated upward | knowledge residue of every sealed segment |
| episodic | a run's mechanics: spawns, progress, results, blast radii, touches | the log | lives with its run; archived to git when the run finishes | the run's own segment, then `refs/rigger/archive/<run>` |
| perception | code structure, doc links, workflow shape | the tree | re-derived; the log keeps only a ledger entry per file generation | the ledger segment and the graph projection |
| semantic projection | the graph: nodes, edges with validity, aliases, concepts | derived from the three above | rebuildable; refreshed as a snapshot plus deltas | `graph/base.gsnap` + `graph/deltas/` |

Positions stay a single monotonic `u64` across the whole log, as today: a run's boundary is
a position, a `valid_from` is a position, an archive names a position range. A record's
REVISION is its rank among the live records of its stream, computed by the engine from the
per-stream counts every footer carries, never stored: compaction and archive therefore
cannot leave a revision hole, and the repair procedure the current engine documents is
retired. `ExpectedRevision` on append compares against that rank under the journal lock.

---

## 5. The engine

### 5.1 Journal and segments

The log is a sequence of SEGMENT files. Exactly one is ACTIVE and appendable; every other
segment is SEALED. A record is:

```
 +----------+----------+----------+------------------+----------+
 | len u32  | pos u64  | kind u8  | payload (JSON)   | crc32    |
 +----------+----------+----------+------------------+----------+
   kind: 0 = event, 1 = ledger (GenerationIngested / PassRecorded), 2 = progress
```

Segment naming is by first position: `segments/000000003634950.seg`. A segment seals when
the run whose `RunStarted` opened it reaches a terminal state, or when it reaches 64 MB,
whichever first; a run therefore spans one or more consecutive segments and no segment
spans two runs. Sealing appends a FOOTER: the position range, the run id, a per-type
position list (the type index for that segment), the record count and a digest over the
records. A segment with a footer is sealed; one without is active or torn.

Why segments per run: the run is the unit of every lifecycle decision the hive takes
(archive, consolidate, boundary), so making it the unit of storage turns each of those
decisions into a file operation on an immutable file instead of a row-by-row rewrite.

### 5.2 The manifest

`MANIFEST` is a small versioned file listing every sealed segment with its position range,
run id and digest, the active segment's name, the ledger segment, the current graph base
snapshot and its position, and the format version. It is replaced by write-to-temp,
`fsync`, `rename`. A reader opens the manifest first and sees a consistent set of files; a
segment or snapshot not named by the manifest is garbage to be reaped by the owner.

### 5.3 Appending

Any rigger process may append. It takes the JOURNAL lock (`store/JOURNAL.lock`, an
exclusive advisory lock), reads the active segment's length to find the next position,
writes the batch of records, `fsync`s the segment, releases the lock. Batches are the unit
of atomicity: a crash mid-batch leaves a torn tail that the next opener truncates back to
the last record whose checksum verifies, so a batch is either wholly present or wholly
absent. The lock is held for one batch, never across a read, so a hook's `rigger emit`
waits on the supervisor's ingest batch at most once.

Why not route writes through the owner: it would make the owner mandatory for every
`rigger emit` and `rigger result`, and a courier on an idle project would have to start one
to write a single decision. A lock held for one batch is the whole coordination the log
needs; the world authority's declaration-versus-observation rule governs WHICH types a
courier may write and is orthogonal to how bytes reach the file.

### 5.4 Reading

Three read classes, as spec 101 defines them, each served by an index that costs its own
size, never the log's:

- **A run's own events** are the records between the boundary and the head: the manifest
  names the segments, the footers give the offsets, the reader maps only those.
- **Knowledge by type** is the union of every sealed footer's type list for that type plus a
  scan of the active segment; a `read_stream_typed` on 17,424 knowledge events touches
  17,424 records and the footers, not 2.5 million records.
- **Perception** is never read from the log by a command; the graph projection answers it.

`last_position(stream, type)` is a backward walk over footers that stops at the first
segment whose type list is non-empty for that type, then a backward scan inside it.

### 5.5 The graph projection

The graph is a memory-resident structure in the mount owner and a QUERYABLE FILE for
everyone else. The file, `graph/base.gsnap`, is a compressed sparse row (CSR) layout: a
sorted node table (id, kind, payload offset), forward and reverse adjacency arrays, an edge
table (relation, `valid_from`, `valid_to`, source key), an alias table and a string pool,
each section with its offset in a header. A client maps the file and answers `subgraph`,
`resolve`, `locate`, `calls` by binary search and array walks, touching only the pages the
question needs. Nothing is deserialized into a heap.

Changes since the base snapshot are DELTA files, `graph/deltas/<position>.gdelta`, one per
applied batch, each holding the batch's upserts and `valid_to` marks. A reader applies the
deltas after the base; the owner rewrites the base when deltas exceed 8 MB or 10,000
batches, and at unmount, then deletes the folded deltas through the manifest.

Why a queryable file rather than a serialized heap: a one-shot client that had to load the
graph would spend ~150 MB and ~100 ms per question, and five agents' couriers would spend it
concurrently. Mapping means an idle project's graph costs a client exactly the pages it
touches and the machine's page cache shares them across every client.

### 5.6 Subscription

The owner folds its own appends in-process and is notified of foreign appends by watching
the active segment's length (a 250 ms metadata poll from `std`, one watcher thread per
process fanning out to every subscription in it). `subscribe_all` and `subscribe_stream` on
a client are the same watch on the same file, filtered by stream prefix as the namespace
decorator already does. There is no wake-up protocol because the file is the protocol.

Why one watcher per process: today every subscription is its own 25 ms thread contending
for the store's write mutex; the side-car alone stacks a 25 ms database poll under a 50 ms
channel poll (`src/sidecar.rs:149`). One length check per process per 250 ms costs nothing
measurable and cannot contend with appends because it never takes the lock.

### 5.7 Compaction and archive

Compaction is structural work: only the mount owner does it, under the machine-scope
`maintenance.lock`, one project at a time. Three operations, each a whole-segment act:

- **Archive** (spec 107): a finished run's segments are written as one blob at
  `refs/rigger/archive/<run-id>`, a `RunArchived` record is appended, and the segments are
  replaced by their KNOWLEDGE RESIDUE, a new sealed segment holding only the run's knowledge
  records at their original positions. Positions are never renumbered.
- **Ledger fold**: superseded generations in the ledger segment are dropped when spec 101's
  compaction rule allows, rewriting the ledger segment.
- **Snapshot**: the graph base is rewritten from the resident graph and the deltas removed.

Why whole segments: rewriting one immutable file into another and swapping the manifest
is crash-safe by construction (either manifest names the old files or the new), and it
never holds the JOURNAL lock, so appends continue throughout.

### 5.8 Crash and recovery

On open, the owner: reads the manifest; verifies each sealed segment's digest lazily on
first map (a mismatch is a hard fault naming the file); truncates the active segment to
its last valid record; rebuilds the journal and type indexes from footers plus the active
segment; loads the graph base and deltas. Once per mount, in the background, it performs a
FULL REBUILD of the graph from ledger, tree and knowledge records into a shadow structure
and compares it with the loaded projection; on mismatch the rebuild wins and is published as
the new base, and the mismatch is recorded as a `LessonLearned` about the store. Until the
verify completes, `rigger status` labels the graph PROVISIONAL, as the world authority's
snapshot rule requires.

### 5.9 Formats and versioning

Every file starts with a magic and a `u16` format version. The manifest's version is the
store's; a binary whose supported range excludes it refuses to open with the remediation
named (upgrade the binary, or run `rigger store migrate --to <version>`). Checksums are
CRC-32 (IEEE), table-driven, implemented in the tree. Payloads are JSON as today; the
framing, not the payload encoding, is what changes.

---

## 6. Projects

### 6.1 Identity

A project is its root directory, found as today by the bounded walk up to the main repo
root (`require_store_dir`, `src/main.rs:2068`). `.rigger/project.id` holds the stable id
(`project_identity_at`, `src/main.rs:948`) that already names the stream namespace
`proj-<id>-`; machine-scope files are keyed by that id. Two checkouts of one repository are
two projects with two roots and one id only if they share the tracked file, exactly as the
namespace treats them today; a project moved to a new root keeps its id, and its slot row's
recorded root is overwritten on the next mount. The instance registry
(`crates/rigger-store-sqlite/src/registry.rs`, discovery metadata, never truth) keeps its role; the slot substrate is
the held-lock authority the world authority asks for and the registry is not.

### 6.2 The mount lifecycle

```
                 first workload                    idle window elapsed
   UNMOUNTED  ------------------->  MOUNTED  ------------------------->  UNMOUNTING --> UNMOUNTED
   no process    (admission passes)   owner holds OWNER lock             snapshot,
   files only                          resident graph + indexes          release slot,
                                        publishes resident_bytes         close files
       ^                                  |
       |  cold read: any client, any time |  workload = live run, live spawn, held step
       +----------------------------------+  lock, or a client session heartbeat
```

MOUNT happens when the supervisor starts or adopts a run, and when a client asks for a
session (`rigger mcp` serving an interactive session, `rigger dash`). Mounting takes the
OWNER lock, opens the manifest, loads indexes and the resident graph, takes the project's
machine-scope slot and publishes its measured resident bytes there every 30 s alongside the
supervisor beat. UNMOUNT happens when no workload has existed for the idle window (default
10 min): the owner snapshots, releases the slot, closes every file and, if it is a
supervisor with no run, exits. A cold read never mounts.

Why a lifecycle at all: the alternative, a store that is open whenever anyone looks, is what
makes the largest project on a machine set the cost of every command on every project.
Tying memory to workload makes an idle project free by construction, not by tuning.

### 6.3 Segregation

The store handle is minted from one project root and derives every path from it; there is
no API that takes a path. Machine scope holds budgets, slots and the maintenance lock, never
a record, a snapshot or a segment. The owner's watches, maps and locks are all under its own
`.rigger/store/`. Two projects' supervisors share nothing but the slot directory, and a
slot is content-free. As the world authority records, the trust boundary is the OS user:
two projects under one uid are segregated by construction against accident, not against a
deliberately hostile same-uid process.

---

## 7. The resource model

Every resource the engine consumes has a measured quantity, a budget, an admission check
and a remedy the operator can read.

| Resource | Measured how | Budget (default) | On exceeding |
|---|---|---|---|
| resident memory, all mounted projects | each owner publishes its RSS delta since mount in its slot | 10% of `MemTotal` (`budget.toml`) | admission REFUSES a new mount, naming the projects mounted and their bytes; idle projects are unmounted first |
| resident memory, one project | same | 2 GB per project | mount refused with the remedy `rigger store compact` / `reset --runs` |
| mounted projects | slot count | 4 | refuse, name the mounts |
| open segment handles | per owner | 16, LRU | closed and reopened on demand |
| mapped graph bases | per client process (the dashboard serving many projects) | 4, LRU | unmapped and remapped on demand |
| disk under `.rigger/store/` | manifest sums segment and snapshot sizes | the world authority's per-project envelope | archive and compaction are scheduled before any new run starts; below the envelope's hard line a run start is refused |
| structural CPU and I/O | `maintenance.lock` | one at a time machine-wide | wait, never parallel |
| journal lock hold time | measured per append | 250 ms | an append exceeding it is recorded as an anomaly |

Idle unmount runs on a per-project timer; memory pressure runs on admission. When a mount
is refused, the supervisor that asked HOLDS its run with cause `resources` (spec 105's
hold), names the numbers in `rigger status`, and retries on the release cadence; nothing is
evicted from under a live workload to make room. A project that is mounted and idle is the
only thing ever unmounted to make room.

Why refuse rather than evict: the failure the design must never reproduce is the kernel
choosing the victim. A refused mount is a state the operator reads and the run resumes
from; an evicted live project is a crash by another name.

---

## 8. Worked example: two projects, one machine

Project A runs a nine-unit spec; its supervisor holds A's OWNER lock, its resident graph is
190 MB, its slot says so. An agent on A shells out `rigger emit DecisionMade`: the courier
takes A's JOURNAL lock, appends one record, releases; A's owner sees the active segment grow
and folds the record in-process. Project B is idle: no process, 8 GB of segments on disk,
zero memory. The operator runs `rigger peers` in B: a cold read maps B's base snapshot,
walks one neighborhood, exits; no mount, no slot. Then the operator starts a run on B: the
supervisor asks admission for B's expected resident size (its snapshot size plus indexes,
270 MB), the sum stays under the 6.2 GB budget, B mounts. A's run finishes; its segments
archive to `refs/rigger/archive/<run>`, its knowledge residue seals, ten minutes pass with
no workload, A snapshots and unmounts; A's slot is released and A's supervisor exits.
Later a third project C asks to mount at 5.9 GB of prior use: admission refuses, C's
supervisor holds with cause `resources` and `rigger status` on C reads
`HELD - resources - mounted: B 270 MB; budget 6.2 GB; C needs 5.9 GB; remedy: rigger store
compact in C`.

---

## 9. Delivery

Three specs, behind the two ports, after spec 105 has made the supervisor the resident
process and spec 106 has detached it, so the mount owner exists before the engine expects
one. Each spec leaves both backends selectable by config until the last removes the old one.

1. **Spec 109, the segmented log.** Segments, footers, manifest, journal lock, typed and
   boundary reads, computed revisions, tail subscription, torn-tail recovery, the existing
   port conformance suite (`eventstore::contract::assert_contract`) passing against it;
   `rigger store migrate` reads the SQL log through the existing port and writes segments,
   keeping the old file as a backup the reaper never deletes; the progress side-store becomes
   kind-2 records of the `progress` stream in the same journal, so a command opens one store
   handle per project root instead of three. Ends with the SQL log backend deleted.
2. **Spec 110, the graph as a queryable file.** The CSR base snapshot (nodes, edges,
   aliases, the applied-position set and the pending-proof table), delta files, the
   resident graph in the owner, the client mapping path behind the same `Projection` port,
   full rebuild from ledger plus tree plus knowledge with verify-on-mount and the
   PROVISIONAL label. Ends with the SQL graph backend deleted and the SQL dependency gone
   from the build; the `store` lane joins `core` as pure Rust.
3. **Spec 111, mounts and budgets.** Project identity in machine scope, the mount lifecycle
   with idle unmount, cold reads that never mount, the slot substrate with measured resident
   bytes, `budget.toml`, admission with refusal and the `resources` hold, the maintenance
   lock, and `rigger store status` naming every mounted project, its bytes and the budget.

Specs 101, 107 and 108 land first and are the migration path: 101 fixes the read classes
the footers serve, 107 makes the run the unit of archive and the graph rebuildable, 108
fixes what a knowledge residue contains.

---

## 10. Acceptance (measured)

1. **Rebuild determinism.** The graph rebuilt from segments, ledger and tree equals the
   projection folded incrementally, byte for byte in the snapshot format, on this project's
   store and on a synthetic store of two superseded runs.
2. **Cold read cost.** `rigger peers` on an unmounted project with a 73 MB snapshot maps
   under 4 MB of pages and allocates under 16 MB of heap, asserted by a counting allocator
   and `mincore`.
3. **Idle cost.** Ten minutes after a run finishes, no process holds any file under the
   project's `.rigger/store/` and the project's slot is released.
4. **Admission.** With the budget set to the sum of two mounted projects, a third mount is
   refused, its supervisor holds with cause `resources`, and `rigger status` prints the
   projects, bytes, budget and remedy.
5. **Torn tail.** A journal cut mid-record reopens to the last valid record; a batch is
   never half-present.
6. **Concurrent appends.** Sixty-four processes appending concurrently produce a journal with
   contiguous positions, every record checksummed, no interleaving; the port conformance
   suite passes unchanged.
7. **No revision holes.** After `reset --derived` and an archive, every stream's revisions
   are contiguous by construction and `rigger validate` has nothing to repair.
8. **No engine.** The lockfile carries no SQL, key-value or search-engine crate, and the
   `store` feature builds for the pure lane.
