# 124 - Graph folds never mark on contention

**Goal:** every appended event is folded into `graph.db` at its append, or caught up by the next
call through the one in-order catch-up procedure, or passed over with its cause, from the `applied`
ledger and a catch-up cursor, never by an owed mark and a whole-log rebuild; and the whole-log rebuild is kept for a fold-rule
version change and for the one-time prefix violation found when seeding a file
that has no cursor. Today any fold error, a lost write lock included, writes the owed mark
`graph.db.owed` (`mark_lost_fold`,
`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs:609`, called by `Projector::guarded`, :662), after
which every fold is refused (`owed_by_file`, :356) until `rigger setup` rebuilds the graph from the
log (`Projector::rebuild`, :482). On 2026-10-21 that happened three times, each a sibling unit's
courier waiting out the 5 s busy timeout (`open_connection`,
`crates/rigger-store-sqlite/src/sqlite.rs:14`) behind a long fold: 05:32 UTC (a landing's
`GenerationIngested` at 4467360 against a lens courier's `ReviewFinding` at 4467361); 09:44 UTC (a
rebuild's tail folding positions 4467395 to 4467619 in ONE transaction that held the write lock 43
min, re-marked 8 min after the swap); 15:28:58 UTC (the conductor's fold of `GenerationIngested`
4467971, the `src/cli/mod.rs` re-ingest at a landing, held the lock about 3.7 min; `DecisionMade`
4467972 waited 5 s and marked the file, and the landing's remaining 20 entries were appended
unfolded). Each rebuild took 3.7 to 5.1 h over about 69,000 events
(`~/.cache/rigger/setup-owed-120a.log` to `setup-owed-120c.log`), so a 5 h rebuild bought minutes of
a current graph. Spec 122 collapses the hold itself; this spec removes the mark and its rebuild for
the lost fold that a hold of any length can still cause.

## Design

**BASE, decided here.** Spec 124 lands on `rigger-run` on spec 122's landed tree (895f8188) once the
binary is refreshed, so `fold_source` (:1501) is as 122 left it: its transactions open IMMEDIATE
through `in_transaction` (:1329), cut by `fold_transactions` (:1678) at `FOLD_BATCH` (:1673)
non-entry events or one entry alone (`fold_entry_alone`, :1606), each event under its savepoint
(`fold_in_savepoint`, :1555), an entry's batch extracted before its transaction opens
(`extracted_batch`, :1641), and proofs settled once per transaction in the frame (`settle_proofs`,
:4825); a held lock past the busy timeout fails a transaction with today's error text, and a
rebuild pass meeting one fails setup non-zero. Citations are read at 895f8188. The symbol governs,
the line is a pointer. `sqlite.rs` alone means `crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs`.

**EXISTING MECHANISMS, named and joined.** This spec adds no lock file, no marker file, no
background process and no event type. It joins and changes:
- `in_transaction` (:1329) and its IMMEDIATE open: CHANGED by criterion 4. Today it opens with
  `transaction_with_behavior(TransactionBehavior::Immediate)` and `map_err(be)` (:1334-1336), so a
  busy open reaches every caller as the domain's `Error(String)` (`contextgraph.rs:725`); its
  settlement of proofs before the commit (`settle_proofs`) is unchanged. Its open now returns the frame's own result type inside
  `crates/rigger-graph-sqlite`, `FrameError`, before `be` stringifies anything: the variant
  `Contended` carrying the SQLite code for busy or locked, and `Failed` carrying every other open
  error, and every body error, as today, and a third variant, `Recorded`, carrying a `passed_over` row's
  cause text, which the direct fold's body returns for its own position already applied with a row
  (criterion 4); the transaction rolls back with nothing written, and the variant reaches `guarded`
  through `transact` (:648) unchanged. The fold closure `guarded` runs returns this result, so
  `guarded` (:662, its arms :666-680) maps it in one match: `Ok`, FOLDED, the fold's `Ok` as today;
  `Recorded`, RECORDED, the domain's `Err(Error(cause))` with no mark and no suffix; `Contended` and
  `Failed`, LOST, the mark arm with its suffix on tree 4 and, from criterion 2, the contended-open
  wait and the no-mark rule. The domain's `Error`, `Fold` and `EntryFold` are unchanged. `catch_up`, the appender's calls, the direct fold's open and the
  seeding transaction classify CONTENTION on `Contended`; a rebuild pass converts it to the domain's `Error` with
  today's text and propagates it as spec 122's failure (setup non-zero). No port changes and the
  domain's `Error` is untouched.
- `fold_source` (:1501), the one in-order fold body: JOINED by criterion 4 - its cursor and its rules
  come from its caller. The rebuild's shadow fold and tail hand `rebuild_cursor` (:1700) and today's
  rules; `Projector::catch_up` hands `caught_up` and its pass-over.
- `Projector` (:329) with its one connection mutex `conn` (:330): JOINED by criterion 4 as the
  holder of the catch-up wiring and by criterion 2 as the holder of the ceiling; no second mutex is
  added.
- `Projector::guarded` (:662) and `mark_lost_fold` (:609): CHANGED by criterion 2 (the appender's
  calls under the ceiling; no fold writes the mark; the error names its cause).
- `misses_any` (:594), the read of the ledger against the live selection: SPLIT by criterion 4 into
  the any-miss read `owed_against` (:583) uses, which now also excludes positions above the highest
  applied one, positions above `caught_up` on a file carrying it, and missing positions younger
  than `CATCH_UP_HORIZON`, the age rule the seeding applies, as it already excludes those past a
  rebuild cursor (:596), so an old hole under applied positions at or below the cursor still owes a
  rebuild and a hole above the cursor or a young one is caught up by the loop, and the
  prefix-violation read the seeding uses, both run under the seeding's IMMEDIATE lock (THE CURSOR
  AND ITS SEEDING).
  Criterion 6 retires the any-miss read with `owed_against`.
- `owed_mark` (:343), `marked` (:348), `owed_by_file` (:356), `Projector::owed_because` (:571),
  `OWED_LOST_FOLD` and `REBUILD_OWED` (`crates/rigger-domain/src/contextgraph.rs:854`, :859): CHANGED
  by criterion 6.
- `open_shadow` and `REBUILD_STATE` (:1449, :167), the rebuild's own open of its shadow,
  `DROP_REBUILD_STATE` (:183), `rebuild_tail_owed` (:1686): KEPT as the rebuild's own state; the
  shadow's open also creates `caught_up` and the drop sets it (criterion 4, THE CURSOR AND ITS
  SEEDING).
- `RebuildLock` (:296) and `Projector::lock_rebuild` (:426): KEPT unchanged; `catch_up` never takes it.
- `Projector::rebuild` (:482) with its boolean `owed` argument, which `pay_owed_rebuild`
  (`src/cli/setup.rs:1027`) sets to `!causes.is_empty()` (:1048), and `REBUILD_BATCH` (:875): KEPT,
  but for THE DECIDE STEP criterion 4 gives the rebuild (THE REBUILD SHARES THE DECIDE STEP); the seeding's prefix-violation verdict adds `OWED_LOST_FOLD` to the printed causes
  only when it is absent, as `owed_against` does (`sqlite.rs:585`), and the rebuild still receives the boolean
  (criterion 4). The live-selection plan
  (`read_live_selection`, `crates/rigger-store-sqlite/src/eventstore/sqlite.rs:407`) stays the
  rebuild's alone. `rebuild_owed_graph` (`setup.rs:907`): CHANGED by criterion 4 (THE CURSOR AND ITS
  SEEDING, THE SETUP LOOP).
- `EventStore::read_all` (`crates/rigger-domain/src/eventstore.rs:729`), the port's read of the log
  past a position, unbounded (the sqlite backend collects every row past `from`,
  `eventstore/sqlite.rs:1280-1297`; the server-backed one drains its `read_all` stream,
  `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs:809-853`): EXTENDED by criterion 4 with a
  bounded sibling, `read_all_page(from, filter, max)`, the events past `from` that `filter` admits,
  at most `max` of them, in position order. The trait gives it NO default, so no store reads the
  whole remainder of a gap: every store on `catch_up`'s log path implements a bounded page read -
  the embedded store (`impl EventStore for Store`, `eventstore/sqlite.rs:1150`) with a `LIMIT` on
  its query, the server-backed store (`impl EventStore for Store`, `kurrentdb.rs:762`) with
  `ReadAllOptions::max_count` on a read filtered by the server (`all_filter`, `kurrentdb.rs:196`,
  the stream-prefix filter its subscription already uses), and the namespacing store (`Namespaced`,
  `crates/rigger-store-sqlite/src/eventstore/namespace.rs:92`) forwarding it under its scope as it
  forwards `read_all` (:114); `FoldingStore` (`ingest.rs:224`), off that path, forwards it as it
  forwards `read_all` (:249), and each test double implements it. `resolve_store` (`src/cli/mod.rs:751`) stays the one
  constructor of the log it reads.
- Setup's `reextract` closure with its lazy `entry_blobs` and THE ONE ROOT (`setup.rs:926-941`,
  :1009; `tree_root`, `src/cli/mod.rs:5133`): FACTORED by criterion 4 into the one open for write.
- The composition root's write opens - `open_graph` (`src/cli/mod.rs:444`), `StoreLocation::graph`
  (:1403), `migrate_identity_at`'s open (:1179), setup's opens (`setup.rs:1036`, :1066): CHANGED by
  criterion 4 to open through the one open for write, `open_graph_for_write` (WIRING).
- `FoldingStore` (`crates/rigger-grounder/src/ingest.rs:69`), `folding_into` (:88),
  `append_and_fold` (:140), `append_entry_and_fold` (:169), `say_fold_lost` (:203), `Fold::of` and
  `Fold::of_batch` (`contextgraph.rs:775`, :785), `mcpserver::emit_event`
  (`crates/rigger-dash/src/mcpserver.rs:823`), `fold_recorded_result` (`src/cli/run.rs:2922`) and the
  conductor's `Deps` (`crates/rigger-conductor/src/conductor.rs:1836`): UNCHANGED - each folds
  through `Projection::apply`, `apply_batch` or `apply_generation` (`sqlite.rs:1898`, :1917, :1926),
  which reach `guarded`; `folding_into` takes no new argument and `Deps` carries nothing new.
- `not_folded` (`contextgraph.rs:813`), the courier's "not folded into the context graph" line:
  unchanged in shape; it carries `guarded`'s cause text.
- The setup line `passed over {n} event(s) ...` (`setup.rs:1062`) and `graph_rebuild_owed_note`
  (`src/cli/mod.rs:471`), the validate check that reads the graph file's own records: CHANGED by
  criterion 4 (CATCH-UP LINES).
- `open_connection`'s `busy_timeout=5000`: UNCHANGED - it is shared with the event log.
- `hash_blob` (`crates/rigger-worktree-git/src/worktree.rs:1557`) and `BlobBatch::start`'s
  repository detection (`git rev-parse --git-dir`, :1604-1613): CHANGED by criterion 2 (THE BLOB IS
  WRITTEN), the detection factored into the one function both call.

**THE INVARIANT, decided here.** On a file carrying the cursor `caught_up`, `graph.db`'s `applied`
set is always a prefix of the project stream's live selection in position order: every live position
at or below `caught_up` is applied or recorded passed over, and none above it is applied (the
positions `plan_derived_prune` sheds lie outside the live selection and may be applied, a superseded
generation's derived rows having been folded live at their append; the invariant constrains live
positions only). Where the seeding
seeded below young applied positions, "none above it is applied" holds from the first call that
crosses them. A process running an older binary that holds its projector for its life, started
before the refresh (the MCP spawn server, `dashboard.rs:828`), folds directly above the cursor; it is
tolerated exactly as those young applied positions are: `catch_up`'s "applied, advance" arm crosses
its positions, and the invariant holds from the first call that crosses them. Such a process
answers `AlreadyApplied` for its own entry another call passed over, until the file's next ingest
records it again: the transition's cost. Such a process may
still write the mark, which until criterion 6 every call is INERT on and setup's rebuild pays, and
from criterion 6 setup removes. Spec 122 extracts a ledger entry's batch before its write transaction
opens (`extracted_batch`, `sqlite.rs:1641`), which widens the window in which two appenders fold out
of position order; THE INVARIANT covers that window, and the walk rows "Concurrent appenders out of
position order" and "Revert" pin it. So the graph is the in-order fold by construction - the same fold the rebuild and its
tail already run - and no fold arm needs an ordering rule. It becomes true when criterion 2 lands
(UNIT ORDER); every rule below serves it.

