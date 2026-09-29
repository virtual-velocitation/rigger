//! SQLite-backed EventStore. A single connection behind a mutex serializes
//! writes, so concurrent appenders queue instead of deadlocking on the
//! lock-upgrade (SQLITE_BUSY) class. Per-stream revisions and a `UNIQUE(stream,
//! revision)` index give optimistic concurrency; `$all` is `ORDER BY position`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::sqlite::open_connection;

use super::{
    from_nanos, to_nanos, AliasHistory, Appended, ContentIdentity, Direction, Error, Event,
    EventStore, ExpectedRevision, FactIdentity, Filter, Position, Revision, Subscription,
    TypeSelection, NO_STREAM,
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS events (
  position    INTEGER PRIMARY KEY AUTOINCREMENT,
  stream      TEXT NOT NULL,
  type        TEXT NOT NULL,
  id          TEXT NOT NULL,
  data        BLOB NOT NULL,
  meta        TEXT NOT NULL,
  valid_from  INTEGER NOT NULL,
  recorded_at INTEGER NOT NULL,
  revision    INTEGER NOT NULL,
  UNIQUE(stream, revision)
);
CREATE INDEX IF NOT EXISTS idx_events_stream ON events(stream);
CREATE INDEX IF NOT EXISTS idx_events_stream_type ON events(stream, type, position);
";

/// The boundary lookup behind [`EventStore::last_position`]: one seek of
/// `idx_events_stream_type` to the stream-and-type run's highest position.
const LAST_POSITION_SQL: &str =
    "SELECT revision FROM events WHERE stream = ?1 AND type = ?2 ORDER BY position DESC LIMIT 1";

const COLS: &str = "position, stream, type, id, data, meta, valid_from, recorded_at, revision";

/// Where a typed read from revision `?2` starts in the log: the position of the stream's first
/// event at or above that revision - one seek of the `(stream, revision)` index.
const TYPED_READ_START_SQL: &str =
    "SELECT position FROM events WHERE stream = ?1 AND revision >= ?2 \
     ORDER BY revision ASC LIMIT 1";

/// The typed read behind [`EventStore::read_stream_typed`] for `selection`, from the log position
/// `?2`, with the named types bound from `?3` on: `Only` seeks `idx_events_stream_type` once per
/// named type, so its cost is the selected events alone; `Except` walks the stream from `?2` on
/// the stream index and refuses the named types in the query, so a refused row never leaves the
/// store. Both hand back log (position) order.
fn typed_read_sql(selection: TypeSelection) -> String {
    let (types, index, op) = match selection {
        TypeSelection::Only(types) => (types, "INDEXED BY idx_events_stream_type ", "IN"),
        TypeSelection::Except(types) => (types, "", "NOT IN"),
    };
    let binds = (0..types.len())
        .map(|i| format!("?{}", i + 3))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT {COLS} FROM events {index}WHERE stream = ?1 AND position >= ?2 \
         AND type {op} ({binds}) ORDER BY position ASC"
    )
}

/// Where [`Store::read_live_selection`] hands each batch of the selection, with the stream's last
/// position - each row an [`Event`], or what another reader of the selection reads of it.
pub type SelectionSink<'s, T = Event> = dyn FnMut(&[T], Position) -> Result<(), Error> + 's;

