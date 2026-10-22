# 122 - Graph folds bound their write-lock hold

**Goal:** the rebuild's fold passes (`fold_source`: the shadow fold and the tail) and the live fold of
a single ledger entry hold `graph.db`'s write lock for at most one bounded transaction - `FOLD_BATCH`
non-entry events, or one entry alone - with no extraction inside it, each transaction's cost linear
in the proof rows of the owed names, and criterion 5's two-file fold held under its bound; the
full-tree refold is reported (THE MEASUREMENT), not bounded; the multi-event live
append (`apply_batch` through `FoldingStore::append_and_fold`, `crates/rigger-grounder/src/ingest.rs:130-134`:
every run event through `EventStore::append`, a cold `rigger graph build`, the offline graph passes,
`rigger reset --runs`) keeps one transaction per slice, gaining the IMMEDIATE open and the once-per-name
proof settlement. Today a fold holds the
lock for minutes, behind which the 5 s busy timeout (`open_connection`,
`crates/rigger-store-sqlite/src/sqlite.rs:14`) is exhausted, for three causes. Extraction runs inside the transaction: `fold_source`
(`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:1482`) calls `reextract` inside
`in_transaction` (:1494-1498). A pass commits one transaction per read batch: on 2026-10-21 09:44
UTC a rebuild's tail folded positions 4467395 to 4467619 in ONE transaction that held the write
lock 43 min. Proof resolution is quadratic in a name's test references: at 15:26 UTC the
conductor's fold of `GenerationIngested` 4467971 (the `src/cli/mod.rs` re-ingest at a landing) held
the lock about 3.7 min, and a courier's `DecisionMade` 4467972 behind it waited out the busy timeout
and lost its fold. No busy timeout absorbs a lock held for minutes. MEASURED (2026-10-21, this
machine, at c745e589, through the production path `bytes_batch` then `Projector::apply_generation`
into a fresh `graph.db`, harness at `~/.cache/rigger/drafts/op/122-harness/`; criterion 5's timed
test reads main's 3dcf9a8c blobs of the same two files): extracting `src/cli/mod.rs` (12,360 lines,
4,915 events) takes 0.25 s; folding it takes 6.3 s into an empty graph and 32.1 s into a graph
already holding `tests/cli.rs` (29,469 lines), of which `resolve_proofs` (:4578) is 31.1 s - its
2,744 test references rescan 1,351,030 recorded proof rows, each with a `same_file_candidates` query
(:4156), because every test reference re-resolves every proof row its name already has. Folding
`tests/cli.rs` alone takes 43.7 s, 43.5 s of it in `resolve_proofs` (8,892 test references,
1,895,535 rows rescanned). Into a graph holding all 433 tracked `.rs` files the refold of
`src/cli/mod.rs` takes 223.2 s (the 3.7 min hold, reproduced), and the cold fold of the whole tree
takes 3,211 s against 11.3 s of extraction. Re-extraction is under 4 % of a generation's fold; proof
resolution is over 90 %. What happens to a fold that still loses the lock is unchanged by this spec.

## Design