**ONE PROCEDURE, TWO CALLERS, decided here (criterion 4).** The procedure's live selection is the
project stream: an own event appended to any other stream (`FoldingStore::append`,
`crates/rigger-grounder/src/ingest.rs:229-238`, folds whatever stream it appends to through
`append_and_fold`, :140-153) is folded DIRECTLY, as the direct path folds it today, outside the
procedure and its cursor (THE APPENDER). There is one in-order fold procedure,
FOLD FORWARD FROM THE CURSOR, `Projector::catch_up`, an inherent method of the graph crate's
`Projector`; no port method is added to `Projection`, so every other `Projection` is untouched.
Criterion 4's procedure folds foreign positions only and has no notion of a caller's own position.
One call:
- reads, with no write transaction open, whether the file holds rebuild state, owes a rebuild for an
  older fold rule, or (until criterion 6) carries the mark (`owed_by_file` naming `OWED_LOST_FOLD`):
  any of them and the call does nothing (INERT, its outcome carrying the cause - rebuild state, an
  older fold rule, the mark, a cursor recorded beside another log or none (A LOG SWITCHED UNDER A
  STANDING GRAPH, CONSTRAINTS WALK) or no cursor - as a variant of its outcome type, no text); this
  read is only the fast path that avoids taking the lock, and the transaction below re-reads rebuild
  state, the older-rule debt and the cursor's log under the lock, rolling back and returning INERT
  if any stands, and a call that finds no `caught_up` under the lock rolls back and returns INERT
  with the cause variant "no cursor";