/// Store is the SQLite-backed EventStore. The connection is shared (Arc) so a
/// subscription's polling thread reads the same database the writers append to.
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    /// Open (creating if needed) the store at path. Use ":memory:" in tests.
    pub fn open(path: &str) -> Result<Self, Error> {
        let conn = open_connection(path).map_err(be)?;
        conn.execute_batch(SCHEMA).map_err(be)?;
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Whether any stream whose name starts with `prefix` holds an event. An EXACT
    /// prefix comparison (`substr(stream, 1, length(prefix)) = prefix`), never a `LIKE`
    /// pattern, so a prefix carrying SQL wildcards (`_` / `%` - e.g. a project namespace
    /// derived from a directory basename such as `my_repo`) matches literally rather than
    /// as a wildcard. This is a store-level maintenance read: the spec-09 identity
    /// migration uses it to decide whether a project namespace is populated.
    pub fn has_stream_prefix(&self, prefix: &str) -> Result<bool, Error> {
        let conn = self.conn.lock().unwrap();
        let present: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE substr(stream, 1, length(?1)) = ?1)",
                params![prefix],
                |r| r.get(0),
            )
            .map_err(be)?;
        Ok(present != 0)
    }

    /// Rename every stream whose name starts with `from` to the same name with `from`
    /// replaced by `to`, in place, returning the number of DISTINCT streams moved. A
    /// store-level maintenance operation (the spec-09 identity migration): it moves a
    /// project's whole history from one namespace to another while preserving each
    /// event's position, revision, and payload. The prefix comparison is exact (not
    /// `LIKE`), and the caller guarantees the `to` namespace is empty, so the
    /// `UNIQUE(stream, revision)` index never collides. Renaming when nothing matches
    /// `from` moves nothing and returns 0 (idempotent shape).
    pub fn rename_stream_prefix(&self, from: &str, to: &str) -> Result<usize, Error> {
        let mut guard = self.conn.lock().unwrap();
        let tx = guard
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(be)?;
        let renamed: i64 = tx
            .query_row(
                "SELECT COUNT(DISTINCT stream) FROM events WHERE substr(stream, 1, length(?1)) = ?1",
                params![from],
                |r| r.get(0),
            )
            .map_err(be)?;
        tx.execute(
            "UPDATE events SET stream = ?2 || substr(stream, length(?1) + 1) \
             WHERE substr(stream, 1, length(?1)) = ?1",
            params![from, to],
        )
        .map_err(be)?;
        tx.commit().map_err(be)?;
        Ok(renamed as usize)
    }

    /// Prune the DUPLICATION an already-bloated log accumulated in its derived index, and reclaim
    /// the disk it held: for each type `identity` covers, keep the LATEST event per distinct
    /// content key within each stream under `stream_prefix`, carry that key's earliest valid-time
    /// onto the recording it keeps, delete every earlier recording, then `VACUUM` so the file
    /// actually shrinks.
    ///
    /// This is the COMPACTION half of spec 60 - the supported way to shed duplication a store
    /// accreted BEFORE the ingest dedup existed. The dedup above the port stops new duplication;
    /// this removes the pile already on disk. Deleting rows and reclaiming a file is a mechanic of
    /// the embedded store, so it lives here rather than on the port: a backend that cannot do it
    /// says so to the operator instead of silently reporting a prune that did not happen.
    ///
    /// It takes the [`ContentIdentity`] policy value, rather than a metadata-key string plus a type list plus a carry list: the policy already
    /// exists as one injected value, and re-spelling its fields as positional parameters is a
    /// second parallel expression of one rule that can be passed in the wrong order and can drift
    /// a call site at a time. The valid-time partition property 2 rests on is part of that value
    /// ([`ContentIdentity::with_reasserting_types`]) for exactly that reason, and it is CHECKED
    /// here before a single row is read, because it is the one input to this function that can
    /// corrupt the projection while leaving every row looking intact.
    ///
    /// Four properties, each load-bearing:
    ///
    /// 1. **Latest generation per identity, then latest recording per key.** A content key
    ///    names a batch identity AND its content generation ([`ContentIdentity::key_parts`]), so
    ///    every recording of a generation that is not its identity's LATEST is shed, and of the
    ///    latest generation only each key's last recording survives. Nothing shed is ever read
    ///    again: the ingest sinks seed from the latest generation only, and a file that returns
    ///    to an earlier content re-emits its batch. A key the policy cannot parse (or a policy
    ///    that declares no parser) keeps exact-key semantics: its latest recording survives.
    ///    The selection is [`plan_derived_prune`], shared with the read-only preview.
    /// 2. **A RE-ASSERTED fact's valid-time is CARRIED, not dropped.** A projection that
    ///    re-asserts a fact in place keeps its EARLIEST valid-time ("it has held since it first
    ///    became true"), so deleting that key's earliest recording would silently re-date the
    ///    fact to whichever recording survived - and for the design-intent edge class the date IS
    ///    the value. The policy's own declaration ([`ContentIdentity::reasserts`]) names the types
    ///    this is true of; each of their surviving rows takes the `MIN(valid_from)` of every
    ///    recording of its identity asserting the same fact, as the policy's
    ///    [`ContentIdentity::facts`] keys it, before the deletes run. Because a minimum is
    ///    associative and every deleted row's valid-time is at or above the minimum retained on
    ///    its survivor, the compacted log then yields exactly the valid-times the whole log
    ///    yields. A type NOT named here is one whose batch SUPERSEDES the subject's prior
    ///    assertions, so the surviving (latest) recording's own valid-time is already the one a
    ///    fold arrives at, and carrying an earlier one onto it would MOVE the graph rather than
    ///    preserve it. WHICH types are which is not this store's knowledge to hold - it is a fact
    ///    about the fold, so it arrives as data (see `contextgraph::refold_supersedes_prior_edges`
    ///    and `ingest::reasserted_derived_types`, where the partition is derived once).
    ///
    ///    THERE IS NO SAFE DEFAULT FOR AN UNDECLARED PARTITION, so this REFUSES rather than
    ///    picking one. Treating an undeclared policy as "nothing re-asserts" would not be the
    ///    fail-safe direction: the deletes below run over every covered type either way, so an
    ///    unnamed re-asserting type would have its earliest recordings deleted with no carry and
    ///    every one of its facts silently re-dated - the exact corruption this property exists to
    ///    prevent. The opposite default fails the other way, dragging a superseded fact back to a
    ///    date its fold retired. A policy that never declared the partition, or that declares a
    ///    type it does not cover, is therefore an [`Error::Backend`] before any row is read.
    ///    Declaring an EMPTY list is a different thing and is honored: it is a caller stating that
    ///    none of its types re-assert.
    /// 3. **Nothing else is touched.** Only the types `identity` covers are eligible, and within
    ///    them only a row whose key is recorded again LATER in the same stream. A row with no key
    ///    at all names no content generation and is never provably redundant, so it is left alone -
    ///    the fail-safe direction. Every surviving row keeps its position, its per-stream revision,
    ///    its type, its id, its payload bytes and its metadata; the ONLY column this writes is the
    ///    valid-time of a surviving DERIVED row whose duplicates it deleted, and it writes the
    ///    value the fold would have derived anyway. No non-derived row is read, written, or moved.
    /// 4. **The gaps it leaves are safe.** Deleting from the middle of a stream leaves holes in
    ///    that stream's revisions, which is exactly why [`Store::append`] reads the stream's
    ///    current revision as `MAX(revision)` rather than counting rows - see the comment there.
    /// 5. **Everything after the commit is a REPORT, not an outcome.** The deletes are durable the
    ///    moment the transaction commits; the space reclamation that follows can still fail, and
    ///    when it does this returns the counts with the failure NAMED beside them rather than an
    ///    `Err` that says only that something went wrong with a log which HAS been pruned. And it
    ///    only runs at all when the FILE has free space to reclaim - never merely because this
    ///    pass deleted something, and never merely because it did not. A file holding no free
    ///    page is left exactly as it stands rather than rewritten in full to reclaim nothing;
    ///    a file holding free pages is reclaimed even by a pass that deleted nothing, which is
    ///    what makes re-running the command after a failed reclamation the remedy this reports
    ///    tell an operator it is.
    pub fn prune_derived_index(
        &self,
        stream_prefix: &str,
        identity: &ContentIdentity,
    ) -> Result<PrunedDerived, Error> {
        self.prune_derived_index_compacting_with(stream_prefix, identity, compact_in_place)
    }

    /// A read-only PREVIEW of what [`prune_derived_index`] would delete (spec 68, "the reset
    /// surface"): for each type `identity` covers, the count of rows the prune's own selection
    /// ([`plan_derived_prune`]) marks for deletion - every recording of a superseded generation
    /// and every earlier recording of a surviving key. No row is touched, no valid-time carried,
    /// no `VACUUM` run.
    ///
    /// Unlike [`prune_derived_index`] this needs no [`ContentIdentity::reasserting`] declaration:
    /// that check exists because a DELETE has to know whether a surviving row's valid-time must be
    /// carried forward, and a count writes nothing, so the one input that check guards against
    /// getting wrong is not read here at all.
    ///
    /// `rigger reset`'s bare-menu preview reads this so its printed count can never drift from
    /// what a real `--derived` removes - both come from the one selection.
    pub fn count_derived_duplicates(
        &self,
        stream_prefix: &str,
        identity: &ContentIdentity,
    ) -> Result<DerivedPreview, Error> {
        let guard = self.conn.lock().unwrap();
        let plan = plan_derived_prune(&guard, stream_prefix, identity, &[])?;
        Ok(DerivedPreview {
            removed: plan.removed_per_type(identity.types()),
            superseded_generations: plan.superseded,
        })
    }

    /// [`Store::prune_derived_index`] with its post-commit space reclamation INJECTED.
    ///
    /// The seam exists because that step's real failures - a temporary directory too small for the
    /// full copy the rewrite stages there, a writer holding the file past the busy timeout - are
    /// properties of the machine, not of this code, so the only way to pin what the prune does
    /// WITH a failure is to hand it one. Production has exactly one implementation
    /// ([`compact_in_place`]) and the public entry point above passes it; nothing chooses.
    fn prune_derived_index_compacting_with(
        &self,
        stream_prefix: &str,
        identity: &ContentIdentity,
        compact: impl FnOnce(&Connection) -> Result<Compaction, Error>,
    ) -> Result<PrunedDerived, Error> {
        // THE PARTITION IS CHECKED BEFORE ANY ROW IS READ (property 2), so a policy that cannot be
        // acted on never takes the write lock at all.
        let reasserting = reasserting_types(identity)?;
        let mut guard = self.conn.lock().unwrap();
        // THE OPERATOR'S BEFORE, taken before a single row is deleted. What the reclamation is
        // reported as is the space the LOG LOST ON DISK across the whole command, so it is
        // measured where the command starts rather than derived from a page count inside the
        // rewrite: a page count is the database's LOGICAL size, it counts pages living only in an
        // un-checkpointed `-wal`, and a figure computed from it can name a reclamation over a
        // file that grew. This is the number an operator reproduces by measuring the log before
        // they run the command and again after, which is the only check they can make.
        //
        // `None` for a database with no file behind it (`:memory:`, a temporary database): there
        // are no bytes on disk to have lost, so the reclamation below is reported as UNMEASURED
        // rather than as a zero that claims a measurement was taken.
        let db_file = guard
            .path()
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string());
        let on_disk_before = db_file.as_deref().map(bytes_on_disk);
        let types = identity.types();
        let removed: Vec<(String, usize)>;
        let superseded_generations: usize;
        {
            // ONE transaction for the whole prune: a partial compaction is not a state an operator
            // can reason about. The carry-forward shares it, so a log can never be left with its
            // duplicates deleted and its survivors' valid-times un-carried.
            //
            // WHAT `BEGIN IMMEDIATE` BUYS, AND WHAT IT DOES NOT. It takes the write lock up front,
            // so this transaction cannot fail the deferred lock upgrade a read-then-write
            // transaction attempts half way through - that failure mode is closed. It does NOT
            // make a concurrent appender safe: the lock is held for the WHOLE delete, which on a
            // large log runs for longer than `busy_timeout` (5000ms, set by `crate::sqlite::open_connection` -
            // measured at roughly 8s of held lock on a 165MB log), and an appender that waits out
            // its timeout gets `database is locked` and does NOT retry. So a prune over a big log
            // can cost a concurrent writer its append. That is why this is maintenance run BETWEEN
            // runs and never against a live one, which is what the shipped guidance says; the
            // window is bounded here, not eliminated.
            let tx = guard
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(be)?;
            // The selection is read INSIDE the write transaction, so what it decided is exactly
            // what the deletes below act on: no append can land between the two.
            let plan = plan_derived_prune(&tx, stream_prefix, identity, &reasserting)?;
            {
                // Property 2: a surviving re-asserting row takes its fact's EARLIEST valid-time,
                // decided while every recording was still in the log.
                let mut carry = tx
                    .prepare("UPDATE events SET valid_from = ?2 WHERE position = ?1")
                    .map_err(be)?;
                for (position, earliest) in &plan.carries {
                    carry.execute(params![position, earliest]).map_err(be)?;
                }
                let mut delete = tx
                    .prepare("DELETE FROM events WHERE position = ?1")
                    .map_err(be)?;
                for (_, position) in &plan.deletes {
                    delete.execute(params![position]).map_err(be)?;
                }
            }
            removed = plan.removed_per_type(types);
            superseded_generations = plan.superseded;
            tx.commit().map_err(be)?;
        }

        // FROM HERE ON THE DELETES ARE DURABLE, so nothing below may turn this call into an
        // `Err`. An error return would tell the operator only that something failed, about a log
        // that HAS been pruned: not the per-type counts, not that a prune happened at all - the
        // one outcome this command's design says an operator cannot detect. So the reclamation's
        // failure is CARRIED BACK beside the counts instead, and the same honesty that reports an
        // unmeasurable reclamation as unmeasured reports an unrun one as named.
        //
        // AND WHETHER IT RUNS AT ALL IS DECIDED BY THE FILE, not by this pass's deletes - see
        // [`compact_in_place`], which skips a file holding no free page. The two directions are
        // one rule and both matter. A rewrite over a file with nothing to reclaim holds the write
        // lock for a full scan and stages a COMPLETE copy of the database in the temporary
        // directory SQLite resolves (a different, typically much smaller filesystem than the one
        // holding the log) to reclaim nothing at all - and that is the path the shipped guidance
        // calls the expected one, so it is the path an operator runs most. A rewrite gated the
        // OTHER way, on this pass having deleted something, would never run again over the log a
        // FAILED reclamation leaves behind: the first pass took the duplication, so the re-run
        // this report tells the operator is safe deletes nothing, and the space it was told to
        // re-run for would stay in the file forever.
        match compact(&guard) {
            // Nothing to reclaim, nothing rewritten: zero bytes is the MEASUREMENT here, not a
            // measurement that could not be taken - but only where a FILE existed to measure.
            // A database with no file behind it has no reading to report, and a `Some(0)`
            // beside `on_disk_measured: false` would claim a measurement the flag denies;
            // unmeasured is the honest report there, exactly as on the pending path below.
            Ok(Compaction::Skipped) => Ok(PrunedDerived {
                removed,
                superseded_generations,
                reclaimed_bytes: db_file.as_deref().map(|_| 0),
                compaction_ran: false,
                on_disk_measured: db_file.is_some(),
                compaction_error: None,
            }),
            // The rewrite ran and its result is on disk NOW, so the before taken above and the
            // after taken here bracket the whole command: their difference is what the log lost.
            Ok(Compaction::Landed) => Ok(PrunedDerived {
                removed,
                superseded_generations,
                reclaimed_bytes: db_file
                    .as_deref()
                    .zip(on_disk_before)
                    .map(|(db, before)| before.saturating_sub(bytes_on_disk(db))),
                compaction_ran: true,
                on_disk_measured: db_file.is_some(),
                compaction_error: None,
            }),
            // The rewrite ran but its result has NOT landed: the freed frames are still in the
            // write-ahead log, so any difference measured now is between two states of a move
            // that has not finished. Unmeasured is the honest report.
            Ok(Compaction::Pending) => Ok(PrunedDerived {
                removed,
                superseded_generations,
                reclaimed_bytes: None,
                compaction_ran: true,
                on_disk_measured: db_file.is_some(),
                compaction_error: None,
            }),
            Err(e) => Ok(PrunedDerived {
                removed,
                superseded_generations,
                reclaimed_bytes: None,
                compaction_ran: true,
                on_disk_measured: db_file.is_some(),
                compaction_error: Some(e.to_string()),
            }),
        }
    }

    /// Measure the derived-index REDUNDANCY already sitting in the log, WITHOUT deleting
    /// anything: across every type `identity` covers, within streams under `stream_prefix`, how
    /// many rows carry a covered key versus how many of them a compaction would KEEP.
    ///
    /// The READ-ONLY twin of [`Store::prune_derived_index`]: both are answered by the one
    /// selection [`plan_derived_prune`], so `rigger validate`'s bloat advisory (spec 68) measures
    /// exactly the rows `rigger reset --derived` would shed - superseded generations as well as
    /// earlier recordings of one key - and can never drift from a second, independently re-derived
    /// definition of "redundant" (Design: "one measurement authority per advisory ... no shadow
    /// accounting"). No row is touched, no valid-time carried.
    pub fn measure_derived_duplication(
        &self,
        stream_prefix: &str,
        identity: &ContentIdentity,
    ) -> Result<DerivedDuplication, Error> {
        let guard = self.conn.lock().unwrap();
        let plan = plan_derived_prune(&guard, stream_prefix, identity, &[])?;
        Ok(DerivedDuplication {
            rows: plan.rows,
            kept: plan.rows - plan.deletes.len(),
        })
    }

    /// Stream the LIVE SELECTION of `stream_prefix` + `stream` after position `after` (spec 101):
    /// exactly the rows [`Store::prune_derived_index`] keeps - every non-derived event, and of the
    /// derived index each identity's latest generation at the latest recording of each key, its
    /// valid-time carried back exactly as the prune carries it - so a graph folded from it is the
    /// one folded from the compacted log, by construction: both act on the one selection,
    /// [`plan_derived_prune`], over the same `stream_prefix`.
    ///
    /// The stream is read ONCE, in position order, and handed to `sink` in batches of at most
    /// `batch` events, never materialized whole; each batch goes with the stream's last position,
    /// so a caller can say how far along it is. The selection and the stream are read inside one
    /// read transaction, so they describe one state of the log, and nothing is written.
    pub fn read_live_selection(
        &self,
        stream_prefix: &str,
        stream: &str,
        identity: &ContentIdentity,
        after: Position,
        batch: usize,
        sink: &mut SelectionSink,
    ) -> Result<(), Error> {
        self.read_live(
            (stream_prefix, stream, identity),
            after,
            batch,
            COLS,
            &|row, carried| {
                let mut e = row_to_event(row)?;
                if let Some(earliest) = carried {
                    e.valid_from = from_nanos(earliest);
                }
                Ok(e)
            },
            sink,
        )
    }

    /// Stream the POSITIONS of the live selection of `stream_prefix` + `stream` (spec 101) - the
    /// selection [`Store::read_live_selection`] hands, read the same way from the same plan, but
    /// the positions alone, never an event's payload - to `sink` in batches of at most `batch`:
    /// what `rigger setup` reads a graph's ledger of folded positions against.
    pub fn read_live_positions(
        &self,
        stream_prefix: &str,
        stream: &str,
        identity: &ContentIdentity,
        batch: usize,
        sink: &mut dyn FnMut(&[Position]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.read_live(
            (stream_prefix, stream, identity),
            0,
            batch,
            "position",
            &|row, _| row.get::<_, i64>(0).map(|p| p as Position),
            &mut |positions, _| sink(positions),
        )
    }

    /// The one reader of a live selection: plan it ([`plan_derived_prune`]) for the
    /// `(stream_prefix, stream, identity)` it names, then read the stream's rows past `after` -
    /// the columns `cols`, the position first - in position order, skip every row the plan sheds,
    /// and hand `sink` each surviving row as `row` reads it, given the earliest valid-time the plan
    /// carries onto it, in batches of at most `batch` with the stream's last position.
    fn read_live<T>(
        &self,
        (stream_prefix, stream, identity): (&str, &str, &ContentIdentity),
        after: Position,
        batch: usize,
        cols: &str,
        row: &dyn Fn(&rusqlite::Row, Option<i64>) -> rusqlite::Result<T>,
        sink: &mut SelectionSink<T>,
    ) -> Result<(), Error> {
        let reasserting = reasserting_types(identity)?;
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction().map_err(be)?;
        let plan = plan_derived_prune(&tx, stream_prefix, identity, &reasserting)?;
        let shed: std::collections::HashSet<i64> =
            plan.deletes.iter().map(|(_, position)| *position).collect();
        let carried: std::collections::HashMap<i64, i64> = plan.carries.into_iter().collect();
        let stream = format!("{stream_prefix}{stream}");
        let head: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(position), 0) FROM events WHERE stream = ?1",
                params![stream],
                |r| r.get(0),
            )
            .map_err(be)?;
        let mut stmt = tx
            .prepare(&format!(
                "SELECT {cols} FROM events WHERE stream = ?1 AND position > ?2 ORDER BY position"
            ))
            .map_err(be)?;
        let mut rows = stmt.query(params![stream, after as i64]).map_err(be)?;
        let mut kept = Vec::with_capacity(batch);
        while let Some(r) = rows.next().map_err(be)? {
            let position: i64 = r.get(0).map_err(be)?;
            if shed.contains(&position) {
                continue;
            }
            kept.push(row(r, carried.get(&position).copied()).map_err(be)?);
            if kept.len() == batch {
                sink(&kept, head as Position)?;
                kept.clear();
            }
        }
        if !kept.is_empty() {
            sink(&kept, head as Position)?;
        }
        Ok(())
    }
}