**BASE, decided here.** Spec 122 lands on `rigger-run` at the rigger-run tip at launch (the commit
spec 120's unit c4 integrates). Citations are read at c745e589, and 72f7d95f holds the same
symbols; the symbol governs, the line is a pointer. `sqlite.rs` alone means
`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`.

**EXISTING MECHANISMS, named and joined.** This spec adds no lock file, no marker file, no
background process and no event type. It joins and changes:
- `in_transaction` (:1323), the one transaction frame, DEFERRED today (`conn.transaction()`,
  :1327). Its callers: `Projector::transact` (:648) for `Projector::prune` (:757),
  `Projector::migrate_project` (:884), `apply_batch` (:1773) and `apply_generation` (:1782);
  `prune_run_closure` (:1342); `fold_source` (:1494). CHANGED by criterion 3 (the IMMEDIATE open)
  and criterion 5 (proof settlement before its commit, with the signature
  `in_transaction(conn, project, body)`: its three call sites - `transact` for its four methods,
  `prune_run_closure` and `fold_source` - pass their project, and settlement with no owed name is a
  no-op). `REBUILD_STATE` (:165) and
  `DROP_REBUILD_STATE` (:183) open with a raw `BEGIN`: CHANGED by criterion 3 to `BEGIN IMMEDIATE`.
  `open_current` (:1606) already opens IMMEDIATE, and the in-place migrations under `schema`
  (:1619) run as autocommit statements: both unchanged.
- `fold_source` (:1482), the one in-order fold body of the rebuild's shadow fold and its tail:
  idempotent through the `applied` ledger, it folds each event in its own savepoint (:1496-1510),
  passes over an event whose payload the fold rejects (`check_fold_payload`, :1503) by recording it
  applied and propagates any other failure with its transaction rolled back (:1502-1505); today it
  commits one transaction per batch its source hands. CHANGED by criterion 3 (the transaction cut)
  and criterion 1 (extraction before the transaction). Its progress report (`RebuildProgress`,
  :1528) stays once per read batch.
- `apply_batch` (:1772), the multi-event live fold `FoldingStore::append_and_fold` (`ingest.rs:140`)
  hands its whole slice: one transaction per slice, as today; it gains the IMMEDIATE open and the
  settlement through `transact`.
- `REBUILD_BATCH` (`src/cli/setup.rs:874`) and `stream_past`'s `batch` (:208): read sizes, a memory
  bound, unchanged; never a transaction bound once criterion 3 lands.
- `relabel_owed` (`SCHEMA`, :96): JOINED as the owed-table idiom `proofs_owed` follows - a row per
  debt, deleted when paid. `relabel_owed_communities` (:2784) stays where it runs, at the end of
  `fold_new` (:1961) and `fold_entry` (:2026).
- `open_connection`'s `busy_timeout=5000`: UNCHANGED - it is shared with the event log.
- `emit_keyed_batch` (`crates/rigger-conductor/src/conductor.rs:3367`) extracts through
  `entry_of_batch` (`ingest.rs:857`) before its append, and `FoldingStore::append_entry_and_fold`
  (`ingest.rs:169`) hands `apply_generation` a batch it only moves out: UNCHANGED.
- `Projector::guarded` (:662), its mark on a failed fold (`mark_lost_fold`, :610) and
  `rebuild_owed_graph`'s repair (`src/cli/setup.rs:906`) are unchanged by this spec.

**EXTRACTION OUTSIDE THE WRITE TRANSACTION, decided here (criterion 1).** The live path already
extracts first: `entry_of_batch` extracts and hashes before the append and `fold_entry`
(`ingest.rs:216`) hands `apply_generation` a batch it only moves out. The rebuild path does not:
`fold_source` calls `reextract` inside `in_transaction` (:1494-1498), so a transaction that wrote
the entry's `applied` row holds the write lock through a git blob read and a parse. Since every entry
folds in a transaction of its own (ONE BATCHING RULE), `fold_source` decides, by a read with no
transaction open (the entry's `applied` row, and whether its generation is the identity's current
one, the two checks `fold_entry` makes at :1997-2004), whether the entry will ask its batch; only then
does it call `reextract`, before the entry's transaction opens, and the transaction folds the
computed batch through `fold_entry` exactly as the live path does. The computed batch lives in the
folding process's memory between extraction and write; a write that fails, or a crash, leaves the
position absent from `applied` and nothing half-written. `fold_entry`'s own checks stay in the
transaction. An entry another writer folded meanwhile folds as `AlreadyApplied`, or as `ReRecording`
when another writer installed its generation, and the computed batch is dropped. THE REVERSE RACE:
when the pre-read said no batch (a re-recording) and `fold_entry` asks its batch anyway (a live fold
installed another generation meanwhile), the batch function answers a distinct value - the variant
`Unread` of `BatchAnswer`, a graph-crate enum (`Batch` or `Unread`) the batch function `fold_entry`
takes returns, the live path's `EntryBatch` adapted into its `Batch` variant - on which the entry's body (criterion 1's) runs
`ROLLBACK TO fold_event`, so its `applied` row (`record_applied`, :1997, written before the batch is
asked, :2010) is undone with the rolled-back savepoint, and returns a typed non-error outcome: `fold_entry` returns
`EntryOutcome`, a graph-crate enum (`Folded(EntryFold)` or `Unread`), and `apply_generation` (:1781)
maps it back to `EntryFold` for the live path, so the domain's `EntryFold` and `EntryBatch`
(`crates/rigger-domain/src/contextgraph.rs:745`) stay unchanged; on it
`fold_source` skips the cursor update (:1523), so the entry's transaction, an entry being one alone
(ONE BATCHING RULE), ends holding no write, and `in_transaction`'s commit-on-`Ok` contract is
unchanged; `fold_source` matches on that outcome, never on an error text, re-reads with no
transaction open, extracts, and retries that entry ONCE; a second miss propagates as today's
failure (setup exits non-zero, the rerun resuming from the last commit). The entry's body is one
function `fold_source` calls with its batch function as an argument, so criterion 1's Unread tests
(Notes) call it directly with a stub, no `cfg(test)` hook. A `reextract` failure before the open
propagates as today, with no transaction open and the cursor at the last committed transaction;
`fold_source`'s doc (:1474-1476) drops its "with the batch rolled back" for that failure. The retry
then folds the
entry over the newer generation, as a fold of entries out of position order does today: `fold_entry`
compares the entry's generation with the identity's current one only (:2002), and no record carries
the position a current generation came from - `generations` holds `(project, identity, generation,
prior)` (`SCHEMA`, :60-63; written at :2439), and the positions `edge_assertions.source` and
`refs.position` carry exist only for a generation that asserted an edge or a reference - so no
later-position check is made without a new column, which this spec forbids. A pre-read that cannot parse the entry
(`GenerationIngested::parse`) does not extract: the entry folds in its own transaction with no batch,
`fold_entry` fails its parse (:2000), and `fold_source` passes it over and records it applied, as
today. This does NOT shorten the live hold of the 15:26 UTC incident; that hold is the fold's own SQL, criterion 5's.

**ONE BATCHING RULE, decided here (criterion 3).** `fold_source` commits a transaction after at most
`FOLD_BATCH` non-entry events (256, one constant in `sqlite.rs`), and cuts before AND after every
ledger entry of perception, so an entry always folds in a transaction of its own (that it opens after the
entry's extraction is criterion 1's); each transaction advances the cursor in itself - for the rebuild's shadow fold and the
rebuild's tail alike. A concurrent writer therefore waits at most one transaction: `FOLD_BATCH`
non-entry events, or one entry alone. A crash mid-pass resumes from the last committed transaction.
`REBUILD_BATCH` and `stream_past`'s `batch` stay read sizes; a read batch is folded in as many
transactions as the rule cuts it into, and its progress is reported once, after its last. THE SEAM:
the pass holds no transaction between read batches, so criterion 3's test source hands two read
batches, its writer commits between them, and the test checks the writer's commit position in
`applied` and the cursor's value read between the two batches, the concurrent-writer part; and the
in-batch part: inside one read batch of more than `FOLD_BATCH` events, the test's `reextract` for
the second entry reads the committed cursor and `max(position)` of `applied` from a second
connection, and both equal the position just before that entry (today both equal the read batch's
start).

**THE IMMEDIATE OPEN, decided here (criterion 3).** The transaction frame opens IMMEDIATE:
`in_transaction`, `REBUILD_STATE` and `DROP_REBUILD_STATE`, each with today's one busy-timeout wait
and no retry loop; in write-ahead logging the write lock is then held from the open to the commit. A
held lock past the busy timeout fails the transaction with today's error text and today's handling in
every caller: `guarded` for the live fold, propagation and setup's non-zero exit for the rebuild.
Why: IMMEDIATE takes the write lock at the open, so a frame meeting a held lock fails before its body
runs, and a body that reads before it writes (`migrate_project`'s `column_exists`, :890;
`prune_run_closure`'s gathered read, :1343) never meets BUSY_SNAPSHOT, which the busy timeout does
not absorb. The open and the cut are one mitigation: the IMMEDIATE open is what makes ONE BATCHING
RULE's wait bound hold, since without it a writer can fail with BUSY_SNAPSHOT mid-body instead of
waiting. The test that proves it is criterion 3's open test (Notes).

**PROOFS SETTLE ONCE PER TRANSACTION, decided here (criterion 5).** The three callers of
`resolve_proofs` - the retire path (:2721), the definition arm (:3228) and `fold_test_evidence`
(:4534) - stop resolving and instead owe the name: a row in `proofs_owed (name TEXT PRIMARY KEY)` in
`SCHEMA`. A projector folds one project, so the table carries no `project` column and joins neither
`PROJECT_KEYED_LEDGERS` (:143) nor `DROP_LEDGERS` (:126), and the doc at :139 stays true; it is empty
after every commit. Settlement belongs to the transaction frame: `in_transaction(conn, project,
body)` resolves each owed name once through `resolve_proofs` and deletes its row before its commit,
outside every per-event savepoint - once per `fold_source` transaction and once per live `transact`.
`resolve_proofs(tx, name, project)` (:4578) is kept as it is and is what the settlement calls. Where
a proof lands stays the pure function of the live definitions and references that `resolve_proofs`'
doc states. THE REVIVED PROOF: a re-ingest retires every definition its prior generation held after
the batch's first event (`fold`, :2396-2401), stripping the proof keys from the retired copy
(`definition_retired`, :2720), and later events of the same transaction revive the node from that
copy (`ensure_node`, :4682-4686); today the retire-time and revive-time `resolve_proofs` move each
row's target away and back, so `write_proof` restores the attributes. Settled once, the targets do
not move, so settlement resolves each owed name and runs `write_proof` (:4637) once on every live
target the rows hold after resolving (from `proofs`, no new table); a row `resolve_proofs` moves is
written by `resolve_proofs` itself at its old and new targets, so a definition retired and revived
within one transaction regains `proven_by` and `proof_evidence`. Per owed name this is r
resolutions, at most two writes per row `resolve_proofs` moves and at most r writes after it, so
the folded rows equal today's. THE EQUALITY ORACLE: a second connection splits the
fixture at every non-entry event and at every entry boundary, folding each part in its own
`in_transaction` (an entry's batch folds whole at one position, :2019-2024), so settlement runs after
every part, the finest interleaving the frame allows, and the one-transaction fold on the first connection is compared
against it on `projection_but_applied`; its fixture includes a re-folded generation that keeps a
proven definition, and the test asserts that definition's `proven_by` and `proof_evidence`; a retire and a revive
within one batch settle once on both connections, so that assertion, not the equality, is the one
guard on THE REVIVED PROOF. THE
COUNT TEST'S INSTRUMENT: a statement-trace callback
(rusqlite's `Connection::trace`) each connection of the test installs, counting into a static of the
test every execution of the statement text `leaf_definitions_sql(&admitted_sql(Admitted::LiveCallTargets),
true)` builds (:4029, run by `definitions_with_suffix`, :4091-4108). rusqlite 0.32's
`Connection::trace` hands the callback the SQL with its bound parameters replaced by their values
(the legacy `sqlite3_trace`), so the count compares each traced statement against the built text up
to its first bound parameter, a prefix match; in the fixture only `resolve_proofs` runs a statement
with that prefix, since its non-entry `EdgeInferred` test references go straight to
`fold_test_evidence` (:3251-3252); no production counter and no
`cfg(test)` hook. Per `resolve_proofs`' body, one call of it runs that text once for its `only`
lookup (:4579, no file) and once per proof row of the name for its same-file lookup
(`same_file_candidates`, :4602, :4162). The fixture folds k test references to one name, none of
them `fresh`, from one file, with NO non-test references, over a graph already holding r proof rows
of that name; `fold_test_evidence` records each reference's row before it resolves (:4523-4534).
Settled once per owed name, the transaction executes the text 1 + r + k times; the oracle, resolving
after every reference, executes it k(1 + r) + k(k + 1)/2 times; the test asserts both counts. The
two connections fold one after the other, and the test reads and resets the static after the
seeding fold and after each counted fold. The
graph crate's production `rusqlite` entry (`crates/rigger-graph-sqlite/Cargo.toml:32`, features
`bundled` and `backup`) does not enable `trace`; a target-gated `rusqlite` entry under the crate's
`[dev-dependencies]` enables it, never the production entry. THE TIMED TEST'S CLOCK: the timed
periphery test measures the CPU time of the thread that runs the fold (SQLite runs on that thread)
as the first field of `/proc/thread-self/schedstat` (nanoseconds on the CPU, no tick conversion),
read before and after the fold, failing loudly when the file is absent; `libc` is no direct
dependency of the graph crate or the workspace. Only Linux enforces the bound: the test is compiled
on Linux alone (`#[cfg(target_os = "linux")]`; CI's test job and the gate battery run there) and on
Linux never skips. Its fixture is the two paths at main's commit 3dcf9a8c, `src/cli/mod.rs` (616,482
bytes, 12,362 lines) and `tests/cli.rs` (1,396,899 bytes, 29,121 lines), read with `git show
3dcf9a8c:<path>` at test time; there is no committed copy and no reachability guard. The test fails
loudly when that history is absent; CI's test job checks out with `fetch-depth: 0`
(`.github/workflows/rust.yml:67`). It does not measure wall clock: gate batteries run two at a time
beside cargo builds on every core, so wall clock measures the machine's load, not the fold. Its bound
is 5 s of CPU. The Goal's 32.1 s is wall clock on an idle core over c745e589's blobs, not the
fixture's, so the pre-fix baseline on the fixture is measured by the timed test's own red run (the
test exists before the fix): the red run must exceed the 5 s bound, and the unit's hand-over reports
that red-run CPU figure beside the post-fix figure the same test measures. The bound is the
criterion's pass condition; the margin under it is reported, never required: the unit's hand-over
states the red-run figure, the post-fix figure and the floor (the same fold with the settlement
switched off), each as thread CPU on this fixture in the debug build the gates run. At the unit's
first attempt they were 78.1 s red, 2.36 s and 2.41 s post-fix, 1.80 s and 1.81 s floor, so the
bound stands at about 2.1 times the post-fix figure and the floor caps any attainable margin below
2.8 times on this fixture; a multiple of the post-fix figure is therefore no condition. A missed
bound escalates with the measured figures and never loosens the bound. THE MEASUREMENT (reported,
not bounded, and not a
test): criterion 5's unit reports in its hand-over the wall-clock refold of `src/cli/mod.rs` into a
graph holding the whole tree, through the production path (`rigger graph build` on a scratch
checkout, or the harness's `bytes_batch` then `Projector::apply_generation`), beside the Goal's
223.2 s. The figure carries no pass or fail threshold, and the busy-timeout outcome of the live fold
at full-tree scale is outside this spec.

**CONSTRAINTS WALK, decided here.**
- Empty: a source that hands nothing past the cursor opens no transaction and moves nothing; a
  transaction with no owed name settles nothing.
- Repeated fold of one position: `applied` makes it a no-op in every path; a name owed twice in one
  transaction is one `proofs_owed` row and one resolution.
- Crash between extraction and write: the position is absent from `applied` and the computed batch
  is lost with the process; the rebuild's next pass resumes from the last committed transaction and
  re-extracts it.
- Crash mid-tail or mid-shadow-fold: the cursor of the last committed transaction resumes it; a
  transaction is never half-written, since IMMEDIATE changes only when the lock is taken.
- Concurrent writer against a fold pass: each waits at most one transaction of the other -
  `FOLD_BATCH` non-entry events or one entry alone, extraction excluded once criterion 1 lands, proofs
  settled once per owed
  name - and takes the lock at its open; a wait past the busy timeout fails that transaction with
  today's error text, and nothing of it is written.
- The reverse race: an entry whose pre-read said no batch and whose transaction asks one answers
  `Unread`: its `applied` row is undone with the rolled-back savepoint and the cursor is not advanced,
  so nothing is written; it is re-read, extracted and retried once, and a second miss propagates,
  the rerun resuming from the last commit; the retried entry folds over a newer generation as an
  out-of-order fold does today.
- Retired and revived in one transaction: settlement rewrites the proof attributes of every live
  target the owed name's rows hold after resolving, so the revived definition keeps its proof.
- A malformed entry: a pre-read that cannot parse it extracts nothing; its own transaction folds it
  with no batch, `fold_entry` fails its parse, and `fold_source` passes it over and records it
  applied, as today.
- The seam between read batches: the pass holds no transaction there, so a concurrent writer's
  commit lands between two read batches deterministically; criterion 3's test commits there.
- Setup beside a live run: the shadow fold and the run-closure prune write private files only the
  rebuild opens (the shadow, `open_shadow`, and the pruned copy, `prune_a_copy`), so they meet no
  lock of the run's; the swap is SQLite's online backup, whose busy steps it retries
  (`run_to_completion`, :507), unchanged; the tail and the rebuild state's drop meet the run's lock
  for at most one of the run's transactions, and past the busy timeout setup exits non-zero with
  today's error, the rerun resuming from the last committed transaction; the run's folds meet the
  tail's lock for at most one tail transaction, and the tail meets a run's fold for its whole
  transaction - one entry alone, or one multi-event slice through `append_and_fold`, as today.
- DROPPED: a test reference or definition a later generation drops owes its name, so its proofs
  re-resolve before that transaction's commit (criterion 5's fixture drops one of each).
- Existing data: a store that predates the change folds as before - `proofs_owed` arrives through
  `SCHEMA`'s `CREATE TABLE IF NOT EXISTS`, which `pay_owed_rebuild`'s `Projector::open`
  (`src/cli/setup.rs:1035`; `open_current`, :1602-1604) runs on the live file before a tail-only
  rebuild folds it, since a swapped-in file records the current fold rule (`prune_a_copy`), the
  shadow branch running `schema` itself (:497); the table is empty after every commit, and the
  transaction cut and the IMMEDIATE open change no folded row, so `PROJECTION_VERSION` stays 3 and no rebuild is owed.

**UNIT ORDER AND LANDING ORDER, decided here.** 3, 1, 5. Criterion 3 cuts `fold_source`'s
transactions and opens them IMMEDIATE, and criterion 1 moves its extraction out; both edit
`fold_source`, so 3 lands first and 1's concurrent-writer test runs on 3's IMMEDIATE open. Criterion
5 is independent of both and lands last.

## Notes (non-criteria)

CRITERION 1'S UNREAD TESTS: `an_entry_whose_batch_is_unread_is_undone_and_retried_once` calls the
entry's body with a stub batch function answering `Unread` on the first ask and the batch on the
retry, and asserts the undone `applied` row, the unmoved cursor and the single retry;
`an_entry_unread_twice_fails_the_pass` answers `Unread` twice and asserts the propagated failure.
The barrier test stays the proof of extraction outside the transaction.

CRITERION 3'S OPEN TEST: a second connection holds the write lock; `in_transaction` on the first
fails at its open once the busy timeout passes, its body - one that records whether it ran - never
run, and nothing written.

FOLD-TIMING HARNESS: `~/.cache/rigger/drafts/op/122-harness/` (outside the repository) held the
measuring binary and its instrumented private copies of the graph crate; the Goal's figures come from
its `run4/` and `run5/` runs and the full-tree run. Criterion 5's unit reports its own CPU figures,
red and green, from its timed test.

LANES: criterion 5's timed periphery test needs `bytes_batch`, so it is gated on the `symbols`
feature; its count test is not, and runs in both lanes.

WHAT THE GATES CANNOT SEE: criterion 5's timed test runs under gate load and measures thread CPU,
which load barely moves, and its bound stands about 2.1 times over the post-fix figure (THE TIMED
TEST'S CLOCK); the full-tree refold of `src/cli/mod.rs` is no test, so
criterion 5's unit reports THE MEASUREMENT in its hand-over, and the adjudicator demands it.

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line; never the host vendor's name.
- A line citation names the symbol at c745e589; the symbol governs, the line is a pointer.
- No new event type, no new dependency, no change to the event store's schema, no new lock or marker
  file, no background process; enabling `rusqlite`'s `trace` feature on a target-gated `rusqlite`
  entry under the graph crate's `[dev-dependencies]` is criterion 5's one Cargo change and no new
  dependency; the production entry is unchanged.
- `open_connection`'s pragmas stay as they are.
- The fix lives in `crates/rigger-graph-sqlite`, its tests there and in `tests/`; nothing else
  changes behaviour.
- `rigger setup`'s user-visible lines stay as they are.
- Every lane `.rigger/gates/lanes.sh` runs green (both feature lanes, `--no-default-features`
  included; clippy -D warnings; test) and `cargo fmt --check` passes on the tree.
- The planner's batch order is UNIT ORDER's sequence 3, 1, 5, never checkbox position.

## Done when

- [ ] a test proves EXTRACTION RUNS OUTSIDE THE WRITE TRANSACTION: while `fold_source`'s `reextract` blocks on a barrier for a ledger entry, a second connection to the same `graph.db` commits a write.
  The folded rows, compared on the projection `projection_but_applied`, equal those of folding the entry with its batch in hand. This criterion OWNS the read-before-transaction check, the extraction's place in `fold_source`, the `Unread` answer (`BatchAnswer`, `EntryOutcome`) with its savepoint rollback, its one retry and the Unread tests; the transaction frame is criterion 3's, proof settlement criterion 5's, NOT this one's.
- [ ] a test proves A FOLD PASS COMMITS IN BOUNDED TRANSACTIONS: a `fold_source` tail over more than `FOLD_BATCH` events and two ledger entries lands a concurrent writer's commit between its two read batches (THE SEAM).
  Its cursor advances per transaction, and the `applied` ledger's positions, compared on the position projection, equal the log's, and the open test Notes names passes. This criterion OWNS `FOLD_BATCH`, the transaction cut and the IMMEDIATE open; extraction's place is criterion 1's, proof settlement criterion 5's, NOT this one's.
- [ ] a test proves PROOFS SETTLE ONCE PER TRANSACTION: a trace count of the definition lookup matches its stated formula, and a timed test folds `src/cli/mod.rs` over `tests/cli.rs` within 5 s of thread CPU.
  Its rows, compared on the projection `projection_but_applied`, equal THE EQUALITY ORACLE's per-event folds across a generation dropping a reference and a definition. This criterion OWNS `proofs_owed`, its settlement in the frame, the `trace` feature and both tests; the transaction frame's open is criterion 3's, extraction's place criterion 1's, NOT this one's.