- reads one page past `caught_up` through `read_all_page` (at most `FOLD_BATCH` events, forwards,
  filtered to the project's stream prefix, then to the project stream), outside any transaction; an
  empty store page, and only that, is AT HEAD and opens no transaction, and a page the narrowing
  empties still advances the cursor;
- decides the page's positions in order by THE DECIDE STEP, one function of `fold_source`'s body
  that `catch_up`, the rebuild's shadow fold and its tail share (criterion 4), up to the page's first
  transaction cut (`fold_transactions`, `sqlite.rs:1678`: a run of at most `FOLD_BATCH` non-entries,
  or one ledger entry alone), so a call re-extracts at most one entry and discards none, the
  positions past the cut being decided by the next call's read: applied or recorded passed over,
  advance; a non-entry event, fold it from the bytes the read handed the call, which are the whole
  event; a foreign ledger entry, at any age and on either log, fold it re-extracted before its
  transaction (`extracted_batch`, BASE) through `resolve_entry`'s order (`ingest.rs:717`: the
  object database through the blob source, this process's tree, no bytes), else pass it over
  `resolved to nothing`; a payload the judge `check_fold_payload` rejects, passed over with `payload
  rejected`, and a re-extracted batch any of whose events the judge rejects, passed over with `fold
  failed`; CONTENTION and ENVIRONMENT, a fold arm's own `Err` over an accepted payload or batch
  included, as PASSED OVER WITH ITS CAUSE defines them;
- folds what it decided through `fold_source`'s body in ONE IMMEDIATE transaction ending at that cut,
  each position in its own savepoint, re-reading `caught_up` under the lock; a position the
  lock-time cursor has already passed takes the "applied or recorded passed over, advance" arm,
  which THE INVARIANT guarantees holds for it, so no read is dropped; a page the lock-time cursor has
  passed in full is read again ONCE from that cursor inside the call: the call rolls its IMMEDIATE
  transaction back, re-reads the page and makes the re-extraction decision outside any transaction,
  then opens its one transaction again, the rolled-back open being no transaction cut, so the call
  still commits once; and a second such page ends
  the call SKIPPED, having advanced nothing, an outrun SKIPPED carrying the same SKIPPED line
  (CATCH-UP LINES), the appender's ceiling and setup's loop governing it, so
  a call always ends in one of its named outcomes. It commits `position = max(position, ?)`, the
  last position the page read up to the cut, whether or not the narrowing kept it - a position of
  another stream is no position of the project stream, so THE INVARIANT is untouched - and the
  cursor never moves backward.
So one call is one transaction, and a caller that needs to pass its own position loops calls; a
call's read is one page, so a retry re-reads one page, never the gap. A call takes the projector's
connection mutex for its transaction and, briefly and outside any transaction, for the
re-extraction decision (`extracted_batch`, `sqlite.rs:1641`: `has_applied`, :2117, and
`is_re_recording`, :2216, read on its connection), and releases it for its page read and for the
extraction itself. Every ledger entry folds in order, each its own transaction cut, and a superseded
generation is retired by the next entry's fold as the live fold has always done. The selection plan
sheds only derived-index rows (superseded generations and exact-key duplicates of the pre-ledger
derived types, `plan_derived_prune`); no walk appends such rows since ledger entries replaced them,
so they lie below any seeded cursor, and one the procedure meets is folded through `fold_new` as the
live fold has always folded it. THE OWN ENTRY (criterion 2): the appender's loop passes `catch_up`
one more argument on every call - the position it has just appended together with the extracted
batch it already holds (`entry_of_batch` extracts before the append, `ingest.rs:857`), or none - and
criterion 2 adds its arm to the decide step: that one position folds from the given batch without
asking the re-extraction, a saving, not a correctness rule. The batch lives in the appender's call
frame for the loop's duration and nothing longer; when the appender gives up or exits it drops the
batch, and the position is foreign like any dead appender's. Non-entry appends pass none, since the
read's bytes are the whole event. THE CLOCKS: `CATCH_UP_HORIZON`, `FOLD_LOCK_CEILING` (10 min, one
constant in `sqlite.rs`) plus a margin of 60 s, is the age bound of a file WITHOUT a cursor alone -
the seeding's young rule and the any-miss read's young exclusion (THE CURSOR AND ITS SEEDING,
`misses_any`), where a missing position younger than it may be a direct fold still waiting out its
ceiling, and every direct fold's open after its first opens no later than its stamp plus the ceiling
plus one busy timeout (its ceiling re-checked before each retry's open, THE APPENDER); an event
already past its ceiling at its first open (a long extraction between `Event::new` and the append,
a suspend) still gets that open and is one the seeding may count old, which costs at most one
rebuild and never a lost position, its fold landing and the verdict being conservative - and it is read on the embedded log alone, where the store's stamp `recorded_at` on the
rows read and the folding host's wall clock `now` are one clock; on the server-backed log every
missing position counts as old, so the server's stamp (`recorded_at: rec.created`,
`kurrentdb.rs:265`) and its skew move nothing. Tree 4 (criterion 4) lands the two constants,
`FOLD_LOCK_CEILING` and `CATCH_UP_HORIZON`, which the seeding's young rule and the any-miss read
read there. The ceiling compares `now` against the appender's own event copy's stamp (THE
APPENDER). No monotonic clock is used. The ceiling the projector holds and its injector
`Projector::with_lock_ceiling(ceiling)`, which production never calls, land with criterion 2's
appender in `guarded`, their first reader; the horizon is the constant, a test of the age bound stamping `recorded_at` on the
events it hands (HORIZON IN TESTS). TWO CALLERS: the appender in `guarded` (criterion 2) and `rigger
setup`'s loop (THE SETUP LOOP); nothing else runs it.
THE REBUILD SHARES THE DECIDE STEP: the rebuild's shadow fold and tail decide every position by it,
so they pass over what it passes over and re-extract every ledger entry at any age. The rebuild has no own entry. A rebuild pass meeting a lock past the busy timeout fails setup
non-zero (spec 122's rule), where `catch_up` SKIPS. What changes under criterion 4: since
`Projector::rebuild` (:482) is an associated function that opens its own connections (:491) and
holds no projector, it becomes a method of the projector `pay_owed_rebuild` already opens on the
held file (`setup.rs:1036`), the held lock still its argument; `RebuildSource` (:195) and
`rebuild_cursor` (:1700), committed per transaction (BASE), are unchanged; the swap (:482-545)
carries the shadow, the tail resumes from the rebuild cursor to the log's head, and the drop sets
`caught_up` to the final rebuild cursor. A fold arm's own `Err` over an accepted payload
propagates as today (`fold_in_savepoint`, :1565), as every ENVIRONMENT failure does. It is safe: from the swap to the drop the live file carries
rebuild state, so every `catch_up` call is INERT and the tail meets no concurrent in-order folder.
THE REBUILD RUNS WITH NO RUN LIVE: these rules are the procedure's own correctness, not separate
mitigations: the rebuild's tail is the only in-order folder from the swap to the drop, so the
invariant holds only if no second rebuild starts beside a run (the refusal), a standing one is
finished by the one process that can (the exemption), the tail shares the projector's decide step
and cursor (the rebuild as a method), and a file exists for the cursor to be seeded in (cold-start
creation). Setup's rebuild joins the live-run refusal `rigger reset
--derived` already has (`refuse_derived_reset_if_live`, `src/cli/hygiene.rs:764`), reading the same
liveness through `live_writer_facts` (:824) and `LiveWriterFacts` (:783): no `rigger step` lock
held, no in-flight spawn of the current run whose liveness marker is inside its bound
(`rigger::liveness::live_spawns`, `crates/rigger-driver/src/liveness.rs:314`), and no driver
registration for the store with a heartbeat inside `DEFAULT_IDLE_MS` (`live_driver_registrations`,
:883); the probe holds the step lock for the rebuild (`LiveWriterProbe`, :812). When a rebuild is
owed and a run is live, setup prints the refusal CATCH-UP LINES names and exits non-zero, offering
no override (`--force-live` is `reset --derived`'s alone, `hygiene.rs:106-116`). Finishing
STANDING rebuild work is EXEMPT from the refusal and runs beside a live run: resuming a resumable
shadow, whether or not a cause is owed, and finishing a swapped-in tail no cause is owed beside
(`Projector::rebuild`'s `swap_unfinished` arm and its `rebuild_tail_owed` arm, `sqlite.rs:495-496`,
:522), safe there because every appender call is INERT on rebuild state. The refusal refuses a NEW
rebuild while a run is live: a pay that starts a fresh shadow and folds the log from position 0, as
`Projector::rebuild_starts_fresh` answers, which is a cause owed with no shadow standing, with or
without a swapped-in tail, and any standing shadow `open_shadow` discards as not resumable.
When no rebuild is owed, setup's loop runs, since it is safe
beside appenders. An entry appended while a rebuild runs, an operator ingest's (`rigger graph
build`) included, is re-extracted by the tail at any age (THE DECIDE STEP). WIRING: the projector
holds the catch-up wiring - the log and the re-extraction - installed by the composition root's one
open for write, `open_graph_for_write`, one function in `src/cli/mod.rs` that opens the `Projector`
(`Projector::open`, `sqlite.rs:405`) and installs both. The log is the project stream of the store
directory holding the graph file, built through `resolve_store` over the command's store selection
(`store_selection`, `src/cli/mod.rs:737`) and namespaced by the store's identity, opened at the
projector's first call and held for its life. The re-extraction is factored from setup's
`reextract` closure with its lazy `entry_blobs`: a factory the projector holds, building a fresh
re-extraction for each call that resolves THE ONE ROOT of that store directory and starts its blob
batch process only at the call's first entry, ending it with the call. Every projector the one open
for write makes holds it; in the light lane its re-extraction resolves no entry, so every foreign
entry is passed over as resolved to nothing there. A read-only open holds none and folds nothing; a
test projector without one folds an entry as one resolving to nothing. `open_graph` is the one open
plus its owed refusal; `StoreLocation::graph`, `migrate_identity_at` and setup open through the one
open; a read-only open (`standing_graph`, `src/cli/mod.rs:460`; `open_graph_to_read`, :484; the
dashboard's reads, `dashboard.rs:509`, :528, :552; `reset_menu`, `src/cli/hygiene.rs:210`) folds
nothing and needs none.

**THE BLOB IS WRITTEN, decided here (criterion 2).** `hash_blob`
(`crates/rigger-worktree-git/src/worktree.rs:1557`), the one hash function `entry_of_batch` hashes
an entry's bytes through before the append, writes the object it hashes, hardened on disk before
the call returns - `git -c core.fsync=loose-object hash-object -w --no-filters --stdin` - whenever
`root` is inside a repository, by the one detection `BlobBatch::start` applies (`git rev-parse
--git-dir`, `worktree.rs:1604-1613`; `entry_blobs`, `setup.rs:1009`), and hashes without writing
outside one, where no object database can be asked; a detection whose `git` cannot start, and a
write that fails, fail the hash, naming the command, the root and the cause, as every failure of it
does. This supersedes spec 107's rule that the hash writes no object (its doc,
`worktree.rs:1552-1556`). Every worktree of a repository shares its object database, and the store
belongs to the main worktree's top level, so one store is one object database on one host: any
same-host call re-extracts an entry, an uncommitted generation's included, from the object database
through `resolve_entry`'s first source with the appender's own bytes, and the appender's own call
then meets it applied (`AlreadyApplied`). LIFECYCLE: the object is an unreachable loose object until
the attempt checkpoint or the operator commits the same bytes, which gives the same id and makes it
reachable; git's own prune of unreachable loose objects (`gc.pruneExpire`, two weeks by default,
`git gc --auto` firing on rigger's own git commands) is its reaper. `--no-filters` means the id may
differ from the one a filtered `git add` later produces, which costs one more dangling object and
nothing else. `reset_derived` writes too, through the same one hash function (`cmd_reset` hands it
`hash_blob`, `src/cli/hygiene.rs:117-119`): its migration rewrites one row per identity, its latest
derived batch, into a ledger entry, so it writes at most one object per migrated identity, each the
bytes of a generation the tree held, most already reachable from a commit under the same id, where
the write is a no-op, the rest unreachable loose objects under the same reaper, and the migrated
entries become resolvable from the object database. An own entry another call passed over `resolved to nothing` answers not-folded with
that cause, and the file's next ingest records it again; on one host that happens only when no one
wrote the blob (an older binary's appender, or a hash outside a repository).

THE APPENDERS to
the project stream and their opens, which criterion 2's audit test pins:
- the conductor (`rigger run`, `rigger step`): `Deps::folding` (`conductor.rs:1865`), its event
  append `self.deps.folding().append(STREAM, ..)` (:3232) and `emit_keyed_batch`'s
  `append_entry_and_fold` (:3393), and `fresh_run_if_requested`
  (`run.rs:1453`), over `open_graph` (`run.rs:552`, :1445, :1814);
- `rigger graph build` (`record_batch` and `run_graph_pass`, `src/cli/graph.rs:716`, :836), `rigger
  critique` (`copy_findings`, `src/cli/critique.rs:283`) and `rigger reset --runs`
  (`close_landed_units`, `hygiene.rs:1040`), over `open_graph` (`graph.rs:612`, :829;
  `critique.rs:106`; `hygiene.rs:951`);
- `rigger emit` (`emit_event`, `graph.rs:1106`), `rigger result` (its append, `run.rs:2816`, :2830,
  then `fold_recorded_result`), `rigger resume-unit` (`run.rs:415`) and the MCP spawn server's
  `rigger_emit` (`mcpserver.rs:431`), over `StoreLocation::graph`;
- the identity-migration decision (`emit_event`, `src/cli/mod.rs:1125`), which `rigger setup` and
  other commands record through `migrate_identity_at`'s open; setup appends nothing else;
- `rigger reset --derived` opens through `open_graph` (`hygiene.rs:515`) for its owed refusal alone
  and appends nothing.
`rigger progress` (the separate progress store, `src/cli/observe.rs:195-198`), the canary
(`canary::STREAM`, `crates/rigger-conductor/src/canary_store.rs:352`) and the replay's isolated
store (`src/cli/mod.rs:2575`) append to no project stream.

**THE APPENDER AUDIT, decided here (criterion 2).** Its scope, appends naming the project stream's
constants, is unchanged, since no claim rests on an append to another stream not existing: such an
own event is folded directly (THE APPENDER). The audit test joins the tree's source-scanning
contract test, `tests/boundary_audit.rs`, beside its Rule 2 (`misplaced_constructors`, :613, and
`adapters_are_constructed_only_in_the_composition_root`, :645), and reads the source as Rule 2 does:
`production_sources` (`tests/common/repo.rs:224`, every `.rs` file under `src/` and each
`crates/*/src/`), each cut by `production_part` (:327), with `cfg_test_ranges`
(`tests/common/source_audit.rs:140`) and comment lines skipped, each hit named `file:item` through
`enclosing_fn_line` (:189). It scans for two token families:
- APPEND: a call `.append(`, `append_and_fold(`, `append_entry_and_fold(` or `emit_event(` whose
  argument list, read to its matching close parenthesis, names the project stream's constant:
  `conductor::STREAM` or `run::STREAM` (`crates/rigger-domain/src/run.rs:32`, re-exported at
  `conductor.rs:63`), or a bare `STREAM` in a file whose `use` of a
  `STREAM_MODULES` member (`run` and `conductor`, `tests/boundary_audit.rs`) names `STREAM`;
- OPEN: `Projector::open(`, `open_graph_for_write(`, `open_graph(` and `.graph()`
  (`StoreLocation::graph`, `src/cli/mod.rs:1403`).
The hits are compared as a set of `file:item` entries against this block's allow-list and nothing
else, so a new site fails the test until it is listed, and a listed site that vanished fails it too.
The allow-list is tree 4's set, the one criterion 2 lands on; each line is a pointer at 895f8188:
- APPEND, fourteen entries, each with the store its call reaches, the set criterion 4 leaves as it
  stands at 895f8188: `crates/rigger-conductor/src/conductor.rs:append_and_fold_batch` (:3232) and
  `:emit_keyed_batch` (:3393) (the conductor's folding store);
  `crates/rigger-store-sqlite/src/run_store.rs:record_rebase` (:43) and `:start_fresh` (:219) and
  `crates/rigger-store-sqlite/src/spawn_store.rs:one_position` (:65) and `:record_result_if_absent`
  (:137) (port-generic appenders, listed with the stores their callers hand them: the conductor's
  folding store, `fresh_run_if_requested`'s folding store, and `rigger result`'s raw store then
  `fold_recorded_result`); `src/cli/critique.rs:copy_findings` (:283), `src/cli/graph.rs:record_batch`
  (:717) and `:run_graph_pass` (:836) and `src/cli/hygiene.rs:close_landed_units` (:1040) (a folding
  store over `open_graph`); `src/cli/graph.rs:cmd_emit` (:1106) and
  `src/cli/mod.rs:migrate_project_identity` (:1125) (`emit_event` over `StoreLocation::graph` and
  `migrate_identity_at`'s open); `src/cli/run.rs:cmd_resume_unit` (:415) (a folding store over
  `StoreLocation::graph`); `src/cli/mod.rs:cmd_replay` (:2575) (the replay's isolated store, no
  project graph);
- OPEN, derived from WIRING: the one open for write, `src/cli/mod.rs:open_graph_for_write`, the one
  hit of `Projector::open(` that may write; its callers `src/cli/mod.rs:open_graph` (:444),
  `src/cli/mod.rs:graph` (`StoreLocation::graph`, :1403), `src/cli/mod.rs:migrate_identity_at`
  (:1179) and `src/cli/setup.rs:rebuild_owed_graph`
  (THE CURSOR AND ITS SEEDING), the hits of `open_graph_for_write(`; the read-only opens, the other
  hits of `Projector::open(`: `src/cli/mod.rs:standing_graph` (:464), `src/cli/mod.rs:open_graph_to_read`
  (:491), `src/cli/dashboard.rs:dash_read_graph` (:509), `:dash_read_whole_graph` (:528) and
  `:dash_read_calls` (:552), and `src/cli/hygiene.rs:reset_menu` (:210); the `open_graph(` callers
  `src/cli/critique.rs:cmd_critique` (:106), `src/cli/graph.rs:cmd_graph_build` (:612) and
  `:run_graph_pass` (:829), `src/cli/hygiene.rs:reset_derived` (:515) and `:reset_runs` (:951), and
  `src/cli/run.rs:cmd_step` (:552), `:run_cli` (:1445) and `:run_workflow` (:1814); the `.graph()`
  callers `src/cli/dashboard.rs:cmd_mcp` (:828), `src/cli/graph.rs:cmd_emit` (:1106) and
  `src/cli/run.rs:cmd_resume_unit` (:415) and `:fold_recorded_result` (:2930).
The MCP server's emit (`mcpserver.rs:431`) names its held stream, not the constant, and the audit
finds it through its open (`src/cli/dashboard.rs:cmd_mcp`).

**THE CURSOR AND ITS SEEDING, decided here (criterion 4).** The cursor is the table `caught_up (id
INTEGER PRIMARY KEY CHECK (id = 0), position INTEGER NOT NULL, store TEXT)`, seeded by `INSERT OR
IGNORE` only in the transaction that creates it, `store` the identity of the log the file folds (A
LOG SWITCHED UNDER A STANDING GRAPH, CONSTRAINTS WALK). Opens may create `SCHEMA`'s tables (`passed_over`). An open whose
`SCHEMA` statement (`schema`, `sqlite.rs:1764`, through `open_current`, :1747) meets the lock past
the busy timeout fails with today's text, as `be` gives it, on every open, read-only or write: it is
CONTENTION at the open and the one CONTENTION no caller waits out, since the statement takes the
lock once per store, at the first open after the upgrade, and `rigger setup` with no run live, the
refresh procedure's step after every binary change, is that open in practice; a courier whose open
so fails answers not-folded with that text, as any failed open does today, and the next open
creates the table. `caught_up`
stays off the general open path, for its seeding read; the rebuild's own open of its shadow is not
that path. State placement: the `applied` ledger is the only record of what folded and `caught_up`
the only record of how far the prefix reaches; an in-memory seen-set or a per-process high-water mark
is NOT an implementation of the cursor. SEEDING: the seeding's one IMMEDIATE transaction, which creates `caught_up`,
takes the lock first and runs the seeding's verdict under it, never before it: the prefix-violation
read of the ledger against the live selection and, on tree 4 onward, the owed check's any-miss read
it shares (`misses_any`), `pay_owed_rebuild`'s `owed_against` (`setup.rs:1036`) taking that read's
result rather than reading the ledger again; on a file already carrying `caught_up` the same
transaction reads no log rows, runs no any-miss read and writes nothing: at every setup, never
mid-run, setup reads the log's head once through the event-store port's bounded head read
(`EventStore::log_head`) and hands it to the seeding (`Projector::seed_cursor`), where a cursor
recorded beside another log than the one setup selects, or beside none, a head below `caught_up`,
or no head beside a cursor above 0, is the prefix-violation verdict (`Seeding::Ran { lost: true }`),
which setup routes as it routes the verdict of a file without `caught_up`, and a cursor recorded
beside the selected log with a head at or above it, or no head beside a cursor at 0, keeps the
cursor.
A direct fold in flight holds the lock
for its whole transaction, so a seeding that meets it is CONTENTION and SKIPs with the seeding line
(THE TWO WRITES MEETING A LOCK), issuing no verdict, and a position missing under the lock has no
writer holding a transaction. No live position missing below the highest applied one seeds the
cursor at the highest applied position, and an empty ledger beside an empty log seeds 0; a live
position missing below an applied one counts as a violation only when it is both missing under the
lock and older than `CATCH_UP_HORIZON` (its store stamp read from the rebuild's own source past it):
the age bound covers the window between an appender's append and its transaction's open, since a
direct fold re-checks its ceiling before each retry's open (THE APPENDER), its first open's
lateness the conservative case THE CLOCKS names, a younger one possibly a
direct fold still waiting out its ceiling, and the lock covers the transaction's
hold - on the embedded log alone (THE CLOCKS): on the
server-backed log every missing position counts as old, so a hole found at seeding is a prefix
violation and the first seeding of a file on a shared log rebuilds it once, as setup already does
for such a file - and with young missing positions alone the
cursor is seeded just below the lowest of them, `catch_up` advancing over the applied positions above
it; an old missing position, an empty ledger beside a non-empty log, or a highest applied position
above the log's head (an `events.db` replaced under a standing `graph.db`, found at every setup, never
mid-run: here on a file without `caught_up`, by the head comparison above on a file carrying it) is
a prefix violation, which
only a rebuild can repair in order, so no cursor is created and the seeding returns the
prefix-violation verdict, a verdict of this setup alone and no record of the file, which setup adds
to `pay_owed_rebuild`'s printed causes as `OWED_LOST_FOLD` only when it is absent, as `owed_against`
does (`sqlite.rs:585`), so `Projector::rebuild`
receives `owed` true, from criterion 4's tree onward. The transaction then writes the seed row
(`INSERT OR IGNORE`), or nothing on a violation, and commits, once per file; it is sound, since the
highest applied position cannot move while the lock is held. THE TWO WRITES MEETING A LOCK: contention past the busy timeout in
the seeding's IMMEDIATE transaction drops only its lock-bound reads - the seeding, the any-miss
read and the verdict. The owed check still pays what `owed_because` (`sqlite.rs:571`) reads off the
file with no lock (an older-rule debt; until criterion 6, the mark) under the live-run refusal,
exactly as it pays them when the seeding is not contended; then the call loop is skipped with the
seeding line CATCH-UP LINES names, the next setup seeding, and setup exits zero unless a rebuild
pass fails (spec 122's rule) or the live-run refusal fires; any other error of the transaction fails setup non-zero. The drop's transaction is a rebuild pass and keeps spec 122's rule for rebuild passes:
today's error text, setup exits non-zero, and the rerun resumes with rebuild state standing and
performs the drop. THE REBUILD: the rebuild's shadow open (`open_shadow` and `REBUILD_STATE`, :497-501, the drop at :538-540) creates
`caught_up` seeded at the rebuild cursor, so the swapped-in file carries it beside its rebuild state;
the drop's transaction sets it to the final rebuild cursor, creating it where a swap made by an
older binary left none. `rebuild_owed_graph` runs, in order: `Projector::forget_orphaned_mark` when no
`graph.db` stands; the rebuild lock (`setup.rs:913`); the identity migration
(`migrate_local_identity`, :917), before anything reads the log, since a scaffold may just have
minted the durable identity while the history sits under the legacy namespace (:914-916), so the
finish, the seeding and the pay read the log under the migrated identity, its emit INERT at once on
rebuild state (THE APPENDER); the open through the one open, which creates a missing file and
`SCHEMA`'s tables; the seeding of a file without `caught_up` that holds no rebuild state and owes no
older-rule rebuild, and the owed check's any-miss read, in the one IMMEDIATE transaction (SEEDING),
whose contention drops only those reads (THE TWO WRITES MEETING A LOCK);
the owed check, which pays `owed_because`'s causes whether or not the transaction was contended
(the seeding's verdict and, on a file without `caught_up`, the any-miss read,
which excludes positions above the highest applied one and missing positions younger than the
horizon, join its causes) and the rebuild it owes, or the finishing of a
standing rebuild state (THE REBUILD RUNS WITH NO RUN LIVE); then the call loop; last, from criterion 6, the removal of a standing `.owed` mark whenever the
file carries `caught_up` at that step, whether or not the seeding's transaction was contended, so a marked file this setup seeds loses its mark in this run. COLD START: today
`rebuild_owed_graph` returns before doing anything when no `graph.db` stands (`setup.rs:909-912`) and
leaves the file to the first fold's open, whose holes the next setup rebuilds; now setup creates the
file in that same run, seeding 0 beside an empty log and rebuilding it beside a non-empty one.
Read-only surfaces still never create the file (`standing_graph`). A file a fold creates outside
setup, and a store that predates the change, carry no `caught_up` until the next `rigger setup` seeds
them; the refresh-and-setup that follows each landed spec does.

**PASSED OVER WITH ITS CAUSE, decided here (criterion 4).** THREE FAILURE CLASSES, decided by their
source, never by age. A fold arm's error reaches `fold_source`'s body already as the domain's
`Error(pub String)` (`crates/rigger-domain/src/contextgraph.rs:725`), since `be` (:1893) stringifies
every error it is handed, so the class is read where each source raises it:
- CONTENTION: a `rusqlite::Error` whose SQLite code is busy or locked, read as `FrameError::Contended`
  at the IMMEDIATE open before `be` stringifies it - the one statement that can meet the lock, since
  the open takes it for the whole transaction (BASE) - by `catch_up`, the appender's calls, the direct
  fold's open and the seeding transaction; the call SKIPS, nothing written, the cursor still; an
  open's `SCHEMA` statement meeting the lock is CONTENTION too and is waited out by no caller (THE
  CURSOR AND ITS SEEDING);
- CONTENT, only: a failing position whose payload, or for a ledger entry any event of its batch,
  the deterministic judge `check_fold_payload` (`contextgraph/mod.rs:83`) rejects - whether the
  batch was re-extracted for a foreign entry or given by its own appender - and an entry resolving
  to nothing: passed over with its cause, so the prefix moves on and setup's line and the validate
  note (CATCH-UP LINES) carry the cause. Today's rebuild judges only an event's own payload
  (`fold_in_savepoint`, `sqlite.rs:1555`, the judge at :1565) and propagates a rejected batch event
  (`fold_source`'s doc, :1489-1491); THE DECIDE STEP extends the judge to the batch, for every
  caller;
- ENVIRONMENT, what remains: the open's `FrameError::Failed`, a failure of the re-extraction's blob
  source (`resolve_entry`'s `Err`, which only the blob source raises, `ingest.rs:733`: a blob batch
  process that dies or fails its I/O), a statement outside any fold arm (the cursor write, the
  `applied` row, a pass-over's own row), and a fold arm's own `Err` over a payload or batch the
  judge accepted, propagated as today (`fold_in_savepoint`, :1565) - the call rolls back whole and
  stops (FAILED) with the error text, never passing a position over, the cursor unchanged, and every
  later call retries the position. It recovers by itself when the environment heals (disk, I/O,
  memory); one that repeats over an accepted payload is, by the judge's contract
  (`check_fold_payload` is the one judge of what the fold deterministically rejects), a defect of
  the fold or of that contract, fixed at root by moving the check into the judge, and recovers when
  the fixed binary runs. Its reports are setup's loop stopping FAILED non-zero with the error text
  (CATCH-UP LINES) and the courier's not-folded line (THE APPENDER). A blob-source `Err` stays
  ENVIRONMENT and never falls through to the tree (`resolve_entry`, `ingest.rs:717`: a blob source
  that fails fails the resolution), since a fall-through could pass a live entry over `resolved to
  nothing` on a transient death of the batch process: every call builds a fresh re-extraction
  (WIRING's factory), so a batch process that died is restarted by the next call, and a corrupt
  object that kills it on every call is repaired as the batch process's own error text names,
  restoring or removing the object (`BlobBatch::blob`, `worktree.rs:1638`), which setup's FAILED
  stop and the courier's not-folded line carry.
Only the open's busy or locked code, read as `FrameError::Contended`, decides CONTENTION; no code
inside a fold arm and no message text decides a class, and `FrameError`'s `Recorded` is
no failure class but the own-position answer (EXISTING MECHANISMS).
CONTENT failures, and entries resolving to nothing, are the only positions recorded passed over,
whenever met, so the prefix moves on: the position's `applied` row and a row in `passed_over (position INTEGER PRIMARY KEY, cause
TEXT NOT NULL)` in `SCHEMA` are written in one transaction, the cause being the class (`payload
rejected` for the event's own payload the judge rejects, `fold failed` for a batch it rejects) and
the error text, or the cause `resolved to nothing`. CONTENTION in a call
is waited out by the appender under its ceiling and retried by setup's rerun; ENVIRONMENT at the
appender's own ledger entry is retried by that appender under its ceiling, keeping its batch, as
SKIPPED is (THE APPENDER), and ENVIRONMENT at a foreign position or
an own non-entry returns at once and is retried by every later call, setup's loop included;
CONTENT is final. The
rebuild's payload rejects and `catch_up`'s write the same row. A
whole-log rebuild under a later fold rule retries a passed-over position, whose effect returns only
where that rule folds its content; a passed-over entry's identity is also recorded again at its file's next
ingest, since `entry_of_batch` reads the graph's held generation on every batch (`ingest.rs:887`).
Passed-over positions are reported through the setup line and the validate note CATCH-UP LINES
names; `identities_behind_their_recording` (`setup.rs:1080`) reads generations, not the ledger, so
it is not the one. An entry the procedure resolves to nothing - no source extracts to its
generation, a hash mismatch included (`resolve_entry`, `ingest.rs:717`, answering `Ok(None)`) -
folds nothing and is recorded passed over with cause `resolved to nothing` in the same `passed_over`
row, where today the rebuild records it applied; an appender whose own entry another call so passed
over answers not-folded with that cause (THE APPENDER), and the file's next ingest records the
generation again. A re-extracted entry's bytes come from the object database, which holds every
blob a same-host appender hashed (THE BLOB IS WRITTEN), else from the tree.

**THE SETUP LOOP, decided here (criterion 4).** `rigger setup` loops calls on its own projector,
passing no own entry, while a call advances the cursor, and stops at the first that advances
nothing; a call whose read was dropped re-reads within itself, so every call ends in a named
outcome: AT HEAD is success; the loop is skipped with the seeding line when the seeding's
transaction was contended, after the owed check paid `owed_because`'s causes (THE TWO WRITES
MEETING A LOCK); INERT ends the loop with exit zero,
the next setup paying what the file owes; SKIPPED reports the skip and exits zero; FAILED reports the
position and the error and exits non-zero. After the loop setup prints the passed-over line once,
reporting the `passed_over` rows the file holds at the end of setup, by cause, read from the table,
when their total is above zero (CATCH-UP LINES). The live-run refusal of an owed rebuild is a distinct line
and exits non-zero.

**THE APPENDER, decided here (criterion 2).** On every fold `guarded` first reads `caught_up` with
no transaction open, and that read picks the path and the suffix; an own event whose stream is not
the project stream takes the direct fold whatever the read finds, outside the procedure and its
cursor, keeping the direct fold's contended-open wait and the no-mark rule, the call path and its
success test governing project-stream positions only, and THE INVARIANT, which constrains live
positions only, untouched by its applied row: present, the call path (the loop
of `catch_up` calls below); absent, the direct fold. For a project-stream event the direct fold's IMMEDIATE transaction re-reads
`caught_up` under the lock and, finding it (a seeding landed between the two reads), ends unwritten
and takes the call path; an event for another stream folds in that same direct transaction whatever
either read finds, and the success test does not apply to it, its answer being the direct fold's,
with no suffix on its not-folded line (CAUSE TEXTS), since no later fold re-derives it; an own
position's `passed_over` answer, on either path, also carries no suffix, matching CAUSE TEXTS, since
no later fold catches it up and the position is already in `applied`. The first
open of a direct fold ALWAYS proceeds; the ceiling check applies only to a retry after
`FrameError::Contended`, made after the retry acquires the projector's connection mutex and right
before its IMMEDIATE open, past it returning not-folded with the write-lock text and the direct
suffix, so every open after the first opens no later than its stamp plus the ceiling plus one busy
timeout, which `CATCH_UP_HORIZON`'s 60 s margin covers; the first open's lateness is the
conservative case THE CLOCKS names. A `FrameError::Contended` at the direct fold's open is CONTENTION and is
treated exactly as the call path's SKIPPED: waited out under `FOLD_LOCK_CEILING`, one busy-timeout
interval with the connection mutex released, then the fold retried from the first read, so the
seeded or unseeded decision is re-made each time; past the ceiling it returns the write-lock text
with the direct suffix. So a direct fold meeting a lock past the busy timeout no longer fails at
once, the Goal's rule on the unseeded path too, and criterion 2's checkbox and the walk line "an
appender call meeting setup's waits and retries" hold on both paths. On a projector holding the catch-up wiring and a file carrying
`caught_up`, `guarded` refuses at once as today when the file owes its rebuild
(`REBUILD_OWED`); else it runs `catch_up` in calls under `FOLD_LOCK_CEILING`, passing its own entry
(THE OWN ENTRY: for `apply_generation`, the entry's position and the batch it is handed, moved out
once; for `apply` and `apply_batch`, none). The ceiling bounds the appender's total wall time:
it is checked after every call, advancing or not, against `now - recorded_at` of the appender's own
lowest appended position (its event copy's stamp, set at the copy's construction, `Event::new`,
`eventstore.rs:121-130`, on the folding host's clock; for `rigger result` the copy is built after
the append, `fold_recorded_result`, `src/cli/run.rs:2927-2930`, through `res.to_event()` with only
`position` set, so there the stamp is later than the store's by the append's duration, which
lengthens the wait by that much and never shortens it). A call that advanced nothing is followed by
one busy-timeout interval with the projector's connection mutex (`transact`, `sqlite.rs:648-649`,
which `whole`, `subgraph` and `current_generation` also take) released; a call that advanced is
followed by the next call at once, with no interval. Past the ceiling the appender returns
not-folded: after a call that advanced nothing, with that call's CAUSE TEXT (the write-lock text
after SKIPPED, the INERT text, the last call's error after FAILED at its own entry); after a call
that advanced, with the catch-up text and the call-run suffix, true there, since the chain advanced
and the next fold or setup's loop reaches the position. After every call, whatever
its outcome (AT HEAD and SKIPPED included), and before the ceiling, the appender's success test is
one read of its own outside any transaction: `caught_up` and its own positions' `passed_over` rows,
read together in one statement; it answers folded when its last position is at or below the cursor
and no own position has a row, so a call that folds the appender's last own position ends the loop
folded whatever the clock reads, and not-folded with the first row's cause when a row exists; the
ceiling returns only an appender still short of the cursor. That read, not the call's locked read,
is the source of the not-folded cause: a row is written in the transaction that moves the cursor
past its position, so a read that sees the cursor past it sees the row. A folded entry's outcome is
the one its call folded it with, or `AlreadyApplied` when another process folded it first. Any own
position the success read finds in `passed_over` - a non-entry payload the judge
rejects under `apply` or `apply_batch`, or its own batch the judge rejects under
`apply_generation` (`payload rejected`, `fold failed`), or its own
entry another call passed over `resolved to nothing` (THE BLOB IS WRITTEN) - makes it return
its not-folded error with that row's cause text, without a retry, whatever its fold form; a batch is
folded only when every own position folded. The courier's not-folded line (`not_folded`, `contextgraph.rs:813`, through
`fold_clause`, :819) carries that text unchanged, `Fold::of`, `Fold::of_batch` and `Fold::settle`
(:775, :785, :798) taking the error's text as it stands. INERT on rebuild state is waited out under the ceiling, keeping
its batch, exactly as SKIPPED is: one busy-timeout interval with the connection mutex released, then
another call, the fast-path read taking no lock, so the wait never contends with the tail; its first
call after the drop folds from the cursor the drop set, its own entry from its batch through the
own-entry arm. INERT on an older-rule debt, on a cursor recorded beside another log or none, or,
until criterion 6, on the mark returns at once, since only setup repairs those; INERT on rebuild state returns at once too, with
the INERT text and the call-run suffix, when the appender's own process holds that file's rebuild
lock, since only that process's tail folds the event and it runs next, joining that arm. The tell is
the `RebuildLock` `Projector::lock_rebuild` (`sqlite.rs:426`) returns: `migrate_local_identity`
(`src/cli/mod.rs:1144`) and `migrate_identity_at` (:1161) take it as an `Option<&RebuildLock>`,
setup's `rebuild_owed_graph` passing `Some(&held)` (`setup.rs:913`, :917) and every other caller
(`critique.rs:101`, `eval.rs:217`, :286, `run.rs:537`, :1434, :1799, `hygiene.rs:38`, :61) `None`,
and `migrate_identity_at`, opening through the one open for write, records on that projector that
its process holds the lock when `RebuildLock::path` (`sqlite.rs:305`) names the projector's file; no other
open records it; INERT with no cursor takes the direct-fold
path, as on a file without `caught_up`. The ceiling bounds every wait: SKIPPED (CONTENTION),
INERT on rebuild state, and FAILED (ENVIRONMENT) whose failing position is the appender's OWN
LEDGER ENTRY are waited out under it, the last keeping its batch, so a transient failure recovers in
seconds; FAILED at a foreign position or
at an own non-entry, a fold arm's own `Err` included, returns at once with the error text and the
call-run suffix, with no retry of its own, since a non-entry folds from the read's bytes at any age
and nobody waits on it: every later call retries the position and reaches it once the cause clears,
and setup's loop stopping FAILED non-zero with the error text is the report of one that persists.
Past the ceiling it returns
and drops its batch, the entry becoming foreign like any dead appender's. Criterion 2's appender
alone maps INERT's cause variant to CAUSE TEXTS. The emit
caller's visible wait is bounded by the ceiling plus one call; the attempt count is process-local
and that is correct, since the lock and the cursor decide who folds. On a projector without the
wiring or a file without `caught_up`, `guarded` folds its events directly in one transaction as
today, except that a direct fold meeting its own position already applied answers not-folded with
that position's `passed_over` cause when a row exists, a recorded outcome, `FrameError::Recorded`,
which `guarded` maps to the domain's `Err(Error(cause))` with no mark and no suffix (EXISTING
MECHANISMS, `in_transaction`) (LANDING ORDER, tree 4; that answer is criterion 4's, and this criterion owns only the direct
fold's contended-open wait and no-mark rule). On both paths `guarded` no longer writes the mark for any failure; its error names its cause
(CAUSE TEXTS) and ends with its path's suffix: a call-run fold with ` - the next fold through the
folding store catches it up`, a direct fold with ` - the next rigger setup finds the event missing
from graph.db and folds it`, true whether setup's loop catches the gap up after seeding or the
rebuild pays a prefix violation; the courier's line (`not_folded`, through `say_fold_lost` or
`Fold::of`'s reason) carries that text unchanged in shape.

**THE MARK, decided here (criterion 6).** `owed_by_file` no longer reads the mark, so a marked file
folds and answers; the any-miss read and `owed_against` are retired; `OWED_LOST_FOLD` keeps its text,
its doc (`contextgraph.rs:852-853`) is rewritten to the seeding verdict's meaning, the cause setup
names when the seeding reports a prefix violation (that route is criterion 4's), and it is dropped
from `REBUILD_OWED`'s composition (:859), which names the older fold rule alone; `catch_up`'s mark arm is lifted; and `rigger
setup` removes an existing `.owed` mark whenever the file carries `caught_up` at that step, whether
or not the seeding's transaction was contended, as the last ordered step
of `rebuild_owed_graph` (THE CURSOR AND ITS SEEDING), so in the same run that seeds it, and the rebuild's swap
keeps removing a standing one (`Projector::rebuild`, :516-517). The mark's
format (an empty file read by `marked`) is unchanged, so no migration is owed. `PROJECTION_VERSION`
stays 3: `caught_up` arrives through the seeding and `passed_over` through `SCHEMA`'s `CREATE TABLE
IF NOT EXISTS`, so the upgrade bumps no fold rule.

**CONSTRAINTS WALK, decided here.**
- Empty: a call with nothing past the cursor opens no transaction and moves nothing; an empty
  ledger beside an empty log seeds 0.
- A log replaced under a standing graph: at every setup, never mid-run, an `events.db` whose head is
  below `graph.db`'s highest applied position, or below its `caught_up` cursor on a file carrying
  one, is a prefix violation at the seeding, so the rebuild pays it, its shadow seeding the cursor
  anew, never a cursor above the head that later appends would sit at or below unread.
- A log switched under a standing graph: the store arm is picked on every command (flag,
  `KURRENTDB_CONN`, the secret file or config), so the log a file folded can change under it with a
  head at or above its cursor. Beside its position, `caught_up` records the identity of the log the
  file folds, the store selection: its backend kind and locator, the embedded store's path or, for
  the server-backed store, a digest of its connection target (the registry's FNV-1a, stable across
  toolchains), never the connection string, which may carry credentials. The seeding and the
  rebuild's shadow open and drop write it with the position. Both reads of the cursor compare it
  with the selected store's through one comparison, reading no log row: the seeding at every setup,
  and every catch-up call, at its first read and again at its cut's read under the write lock. A
  cursor recorded beside another log, or beside none (a `caught_up` table that predates the column
  reads as none, never an error), is a prefix violation at the seeding, so the rebuild pays it from
  the selected log, its shadow seeding the cursor and the identity anew; a catch-up meeting it folds
  nothing and is INERT on it, so the file is never folded forward from that cursor and an appender's
  calls answer the lost fold's refusal (`REBUILD_OWED_FOR_LOST_FOLD`, naming `OWED_LOST_FOLD`) at
  once, before their success test, which would read that log's prefix as the selected one's, until
  the next setup's seeding pays the rebuild. A standing shadow
  whose cursor was recorded beside another log, or beside none, is not resumed but replaced by a
  fresh one. A hand-replaced `events.db` at the same path holding a
  longer foreign log is outside what setup can see, since its identity is that path, and no reset
  verb replaces `events.db` (`src/cli/hygiene.rs`).
- Repeated fold of one position: `applied` makes it a no-op in every path; an appender whose own
  entry another process folded first meets it applied, and its batch is dropped.
- Revert (an identity's later generation, one returning to an earlier content included): its entry
  folds in order as its own transaction and retires the generation before it, as the live fold does;
  a gap holding several generations of one identity costs one fold per generation, which buys the
  invariant with no special case.
- Concurrent appenders out of position order: the later one's call folds the earlier one's non-entry
  position from the read and an earlier ledger entry re-extracted from its blob in the object
  database (THE BLOB IS WRITTEN), then its own, in order; the earlier one's next call meets them
  applied (`AlreadyApplied`). Two appenders never fold out of order.
- Two calls on one cursor: they read from the same cursor and commit in any order; the
  `max(position, ?)` write and the in-transaction drop of a read behind the cursor keep it monotone,
  and `applied` makes any overlap a no-op.
- Two processes on one host: each folds every position in order, its own entry from its own batch
  and every other entry from its blob in the shared object database; whichever commits first, the
  other meets it applied. A conductor in a unit worktree and the MCP server are such a pair: the
  worktree's uncommitted blob is in the object database the main worktree's re-extraction reads.
- Several hosts sharing one server-backed log, each with its own `graph.db`: each host's file folds
  every other host's position as foreign - a non-entry from the read, a ledger entry re-extracted
  from this host's object database or tree when its blob is present, else passed over `resolved to
  nothing` - so no call waits on another host; the live-run refusal reads this host's liveness
  alone, which is enough, since the rebuild waits on no entry.
- An appender dies between its append and its fold: the next call on any process of the host folds
  the entry from its blob; on another host it is passed over `resolved to nothing`.
- Appender against setup: setup's calls and the appender's share the one lock; a setup call meeting
  the appender's lock skips with exit zero, the seeding included, which then issues no verdict, and an appender call meeting setup's waits and retries.
- Appender against the rebuild: a NEW rebuild runs with no run live (THE REBUILD RUNS WITH NO RUN
  LIVE), so its appender is a courier outside a run, and the finishing of a standing state beside a
  live run meets the run's appenders the same way; an event appended during the shadow fold folds as
  usual into the live file, and the swap's file has it re-read from the log by the tail, an entry
  re-extracted from its blob; from the swap to the drop every call is INERT, advancing nothing, and
  it waits under the ceiling, checked after every call, a call that passed the fast-path read before the swap finding rebuild state under its
  lock after it; a tail longer than the ceiling makes it return, and the next call folds its event.
- Owed rebuild beside a live run: on a file owing an older-rule rebuild (and, until criterion 6, a
  marked one) `guarded` refuses every fold with `REBUILD_OWED`, and setup refuses a NEW rebuild of
  it while the run is live, so the one route is to stop the driver, run `rigger setup` and relaunch by
  adoption; the state is reached only by a fold-rule change, never by contention. On a file setup
  found in prefix violation while a run is live, the appenders keep folding directly, unrefused,
  until a setup with no run live rebuilds it; no file record is written for the violation.
- An operator ingest beside setup's rebuild: `rigger graph build` is no run, so the rebuild runs; its
  entries are re-extracted by the shadow fold or the tail from their blobs and the operator's tree. A
  file swapped in by an older binary carries no `caught_up` in that window, so its appenders fold
  directly as today, the tail's per-position guard meeting them exactly once. A rebuild pass meeting
  an appender's lock past the busy timeout fails with today's error and setup exits non-zero, never
  marking anything.
- A dead appender, or a process exiting with its batch: its batch dies with it and its position is
  foreign; the next call folds a non-entry from the read and an entry re-extracted from its blob (or
  passes it over on a CONTENT failure, or stops FAILED on an ENVIRONMENT one).
- The MCP server's long life and a courier's short one: neither keeps anything between folds; each
  call's own entry lives only in its appender's call frame. The MCP server's projector holds one log
  connection for its life and starts a blob batch process only inside a call that meets an entry.
- Crash between extraction and write, mid-call or mid-wait: the position is unapplied and nothing is
  half-written; the cursor of the last committed call stands; the next call re-extracts and folds it.
- A crashed rebuild and a pending identity migration at one setup: setup takes the rebuild lock and
  runs the migration; on a file holding rebuild state (a standing tail) its DecisionMade, appended
  above the rebuild cursor, is INERT at once, its process holding the lock (THE APPENDER), so the
  migration's line carries its not-folded clause (`migrate_identity_at`'s report, `src/cli/mod.rs:1185-1189`) and setup
  goes on to the finish, whose tail folds it and whose drop sets `caught_up`; beside a shadow not yet swapped it folds into the live file as usual
  and the swap's tail re-reads it from the log.
- A contended seeding on a file owing an older rule: the seeding's transaction meets the lock, so
  the seeding, the any-miss read and the verdict are dropped; the owed check still pays the
  older-rule rebuild `owed_because` reads, under the live-run refusal, then the loop is skipped
  with the seeding line and the next setup seeds; setup exits zero unless a rebuild pass fails or the live-run refusal fires. A
  marked file before criterion 6 is paid the same way, the mark being an `owed_because` cause; a
  file already carrying `caught_up` issues no verdict under the contention, pays any
  `owed_because` cause, and skips its loop with the seeding line, the next setup's loop catching it
  up.
- Crash mid-tail: the rebuild resumes the tail from its cursor, and its drop sets `caught_up`; a run
  launched meanwhile does not hold that back: setup's rerun finishes the standing state beside it
  (THE REBUILD RUNS WITH NO RUN LIVE), the run's appenders INERT under their ceiling until the drop,
  and one whose ceiling runs out returns, its positions foreign.
- Cold start: setup creates and seeds the file (THE CURSOR AND ITS SEEDING); a file a fold created
  before any setup folds directly until setup seeds it, waiting out a contended direct fold's open
  under the ceiling. An event constructed more than the ceiling before its append (a long extraction
  between `Event::new` and the append) gets one call, which folds it when its position is next, and
  otherwise returns with that call's text, or the catch-up text after an advancing call, the next
  fold reaching it; on the direct path such an event still gets its first open, which folds it, and
  only a retry after `FrameError::Contended` returns past the ceiling; the seeding may count its
  position old meanwhile (THE CLOCKS).
- A store without `caught_up` (one that predates the change): setup's seeding seeds it at its highest
  applied position when no live position below it is missing, and its gaps above that position are
  then caught up by the loop, never rebuilt; else the rebuild pays it; until it is seeded its
  appenders fold directly, a contended open waited out under the ceiling.
- A marked store: before criterion 6 every call on it is INERT and setup's rebuild pays the mark;
  from criterion 6 the mark is read by nothing but setup's removal.
- A marked file this setup seeds: before criterion 6 the mark owes the rebuild, whose swap removes
  it (`Projector::rebuild`, `sqlite.rs:516-517`); from criterion 6 the seeding commits, the loop
  folds, and the last ordered step removes the mark in this run.
- A store with rebuild state: every call is INERT until the drop sets the cursor.
- A store with a prefix violation: no cursor is created, and the seeding's verdict, printed by setup
  and recorded nowhere, makes the rebuild pay it; with a run live the appenders keep folding
  directly, unrefused, until a setup with no run live rebuilds it.
- Seeding beside live direct folds: an unseeded file folded directly out of order may hold an
  applied position above a young missing one still in flight; the seeding seeds just below the
  lowest young missing position, which lies above `caught_up`, and a file carrying it reads no log
  rows at setup (criterion 4), so no rebuild is owed for it,
  and the loop's calls fold the missing one and advance
  over the applied ones above. The seeding's reads run under its lock, so a direct fold beside it
  waits the reads out as CONTENTION under its ceiling, and a seeding meeting a direct fold's
  transaction skips with the seeding line, issuing no verdict.
- Suspend: one wall clock serves the ceiling, so a machine suspended mid-wait resumes with `now -
  recorded_at` advanced for every caller alike; a suspend between an event's construction and its
  append leaves its appender one call, past the ceiling after it, the next fold or setup's loop
  reaching the position; on the direct path it leaves the first open, which always proceeds, and
  no retry after `FrameError::Contended`.
- Clock skew on the server-backed log: the age bound is not read there (THE CLOCKS), so skew moves
  nothing; the ceiling reads the appender's own event copy's stamp, on its own host.
- A ledger entry whose re-extraction resolves to nothing, a hash mismatch included: passed over with
  `resolved to nothing`, folding nothing; the next walk of its file records it again.
- A failing re-extraction: a blob batch process that dies or fails its I/O is ENVIRONMENT (FAILED,
  never passed over, no fall-through to the tree), retried by the next call, whose fresh
  re-extraction restarts the process; a corrupt object that kills it on every call stops each call
  FAILED with the batch process's text naming the object and its repair (`worktree.rs:1638`); one that cannot start leaves no blob source
  (`entry_blobs`, `setup.rs:1009-1015`), so the entry resolves from the tree as today; an entry that
  resolves to nothing is passed over with `resolved to nothing`; a re-extracted batch the judge
  rejects is CONTENT, passed over with `fold failed`, and its file's next ingest records it again; a
  fold arm's own `Err` over a batch the judge accepted is ENVIRONMENT, FAILED, retried by every
  later call.
- An own position passed over: the judge rejects the appender's own
  non-entry payload (`apply`, `apply_batch`) or its own batch (`apply_generation`); the call records
  it passed over at once, the prefix moves on, and the appender returns its not-folded error with the
  row's cause text, without a ceiling retry; an own position another call passed over earlier
  answers the same, read by the appender's success read after the call, whatever the call's
  outcome, AT HEAD and SKIPPED included, `resolved to nothing` included, which on
  one host means no one wrote the blob (THE BLOB IS WRITTEN).
- A transient failure at an appender's own entry: FAILED (ENVIRONMENT) whose failing position is its
  own ledger entry is retried under the ceiling, the batch kept, one busy-timeout interval apart; a
  successor folds the entry from its blob meanwhile, and the appender's retry meets it applied; past
  the ceiling the appender drops the batch and the entry is foreign like any dead appender's. FAILED
  at an own non-entry returns at once: the next call folds it from the read.
- A long gap: each call reads one page of at most `FOLD_BATCH` events past the cursor through
  `read_all_page`, outside the lock, and folds up to the page's first cut, so a gap of n non-entries
  costs about n / `FOLD_BATCH` calls and as many page reads, and a gap of m ledger entries costs m
  calls, each one page read bounded by `FOLD_BATCH` rows and one re-extraction; a retry re-reads one
  page; a page of other streams under the project's prefix, emptied by the narrowing, advances the
  cursor over the positions it read, and AT HEAD is only an empty store page. Every one of those
  calls advances, so an appender behind the gap crosses it call after call with no interval until
  its own position folds or its ceiling passes, when it returns the catch-up text and the next fold
  continues. The rebuild's
  tail, which folds many cuts per pass, remains the route for a gap setup finds.
- A gap of many foreign entries ahead of an appender's own position: one re-extraction per call,
  every call advancing; within the ceiling the own entry folds when the calls reach it, and past it
  the appender returns the catch-up text and the next fold reaches it.
- The operator reading during a call (`open_graph_to_read`): write-ahead logging serves the last
  commit; a read in a process whose appender is waiting meets the mutex only for one call's
  transaction and its brief re-extraction decision, never for its page read or the extraction
  itself.
- DROPPED: a CONTENT failure is passed over at once in any call, so no position the judge rejects,
  and no entry resolving to nothing, wedges the prefix; CONTENTION and ENVIRONMENT, a fold arm's own
  `Err` included, are never passed over and are retried by every later call, or at an appender's own
  entry by its own next call, a repeating ENVIRONMENT failure being a defect setup's loop reports by
  stopping FAILED non-zero.

**UNIT ORDER AND LANDING ORDER, decided here.** 4, 2, 6, on 122's landed tree. The Done-when
checkboxes map by position to the numbers this Design uses: the first is criterion 2 (the appender),
the second criterion 4 (the procedure), the third criterion 6 (the mark). Each tree below has
exactly one owner per behaviour.
- Tree 4: setup refuses a new owed rebuild beside a live run and finishes a standing rebuild state
  beside one, and the decide step passes an entry resolving to nothing over with `resolved to
  nothing` (criterion 4). These tests are tree 4's
  (criterion 4): setup refusing a new rebuild while the `rigger step` lock is held, finishing a
  standing shadow and a standing tail beside it, cold-start creation, and a file without `caught_up`
  whose ledger misses an old position below an applied one being rebuilt (the verdict's route); the rebuild as a method of
  its projector is pinned by the existing rebuild tests compiling against it (RETIRED TESTS). The appender folds directly
  through `guarded` as today, out of position order, and a
  failed fold still writes the mark with today's error and suffix (today's code, no criterion's).
  `catch_up` has one production caller, setup's loop, and folds foreign positions only, so no arm of
  it is test-only. Setup runs criterion 4's order: it creates and seeds the file, pays an owed
  rebuild (the mark, an older rule, or the seeding's prefix-violation verdict, which on a file
  carrying `caught_up` is its head comparison alone)
  through THE DECIDE STEP, then loops calls that fold in
  order what the appenders left above `caught_up`, a hole there owing no rebuild, printing CATCH-UP
  LINES
  (criterion 4), the SKIPPED line ending with "the next rigger setup continues it". `catch_up` is
  INERT on a marked file. Tree 4 lands the constants `FOLD_LOCK_CEILING` and `CATCH_UP_HORIZON`,
  which the seeding's young rule and the any-miss read read, and no held ceiling. The horizon bound
  is guaranteed only from tree 4 and 2 (criterion 2's re-check before each retry's open), so a tree-4
  seeding verdict may over-report a prefix violation, a conservative outcome (an unnecessary
  rebuild, or the live-run refusal exiting non-zero), never a lost position. `hash_blob` writes
  no object yet, so a gap's entry whose blob is
  uncommitted resolves from no object database and is passed over `resolved to nothing`, recorded
  again at its file's next ingest. A direct fold that meets its own position already applied
  (`has_applied`, `sqlite.rs:2117`) reads `passed_over` for that position in the same transaction
  and answers not-folded with the row's cause when a row exists, as THE APPENDER's own-position rule
  answers, so the appender's two paths answer alike (criterion 4, the direct fold's own-position `passed_over` answer; criterion 2 owns only the direct
  fold's contended-open wait and no-mark rule). That answer is a RECORDED OUTCOME, not a lost fold:
  the direct fold's body returns `FrameError::Recorded` with the row's cause text, and `guarded`
  maps it to the domain's `Err(Error(cause))` in the same match that passes the fold's `Ok`
  (`folded => return folded` today, `sqlite.rs:668`) and sends LOST to the mark arm, so it writes no
  mark (:670) and carries no suffix (:676-679), on tree 4 and after (EXISTING MECHANISMS,
  `in_transaction`); criterion 4's
  test pins it: a direct fold meeting its own passed-over position answers the cause, writes no mark,
  and the next fold is not refused, so setup's loop keeps running beside a live run. The seeding's
  verdict and the owed check's any-miss read run in the seeding's one IMMEDIATE transaction. THE INVARIANT does not hold: appenders fold above gaps.
- Tree 4 and 2: the ceiling the projector holds and `Projector::with_lock_ceiling` land with the
  appender, their first reader; on a seeded, wired file the appender loops calls under the ceiling with its own entry
  as their argument, the own-entry arm's one production caller, waits out rebuild-state INERT and
  SKIPPED and FAILED at its own ledger entry, keeping its batch, returns at once on FAILED at a
  foreign position or an own non-entry and on any own position passed over; on a file without
  `caught_up` it folds directly, waiting out a contended open the same way; `hash_blob` writes the
  blob it hashes inside a repository, so every same-host entry re-extracts from the object database;
  no fold writes the mark (criterion 2). A call-run cause text prints ` - the next fold through the
  folding store catches it up`, true there since no fold writes the mark and `guarded`'s owed check
  refuses a marked file before any call, and a direct fold's prints ` - the next rigger setup finds
  the event missing from graph.db and folds it`; the SKIPPED line is reworded to end with "the next
  fold through the folding store or the next rigger setup continues it" (criterion 2). A mark left from before stays a
  refusal cause, `catch_up` is INERT on it, and setup's rebuild pays it (criterion 4's order
  and rebuild); a hole above `caught_up`, or one whose missing position is younger than
  `CATCH_UP_HORIZON`, owes no rebuild and is caught up by the loop (criterion 4). THE INVARIANT holds on every file carrying `caught_up`.
- Tree 4, 2 and 6: the mark is written by no fold of this binary and read by nothing but setup's
  removal and the swap's; `catch_up`
  runs on a formerly marked file (criterion 6); the cause texts, the suffixes and the SKIPPED line are
  as on tree 4 and 2 (criterion 2); `REBUILD_OWED` names the older rule alone and the any-miss read
  is gone, setup rebuilding only for an older rule or the seeding's verdict (criterion 6 for the
  removal, criterion 4 for the verdict's route). THE INVARIANT holds as on tree 4 and 2.
Criterion 2 needs 4's procedure and wiring; criterion 6 needs 2 (no fold writes the mark before the
mark stops being read) and lands last.

## Notes (non-criteria)

CAUSE TEXTS (criterion 2), each one constant in `sqlite.rs`, each followed by its path's suffix, two
constants in `sqlite.rs` replacing today's " - the next `rigger setup` finds the event missing from
graph.db and rebuilds it" (`sqlite.rs:677`): a call-run fold's " - the next fold through the folding
store catches it up", and a direct fold's (an unseeded file, or a projector without the wiring)
" - the next rigger setup finds the event missing from graph.db and folds it", today's text with one
word changed, which a direct fold of an own event on a stream other than the project stream does
not carry: its not-folded line has no suffix, since no later fold re-derives it, setup's position
read, the rebuild source (`src/cli/setup.rs:954-957`, :967-972, :990-992) and the procedure's
narrowing reading the project stream alone:
- past the ceiling while contended: `graph.db's write lock stayed held past {d}`, where d is the
  projector's held ceiling in whole minutes (`{m} min`) at or above one minute and in whole seconds
  (`{s} s`) below it, followed by the call-run suffix on the call path and the direct suffix on the
  direct fold's contended open;
- past the ceiling after a call that advanced: `graph.db's catch-up had not reached this event after
  {d}`, d as above, followed by the call-run suffix;
- FAILED (ENVIRONMENT) at a foreign position or an own non-entry, a fold arm's own `Err` included,
  at once, followed by the call-run suffix, setup's loop stopping FAILED non-zero being the report of
  one that persists; FAILED at the
  appender's own ledger entry, past the ceiling, the last call's error; and a direct fold's failure:
  the error's own text, as `be` gives it today;
- an own position passed over, by its own call or another's, at once: the cause text of the row
  the appender's success read finds (THE APPENDER) (`payload rejected: ...` or `fold failed: ...`), with no suffix, since no later fold catches
  it up; under `apply_batch`, the first such row's; an own entry passed over `resolved to nothing`
  answers that cause, with no suffix, its file's next ingest recording it again;
- the INERT variants, mapped by the appender alone (`catch_up` returns no text): past the ceiling
  while the file holds rebuild state, or at once when the appender's process holds the file's
  rebuild lock, `graph.db holds an unfinished rebuild`; at once on an older
  rule or, until criterion 6, the mark, `REBUILD_OWED`, with no suffix; no cursor, no text, the
  direct fold's own suffix applying. Setup's loop prints nothing
  for INERT, since it runs the rebuild before its loop.
The refusal before any call of a file that owes its rebuild stays `REBUILD_OWED`, as today.

CATCH-UP LINES (criterion 4), the exact texts:
- success: `rigger setup` prints `caught graph.db up with the event log: folded {n} event(s) it
  missed` when n is above 0; n is the count of positions this setup's calls newly recorded in
  `applied`, passed-over positions included, since each is recorded;
- FAILED (setup exits non-zero): `graph.db catch-up stopped at position {P}: {error} - rerun rigger
  setup once the cause is fixed`, `{error}` being the ENVIRONMENT error's text;
- the seeding skipped (exits zero): `graph.db catch-up was not started: another writer holds the
  write lock - the next rigger setup starts it`, which no criterion rewords;
- the rebuild refused beside a live run (exits non-zero): `rigger setup: refusing to rebuild
  graph.db while the run is live - {reasons}; stop the run's driver, run rigger setup, then relaunch
  the run by adoption`, `{reasons}` being `live_writer_reasons`' joined with "; ";
- SKIPPED in setup (exits zero), whether the call met the lock or another writer outran its read
  (ONE PROCEDURE): `graph.db catch-up stopped at position {P}: another writer holds the write lock
  or advanced the cursor past this call's read - the next rigger setup continues it`; criterion 2, at its landing, rewords its tail to
  `the next fold through the folding store or the next rigger setup continues it`, naming both
  routes;
- passed over (replacing `passed over {n} event(s) whose payload the fold rejects, recorded as
  folded`, `setup.rs:1062`): `passed over {n} event(s) the fold could not fold ({r} rejected
  payload(s), {f} fold failure(s), {z} resolved to nothing), recorded as folded - a rebuild under a later fold rule
  retries them`, printed once per setup, after the loop, whenever n is above 0, the counts being the
  `passed_over` rows the FILE HOLDS at the end of setup, by cause (`payload rejected`, `fold
  failed`, `resolved to nothing`), read from the table in one query: not scoped to this setup's
  writes, so `Rebuilt` (`sqlite.rs:386-391`) gains no counts, no setup marker is needed, and the rows
  a crashed shadow pass wrote are reported by the setup that swaps it in;
- validate: `graph_rebuild_owed_note` adds `note: graph.db passed over {n} event(s) ({r} rejected
  payload(s), {f} fold failure(s), {z} resolved to nothing); a rebuild under a later fold rule retries them` when n is
  above 0, reading the same table as the setup line; a file without the `passed_over` table reads as
  zero, and positions passed over before
  this change, which carry no `passed_over` row, are not counted.

MARK LINES (criterion 6): `REBUILD_OWED` reads "graph.db owes one rebuild from the event log (it was
folded under an older fold rule) - run `rigger setup` to rebuild it"; the `rebuilding graph.db ...:
it misses an event the log holds` line is printed only for a file without `caught_up` whose seeding
finds a prefix violation. Setup removes the mark and says nothing of it, beside a file that carries
`caught_up` at its last step or once its file is gone; a mark no removal can remove fails setup with
its stderr ending in a line naming the mark, and a catch-up that stopped at a failure fails setup
with its own line (CATCH-UP LINES) whatever the removal answered. Every other setup line stays as it
is.

RETIRED TESTS: a test is rewritten by the criterion whose tree first turns it red, keeping what it
asserts beside the change, never deleted without a replacement; the mark's part of a test stays with
criterion 2 (the mark write) or criterion 6 (the mark as a refusal cause). In
`crates/rigger-graph-sqlite/src/contextgraph/sqlite.rs` (`sqlite.rs`) and the test files named:
- criterion 4, the any-miss read's exclusion above the highest applied position, each rewritten so
  its hole lies below an applied position, still owed on tree 4: the helper
  `the_ledger_owes_the_lost_fold_and_a_rebuild_pays_it` (:6008-6013) and the ledger-hole assertions of
  `a_lost_fold_whose_mark_cannot_be_written_is_owed_by_the_ledger_alone` (:6721) and
  `an_event_appended_but_never_folded_is_owed_by_the_ledger` (:6764);
  `the_ledger_is_read_only_for_a_cause_the_file_does_not_already_carry` (:6787, its assertion at
  :6807-6810, the causes staying `[OWED_OLDER_RULE, OWED_LOST_FOLD]`); the deleted cursor position of
  `a_rebuild_interrupted_in_its_tail_is_finished_by_setup_reading_only_the_tail` (:6349, :6438); in
  `tests/compaction_generations_periphery.rs`,
  `a_log_with_no_live_selection_is_read_for_positions_alone_and_rebuilt_reading_each_event_once`
  (:585), `a_fold_lost_where_its_mark_cannot_be_written_is_paid_by_the_next_setup_from_the_ledger`
  (:2875), `an_event_appended_but_never_folded_is_paid_by_the_next_setup_from_the_ledger` (:2930),
  `setup_whose_rebuild_meets_a_storage_error_fails_and_the_next_setup_folds_the_event` (:3126), their
  helper `the_next_setup_pays_the_missing_event` (:2836), and the other writer's append of
  `setup_rebuilds_a_graph_db_missing_an_event_another_writer_appended_to_the_server_log` (:2958);
- criterion 4, the passed-over line and the missing-file return:
  `setup_says_how_many_events_its_rebuild_passed_over_for_a_payload_the_fold_rejects` (:3076, also a
  hole below an applied position), `an_entry_whose_payload_does_not_parse_is_passed_over_by_a_rebuild_that_completes`
  (`tests/ledger_rebuild.rs:1216`, the line at :1229) and
  `rigger_setup_on_a_project_without_a_graph_db_creates_none`
  (`tests/compaction_generations_periphery.rs:1588`), which now asserts the file is
  created and seeded at 0;
- criterion 4, an entry resolving to nothing passed over, each asserting `resolved to nothing` in
  the passed-over count where it asserted the entry recorded applied:
  `a_rebuild_over_an_entry_no_source_resolves_reaches_the_facts_of_every_latest_entry_that_does`
  (`tests/ledger_rebuild.rs:780`), `without_an_extraction_every_entry_folds_nothing_and_its_position_is_recorded`
  (:1153), the parsable entry of :1216, and every test whose rebuild's re-extraction answers
  `Ok(None)` for an entry and asserts its count or its `applied` row, such as `rebuild_as_setup_does`'
  users (`sqlite.rs:5990`);
- criterion 4, the live-run refusal: any setup test whose owed rebuild runs while its project holds a
  held step lock, an in-flight spawn with a marker inside its bound, or a live driver registration;
- criterion 4, THE DECIDE STEP: `a_rebuild_whose_re_extracted_batch_the_fold_rejects_fails_rather_than_passing_it_over`
  (`tests/ledger_rebuild.rs:1251`), which now asserts the batch passed over with `fold failed` and the
  rebuild completing;
- criterion 4, the rebuild as a method of its projector, compile-only: every `Projector::rebuild`
  call in a test, through the helpers `rebuild_in_batches` (:5741), `rebuild_as_setup_does` (:5990),
  `rebuild` (`tests/compaction_generations_periphery.rs:476`), `rebuild_from_the_log`
  (`tests/common/fixtures/ledger.rs:324`) and `rebuild_resolving` (`tests/ledger_rebuild.rs:929`),
  and directly, calls it on a projector;
- criterion 4, the seeding's age bound: every test of `tests/ledger_rebuild.rs` that runs `rigger
  setup` over ledger entries it appended (:327, :553, :645, :780, :959, :1098, :1153, :1216, :1537,
  :1660, :1949, :1986, :2045, :2129, :2287, :2366, :2533) and asserts a rebuild over a hole below an
  applied position backdates the hole (HORIZON IN TESTS); one whose ledger is empty beside a
  non-empty log, a prefix violation at any age, needs none;
- criterion 2, THE BLOB IS WRITTEN: `hash_blob_applies_no_filter_and_writes_no_object`
  (`crates/rigger-worktree-git/src/worktree.rs:3041`), which now asserts the object database holds
  the unfiltered id after the hash inside a repository, and a hash outside one writing nothing;
- criterion 2, the mark write and the lost-fold text: the error text of :6721;
  `a_failed_fold_into_a_current_file_marks_it_owed_until_a_rebuild_pays_it` (:5633);
  `the_generic_fold_refuses_a_ledger_entry_naming_apply_generation_and_marks_the_graph_owed` (:15961);
  `apply_generation_refuses_on_a_graph_that_owes_its_rebuild_and_the_generation_still_reads`
  (:15999), owed by an older rule instead; in `tests/compaction_generations_periphery.rs`,
  `an_emit_whose_fold_fails_does_not_claim_it_folded` (:2321),
  `an_mcp_emit_whose_fold_fails_answers_not_folded_with_the_reason` (:2362),
  `a_result_whose_fold_fails_says_it_was_not_folded` (:2577),
  `a_fold_lost_to_a_lock_marks_the_graph_owed_until_setup_rebuilds_it` (:2694),
  `a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints` (:3287),
  `a_graph_build_whose_fold_is_lost_to_a_lock_says_so_and_the_next_build_refuses` (:3384),
  `a_step_whose_fold_is_lost_to_a_lock_says_so_and_the_next_step_refuses` (:3454),
  `reset_runs_says_whether_the_integration_it_recorded_for_a_landed_unit_was_folded` (:3529),
  `an_owed_mark_whose_graph_is_removed_is_dropped_by_setup_and_the_next_emit_folds` (:2780, its mark
  written by hand) and `an_emit_while_the_rebuild_folds_its_tail_is_folded_at_once_and_met_exactly_once`
  (:2215, the emit now waiting out rebuild state and folding after the drop, met exactly once);
  `a_pass_whose_fold_is_lost_to_a_lock_says_so_and_the_next_pass_refuses`
  (`tests/common/layer_cli.rs:289`); in `tests/ledger_entry_fold_periphery.rs`,
  `the_mark_a_refused_public_fold_writes_binds_a_second_projector_on_the_file` (:364),
  `the_mark_a_failed_ledger_fold_writes_binds_a_second_projector_on_the_file` (:404) and
  `the_ledger_form_answers_a_fold_it_could_not_make_beside_an_entry_that_is_on_the_log` (:912); the
  mark part of `an_entry_the_generic_fold_refused_is_paid_by_setup_through_the_ledger_fold`
  (`tests/ledger_rebuild.rs:1098`); and the fixture `owe_a_rebuild`
  (`tests/common/fixtures/ledger.rs:358`), which owes by a failed fold's mark and now owes by an
  older fold rule, keeping its callers (`src/cli/graph.rs:1433`,
  `crates/rigger-conductor/src/conductor.rs:21523`, :22362, `tests/run_sink_periphery.rs:701`). A
  binary cannot inject the ceiling, so each binary test's holder releases the lock within it and the
  test asserts the event lands with no mark; a past-ceiling text is asserted in-crate through
  `with_lock_ceiling`, and an owed state a test needs comes from an older fold rule. The suffix
  constant, criterion 2: `SETUP_PAYS` and `lost_fold` (`tests/common/fixtures/fold.rs:326`, :330),
  which the crate's own tests reach through `test_support` (`crates/rigger-graph-sqlite/src/lib.rs:63`),
  every test asserting through them following;
- criterion 6, the mark as a refusal cause and the any-miss read:
  `a_graph_file_owes_its_rebuild_for_each_cause_it_carries` (:6816);
  `the_owed_mark_of_a_graph_file_that_is_gone_is_dropped_and_one_that_stands_is_kept` (:6866), the
  kept mark no cause; `rigger_setup_names_both_causes_a_graph_db_owes_its_rebuild_for_and_pays_both`
  (`tests/compaction_generations_periphery.rs:1406`), naming the older rule alone and the mark
  removed; `a_graph_that_owes_its_rebuild_refuses_every_call_answered_or_not_naming_rigger_setup`
  (`tests/spec_critique_periphery.rs:1125`), owed by an older rule; and the `owed_against` reads that
  remain after criterion 4's rewrites (:6764, :6787, :6158, :6349, the helpers
  `owed_against_the_log`, :5983, and `rebuild_as_setup_does`, :5990, and
  `tests/compaction_generations_periphery.rs:585`), which read `owed_because` and the seeding's
  verdict instead.
HORIZON IN TESTS: the horizon is the constant and is not injected; `with_lock_ceiling(ceiling)`
injects the ceiling alone (criterion 2). An in-crate test of the age bound (the seeding's young rule, the any-miss
read's young exclusion) stamps `recorded_at` on the events its source hands, as
`rebuild_as_setup_does` (`sqlite.rs:5990`) takes its log as events; a binary test backdates
`recorded_at` on the log rows it writes (a stored column,
`crates/rigger-store-sqlite/src/eventstore/sqlite.rs:31`) past the horizon through one fixture in
`tests/common`, which every binary test whose setup must rebuild over a hole below an applied
position calls before its setup (criterion 4).

WHAT THE GATES CANNOT SEE: the production incident shape (a sibling courier against a minutes-long
fold) is reproduced only by criterion 2's periphery test; the adjudicator demands it.

LANES: in the light lane every foreign entry is passed over as resolved to nothing, and the tests of that pass-over run there; a test that needs a resolution through
`resolve_entry` is gated on the `symbols` feature; every other test of this spec runs in both
lanes.

## Global constraints

- Hyphens, never em or en dashes, ASCII only, in every added line; never the host vendor's name.
- A line citation names the symbol at 895f8188; the symbol governs, the line is a pointer.
- No new event type, no new dependency, no change to the event store's schema, no new lock or marker
  file, no background process; `read_all_page` extends the event-store port with a bounded form of
  its existing positional read, which is neither a schema change nor a new dependency.
- `open_connection`'s pragmas stay as they are.
- Every append to the project stream goes through a folding store or `Fold::of` over a projector the
  one open for write made, and so through `guarded`; criterion 2 owns this constraint and its
  audit, THE APPENDER AUDIT.
- The fix lives in `crates/rigger-graph-sqlite` (the procedure, the appender, the texts),
  `crates/rigger-domain` (`read_all_page` on the port, criterion 4; `OWED_LOST_FOLD` and
  `REBUILD_OWED`, criterion 6), `crates/rigger-store-sqlite` (`read_all_page` in the sqlite, the
  server-backed and the namespacing stores, criterion 4), `crates/rigger-worktree-git` (`hash_blob`,
  criterion 2) and `src/cli` (the one open for write, setup, the validate note); tests there and in `tests/`, each test double implementing
  `read_all_page`. `crates/rigger-grounder` (`FoldingStore`'s forward of `read_all_page`, criterion
  4), `crates/rigger-dash` and `crates/rigger-conductor` change no behaviour.
- `rigger setup`'s user-visible lines stay as they are except the lines Notes names.
- Every lane `.rigger/gates/lanes.sh` runs green (both feature lanes, `--no-default-features`
  included; clippy -D warnings; test) and `cargo fmt --check` passes on the tree.
- The planner's batch order is UNIT ORDER's sequence 4, 2, 6, never checkbox position.

## Done when

- [ ] a periphery test proves AN APPENDER WAITS UNDER THE CEILING AND NEVER MARKS: an appender's positions behind a write lock held past the busy timeout land in `applied` once the lock is released, no `graph.db.owed` written.
  Its own entry folds from its batch, a second process re-extracts a young entry's uncommitted blob from the object database and folds it before its appender's call, which then answers `AlreadyApplied`, and past an injected ceiling it returns with the cause text Notes names. This criterion OWNS the appender's calls under `FOLD_LOCK_CEILING` in `Projector::guarded`, THE OWN ENTRY and its arm, the ceiling the projector holds and `Projector::with_lock_ceiling`, THE BLOB IS WRITTEN (`hash_blob` writing the object it hashes), its direct fold's contended-open wait and no-mark rule, the no-mark rule on the call path, CAUSE TEXTS with both suffixes, the SKIPPED line's rewording, and the appender audit with its test; the procedure, the wiring and `passed_over` are criterion 4's, the live-run refusal and the standing-rebuild exemption are criterion 4's, the mark criterion 6's, NOT this one's.
- [ ] a test proves ONE PROCEDURE FOLDS FORWARD FROM THE CURSOR: a foreign non-entry folds in order, and a foreign ledger entry folds re-extracted at any age, else is passed over `resolved to nothing`.
  The re-extraction follows `resolve_entry`'s order, reading the object database when the entry's blob is held there, else the tree, and through the binary `rebuild_owed_graph` creates the file on a cold start, seeds `caught_up`, rebuilds a file without `caught_up` whose ledger misses an old position below an applied one, and loops calls until one advances nothing, a page read over a long gap reading one page, refuses a new rebuild with the refusal line and a non-zero exit while the `rigger step` lock is held (`LiveWriterFacts::step_lock_held`, `src/cli/hygiene.rs:784`), and beside that held lock finishes a standing shadow and a standing tail, the swap landing (Notes HORIZON IN TESTS). This criterion OWNS `Projector::catch_up`, THE DECIDE STEP, `read_all_page`, `CATCH_UP_HORIZON`, `FOLD_LOCK_CEILING`, the three classes with `FrameError`, `caught_up` with its seeding and the verdict's route, `passed_over`, the direct fold's own-position `passed_over` answer, the wiring, the setup loop, CATCH-UP LINES and setup's rebuild beside a run: the live-run refusal of a new rebuild, the standing-rebuild exemption, `Projector::rebuild` as a method and cold-start creation; the appender is criterion 2's, the mark criterion 6's, NOT this one's.
- [ ] a test proves THE MARK IS NO REFUSAL CAUSE: a `graph.db.owed` mark beside a file carrying `caught_up` refuses no fold and makes `rebuild_owed_graph` rebuild nothing, and setup removes the mark.
  A file without `caught_up` whose ledger misses an old position below an applied one is still rebuilt, the regression of retiring the any-miss read. This criterion OWNS `owed_by_file`'s mark arm, `OWED_LOST_FOLD`'s re-meaning, `REBUILD_OWED`'s text, the any-miss read with `owed_against`, lifting `catch_up`'s mark arm and the mark removal; the seeding and its verdict's route are criterion 4's, the live-run refusal and the standing-rebuild exemption are criterion 4's, the cause texts and suffixes criterion 2's, NOT this one's.