/// What [`Store::count_derived_duplicates`] previews a `rigger reset --derived` would remove,
/// counted exactly as [`PrunedDerived`] reports the prune itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DerivedPreview {
    /// The rows the prune would delete, per covered type in the policy's order, zeros included.
    pub removed: Vec<(String, usize)>,
    /// How many of them record a superseded generation of their file.
    pub superseded_generations: usize,
}

/// What one derived-index compaction deletes and re-dates, decided by [`plan_derived_prune`].
struct DerivedPrunePlan {
    /// How many covered, keyed rows the selection weighed.
    rows: usize,
    /// `(index into the policy's types, position)` of every row to delete.
    deletes: Vec<(usize, i64)>,
    /// `(position, earliest valid-time)` of every surviving re-asserting row whose fact was
    /// first recorded earlier than its own valid-time.
    carries: Vec<(i64, i64)>,
    /// How many of `deletes` record a superseded generation.
    superseded: usize,
}

impl DerivedPrunePlan {
    /// The deletes counted per type, in the order `types` names them, zeros included.
    fn removed_per_type(&self, types: &[String]) -> Vec<(String, usize)> {
        let mut counts = vec![0usize; types.len()];
        for (t, _) in &self.deletes {
            counts[*t] += 1;
        }
        types.iter().cloned().zip(counts).collect()
    }
}

/// One re-asserted fact's valid-time as the selection walks its recordings newest first: the
/// earliest valid-time of its UNBROKEN run of generations, the run (see [`plan_derived_prune`])
/// it was last met in, and whether an older generation that did not assert it has already ended
/// the run.
struct FactRun {
    earliest: i64,
    run: usize,
    broken: bool,
}

/// The aliases each stream under `stream_prefix` defines, replayed from the log in position order
/// through the policy's own reading of a definition ([`FactIdentity::alias`]).
fn alias_histories(
    conn: &Connection,
    stream_prefix: &str,
    facts: &FactIdentity,
) -> Result<std::collections::HashMap<String, AliasHistory>, Error> {
    let mut stmt = conn
        .prepare(
            "SELECT position, stream, data FROM events
              WHERE type = ?2 AND substr(stream, 1, length(?1)) = ?1
              ORDER BY position",
        )
        .map_err(be)?;
    let mut rows = stmt
        .query(params![stream_prefix, facts.alias_type])
        .map_err(be)?;
    let mut histories: std::collections::HashMap<String, AliasHistory> =
        std::collections::HashMap::new();
    while let Some(row) = rows.next().map_err(be)? {
        let position: i64 = row.get(0).map_err(be)?;
        let stream: String = row.get(1).map_err(be)?;
        let data: Vec<u8> = row.get(2).map_err(be)?;
        if let Some((alias, canonical)) = (facts.alias)(&data) {
            histories
                .entry(stream)
                .or_default()
                .define(position as Position, alias, canonical);
        }
    }
    Ok(histories)
}

/// The types `identity` declares as re-asserting a fact in place - the valid-time partition every
/// act on [`plan_derived_prune`]'s carries needs - refused rather than defaulted when it is
/// undeclared or names a type the policy does not cover. Both failures are silent when they are
/// wrong - a re-dated fact leaves every row looking perfectly intact.
fn reasserting_types(identity: &ContentIdentity) -> Result<Vec<String>, Error> {
    let Some(declared) = identity.reasserting() else {
        return Err(Error::Backend(format!(
            "prune_derived_index: the content-identity policy for {:?} has not declared which \
             of its types re-assert a fact in place (ContentIdentity::with_reasserting_types). \
             Without it a compaction cannot know whether a key's EARLIEST recorded valid-time \
             is the one the projection holds, and either default silently re-dates facts. \
             Refusing rather than guessing.",
            identity.types()
        )));
    };
    if let Some(stray) = declared.iter().find(|t| !identity.covers(t)) {
        return Err(Error::Backend(format!(
            "prune_derived_index: the content-identity policy declares {stray:?} as \
             re-asserting, but does not cover that type ({:?}). A declaration naming a type \
             this policy will never prune describes some other policy, so it cannot be the \
             partition for this one. Refusing rather than pruning against a declaration that \
             does not fit.",
            identity.types()
        )));
    }
    Ok(identity
        .types()
        .iter()
        .filter(|t| identity.reasserts(t) == Some(true))
        .cloned()
        .collect())
}

/// The ONE selection of a derived-index compaction, shared by the prune, its read-only preview
/// and the `rigger validate` bloat measurement: which rows go, and which surviving rows take an
/// earlier valid-time.
///
/// One pass over the covered, keyed rows under `stream_prefix`, NEWEST FIRST, so the first row
/// met for a `(stream, batch identity)` names that identity's LATEST recorded generation
/// ([`ContentIdentity::key_parts`]) and the first row met for a `(stream, type, key)` is that
/// key's latest recording. A row is deleted when its generation is not its identity's latest
/// (a superseded generation: a file that later returns to that content re-emits its batch, so
/// the recording is never needed again), or when a later recording of its exact key exists (the
/// exact-key dedup). A key the policy cannot parse is its own identity, so it is only ever
/// deduplicated, never shed as superseded.
///
/// For the `reasserting` types the fold keeps the EARLIEST valid-time of a fact that has held
/// WITHOUT A BREAK: a newer generation revives each fact its prior generation asserted, and a
/// fact a generation dropped is new again when a later one asserts it. So the walk numbers each
/// identity's RUNS - maximal stretches of recordings of one generation, run 0 the latest - and a
/// surviving row takes the minimum valid-time over the recordings of the same identity asserting
/// the same fact - keyed by the policy's [`FactIdentity`], the fold's own key, with names resolved
/// through the aliases its stream defined before each recording - in consecutive runs from its own,
/// stopping at the first run that does not assert it, and over the earlier recordings of its own
/// exact key in its own run, however their payloads are spelled. That carries a fact every
/// generation re-asserted back to the generation that first asserted it, and never past a
/// generation that dropped it.
fn plan_derived_prune(
    conn: &Connection,
    stream_prefix: &str,
    identity: &ContentIdentity,
    reasserting: &[String],
) -> Result<DerivedPrunePlan, Error> {
    use std::collections::{HashMap, HashSet};
    let key = key_expr(identity.meta_key());
    let types = identity.types();
    let sql = format!(
        "SELECT position, stream, type, {key}, valid_from,
                CASE WHEN type IN ({reasserting}) THEN data END
           FROM events
          WHERE type IN ({covered})
            AND substr(stream, 1, length(?1)) = ?1
            AND {key} IS NOT NULL
          ORDER BY position DESC",
        reasserting = type_list(reasserting),
        covered = type_list(types),
    );
    let mut stmt = conn.prepare(&sql).map_err(be)?;
    let mut rows = stmt.query(params![stream_prefix]).map_err(be)?;
    let aliases = match identity.facts() {
        Some(facts) => alias_histories(conn, stream_prefix, facts)?,
        None => std::collections::HashMap::new(),
    };
    // (stream, identity) -> (latest generation, generation of the run being walked, its number).
    let mut runs: HashMap<(String, String), (String, String, usize)> = HashMap::new();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    // (stream, type, identity, fact key) -> the fact's run so far.
    type Fact = (String, String, String, Vec<u8>);
    let mut facts: HashMap<Fact, FactRun> = HashMap::new();
    // (stream, type, key) of a surviving re-asserting row -> (its run, the earliest valid-time of
    // the key's recordings in that run): the exact-key carry, blind to how a payload is spelled.
    type Key = (String, String, String);
    let mut keys: HashMap<Key, (usize, i64)> = HashMap::new();
    // position of a surviving re-asserting row -> (its own valid-time, its fact, its key).
    let mut survivors: Vec<(i64, i64, Fact, Key)> = Vec::new();
    let mut plan = DerivedPrunePlan {
        rows: 0,
        deletes: Vec::new(),
        carries: Vec::new(),
        superseded: 0,
    };
    while let Some(row) = rows.next().map_err(be)? {
        let position: i64 = row.get(0).map_err(be)?;
        let stream: String = row.get(1).map_err(be)?;
        let type_: String = row.get(2).map_err(be)?;
        let content_key: String = row.get(3).map_err(be)?;
        let valid_from: i64 = row.get(4).map_err(be)?;
        let payload: Option<Vec<u8>> = row.get(5).map_err(be)?;
        let Some(type_index) = types.iter().position(|t| *t == type_) else {
            continue;
        };
        plan.rows += 1;
        let (batch, generation) = identity
            .key_parts(&content_key)
            .unwrap_or((content_key.as_str(), ""));
        let batch = batch.to_string();
        let walk = runs
            .entry((stream.clone(), batch.clone()))
            .or_insert_with(|| (generation.to_string(), generation.to_string(), 0));
        if walk.1 != generation {
            walk.1 = generation.to_string();
            walk.2 += 1;
        }
        let run = walk.2;
        let superseded = walk.0 != generation;
        let exact = (stream.clone(), type_.clone(), content_key);
        let survives = !superseded && seen.insert(exact.clone());
        if !survives {
            plan.deletes.push((type_index, position));
        }
        if superseded {
            plan.superseded += 1;
        }
        let fact = payload.and_then(|payload| match identity.facts() {
            Some(facts) => {
                let history = aliases.get(&stream);
                let resolve = |mention: &str| match history {
                    Some(h) => h.resolve(mention, position as Position),
                    None => mention.to_string(),
                };
                (facts.fact)(&type_, &payload, &resolve)
            }
            None => Some(payload),
        });
        if let Some(fact) = fact {
            if let Some((key_run, earliest)) = keys.get_mut(&exact) {
                if *key_run == run {
                    *earliest = (*earliest).min(valid_from);
                }
            }
            let fact_key = (stream, type_, batch, fact);
            let fact = facts.entry(fact_key.clone()).or_insert(FactRun {
                earliest: valid_from,
                run,
                broken: false,
            });
            fact.broken |= run > fact.run + 1;
            if !fact.broken {
                fact.earliest = fact.earliest.min(valid_from);
                fact.run = run;
            }
            if survives {
                keys.insert(exact.clone(), (run, valid_from));
                survivors.push((position, valid_from, fact_key, exact));
            }
        }
    }
    for (position, own, fact, exact) in survivors {
        let earliest = facts[&fact].earliest.min(keys[&exact].1);
        // `earliest` is a minimum over a set holding `own`, so inequality is the one case with
        // an earlier date to carry.
        if earliest != own {
            plan.carries.push((position, earliest));
        }
    }
    plan.carries.sort_unstable();
    Ok(plan)
}

/// What [`Store::measure_derived_duplication`] found: how many rows carry a covered derived-
/// index key, and how many of them a compaction keeps - the read-only measurement `rigger
/// validate`'s bloat advisory (spec 68) warns from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DerivedDuplication {
    /// Rows in scope carrying a covered, non-null key.
    pub rows: usize,
    /// Of those, the rows `rigger reset --derived` keeps: the latest recording of each key of
    /// each identity's latest generation.
    pub kept: usize,
}

impl DerivedDuplication {
    /// Rows per kept row: `1.0` when a compaction would shed nothing (or there are no covered
    /// rows at all - `kept == 0` is guarded rather than divided by, since "nothing to measure" is
    /// not evidence of bloat), rising with the log's redundancy.
    pub fn factor(&self) -> f64 {
        if self.kept == 0 {
            1.0
        } else {
            self.rows as f64 / self.kept as f64
        }
    }
}

/// Total bytes the database at `db` occupies on disk: the main file plus its write-ahead log,
/// which is where a WAL-mode database's most recent pages live until a checkpoint folds them
/// back. Counting only the main file would report a reclamation over a log whose `-wal` had just
/// grown by more than the file shrank.
///
/// A file that is not there counts as zero rather than failing: the `-wal` does not exist before
/// the first write and is deleted on a clean close, and neither absence is an error about the
/// space the log occupies.
fn bytes_on_disk(db: &str) -> u64 {
    let len = |p: &str| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    len(db) + len(&format!("{db}-wal"))
}

/// What the post-commit space reclamation did to the file, which is the only thing about it the
/// prune cannot work out for itself.
///
/// Three outcomes rather than a byte count, because HOW MANY bytes the log lost is a property of
/// the whole command (measured either side of it by [`Store::prune_derived_index`]) while WHETHER
/// the file was rewritten, and whether the rewrite has landed on disk yet, are properties only
/// this step knows. Reported as a value rather than inferred by the caller from a zero, because
/// "was not rewritten" and "was rewritten and reclaimed nothing" are different things to tell an
/// operator and neither can be read off a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compaction {
    /// The file held no reclaimable free space, so it was NOT rewritten at all.
    Skipped,
    /// The file was rewritten and the result is on disk now: the truncating checkpoint folded the
    /// write-ahead log back into the main file.
    Landed,
    /// The file was rewritten, but the freed frames are still in the `-wal`: a concurrent reader
    /// held a snapshot of the write-ahead log, so they land at some later checkpoint instead.
    Pending,
}

/// Reclaim on disk the space the file is holding free, and report whether that reclamation has
/// landed - or that there was none to do.
///
/// Separated from the prune because it runs AFTER the commit, where a failure is a fact to report
/// rather than an outcome to propagate: by the time this is called the deletes are durable, so its
/// `Err` describes an un-reclaimed log rather than an un-pruned one.
fn compact_in_place(conn: &Connection) -> Result<Compaction, Error> {
    // WHAT THERE IS TO RECLAIM DECIDES WHETHER THE FILE IS TOUCHED - not what this pass deleted.
    // The freelist is where every delete's freed pages go and where they stay until something
    // vacuums, so it is the exact question "is a rewrite worth its cost", asked of the file
    // rather than of the caller. It answers the two directions the caller must not get wrong:
    // a file with nothing to reclaim is never rewritten to reclaim nothing, and a file that IS
    // holding free space is reclaimed even when this pass deleted none of it - which is what
    // makes re-running the command the real remedy for a reclamation that failed.
    let free_pages: i64 = conn
        .query_row("PRAGMA freelist_count", [], |r| r.get(0))
        .map_err(be)?;
    if free_pages == 0 {
        return Ok(Compaction::Skipped);
    }
    // VACUUM cannot run inside a transaction, so it follows the commit.
    conn.execute_batch("VACUUM").map_err(be)?;
    // Fold the WAL back into the main file so the shrink lands on disk NOW rather than at some
    // later checkpoint: the reported reclamation must match what the operator sees on disk.
    //
    // `PRAGMA wal_checkpoint` RETURNS ITS OUTCOME, and TRUNCATE is the mode that can decline:
    // its first column is 1 when a reader still held a snapshot of the write-ahead log, in
    // which case the frames stay in the `-wal` file and the file on disk did NOT shrink -
    // total bytes on disk can even go UP. Discarding that column is what would turn the
    // caller's before-and-after into a claim, so it is read. Retried a bounded number of
    // times because a blocked checkpoint is transient (the deletes are already committed and
    // the vacuum is done, so this is only about WHEN the frames land), and when it is still
    // blocked the reclamation is reported as UNKNOWN rather than as a number the operator's
    // own `ls` contradicts.
    for attempt in 0..CHECKPOINT_TRUNCATE_ATTEMPTS {
        let busy: i64 = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .map_err(be)?;
        if busy == 0 {
            return Ok(Compaction::Landed);
        }
        if attempt + 1 < CHECKPOINT_TRUNCATE_ATTEMPTS {
            std::thread::sleep(CHECKPOINT_TRUNCATE_BACKOFF);
        }
    }
    Ok(Compaction::Pending)
}

/// How many times [`compact_in_place`] asks a blocked `wal_checkpoint(TRUNCATE)` again before it
/// reports the on-disk reclamation as unmeasured, and how long it waits between asks.
///
/// Bounded and short on purpose: the prune's transaction has already committed and its vacuum has
/// already run by the time this matters, so the only thing at stake is whether the freed frames
/// land in the main file NOW or at the next checkpoint some later writer performs. Waiting a
/// reader out indefinitely would trade a correct, honestly-reported result for a hang.
const CHECKPOINT_TRUNCATE_ATTEMPTS: u32 = 5;
const CHECKPOINT_TRUNCATE_BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);

/// What one [`Store::prune_derived_index`] pass removed: the rows deleted PER TYPE (in the order
/// the caller named the types, including the types nothing was removed from), the bytes the log
/// lost on disk, whether it was rewritten to lose them at all, whether there was a file to
/// measure them over in the first place, and - when the reclamation failed after the deletes had
/// committed - what went wrong with it.
///
/// Per type, not just a total, because that is what an operator can check a prune against: a
/// single number cannot be compared to what the log was expected to hold. And the reclamation's
/// failure is a FIELD rather than an error return for the same reason: the deletes are durable
/// before the reclamation is attempted, so a prune whose reclamation failed still has counts an
/// operator needs, and an `Err` carrying only the failure describes a log that was in fact pruned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrunedDerived {
    /// `(type, rows deleted)`, in the order the caller named the types.
    pub removed: Vec<(String, usize)>,
    /// How many of the deleted rows recorded a SUPERSEDED generation of their batch identity
    /// (one that is not the latest the log records for it), as opposed to an earlier recording
    /// of a key that survives.
    pub superseded_generations: usize,
    /// Bytes the LOG LOST ON DISK across this whole call, or `None` when that could not be
    /// measured because a concurrent reader still held a write-ahead-log snapshot when the
    /// truncating checkpoint ran, because the reclamation itself failed (see
    /// [`PrunedDerived::compaction_error`]), or because the database has no file behind it (see
    /// [`PrunedDerived::on_disk_measured`], which is what tells those last two `None`s apart).
    ///
    /// MEASURED, NOT DERIVED, and measured over the pair of files an operator's own `du` would
    /// add up: the main database plus its `-wal`, sampled before the deletes and again after the
    /// rewrite has landed. A page-count delta is a tempting substitute and is not the same
    /// number - it is the database's LOGICAL size, it counts pages living only in an
    /// un-checkpointed write-ahead log, and a report built from it can name a reclamation over a
    /// file that grew.
    ///
    /// An `Option`, not a `0`. The bytes are on disk only once the checkpoint folds the
    /// write-ahead log back into the main file and truncates it; while a reader holds a snapshot
    /// that fold is declined, the freed frames stay in the `-wal`, and any difference measured
    /// then is between two states of a move that has not finished. `None` says "unmeasured, the
    /// pages land at the next checkpoint" and is the honest report; `Some(0)` would claim a
    /// measurement that found nothing.
    ///
    /// ONE case is `Some(0)` and is exact: a pass over a file holding NO FREE SPACE, where the
    /// rewrite is deliberately not run at all (see [`PrunedDerived::compaction_ran`]). There
    /// "zero bytes reclaimed" is the measurement rather than a measurement that could not be
    /// taken, and reporting it as `None` would send an operator looking for pages that some later
    /// checkpoint will land.
    pub reclaimed_bytes: Option<u64>,
    /// Whether the file was REWRITTEN at all.
    ///
    /// `false` says the rewrite was deliberately skipped because the file held no reclaimable
    /// free page - the most expensive thing this command can do, declined because it would have
    /// reclaimed nothing. It is carried as its own fact because it cannot be read off the byte
    /// count: "not rewritten" and "rewritten, and it reclaimed nothing" are different things to
    /// tell an operator watching a compaction, and both would be `Some(0)`.
    ///
    /// It is NOT "this pass deleted nothing". A pass that deleted nothing still rewrites a file
    /// that has space to reclaim, which is exactly what makes re-running the command the remedy
    /// for a reclamation that failed after the deletes committed.
    pub compaction_ran: bool,
    /// Whether the before-measurement was TAKEN AT ALL: `true` when this database has a file on
    /// disk, so the pair of sizes the reclamation is a difference of were both sampled; `false`
    /// for a database with no file behind it (`:memory:`, a temporary database), where there was
    /// never anything on disk to measure.
    ///
    /// It exists because `reclaimed_bytes: None` alongside `compaction_ran: true` has TWO causes
    /// and the difference is invisible in the numbers: the truncating checkpoint was declined by
    /// a concurrent reader (the bytes exist and land later), or this database has no file (there
    /// are no bytes and none ever land). A consumer told only "unmeasured" cannot tell them
    /// apart, so it either reports one cause for both - asserting a reader it was never told
    /// about - or reports neither. Only the prune knows, so the prune carries it.
    ///
    /// It says nothing about whether the AFTER measurement was usable: a checkpoint a reader
    /// declined leaves this `true` and the byte count `None`, which is exactly the pair that
    /// separates the two causes.
    pub on_disk_measured: bool,
    /// Why the space reclamation did not complete, when it was attempted and failed - `None` when
    /// it succeeded, and `None` when there was no free space for it to reclaim.
    ///
    /// It is reported rather than returned because it happens AFTER the commit: the rows are gone
    /// from the log whatever this says, so it names a log that is pruned but not shrunk, and
    /// re-running the prune is safe AND useful - the second pass finds nothing to delete, but the
    /// space this one failed to reclaim is still free in the file, so the reclamation is tried
    /// again over it.
    pub compaction_error: Option<String>,
}

impl PrunedDerived {
    /// Every row this pass deleted, across all types.
    pub fn total_removed(&self) -> usize {
        self.removed.iter().map(|(_, n)| n).sum()
    }
}

fn be<E: std::fmt::Display>(e: E) -> Error {
    Error::Backend(e.to_string())
}

/// A single-quoted SQL string literal for `s` (doubling any embedded quote). Used only
/// for policy values rendered into maintenance SQL - never for per-append data, which is
/// always bound.
fn sql_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The SQL expression that reads an event's content key out of its metadata, rendered
/// from one function so every maintenance statement reads the key identically.
///
/// TWO nested quotings, and both are the caller's string: the metadata key is a JSON
/// member name inside a path, and that whole path is then a SQL string literal. So the
/// key is escaped for JSON (a backslash or a quote inside a member name is spelled with a
/// backslash) and the finished path goes through [`sql_literal`] like any other literal,
/// rather than being pasted between hand-written quotes - a single apostrophe in a
/// consumer's key would otherwise end the literal early and leave the rest of the path as
/// stray SQL.
///
/// ONE SHAPE THIS CANNOT ADDRESS, and it fails safe rather than silently wrong: SQLite's
/// JSON path parser accepts an escaped backslash inside a quoted member name but NOT an
/// escaped double quote (3.46). A metadata key carrying a `"` is therefore not reachable
/// by any `json_extract` path, so such a key reads as absent and is never mistaken for
/// another.
fn key_expr(meta_key: &str) -> String {
    format!("json_extract(meta, {})", key_path(meta_key))
}

/// The quoted JSON path a content key is addressed by (`$."<meta_key>"`), as a SQL literal - the
/// ONE rendering of the path, shared by every expression that reads a content key.
fn key_path(meta_key: &str) -> String {
    let path = format!(
        "$.\"{}\"",
        meta_key.replace('\\', "\\\\").replace('"', "\\\"")
    );
    sql_literal(&path)
}

/// The `IN (...)` list of the covered event types.
///
/// A policy that covers NO type renders `NULL`, not an empty list: `type IN ()` is not
/// parsable SQL, while `type IN (NULL)` is never true - so a policy with no covered types
/// matches nothing, which is what covering no type means.
fn type_list(types: &[String]) -> String {
    if types.is_empty() {
        return "NULL".to_string();
    }
    types
        .iter()
        .map(|t| sql_literal(t))
        .collect::<Vec<_>>()
        .join(", ")
}

fn meta_json(m: &BTreeMap<String, String>) -> String {
    serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string())
}

fn parse_meta(s: &str) -> BTreeMap<String, String> {
    serde_json::from_str(s).unwrap_or_default()
}

fn like_of(filter: &Filter) -> String {
    filter
        .stream_prefix
        .as_ref()
        .map(|p| format!("{p}%"))
        .unwrap_or_else(|| "%".to_string())
}

fn row_to_event(r: &rusqlite::Row) -> rusqlite::Result<Event> {
    let meta: String = r.get(5)?;
    Ok(Event {
        position: r.get::<_, i64>(0)? as Position,
        stream: r.get(1)?,
        type_: r.get(2)?,
        id: r.get(3)?,
        data: r.get(4)?,
        meta: parse_meta(&meta),
        valid_from: from_nanos(r.get(6)?),
        recorded_at: from_nanos(r.get(7)?),
        revision: r.get::<_, i64>(8)? as Revision,
    })
}

impl EventStore for Store {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        let mut guard = self.conn.lock().unwrap();
        // BEGIN IMMEDIATE, not the default BEGIN DEFERRED: acquire the write lock up
        // front so a second connection (a separate process - the death courier racing
        // the worker's self-report) QUEUES on `busy_timeout` instead of starting a read
        // snapshot it must later upgrade. A deferred read->write upgrade under WAL with a
        // concurrent writer cannot be resolved by the busy handler (SQLITE_BUSY_SNAPSHOT)
        // and surfaces as a hard `database is locked` backend error; taking the write lock
        // immediately makes concurrent appenders serialize cleanly, so a stale expectation
        // surfaces as the port's `Error::Conflict` (which callers retry) and never as a
        // spurious lock error. This is what the module header promises ("concurrent
        // appenders queue instead of deadlocking on the SQLITE_BUSY class") and what the
        // optimistic-concurrency contract needs to hold across connections, not just
        // within one in-process `Store`.
        let tx = guard
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(be)?;

        // The stream's cursor is the revision its LAST ROW IN POSITION ORDER holds - the
        // most recently written row, whatever revision number it carries - not its row
        // count minus one and not the highest revision value the stream holds ANYWHERE.
        // Position order and "the highest revision value" agree on every stream that has
        // only ever been written through this function: a write only ever lands at
        // `last_revision + 1`, so each new row is simultaneously the newest by position
        // AND the highest by revision, and deleting an arbitrary subset of rows (what the
        // supported compaction, `Store::prune_derived_index`, does, leaving holes in the
        // revision sequence) cannot change that relative order among whatever survives.
        // A count-derived cursor would reissue a revision the stream still holds and
        // collide on the `UNIQUE(stream, revision)` index; the position-order seek is
        // exactly as gap-tolerant (it is a seek on the same `idx_events_stream` index,
        // reverse-ordered, not a table walk), so a compacted stream still gets its true
        // next revision. What position order buys OVER the highest-revision-value seek is
        // honesty when the two have already come apart: read below.
        let last_revision: Revision = tx
            .query_row(
                "SELECT revision FROM events WHERE stream = ?1 ORDER BY position DESC LIMIT 1",
                params![stream],
                |r| r.get(0),
            )
            .optional()
            .map_err(be)?
            .unwrap_or(NO_STREAM);
        let ok = match expected {
            ExpectedRevision::Any => true,
            ExpectedRevision::NoStream => last_revision == NO_STREAM,
            ExpectedRevision::Exact(v) => last_revision == v,
        };
        if !ok {
            return Err(Error::Conflict {
                stream: stream.to_string(),
                expected,
                actual: last_revision,
            });
        }

        // Spec 71 - APPEND REFUSES DISORDER. The candidate revision this call is about
        // to assign, `last_revision + 1`, must exceed the HIGHEST revision the stream
        // records anywhere - not just the one at its newest position. On every stream
        // this function has ever written to alone the two seeks agree (the correct
        // writer's cursor IS the max, so this never fires and costs one indexed seek in
        // the transaction already open). They can only disagree once the stream already
        // carries the incident's signature: a row at an EARLIER position holding a
        // HIGHER revision than the row at the NEWEST position, left behind by a write
        // that came from outside this function entirely (this function itself can never
        // produce it - see the seek above). Refusing here, before the insert, is what
        // stops that signature from silently compounding one honest append at a time,
        // and turns what would otherwise be a bare `UNIQUE(stream, revision)` failure
        // into a named refusal that says why.
        let recorded_max: Revision = tx
            .query_row(
                "SELECT COALESCE(MAX(revision), ?2) FROM events WHERE stream = ?1",
                params![stream, NO_STREAM],
                |r| r.get(0),
            )
            .map_err(be)?;
        if last_revision < recorded_max {
            return Err(Error::OutOfOrder {
                stream: stream.to_string(),
                attempted: last_revision + 1,
                recorded: recorded_max,
            });
        }

        // The store stamps recorded_at on ingest (one clock per batch).
        let recorded_at = to_nanos(SystemTime::now());
        // One slot per handed event, in input order.
        let mut placements: Vec<Option<Position>> = Vec::with_capacity(events.len());
        for (revision, e) in (last_revision + 1..).zip(events) {
            tx.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    stream,
                    e.type_,
                    e.id,
                    e.data,
                    meta_json(&e.meta),
                    to_nanos(e.valid_from),
                    recorded_at,
                    revision
                ],
            )
            .map_err(be)?;
            // The position the STORE issued for this row - read back from sqlite, not
            // derived from any other event's position.
            placements.push(Some(tx.last_insert_rowid() as Position));
        }
        tx.commit().map_err(be)?;
        Ok(Appended::from_placements(placements))
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        let order = direction_sql(dir);
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {COLS} FROM events WHERE stream = ?1 AND revision >= ?2 ORDER BY revision {order}"
        );
        let mut stmt = conn.prepare(&sql).map_err(be)?;
        let rows = stmt
            .query_map(params![stream, from], row_to_event)
            .map_err(be)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(be)
    }

    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        let order = direction_sql(dir);
        let like = like_of(filter);
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {COLS} FROM events WHERE position > ?1 AND stream LIKE ?2 ORDER BY position {order}"
        );
        let mut stmt = conn.prepare(&sql).map_err(be)?;
        let rows = stmt
            .query_map(params![from as i64, like], row_to_event)
            .map_err(be)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(be)
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        let conn = Arc::clone(&self.conn);
        let like = like_of(filter);
        Ok(spawn_subscription(
            move |state: &mut Watermark| {
                let guard = conn.lock().unwrap();
                poll_all(&guard, state.position, &like)
            },
            Watermark {
                position: from,
                revision: NO_STREAM,
            },
        ))
    }

    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
        let conn = Arc::clone(&self.conn);
        let stream = stream.to_string();
        Ok(spawn_subscription(
            move |state: &mut Watermark| {
                let guard = conn.lock().unwrap();
                poll_stream(&guard, &stream, state.revision)
            },
            // `revision > from-1` includes `from`.
            Watermark {
                position: 0,
                revision: from - 1,
            },
        ))
    }

    fn last_position(&self, stream: &str, event_type: &str) -> Result<Option<Revision>, Error> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(LAST_POSITION_SQL, params![stream, event_type], |r| r.get(0))
            .optional()
            .map_err(be)
    }

    fn read_stream_typed(
        &self,
        stream: &str,
        from: Revision,
        selection: TypeSelection,
    ) -> Result<Vec<Event>, Error> {
        let types = match selection {
            TypeSelection::Only(types) | TypeSelection::Except(types) => types,
        };
        let conn = self.conn.lock().unwrap();
        // The read is anchored on the EVENT at revision `from`, never on the revision number:
        // everything the log recorded from that event on is read, so a row a stale writer
        // reissued later at a lower revision is still read where the log holds it. Revision 0
        // (or below) is the stream's start.
        let start: i64 = if from > 0 {
            match conn
                .query_row(TYPED_READ_START_SQL, params![stream, from], |r| r.get(0))
                .optional()
                .map_err(be)?
            {
                Some(position) => position,
                None => return Ok(Vec::new()),
            }
        } else {
            0
        };
        let binds: Vec<rusqlite::types::Value> = [stream.to_string().into(), start.into()]
            .into_iter()
            .chain(types.iter().map(|t| t.to_string().into()))
            .collect();
        let mut stmt = conn.prepare(&typed_read_sql(selection)).map_err(be)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(binds), row_to_event)
            .map_err(be)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(be)
    }

    /// The stream's positions straight off the events table's `(stream, position)` rows, never a
    /// payload column, streamed row by row in position order.
    fn read_stream_positions(
        &self,
        stream: &str,
        batch: usize,
        sink: &mut dyn FnMut(&[Position]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT position FROM events WHERE stream = ?1 ORDER BY position")
            .map_err(be)?;
        let positions = stmt
            .query_map(params![stream], |r| r.get::<_, i64>(0))
            .map_err(be)?;
        super::positions_in_batches(
            positions.map(|p| p.map(|p| p as Position).map_err(be)),
            batch,
            sink,
        )
    }
}

/// The watermark a subscription's polling thread advances as it delivers events.
struct Watermark {
    position: Position,
    revision: Revision,
}

/// Spawn a polling subscription: `poll` returns the next batch given the current
/// watermark; the thread advances the watermark from each delivered event.
fn spawn_subscription<F>(poll: F, start: Watermark) -> Subscription
where
    F: Fn(&mut Watermark) -> rusqlite::Result<Vec<Event>> + Send + 'static,
{
    let (tx, rx) = channel();
    let err = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let err_thread = Arc::clone(&err);
    let handle = std::thread::spawn(move || {
        let mut state = start;
        while !stop_thread.load(Ordering::Relaxed) {
            match poll(&mut state) {
                Ok(events) => {
                    for e in events {
                        state.position = e.position;
                        state.revision = e.revision;
                        if tx.send(e).is_err() {
                            return; // the subscriber was dropped
                        }
                    }
                }
                Err(e) => {
                    *err_thread.lock().unwrap() = Some(e.to_string());
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    Subscription::new(rx, err, stop, handle)
}

fn poll_all(conn: &Connection, after: Position, like: &str) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events WHERE position > ?1 AND stream LIKE ?2 ORDER BY position ASC"
    ))?;
    let rows = stmt.query_map(params![after as i64, like], row_to_event)?;
    rows.collect()
}

fn poll_stream(conn: &Connection, stream: &str, after: Revision) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM events WHERE stream = ?1 AND revision > ?2 ORDER BY revision ASC"
    ))?;
    let rows = stmt.query_map(params![stream, after], row_to_event)?;
    rows.collect()
}

fn direction_sql(dir: Direction) -> &'static str {
    match dir {
        Direction::Forward => "ASC",
        Direction::Backward => "DESC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{plant_free_pages, pragma_i64};

    #[test]
    fn passes_the_contract() {
        crate::eventstore::contract::assert_contract(&Store::open(":memory:").unwrap());
    }

    /// THE BOUNDARY IS A QUERY, on this backend an INDEXED LOOKUP (spec 101): sqlite's own plan
    /// for the lookup is one search of the stream-and-type index, never a scan of the table, so
    /// its cost does not grow with the derived events the stream holds.
    #[test]
    fn the_boundary_lookup_is_one_seek_of_the_stream_and_type_index() {
        let s = Store::open(":memory:").unwrap();
        let conn = s.conn.lock().unwrap();
        let plan: Vec<String> = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {LAST_POSITION_SQL}"))
            .unwrap()
            .query_map(params!["rigger", "RunStarted"], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            plan,
            ["SEARCH events USING INDEX idx_events_stream_type (stream=? AND type=?)"],
            "the lookup must be a single index search with no scan and no sort step"
        );
    }

    #[test]
    fn assigns_per_stream_revisions() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "a",
            ExpectedRevision::Any,
            &[
                Event::new("A0", b"".to_vec()),
                Event::new("A1", b"".to_vec()),
            ],
        )
        .unwrap();
        s.append(
            "a",
            ExpectedRevision::Any,
            &[Event::new("A2", b"".to_vec())],
        )
        .unwrap();
        s.append(
            "b",
            ExpectedRevision::Any,
            &[Event::new("B0", b"".to_vec())],
        )
        .unwrap();
        let a = s.read_stream("a", 0, Direction::Forward).unwrap();
        assert_eq!(a.iter().map(|e| e.revision).collect::<Vec<_>>(), [0, 1, 2]);
        let b = s.read_stream("b", 0, Direction::Forward).unwrap();
        assert_eq!(b[0].revision, 0);
        // stream + valid_from round-trip
        assert_eq!(a[0].stream, "a");
    }

    #[test]
    fn has_stream_prefix_matches_literally_not_as_a_like_pattern() {
        let s = Store::open(":memory:").unwrap();
        // A project namespace whose basename carries a SQL `LIKE` wildcard (`_`).
        s.append(
            "proj-my_repo-run",
            ExpectedRevision::Any,
            &[Event::new("A", b"".to_vec())],
        )
        .unwrap();
        assert!(s.has_stream_prefix("proj-my_repo-").unwrap());
        // The `_` is a LITERAL, not a single-char wildcard: a different name must NOT match.
        assert!(!s.has_stream_prefix("proj-myXrepo-").unwrap());
        assert!(!s.has_stream_prefix("proj-absent-").unwrap());
    }

    #[test]
    fn rename_stream_prefix_moves_history_preserving_revisions() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "proj-old-run",
            ExpectedRevision::Any,
            &[
                Event::new("A", b"1".to_vec()),
                Event::new("B", b"2".to_vec()),
            ],
        )
        .unwrap();
        s.append(
            "proj-old-graph",
            ExpectedRevision::Any,
            &[Event::new("C", b"3".to_vec())],
        )
        .unwrap();
        // An unrelated namespace must be left untouched by the rename.
        s.append(
            "proj-keep-run",
            ExpectedRevision::Any,
            &[Event::new("K", b"".to_vec())],
        )
        .unwrap();

        let n = s.rename_stream_prefix("proj-old-", "proj-new-").unwrap();
        assert_eq!(n, 2, "two distinct streams (run + graph) moved");

        assert!(
            s.read_stream("proj-old-run", 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "the legacy stream is empty after the rename"
        );
        let run = s
            .read_stream("proj-new-run", 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            run.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(
            run.iter().map(|e| e.revision).collect::<Vec<_>>(),
            [0, 1],
            "per-stream revisions are preserved across the rename"
        );
        assert_eq!(
            s.read_stream("proj-new-graph", 0, Direction::Forward)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            s.read_stream("proj-keep-run", 0, Direction::Forward)
                .unwrap()
                .len(),
            1,
            "an unrelated namespace is untouched"
        );

        // Renaming again with nothing left under `from` is a no-op returning 0.
        assert_eq!(s.rename_stream_prefix("proj-old-", "proj-new-").unwrap(), 0);
    }

    #[test]
    fn conflict_reports_actual_revision() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run",
            ExpectedRevision::NoStream,
            &[Event::new("A", b"".to_vec()), Event::new("B", b"".to_vec())],
        )
        .unwrap();
        let err = s.append(
            "run",
            ExpectedRevision::NoStream,
            &[Event::new("C", b"".to_vec())],
        );
        match err {
            Err(Error::Conflict { actual, .. }) => {
                assert_eq!(actual, 1, "two events => last revision 1")
            }
            other => panic!("expected a conflict with actual revision, got {other:?}"),
        }
    }

    #[test]
    fn subscribe_stream_replays_then_goes_live() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "one",
            ExpectedRevision::Any,
            &[Event::new("PRE", b"".to_vec())],
        )
        .unwrap();
        s.append(
            "two",
            ExpectedRevision::Any,
            &[Event::new("OTHER", b"".to_vec())],
        )
        .unwrap();
        let sub = s.subscribe_stream("one", 0).unwrap();
        let first = sub
            .recv_timeout(Duration::from_secs(2))
            .expect("replay PRE");
        assert_eq!(first.type_, "PRE");
        s.append(
            "one",
            ExpectedRevision::Any,
            &[Event::new("LIVE", b"".to_vec())],
        )
        .unwrap();
        let second = sub.recv_timeout(Duration::from_secs(2)).expect("live LIVE");
        assert_eq!(second.type_, "LIVE");
        // the "two" stream's event must never arrive on a "one" subscription
        assert!(
            sub.try_recv().is_none() || sub.try_recv().map(|e| e.stream == "one").unwrap_or(true)
        );
    }

    #[test]
    fn subscribe_all_replays_then_goes_live() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("A", b"1".to_vec())],
        )
        .unwrap();
        let sub = s.subscribe_all(0, &Filter::default()).unwrap();
        let first = sub.recv_timeout(Duration::from_secs(2)).expect("replay A");
        assert_eq!(first.type_, "A");
        s.append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("B", b"2".to_vec())],
        )
        .unwrap();
        let second = sub.recv_timeout(Duration::from_secs(2)).expect("live B");
        assert_eq!(second.type_, "B");
    }

    #[test]
    fn read_all_filters_by_prefix() {
        let s = Store::open(":memory:").unwrap();
        s.append(
            "run-a",
            ExpectedRevision::Any,
            &[Event::new("X", b"1".to_vec())],
        )
        .unwrap();
        s.append(
            "other",
            ExpectedRevision::Any,
            &[Event::new("Y", b"2".to_vec())],
        )
        .unwrap();
        let filter = Filter {
            stream_prefix: Some("run-".to_string()),
        };
        let events = s.read_all(0, Direction::Forward, &filter).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].type_, "X");
        assert_eq!(events[0].stream, "run-a");
    }

    #[test]
    fn concurrent_cross_connection_appends_serialize_without_spurious_lock_errors() {
        // Two SEPARATE connections (two `Store` handles on one on-disk db - the
        // two-process shape of the death courier racing a worker's self-report) append
        // to the SAME stream at once, with NO shared in-process mutex to serialize them.
        // Under the default BEGIN DEFERRED a read->write upgrade with a concurrent writer
        // under WAL cannot be resolved by `busy_timeout` (SQLITE_BUSY_SNAPSHOT) and
        // surfaces as a hard `database is locked` backend error the optimistic layer
        // cannot retry. BEGIN IMMEDIATE takes the write lock up front, so the appenders
        // QUEUE and every write lands - which is what the module header promises and what
        // record_result_if_absent's compare-and-append relies on across connections. The
        // in-process contract test (`concurrent_appends_to_distinct_streams...`) cannot
        // reach this: its single `Mutex<Connection>` serializes the appends so they never
        // contend at the sqlite layer.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let path = path.to_str().unwrap().to_string();

        // Open both connections up front (serialized) so we race only the appends.
        let a = Arc::new(Store::open(&path).unwrap());
        let b = Arc::new(Store::open(&path).unwrap());

        const ROUNDS: usize = 40;
        let barrier = Arc::new(std::sync::Barrier::new(2));

        let spawn_writer = |s: Arc<Store>, bar: Arc<std::sync::Barrier>| {
            std::thread::spawn(move || {
                let mut hard_errs = 0usize;
                for _ in 0..ROUNDS {
                    bar.wait();
                    match s.append(
                        "run",
                        ExpectedRevision::Any,
                        &[Event::new("R", b"x".to_vec())],
                    ) {
                        Ok(_) => {}
                        // A stale-expectation conflict is a legitimate optimistic outcome;
                        // a lock error is the regression this test guards against.
                        Err(Error::Conflict { .. }) => {}
                        Err(_) => hard_errs += 1,
                    }
                }
                hard_errs
            })
        };

        let ha = spawn_writer(a.clone(), barrier.clone());
        let hb = spawn_writer(b.clone(), barrier.clone());
        let hard_errs = ha.join().unwrap() + hb.join().unwrap();
        assert_eq!(
            hard_errs, 0,
            "concurrent cross-connection appends must queue, never hard-fail with a lock error"
        );

        // Every one of the 2 * ROUNDS appends is durably recorded, with contiguous,
        // unique per-stream revisions - no lost write, no gap, no duplicated revision.
        let events = a.read_stream("run", 0, Direction::Forward).unwrap();
        assert_eq!(
            events.len(),
            2 * ROUNDS,
            "every concurrent append must be durably recorded"
        );
        let revs: Vec<Revision> = events.iter().map(|e| e.revision).collect();
        let expected: Vec<Revision> = (0..2 * ROUNDS as Revision).collect();
        assert_eq!(
            revs, expected,
            "per-stream revisions must stay contiguous and unique under concurrency"
        );
    }

    /// THE TYPED READ IS ANCHORED ON THE EVENT (spec 101): a typed read from a revision starts at
    /// that revision's EVENT and hands back everything the log recorded after it, in log order -
    /// so a row a stale writer reissued at the newest position under revision 0 (spec 71's
    /// corruption signature) is read after a run boundary at revision 1, where a fold can see the
    /// disorder, and a read from revision 0 still reads the whole stream rather than starting at
    /// the reissued row.
    #[test]
    fn a_typed_read_hands_back_a_reissued_row_where_the_log_recorded_it() {
        let store = Store::open(":memory:").unwrap();
        let events: Vec<Event> = ["Pre", "RunStarted", "Work", "Lesson"]
            .iter()
            .map(|t| Event::new(*t, b"{}".to_vec()))
            .collect();
        store
            .append("s", ExpectedRevision::NoStream, &events)
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("DELETE FROM events WHERE stream = 's' AND revision = 0", [])
                .unwrap();
            conn.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, \
                 revision) VALUES ('s', 'Pre', 'reissued', X'7b7d', '{}', 0, 0, 0)",
                [],
            )
            .unwrap();
        }
        let read = |from: Revision, selection: TypeSelection| -> Vec<(String, Revision)> {
            store
                .read_stream_typed("s", from, selection)
                .unwrap()
                .into_iter()
                .map(|e| (e.type_, e.revision))
                .collect()
        };
        let pairs = |v: &[(&str, Revision)]| -> Vec<(String, Revision)> {
            v.iter().map(|(t, r)| (t.to_string(), *r)).collect()
        };
        assert_eq!(
            read(0, TypeSelection::Except(&[])),
            pairs(&[("RunStarted", 1), ("Work", 2), ("Lesson", 3), ("Pre", 0)]),
            "revision 0 is the stream's start, whatever row now holds it"
        );
        assert_eq!(
            read(0, TypeSelection::Only(&["Pre", "Lesson"])),
            pairs(&[("Lesson", 3), ("Pre", 0)]),
            "an Only read hands back log order too"
        );
        assert_eq!(
            read(1, TypeSelection::Except(&["Lesson"])),
            pairs(&[("RunStarted", 1), ("Work", 2), ("Pre", 0)]),
            "the reissued row sits after the boundary event, so the slice from it holds it"
        );
        assert_eq!(
            read(3, TypeSelection::Only(&["Pre"])),
            pairs(&[("Pre", 0)]),
            "anchored on the event at revision 3, not on revisions at or above 3"
        );
        assert_eq!(
            read(4, TypeSelection::Except(&[])),
            pairs(&[]),
            "a revision past the stream's last reads nothing"
        );
    }

    /// Spec 71 - APPEND REFUSES DISORDER. Reproduces the incident's exact signature
    /// out of band: this shape is UNREACHABLE through the safe `append` API alone (a
    /// correct writer's revision cursor always strictly extends both position and
    /// revision order together, so it can never land at or below a revision the
    /// stream already holds - see [`Store::append`]'s own comment on that seek). A
    /// compaction that deletes a stream's revision-1 row leaves a hole; a stale
    /// writer (an older build, running its OWN insert - never this function) then
    /// reissues that freed revision at the stream's NEWEST position, exactly as the
    /// recorded incident's writer did after the log's derived-index compaction ran.
    /// The next honest append onto that stream must refuse rather than silently
    /// build past the disagreement, naming the stream, both revisions, and the
    /// likely cause.
    #[test]
    fn append_refuses_a_stream_whose_position_order_and_revision_order_already_disagree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let path = path.to_str().unwrap().to_string();
        let store = Store::open(&path).unwrap();

        // A healthy stream: revisions 0..=4, position order and revision order agree.
        for i in 0..5u8 {
            store
                .append("s", ExpectedRevision::Any, &[Event::new("E", vec![i])])
                .unwrap();
        }

        // Out-of-band: delete revision 1's row (the compaction's hole), then reissue
        // that freed revision as a brand-new row at the stream's newest position (the
        // stale writer's insert - this is exactly what `append` refuses to do itself,
        // so it can only be reproduced by going around it).
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("DELETE FROM events WHERE stream = 's' AND revision = 1", [])
                .unwrap();
            conn.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, \
                 revision) VALUES ('s', 'E', 'reissued', X'00', '{}', 0, 0, 1)",
                [],
            )
            .unwrap();
        }
        // Position order for "s" is now: rev 0, rev 3, rev 4, rev 2, rev 1 (newest).
        // MAX(revision) is still 4; the position-order tail is revision 1.

        let err = store
            .append("s", ExpectedRevision::Any, &[Event::new("E", vec![9])])
            .expect_err("an append onto an already-disordered stream must refuse");
        match &err {
            Error::OutOfOrder {
                stream,
                attempted,
                recorded,
            } => {
                assert_eq!(stream, "s", "the refusal names the stream: {err}");
                assert_eq!(
                    *attempted, 2,
                    "names the revision it would have written: {err}"
                );
                assert_eq!(
                    *recorded, 4,
                    "names the revision already recorded that it would not sort after: {err}"
                );
            }
            other => panic!("expected Error::OutOfOrder, got {other:?}"),
        }
        let message = err.to_string();
        assert!(
            message.to_lowercase().contains("stale"),
            "the refusal names the likely cause: {message}"
        );
        assert!(
            message.contains("compaction"),
            "the refusal points at the likely cause's origin: {message}"
        );

        // Nothing was written: the disordered stream is exactly as it was before the
        // refused attempt.
        assert_eq!(
            store.read_stream("s", 0, Direction::Forward).unwrap().len(),
            5,
            "a refused append writes nothing"
        );

        // A correct append on a DIFFERENT, never-disordered stream is untouched: it
        // succeeds and reads back at revision 0, exactly as any first append does.
        store
            .append(
                "clean",
                ExpectedRevision::NoStream,
                &[Event::new("E", vec![1])],
            )
            .expect("a correct append on a healthy stream proceeds normally");
        let clean = store.read_stream("clean", 0, Direction::Forward).unwrap();
        assert_eq!(clean.len(), 1);
        assert_eq!(clean[0].revision, 0);
    }

    // --- Spec 60, criterion 5: the prune's POST-COMMIT half is reported, never propagated ---

    /// A store holding `rounds` recordings of one derived-index replay key, in one namespaced
    /// stream, plus a non-derived event that no prune may touch. The duplication the prune sheds.
    fn seeded_with_duplicated_key(path: &str, rounds: usize) -> Store {
        let mut events = vec![Event::new("RunStarted", b"{}".to_vec())];
        for _ in 0..rounds {
            events.push(keyed(
                crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                "gc/src/a.rs@h1#0",
            ));
        }
        store_with(path, &[("run", events)])
    }

    /// An event of `type_` carrying the derived-index replay key `key`.
    fn keyed(type_: &str, key: &str) -> Event {
        Event::new(type_, b"{}".to_vec()).with_meta(crate::ingest::META_REPLAY_KEY, key)
    }

    /// A store at `path` holding each `(stream, events)` batch, appended in order.
    fn store_with(path: &str, batches: &[(&str, Vec<Event>)]) -> Store {
        let s = Store::open(path).unwrap();
        for (stream, events) in batches {
            s.append(stream, ExpectedRevision::Any, events).unwrap();
        }
        s
    }

    /// The number of rows the log holds for the seeded replay key, read through a connection of
    /// its own so the count is the file's and not the store's view of it.
    fn recordings_of_the_key(path: &str) -> i64 {
        // Read through the store's OWN key expression, so the count can never be of a key this
        // store spells differently from the way the test wrote it.
        let sql = format!(
            "SELECT COUNT(*) FROM events WHERE {} = ?1",
            key_expr(crate::ingest::META_REPLAY_KEY)
        );
        Connection::open(path)
            .unwrap()
            .query_row(&sql, params!["gc/src/a.rs@h1#0"], |r| r.get(0))
            .unwrap()
    }

    // --- Spec 68, VALIDATE ADVISORIES: measure_derived_duplication, the prune's read-only twin ---

    #[test]
    fn measure_derived_duplication_reports_rows_vs_the_rows_a_compaction_keeps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path = path.to_str().unwrap();
        let s = seeded_with_duplicated_key(path, 4);
        let measured = s
            .measure_derived_duplication("", &crate::ingest::derived_index_identity())
            .unwrap();
        assert_eq!(measured.rows, 4, "four recordings of the one covered key");
        assert_eq!(measured.kept, 1, "all four share the same replay key");
        assert_eq!(measured.factor(), 4.0);
    }

    #[test]
    fn measure_derived_duplication_is_read_only_and_never_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path = path.to_str().unwrap();
        let s = seeded_with_duplicated_key(path, 3);
        let _ = s
            .measure_derived_duplication("", &crate::ingest::derived_index_identity())
            .unwrap();
        assert_eq!(
            recordings_of_the_key(path),
            3,
            "measuring must never delete anything - that is the prune's job, not this read"
        );
    }

    #[test]
    fn measure_derived_duplication_scopes_to_the_stream_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_with(
            path.to_str().unwrap(),
            &[
                (
                    "proj-a/run",
                    vec![
                        keyed(
                            crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                            "gc/src/a.rs@h1#0",
                        ),
                        keyed(
                            crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                            "gc/src/a.rs@h1#0",
                        ),
                    ],
                ),
                (
                    "proj-b/run",
                    vec![keyed(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/src/a.rs@h1#0",
                    )],
                ),
            ],
        );
        let measured = s
            .measure_derived_duplication("proj-a/", &crate::ingest::derived_index_identity())
            .unwrap();
        assert_eq!(measured.rows, 2, "only proj-a's rows are in scope");
        assert_eq!(measured.kept, 1);
    }

    #[test]
    fn measure_derived_duplication_on_a_clean_log_reports_no_duplication() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_with(
            path.to_str().unwrap(),
            &[(
                "run",
                vec![
                    keyed(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/src/a.rs@h1#0",
                    ),
                    keyed(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/src/b.rs@h1#0",
                    ),
                ],
            )],
        );
        let measured = s
            .measure_derived_duplication("", &crate::ingest::derived_index_identity())
            .unwrap();
        assert_eq!(measured.rows, 2);
        assert_eq!(measured.kept, 2);
        assert_eq!(measured.factor(), 1.0);
    }

    #[test]
    fn measure_derived_duplication_treats_the_same_key_under_two_covered_types_as_two_distinct_subjects(
    ) {
        // A prune deletes duplicates PER TYPE (`prune_derived_index_compacting_with`'s own
        // per-type loop, `WHERE type = ?1` scoping its own `PARTITION BY stream, key`): each
        // covered type is its own duplicate-key space, so the same replay key recorded once
        // under TWO different types is never a duplicate to the real DELETE - each type's pass
        // only ever sees ITS OWN one row for it. The measurement must report the same zero
        // reclaimable count the prune actually reclaims here, never a cross-type merged
        // overcount (spec 68 Global constraints: one measurement authority, no shadow
        // accounting).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_with(
            path.to_str().unwrap(),
            &[(
                "run",
                vec![
                    keyed(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/src/a.rs@h1#0",
                    ),
                    keyed(crate::contextgraph::TYPE_EDGE_INFERRED, "gc/src/a.rs@h1#0"),
                ],
            )],
        );
        let measured = s
            .measure_derived_duplication("", &crate::ingest::derived_index_identity())
            .unwrap();
        assert_eq!(measured.rows, 2, "one row of each of the two covered types");
        assert_eq!(
            measured.kept, 2,
            "the same key under two DIFFERENT types is two distinct subjects to the per-type \
             prune, not one - each type's own DELETE never sees the other type's row"
        );
        assert_eq!(
            measured.factor(),
            1.0,
            "no row here is actually reclaimable by a real prune, so the factor must not warn"
        );

        // Cross-check against the real compaction: it must reclaim zero rows for this key,
        // proving the measurement's factor of 1.0 matches what actually happens rather than
        // merely being asserted.
        let pruned = s
            .prune_derived_index("", &crate::ingest::derived_index_identity())
            .unwrap();
        assert_eq!(
            pruned.total_removed(),
            0,
            "the real per-type prune reclaims nothing for a key that appears once per type"
        );
    }

    /// Spec 101, criterion 4: a carried valid-time never crosses a generation that dropped the
    /// fact, however many generations before that gap asserted it. `L` is asserted at h1 (10s)
    /// and h3 (15s), dropped at h2 (20s) and asserted again on the return to h1 (30s): the
    /// surviving recording holds from 30s.
    #[test]
    fn a_carried_valid_time_stops_at_the_generation_that_dropped_the_fact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let at = |key: &str, data: &[u8], secs: u64| {
            Event::new(crate::contextgraph::TYPE_DOC_LINK_EXTRACTED, data.to_vec())
                .with_meta(crate::ingest::META_REPLAY_KEY, key)
                .with_valid_from(std::time::UNIX_EPOCH + Duration::from_secs(secs))
        };
        let s = store_with(
            path.to_str().unwrap(),
            &[(
                "run",
                vec![
                    at("gd/docs/f.md@h1#0", b"L", 10),
                    at("gd/docs/f.md@h3#0", b"L", 15),
                    at("gd/docs/f.md@h2#0", b"M", 20),
                    at("gd/docs/f.md@h1#0", b"L", 30),
                ],
            )],
        );
        s.prune_derived_index("", &crate::ingest::derived_index_identity())
            .unwrap();
        let kept: Vec<(i64, i64)> = Connection::open(&path)
            .unwrap()
            .prepare("SELECT position, valid_from FROM events ORDER BY position")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            kept,
            vec![(4, Duration::from_secs(30).as_nanos() as i64)],
            "only the return's recording survives, dated from the return"
        );
    }

    /// Spec 101, criterion 4: the measurement is the prune's own selection, so superseded
    /// generations count as redundancy even when every key is recorded once. Three generations of
    /// one file are three rows of which the prune keeps one: 3.0x, and the rows the measurement
    /// calls redundant are exactly the rows the prune's preview counts.
    #[test]
    fn measure_derived_duplication_counts_superseded_generations_as_the_prune_selects_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_with(
            path.to_str().unwrap(),
            &[(
                "run",
                vec![
                    keyed(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/src/a.rs@h1#0",
                    ),
                    keyed(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/src/a.rs@h2#0",
                    ),
                    keyed(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/src/a.rs@h3#0",
                    ),
                ],
            )],
        );
        let identity = crate::ingest::derived_index_identity();
        let measured = s.measure_derived_duplication("", &identity).unwrap();
        assert_eq!(measured.rows, 3);
        assert_eq!(
            measured.factor(),
            3.0,
            "three generations of which a compaction keeps only the latest"
        );
        let previewed: usize = s
            .count_derived_duplicates("", &identity)
            .unwrap()
            .removed
            .iter()
            .map(|(_, n)| n)
            .sum();
        assert_eq!(
            previewed, 2,
            "the prune's own preview selects the two superseded generations"
        );
    }

    #[test]
    fn measure_derived_duplication_on_an_empty_log_reports_a_factor_of_one_not_a_division_by_zero()
    {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = Store::open(path.to_str().unwrap()).unwrap();
        let measured = s
            .measure_derived_duplication("", &crate::ingest::derived_index_identity())
            .unwrap();
        assert_eq!(measured.rows, 0);
        assert_eq!(measured.kept, 0);
        assert_eq!(
            measured.factor(),
            1.0,
            "no covered rows at all is not duplication - never a NaN/inf from dividing by zero"
        );
    }

    /// Spec 60, criterion 5: everything after the commit is a REPORT, never an error return.
    ///
    /// The deletes are durable the moment the transaction commits, so a failure in the space
    /// reclamation that follows it describes a log that HAS been pruned. Propagating it hands the
    /// operator an error and nothing else - not the per-type counts, not the fact that a prune
    /// happened at all - which is precisely the undetectable outcome this command's design names
    /// as the one it must never produce. So the failure is carried back beside the counts.
    ///
    /// The failing step is INJECTED rather than provoked, because the real triggers (a temporary
    /// directory too small for the full copy the rewrite stages there, a writer holding the file
    /// past the busy timeout) are properties of the machine the test runs on and would make this
    /// pin conditional on the filesystem. That the real step can fail at all is pinned separately
    /// by `the_real_compaction_step_reports_a_file_it_cannot_rewrite_as_an_error`.
    #[test]
    fn a_compaction_that_fails_after_the_commit_still_reports_what_was_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path = path.to_str().unwrap();
        let s = seeded_with_duplicated_key(path, 4);

        let pruned = s
            .prune_derived_index_compacting_with(
                "",
                &crate::ingest::derived_index_identity(),
                |_| Err(Error::Backend("database or disk is full".into())),
            )
            .expect("a compaction that failed after the deletes committed is not a failed prune");

        assert_eq!(
            pruned.total_removed(),
            3,
            "the report must still name what the committed transaction deleted; got {:?}",
            pruned.removed
        );
        assert_eq!(
            pruned.reclaimed_bytes, None,
            "a reclamation whose step failed is unmeasured, not zero"
        );
        assert!(
            pruned
                .compaction_error
                .as_deref()
                .is_some_and(|e| e.contains("database or disk is full")),
            "the report must NAME the failure, or an operator cannot tell a skipped compaction \
             from a failed one; got {:?}",
            pruned.compaction_error
        );
        assert_eq!(
            recordings_of_the_key(path),
            1,
            "and the deletes really are committed: that is why the failure below them cannot be \
             an error return"
        );
    }

    /// Spec 60, criterion 5: the post-commit step this store guards against failing really can
    /// fail, so the capture above is not a defense against an imaginary error.
    ///
    /// A file the process cannot write is the reachable shape of every trigger: the rewrite needs
    /// to write both the database and a full copy of it, and either can be refused.
    ///
    /// FREE PAGES ARE PLANTED FIRST because the rewrite is triggered by the space there is to
    /// reclaim: on a file holding none, the honest answer is to skip the rewrite entirely, and a
    /// step that was never asked to write cannot report that it could not.
    #[test]
    fn the_real_compaction_step_reports_a_file_it_cannot_rewrite_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        drop(Store::open(path.to_str().unwrap()).unwrap());
        plant_free_pages(&path, 400);
        let readonly = Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .unwrap();
        let err = compact_in_place(&readonly)
            .expect_err("a database this connection cannot write cannot be rewritten in place");
        assert!(
            matches!(err, Error::Backend(_)),
            "the step reports a backend failure; got {err:?}"
        );
    }

    /// Spec 60, criterion 5: THE REMEDY THE REPORT PROMISES EXISTS. When the reclamation fails
    /// after the deletes have committed, the command tells the operator that re-running it is
    /// safe - and [`PrunedDerived::compaction_error`] says in so many words that the second pass
    /// "tries the reclamation again". That promise is only true if what triggers the rewrite is
    /// the space there is to reclaim rather than the rows THIS pass deleted: the first pass
    /// deleted them all, so a second pass deletes nothing, and a rewrite gated on its own deletes
    /// would never run again on that log. The space would then be unreclaimable through this
    /// command forever, with the report cheerfully telling the operator to re-run it.
    ///
    /// So the failure is injected on the first pass and the REAL step runs on the second, over a
    /// log whose freed pages are still sitting in the file.
    #[test]
    fn a_rerun_reclaims_the_space_a_failed_reclamation_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap().to_string();
        let s = seeded_with_duplicated_key(&path_str, 4);
        // The deletes of a handful of small rows can free no whole page at all, which would leave
        // this asserting that nothing was reclaimed from a file with nothing in it to reclaim.
        // Planted free pages make the reclamation a definite figure without changing what it is.
        plant_free_pages(&path, 3_000);
        let free_before = pragma_i64(&path, "freelist_count");
        let pages_before = pragma_i64(&path, "page_count");
        assert!(
            free_before > 100,
            "the fixture must leave real free pages, or a reclaimed file and an untouched one \
             look identical; the freelist holds {free_before} page(s)"
        );

        // FIRST PASS: the deletes commit, the reclamation fails.
        let first = s
            .prune_derived_index_compacting_with(
                "",
                &crate::ingest::derived_index_identity(),
                |_| Err(Error::Backend("database or disk is full".into())),
            )
            .expect("a compaction that failed after the deletes committed is not a failed prune");
        assert!(
            first.total_removed() > 0,
            "the first pass must be the one that sheds the duplication; got {:?}",
            first.removed
        );
        assert!(
            first.compaction_error.is_some(),
            "the first pass's reclamation must have failed, or there is nothing for the re-run to \
             retry; got {first:?}"
        );
        assert!(
            pragma_i64(&path, "freelist_count") >= free_before,
            "a reclamation that failed reclaimed nothing: the free pages must still be in the file"
        );

        // SECOND PASS, the one the report told the operator to run. It deletes nothing - the first
        // pass took the duplication - and it must still reclaim the space the first pass could not.
        let second = s
            .prune_derived_index("", &crate::ingest::derived_index_identity())
            .expect("the re-run the report promises is safe");
        assert_eq!(
            second.total_removed(),
            0,
            "the re-run deletes nothing - that is exactly why a rewrite gated on deletes would \
             never retry the reclamation; got {:?}",
            second.removed
        );
        assert_eq!(
            second.compaction_error, None,
            "the re-run's reclamation must succeed; got {second:?}"
        );
        assert!(
            second.reclaimed_bytes.is_some_and(|b| b > 0),
            "the re-run must RECLAIM the space the failed pass left behind, or the report's \
             promise that re-running is safe is a promise that re-running is pointless; got \
             {second:?}"
        );
        assert_eq!(
            pragma_i64(&path, "freelist_count"),
            0,
            "and the file must actually be compact afterwards: VACUUM drives the freelist to zero"
        );
        assert!(
            pragma_i64(&path, "page_count") < pages_before,
            "the re-run must shrink the file it reclaimed from: {pages_before} page(s) before"
        );
    }
}
