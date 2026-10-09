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
    EventBatchSink, EventStore, ExpectedRevision, FactIdentity, Filter, GroupHead, Position,
    Revision, Subscription, TypeSelection, META_GROUP, NO_STREAM,
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

/// The expression the group index and the group lookup both name: the [`META_GROUP`] entry of a
/// row's `meta` as [`key_expr`] reads it, and NULL for a row whose `meta` is not JSON - so a row a
/// broken writer left undecodable is simply not indexed, never an error that fails every later
/// write or the open that creates the index.
fn group_expr() -> String {
    format!(
        "CASE WHEN json_valid(meta) THEN {} END",
        key_expr(META_GROUP)
    )
}

/// The group index behind [`EventStore::latest_in_group`] (spec 101): a PARTIAL expression index
/// over the stream and [`group_expr`], holding only the rows that carry a group, created with the
/// schema. The lookup's `WHERE` names the identical expression, so the planner seeks it.
fn group_index_sql() -> String {
    let group = group_expr();
    format!(
        "CREATE INDEX IF NOT EXISTS idx_events_group ON events(stream, {group}, position) \
         WHERE {group} IS NOT NULL"
    )
}

/// The group lookup: one seek of `idx_events_group` to the group's highest position, handing back
/// the row's position, type and metadata - never its data.
fn latest_in_group_sql() -> String {
    let group = group_expr();
    format!(
        "SELECT position, type, meta FROM events INDEXED BY idx_events_group \
         WHERE stream = ?1 AND {group} = ?2 AND {group} IS NOT NULL \
         ORDER BY position DESC LIMIT 1"
    )
}

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
        conn.execute_batch(&group_index_sql()).map_err(be)?;
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

    /// THE READ-ONLY COUNT of the derived index `stream` holds (spec 107): how many derived
    /// events `rigger reset --derived` sheds from it, how many of those name no file identity,
    /// and the set of `<prefix>/<file>` identities holding one. It is the same read the
    /// migration acts on ([`read_derived`]), so what it answers never drifts from what
    /// [`Store::shed_derived`] deletes; it writes nothing.
    ///
    /// `stream` is one stream's whole name, matched exactly by the one read this count is
    /// ([`read_derived`]): a stream whose name only starts with it is never counted.
    pub fn count_derived(&self, stream: &str) -> Result<DerivedCount, Error> {
        let guard = self.conn.lock().unwrap();
        let read = read_derived(&guard, stream, &std::collections::HashSet::new())?;
        Ok(DerivedCount {
            shed: read.shed,
            unkeyed: read.unkeyed,
            identities: read.identities.into_keys().collect(),
        })
    }

    /// THE MIGRATION'S ONE TRANSACTION (spec 107): convert the derived index `stream` holds into
    /// the ledger, in place, and leave no derived event behind.
    ///
    /// For every `<prefix>/<file>` identity whose latest recording - a derived row or a ledger
    /// entry, each naming its identity in its replay key - is a derived row, the lowest-position
    /// row the live selection keeps for it ([`plan_derived_prune`]: the first row of its latest
    /// batch when that batch was recorded whole) is rewritten IN PLACE into the identity's
    /// ledger entry. Its position, stream, id, revision and recorded-time stay, so every column
    /// a uniqueness rule covers is kept; its type, payload and metadata become the entry's, built
    /// by the entry's one constructor for that generation, the blob and flag `entry_of` answers
    /// for the identity, and the count of distinct replay keys of that generation among the
    /// identity's derived rows. An identity whose latest recording is already an entry has no
    /// row rewritten.
    ///
    /// Every identity holding a derived row then has its EARLIEST SURVIVING RECORDING - its
    /// earliest ledger entry when one precedes the rewritten row, else the rewritten row -
    /// dated at the identity's earliest recorded valid-time, and every remaining row of a
    /// derived type in the stream is deleted, keyed or not. A row with no replay key, or one
    /// whose key does not parse, names no identity: it is deleted and counted as unkeyed.
    ///
    /// `stream` is one stream's whole name. The read of its rows ([`read_derived`]) and the
    /// DELETE match it exactly, so a stream whose name only starts with it has no row rewritten,
    /// re-dated or deleted; the live selection ([`plan_derived_prune`]) matches its stream
    /// argument as a PREFIX, so its plan may also name positions of such a stream, which the
    /// exact read never meets.
    ///
    /// The selection is read INSIDE the write transaction, opened immediate, so no append lands
    /// between what was decided and what is written, and a failure rolls the whole of it back.
    /// The types are the derived list itself ([`crate::ingest::DERIVED_INDEX_TYPES`]) and the
    /// selection the derived index's own policy ([`crate::ingest::derived_index_identity`]),
    /// neither injected: this is the one writer that sheds a derived event. Deleting from a
    /// stream leaves holes in its revisions, which [`Store::append`] tolerates (see the comment
    /// there); a stream whose tail was derived rows ends at a lower revision.
    pub fn shed_derived(
        &self,
        stream: &str,
        entry_of: &dyn Fn(&str) -> (String, bool),
    ) -> Result<ShedDerived, Error> {
        let mut guard = self.conn.lock().unwrap();
        let tx = guard
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(be)?;
        let plan = plan_derived_prune(&tx, stream, &crate::ingest::derived_index_identity(), &[])?;
        let shed_by_the_plan = plan.deletes.iter().map(|(_, position)| *position).collect();
        let read = read_derived(&tx, stream, &shed_by_the_plan)?;
        let mut converted = 0;
        {
            let mut rewrite = tx
                .prepare("UPDATE events SET type = ?2, data = ?3, meta = ?4 WHERE position = ?1")
                .map_err(be)?;
            let mut redate = tx
                .prepare("UPDATE events SET valid_from = ?2 WHERE position = ?1")
                .map_err(be)?;
            for (identity, recorded) in &read.identities {
                let entry = read.entries.get(identity);
                let mut surviving = entry.map(|entry| entry.first);
                if let (true, Some((position, generation))) =
                    (recorded.latest_is_derived, &recorded.kept)
                {
                    let (prefix, file) =
                        crate::retention::GenerationIngested::identity_parts(identity)
                            .expect("an identity cut from a replay key holds its prefix and file");
                    let (blob, excluded) = entry_of(identity);
                    let event = crate::retention::GenerationIngested {
                        prefix: prefix.to_string(),
                        file: file.to_string(),
                        generation: generation.clone(),
                        blob,
                        excluded,
                    }
                    .event(
                        recorded
                            .keys
                            .iter()
                            .filter(|(of, _)| of == generation)
                            .count(),
                    );
                    rewrite
                        .execute(params![
                            position,
                            event.type_,
                            event.data,
                            meta_json(&event.meta)
                        ])
                        .map_err(be)?;
                    surviving = surviving.into_iter().chain([*position]).min();
                    converted += 1;
                }
                let earliest = entry
                    .map(|entry| entry.earliest)
                    .into_iter()
                    .chain([recorded.earliest])
                    .min();
                redate.execute(params![surviving, earliest]).map_err(be)?;
            }
            tx.execute(
                &format!(
                    "DELETE FROM events WHERE stream = ?1 AND type IN ({})",
                    type_list(&derived_types())
                ),
                params![stream],
            )
            .map_err(be)?;
        }
        tx.commit().map_err(be)?;
        Ok(ShedDerived {
            converted,
            shed: read.shed,
            unkeyed: read.unkeyed,
        })
    }

    /// The bytes this store's log occupies on disk: the main file plus its write-ahead log, which
    /// is where a WAL-mode database's most recent pages live until a checkpoint folds them back.
    /// Counting only the main file would report a reclamation over a log whose `-wal` had just
    /// grown by more than the file shrank. `None` for a database with no file behind it
    /// (`:memory:`, a temporary database): there are no bytes on disk to measure.
    ///
    /// A file that is not there counts as zero rather than failing: the `-wal` does not exist
    /// before the first write and is deleted on a clean close, and neither absence is an error
    /// about the space the log occupies.
    pub fn bytes_on_disk(&self) -> Option<u64> {
        let guard = self.conn.lock().unwrap();
        let db = guard.path().filter(|p| !p.is_empty())?;
        let len = |p: &str| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        Some(len(db) + len(&format!("{db}-wal")))
    }

    /// Reclaim on disk the space the log's file is holding free, and report the bytes the log
    /// lost against `on_disk_before`: the size ([`Store::bytes_on_disk`]) the caller measured
    /// BEFORE its own transaction opened, so the figure spans the whole command and is the one an
    /// operator reproduces by measuring the log either side of it. A page count is no substitute:
    /// it is the database's LOGICAL size, it counts pages living only in an un-checkpointed
    /// `-wal`, and a figure computed from it can name a reclamation over a file that grew.
    ///
    /// It returns a [`Reclamation`], never a `Result`, because it runs AFTER its caller's commit:
    /// the caller's change is durable whatever happens here, so a step that fails is NAMED in the
    /// report rather than returned as an error about a log that was in fact changed.
    ///
    /// WHETHER THE FILE IS REWRITTEN IS DECIDED BY THE FILE, never by what the caller deleted
    /// (see [`compact_in_place`]). A file holding no free page is left exactly as it stands
    /// rather than rewritten in full to reclaim nothing; a file holding free pages is reclaimed
    /// whoever freed them, which is what makes calling this again the remedy for a reclamation
    /// that failed. The copy the rewrite stages is held in the process's memory, never in a
    /// temporary directory, so a crash leaves nothing behind to reap.
    pub fn reclaim_space(&self, on_disk_before: Option<u64>) -> Reclamation {
        self.reclaim_space_compacting_with(on_disk_before, compact_in_place)
    }

    /// [`Store::reclaim_space`] with its compacting step INJECTED.
    ///
    /// The seam exists because that step's real failures - too little memory for the copy the
    /// rewrite stages, a writer holding the file past the busy timeout - are properties of the
    /// machine, not of this code, so the only way to pin what a reclamation reports WITH a failure
    /// is to hand it one. Production has exactly one implementation ([`compact_in_place`]) and
    /// the public entry point above passes it; nothing chooses.
    fn reclaim_space_compacting_with(
        &self,
        on_disk_before: Option<u64>,
        compact: impl FnOnce(&Connection) -> Result<Compaction, Error>,
    ) -> Reclamation {
        // The connection is released as soon as the step returns, so the after-size below is
        // read through the same `bytes_on_disk` the caller's before was.
        let outcome = compact(&self.conn.lock().unwrap());
        let on_disk_measured = on_disk_before.is_some();
        match outcome {
            // Nothing to reclaim, nothing rewritten: zero bytes is the MEASUREMENT here, not a
            // measurement that could not be taken - but only where a FILE existed to measure.
            // A database with no file behind it has no reading to report, and a `Some(0)`
            // beside `on_disk_measured: false` would claim a measurement the flag denies.
            Ok(Compaction::Skipped) => Reclamation {
                reclaimed_bytes: on_disk_before.map(|_| 0),
                compaction_ran: false,
                on_disk_measured,
                compaction_error: None,
            },
            // The rewrite ran and its result is on disk NOW, so the caller's before and the
            // after taken here bracket the whole command: their difference is what the log lost.
            Ok(Compaction::Landed) => Reclamation {
                reclaimed_bytes: on_disk_before
                    .zip(self.bytes_on_disk())
                    .map(|(before, after)| before.saturating_sub(after)),
                compaction_ran: true,
                on_disk_measured,
                compaction_error: None,
            },
            // The rewrite ran but its result has NOT landed: the freed frames are still in the
            // write-ahead log, so any difference measured now is between two states of a move
            // that has not finished. Unmeasured is the honest report.
            Ok(Compaction::Pending) => Reclamation {
                reclaimed_bytes: None,
                compaction_ran: true,
                on_disk_measured,
                compaction_error: None,
            },
            Err(e) => Reclamation {
                reclaimed_bytes: None,
                compaction_ran: true,
                on_disk_measured,
                compaction_error: Some(e.to_string()),
            },
        }
    }

    /// A read-only count of the rows the live selection ([`plan_derived_prune`]) sets aside
    /// (spec 68, "the reset surface"): for each type `identity` covers, every recording of a
    /// superseded generation and every earlier recording of a surviving key. No row is touched,
    /// no valid-time carried, no `VACUUM` run.
    ///
    /// Unlike [`Store::read_live_selection`] this needs no [`ContentIdentity::reasserting`]
    /// declaration: that check exists because a reader of the selection has to know whether a
    /// surviving row's valid-time must be carried forward, and a count carries nothing, so the
    /// one input that check guards against getting wrong is not read here at all.
    ///
    /// `rigger reset`'s bare-menu preview reads this.
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

    /// Measure the derived-index REDUNDANCY already sitting in the log, WITHOUT deleting
    /// anything: across every type `identity` covers, within streams under `stream_prefix`, how
    /// many rows carry a covered key versus how many of them the live selection KEEPS.
    ///
    /// Answered by the one selection [`plan_derived_prune`], the one a rebuild reads through
    /// [`Store::read_live_selection`], so `rigger validate`'s bloat advisory (spec 68) measures
    /// superseded generations as well as earlier recordings of one key and can never drift from
    /// a second, independently re-derived definition of "redundant" (Design: "one measurement
    /// authority per advisory ... no shadow accounting"). No row is touched, no valid-time
    /// carried.
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
    /// every non-derived event, and of the derived index each identity's latest generation at the
    /// latest recording of each key, its valid-time carried back to the earliest the fact has
    /// held without a break - the rows the one selection, [`plan_derived_prune`], keeps over the
    /// same `stream_prefix`.
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
        let Some(head) = stream_head(&tx, &stream)? else {
            return Ok(());
        };
        let mut stmt = tx
            .prepare(&format!(
                "SELECT {cols} FROM events WHERE stream = ?1 AND position > ?2 ORDER BY position"
            ))
            .map_err(be)?;
        let rows = stmt
            .query_map(params![stream, after as i64], |r| {
                let position: i64 = r.get(0)?;
                if shed.contains(&position) {
                    return Ok(None);
                }
                row(r, carried.get(&position).copied()).map(Some)
            })
            .map_err(be)?;
        super::in_batches(
            rows.filter_map(|kept| kept.map_err(be).transpose()),
            batch,
            &mut |kept| sink(kept, head),
        )
    }
}

/// THE FORWARD READ: the ONE forward read of a stream this adapter drives - `stream`'s events from
/// revision `from` (inclusive) on, in revision order, handed to `read` as the statement steps, so
/// a caller collects them ([`EventStore::read_stream`]), batches them
/// ([`EventStore::read_stream_batched`]) or polls past a revision a stream subscription has
/// delivered ([`poll_stream`]) and never spells the read a second time. A failure of the read is
/// spelled by `fail`: the port's error for a port read, the database's own for a subscription.
fn read_forward<R, E>(
    conn: &Connection,
    stream: &str,
    from: Revision,
    fail: fn(rusqlite::Error) -> E,
    read: impl FnOnce(&mut dyn Iterator<Item = Result<Event, E>>) -> Result<R, E>,
) -> Result<R, E> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM events WHERE stream = ?1 AND revision >= ?2 ORDER BY revision"
        ))
        .map_err(fail)?;
    let mut events = stmt
        .query_map(params![stream, from], row_to_event)
        .map_err(fail)?
        .map(|e| e.map_err(fail));
    read(&mut events)
}

/// The position of `stream`'s last event - the head a batched read hands with each batch - or
/// `None` for a stream the store does not hold, which a batched read hands nothing of. Read on the
/// caller's connection, inside the read transaction its rows are read in.
fn stream_head(conn: &Connection, stream: &str) -> Result<Option<Position>, Error> {
    conn.query_row(
        "SELECT MAX(position) FROM events WHERE stream = ?1",
        params![stream],
        |r| r.get::<_, Option<i64>>(0),
    )
    .map(|head| head.map(|position| position as Position))
    .map_err(be)
}

/// What [`Store::count_derived`] answers of one stream's derived index (spec 107).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DerivedCount {
    /// How many derived events the stream holds: every one of them is shed by the migration,
    /// the rows it rewrites into ledger entries included.
    pub shed: usize,
    /// How many of them name no file identity: a row with no replay key, or one whose key does
    /// not parse.
    pub unkeyed: usize,
    /// The `<prefix>/<file>` identities holding a derived event, so a file holding a `gc` and a
    /// `gd` batch is two.
    pub identities: std::collections::BTreeSet<String>,
}

/// What one [`Store::shed_derived`] call did (spec 107).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShedDerived {
    /// How many identities had their latest derived batch rewritten into a ledger entry: fewer
    /// than the identities holding a derived event when one's latest recording was already an
    /// entry.
    pub converted: usize,
    /// How many derived events were shed, the rewritten rows counted among them.
    pub shed: usize,
    /// How many of them named no file identity.
    pub unkeyed: usize,
}

/// What one stream's keyed derived rows record of one identity, read in position order.
struct DerivedRecordings {
    /// The earliest valid-time any of the rows carries.
    earliest: i64,
    /// Whether the identity's latest recording is a derived row, and not a ledger entry.
    latest_is_derived: bool,
    /// The position and generation of the lowest row the live selection keeps, the row the
    /// migration rewrites.
    kept: Option<(i64, String)>,
    /// The distinct `(generation, replay key)` pairs the rows carry.
    keys: std::collections::BTreeSet<(String, String)>,
}

/// What one stream's ledger entries record of one identity, read in position order.
struct EntryRecordings {
    /// The position of the identity's earliest entry.
    first: i64,
    /// The earliest valid-time any of the entries carries.
    earliest: i64,
}

/// One stream's perception as [`read_derived`] reads it: the derived rows counted, and what the
/// keyed ones and the ledger entries record of each identity.
#[derive(Default)]
struct DerivedRead {
    shed: usize,
    unkeyed: usize,
    identities: BTreeMap<String, DerivedRecordings>,
    entries: BTreeMap<String, EntryRecordings>,
}

/// The derived list as the owned names a maintenance statement renders.
fn derived_types() -> Vec<String> {
    crate::ingest::DERIVED_INDEX_TYPES
        .map(String::from)
        .to_vec()
}

/// THE ONE READ the migration and its read-only count share (spec 107): every row of perception
/// `stream` holds - the derived list and the ledger entry, by type, the stream matched by its
/// whole name - in position order. Each derived row is counted; one with no replay key, or whose
/// key the one key parser ([`crate::ingest::derived_key_parts`]) does not cut, names no identity
/// and is counted as unkeyed. A keyed derived row and a ledger entry name their identity alike,
/// in their replay key, so an identity's latest recording is whichever of the two the read met
/// last. `shed_by_the_plan` holds the positions the live selection sheds: the first derived row
/// of an identity outside it is the row the migration rewrites.
fn read_derived(
    conn: &Connection,
    stream: &str,
    shed_by_the_plan: &std::collections::HashSet<i64>,
) -> Result<DerivedRead, Error> {
    let sql = format!(
        "SELECT position, type, {key}, valid_from FROM events
          WHERE stream = ?1 AND type IN ({perception})
          ORDER BY position",
        key = key_expr(crate::ingest::META_REPLAY_KEY),
        perception = type_list(&crate::retention::PERCEPTION_TYPES.map(String::from)),
    );
    let mut stmt = conn.prepare(&sql).map_err(be)?;
    let mut rows = stmt.query(params![stream]).map_err(be)?;
    let mut read = DerivedRead::default();
    while let Some(row) = rows.next().map_err(be)? {
        let position: i64 = row.get(0).map_err(be)?;
        let type_: String = row.get(1).map_err(be)?;
        let key: Option<String> = row.get(2).map_err(be)?;
        let valid_from: i64 = row.get(3).map_err(be)?;
        let named = key.as_deref().and_then(crate::ingest::derived_key_parts);
        if type_ == crate::retention::TYPE_GENERATION_INGESTED {
            if let Some((identity, _)) = named {
                let entry = read
                    .entries
                    .entry(identity.to_string())
                    .or_insert(EntryRecordings {
                        first: position,
                        earliest: valid_from,
                    });
                entry.earliest = entry.earliest.min(valid_from);
                if let Some(recorded) = read.identities.get_mut(identity) {
                    recorded.latest_is_derived = false;
                }
            }
            continue;
        }
        read.shed += 1;
        let Some((identity, generation)) = named else {
            read.unkeyed += 1;
            continue;
        };
        let recorded = read
            .identities
            .entry(identity.to_string())
            .or_insert(DerivedRecordings {
                earliest: valid_from,
                latest_is_derived: true,
                kept: None,
                keys: std::collections::BTreeSet::new(),
            });
        recorded.earliest = recorded.earliest.min(valid_from);
        recorded.latest_is_derived = true;
        recorded
            .keys
            .insert((generation.to_string(), key.clone().unwrap_or_default()));
        if !shed_by_the_plan.contains(&position) {
            recorded
                .kept
                .get_or_insert((position, generation.to_string()));
        }
    }
    Ok(read)
}

/// What [`Store::count_derived_duplicates`] counts the live selection setting aside.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DerivedPreview {
    /// The rows the selection sets aside, per covered type in the policy's order, zeros included.
    pub removed: Vec<(String, usize)>,
    /// How many of them record a superseded generation of their file.
    pub superseded_generations: usize,
}

/// What the live selection sets aside and re-dates, decided by [`plan_derived_prune`].
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
            "the live selection: the content-identity policy for {:?} has not declared which \
             of its types re-assert a fact in place (ContentIdentity::with_reasserting_types). \
             Without it the selection cannot know whether a key's EARLIEST recorded valid-time \
             is the one the projection holds, and either default silently re-dates facts. \
             Refusing rather than guessing.",
            identity.types()
        )));
    };
    if let Some(stray) = declared.iter().find(|t| !identity.covers(t)) {
        return Err(Error::Backend(format!(
            "the live selection: the content-identity policy declares {stray:?} as \
             re-asserting, but does not cover that type ({:?}). A declaration naming a type \
             this policy will never select describes some other policy, so it cannot be the \
             partition for this one. Refusing rather than selecting against a declaration that \
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

/// The ONE live selection of the derived index, shared by the rebuild's read, the migration's
/// choice of the rows it converts, the reset menu's count and the `rigger validate` bloat
/// measurement: which rows are set aside, and which surviving rows take an earlier valid-time.
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

/// What the compacting step did to the file, which is the only thing about a reclamation its
/// caller cannot work out for itself.
///
/// Three outcomes rather than a byte count, because HOW MANY bytes the log lost is a property of
/// the whole command (measured either side of it, see [`Store::reclaim_space`]) while WHETHER
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
/// It runs AFTER its caller's commit, where a failure is a fact to report rather than an outcome
/// to propagate: by the time this is called the caller's change is durable, so its `Err`
/// describes an un-reclaimed log rather than an unchanged one.
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
    // VACUUM cannot run inside a transaction, so it follows the commit. THE COPY IT STAGES IS
    // HELD IN MEMORY: `temp_store` is set on this connection first and left there for the
    // connection's life, so the rewrite never writes a second copy of the log into the temporary
    // directory SQLite resolves (often a far smaller filesystem than the one holding the log).
    // The setting is the connection's own, never the process's, and a crash leaves no file
    // behind to reap.
    conn.execute_batch("PRAGMA temp_store = MEMORY; VACUUM")
        .map_err(be)?;
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
/// Bounded and short on purpose: the caller's transaction has already committed and the vacuum has
/// already run by the time this matters, so the only thing at stake is whether the freed frames
/// land in the main file NOW or at the next checkpoint some later writer performs. Waiting a
/// reader out indefinitely would trade a correct, honestly-reported result for a hang.
const CHECKPOINT_TRUNCATE_ATTEMPTS: u32 = 5;
const CHECKPOINT_TRUNCATE_BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);

/// What one [`Store::reclaim_space`] call did to the log's file: the bytes the log lost on disk,
/// whether it was rewritten to lose them at all, whether there was a before-size to measure them
/// against in the first place, and - when the compacting step failed - what went wrong with it.
///
/// The failure is a FIELD rather than an error return because the reclamation follows its
/// caller's commit: the caller's change is durable before it is attempted, so an `Err` carrying
/// only the failure would describe a log that was in fact changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reclamation {
    /// Bytes the LOG LOST ON DISK since the before-size the caller handed in, saturating at zero,
    /// or `None` when that could not be measured because a concurrent reader still held a write-ahead-log snapshot when the
    /// truncating checkpoint ran, because the reclamation itself failed (see
    /// [`Reclamation::compaction_error`]), or because the database has no file behind it (see
    /// [`Reclamation::on_disk_measured`], which is what tells those last two `None`s apart).
    ///
    /// MEASURED, NOT DERIVED, and measured over the pair of files an operator's own `du` would
    /// add up: the main database plus its `-wal`, sampled by the caller before its
    /// transaction and again here after the rewrite has landed. A page-count delta is a tempting substitute and is not the same
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
    /// rewrite is deliberately not run at all (see [`Reclamation::compaction_ran`]). There
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
    /// Whether the before-measurement was TAKEN AT ALL: `true` when the caller handed in a
    /// before-size, so the pair of sizes the reclamation is a difference of were both sampled;
    /// `false` when it handed in none, as for a database with no file behind it (`:memory:`, a
    /// temporary database), where there was never anything on disk to measure.
    ///
    /// It exists because `reclaimed_bytes: None` alongside `compaction_ran: true` has TWO causes
    /// and the difference is invisible in the numbers: the truncating checkpoint was declined by
    /// a concurrent reader (the bytes exist and land later), or this database has no file (there
    /// are no bytes and none ever land). A consumer told only "unmeasured" cannot tell them
    /// apart, so it either reports one cause for both - asserting a reader it was never told
    /// about - or reports neither. Only the reclamation knows, so it carries it.
    ///
    /// It says nothing about whether the AFTER measurement was usable: a checkpoint a reader
    /// declined leaves this `true` and the byte count `None`, which is exactly the pair that
    /// separates the two causes.
    pub on_disk_measured: bool,
    /// Why the space reclamation did not complete, when it was attempted and failed - `None` when
    /// it succeeded, and `None` when there was no free space for it to reclaim.
    ///
    /// It is reported rather than returned because it happens AFTER the caller's commit: the
    /// caller's change stands whatever this says, so it names a log that is changed but not
    /// shrunk, and running the command again is safe AND useful - the space this call failed to
    /// reclaim is still free in the file, so the reclamation is tried again over it.
    pub compaction_error: Option<String>,
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
        // migration, `Store::shed_derived`, does, leaving holes in the
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
        if !expected.admits(last_revision) {
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

    /// The forward read ([`read_forward`]) collected, and reversed for a backward read: `from` is
    /// an inclusive lower bound on revision in both directions, and the direction only orders.
    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        let conn = self.conn.lock().unwrap();
        let mut events: Vec<Event> =
            read_forward(&conn, stream, from, be, |events| events.collect())?;
        if matches!(dir, Direction::Backward) {
            events.reverse();
        }
        Ok(events)
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
        super::in_batches(
            positions.map(|p| p.map(|p| p as Position).map_err(be)),
            batch,
            sink,
        )
    }

    /// The forward read ([`read_forward`]) streamed row by row off the events table in batches,
    /// each handed with the stream's head ([`stream_head`]), read in the same read transaction as
    /// the rows so the two describe one state of the log.
    fn read_stream_batched(
        &self,
        stream: &str,
        from: Revision,
        batch: usize,
        sink: &mut EventBatchSink,
    ) -> Result<(), Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(be)?;
        let Some(head) = stream_head(&tx, stream)? else {
            return Ok(());
        };
        read_forward(&tx, stream, from, be, |events| {
            super::in_batches(events, batch, &mut |events| sink(events, head))
        })
    }
    fn latest_in_group(&self, stream: &str, group: &str) -> Result<Option<GroupHead>, Error> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(&latest_in_group_sql(), params![stream, group], |r| {
            let meta: String = r.get(2)?;
            Ok(GroupHead {
                position: r.get::<_, i64>(0)? as Position,
                type_: r.get(1)?,
                meta: parse_meta(&meta),
            })
        })
        .optional()
        .map_err(be)
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

/// The events of `stream` past revision `after`, read through the one forward read
/// ([`read_forward`]); a failure is the database's own, which the subscription reports as it is.
fn poll_stream(conn: &Connection, stream: &str, after: Revision) -> rusqlite::Result<Vec<Event>> {
    read_forward(conn, stream, after + 1, std::convert::identity, |events| {
        events.collect()
    })
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
    use crate::test_support::{file_len, plant_free_pages, pragma_i64};

    #[test]
    fn passes_the_contract() {
        crate::eventstore::contract::assert_contract(&Store::open(":memory:").unwrap());
    }

    /// EVERY LOOKUP IS ONE SEEK (spec 101): sqlite's own plan for the boundary lookup is one search
    /// of the stream-and-type index, and for the group lookup one search of the partial group index
    /// over the stream and the group entry - never a scan of the table and never a sort step, so
    /// neither cost grows with the events the stream holds.
    #[test]
    fn each_lookup_is_one_seek_of_its_index() {
        let s = Store::open(":memory:").unwrap();
        let conn = s.conn.lock().unwrap();
        for (sql, key, plan) in [
            (
                LAST_POSITION_SQL.to_string(),
                "RunStarted",
                "SEARCH events USING INDEX idx_events_stream_type (stream=? AND type=?)",
            ),
            (
                latest_in_group_sql(),
                "gc/a.rs",
                "SEARCH events USING INDEX idx_events_group (stream=? AND <expr>=?)",
            ),
        ] {
            let got: Vec<String> = conn
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .unwrap()
                .query_map(params!["rigger", key], |r| r.get::<_, String>(3))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(
                got,
                [plan],
                "a single index search with no scan and no sort step"
            );
        }
    }

    /// An events file written before the group index existed gains it when it is opened, so the
    /// first lookup on an upgraded store is answered by the index over the rows already recorded.
    #[test]
    fn an_events_file_from_before_the_group_index_is_indexed_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path = path.to_str().unwrap();
        {
            let old = Connection::open(path).unwrap();
            old.execute_batch(SCHEMA).unwrap();
            old.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
                 VALUES ('rigger', 'X', 'e0', x'', ?1, 0, 0, 0)",
                params![r#"{"group":"gc/a.rs","tag":"before"}"#],
            )
            .unwrap();
            // A row a broken writer left with undecodable metadata: the index skips it.
            old.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
                 VALUES ('rigger', 'X', 'e1', x'', x'ff', 0, 0, 1)",
                [],
            )
            .unwrap();
        }
        let s = Store::open(path).expect("an undecodable row never fails the index's creation");
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE events SET meta = x'ff' WHERE id = 'e1'", [])
            .expect("nor a later write of one");
        let head = s.latest_in_group("rigger", "gc/a.rs").unwrap();
        assert_eq!(
            head,
            Some(GroupHead {
                position: 1,
                type_: "X".to_string(),
                meta: [("group", "gc/a.rs"), ("tag", "before")]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            })
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

    // --- Spec 60, criterion 5: the reclamation after a commit is reported, never propagated ---

    /// A store holding `rounds` recordings of one derived-index replay key, in one namespaced
    /// stream, plus a non-derived event the migration never touches. The duplication the migration sheds.
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

    // --- Spec 68, VALIDATE ADVISORIES: measure_derived_duplication, the live selection's count ---

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
            "measuring must never delete anything - that is the migration's job, not this read"
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

    /// Spec 60, criterion 5: the post-commit step this store guards against failing really can
    /// fail, so the capture `a_compaction_that_fails_after_the_commit_still_reports_what_was_deleted`
    /// pins is not a defense against an imaginary error.
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

    // --- Spec 107, criterion 14: RECLAMATION STAGES IN MEMORY (`Store::reclaim_space`) ---

    /// A file-backed store at `dir`/events.db holding one event and roughly `rows` blobs' worth
    /// of free pages, with the path of its file.
    fn store_holding_free_pages(dir: &std::path::Path, rows: u64) -> (Store, std::path::PathBuf) {
        let path = dir.join("events.db");
        let s = store_with(
            path.to_str().unwrap(),
            &[("run", vec![Event::new("RunStarted", b"{}".to_vec())])],
        );
        plant_free_pages(&path, rows);
        (s, path)
    }

    /// `PRAGMA temp_store` as the store's OWN connection reports it: 0 is the build's default
    /// (a file in the temporary directory), 2 is memory.
    fn temp_store_of(s: &Store) -> i64 {
        s.conn
            .lock()
            .unwrap()
            .query_row("PRAGMA temp_store", [], |r| r.get(0))
            .unwrap()
    }

    /// A file holding free pages is rewritten smaller, the bytes reported are the before-size the
    /// CALLER handed in less what the file occupies afterwards, and the copy the rewrite stages is
    /// held in memory: the store's own connection reports `temp_store` as memory after the call.
    #[test]
    fn reclaim_space_rewrites_a_file_holding_free_pages_smaller_and_stages_the_copy_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let (s, path) = store_holding_free_pages(dir.path(), 3_000);
        // The planted pages are folded out of the write-ahead log first, so the free space sits
        // in the main file and the main file is what the rewrite has to shrink.
        assert_eq!(pragma_i64(&path, "wal_checkpoint(TRUNCATE)"), 0);
        let measured_before = s.bytes_on_disk().expect("a file-backed store has a size");
        let main_before = file_len(&path);
        let pages_before = pragma_i64(&path, "page_count");
        assert_eq!(
            temp_store_of(&s),
            0,
            "the connection starts on the default, so memory afterwards is this call's doing"
        );
        // A before that is NOT the file's own size at the call: the report must be taken against
        // the figure handed in, never against a size the store measured again for itself.
        let handed = measured_before + 4_096;

        let reclaimed = s.reclaim_space(Some(handed));

        let after = s.bytes_on_disk().expect("a file-backed store has a size");
        assert!(
            after < measured_before,
            "the log must occupy less than it did: {measured_before} before, {after} after"
        );
        assert_eq!(
            reclaimed,
            Reclamation {
                reclaimed_bytes: Some(handed - after),
                compaction_ran: true,
                on_disk_measured: true,
                compaction_error: None,
            }
        );
        assert!(
            file_len(&path) < main_before,
            "the main file itself is rewritten smaller, not only its write-ahead log"
        );
        assert_eq!(pragma_i64(&path, "freelist_count"), 0);
        assert!(pragma_i64(&path, "page_count") < pages_before);
        assert_eq!(
            temp_store_of(&s),
            2,
            "the rewrite's copy is staged in memory, on the store's own connection"
        );
    }

    /// The bytes the log lost saturate at zero: a caller whose before-size is below what the file
    /// occupies after the rewrite is told zero, never a wrapped figure.
    #[test]
    fn reclaim_space_reports_zero_when_the_file_ends_larger_than_the_before_it_was_handed() {
        let dir = tempfile::tempdir().unwrap();
        let (s, path) = store_holding_free_pages(dir.path(), 400);

        let reclaimed = s.reclaim_space(Some(1));

        assert_eq!(
            reclaimed,
            Reclamation {
                reclaimed_bytes: Some(0),
                compaction_ran: true,
                on_disk_measured: true,
                compaction_error: None,
            }
        );
        assert_eq!(pragma_i64(&path, "freelist_count"), 0);
    }

    /// A caller with no before-size (a store with no file to measure) still has its free pages
    /// reclaimed, and is told the bytes were never measured rather than that they were zero.
    #[test]
    fn reclaim_space_handed_no_before_size_rewrites_and_reports_the_bytes_unmeasured() {
        let dir = tempfile::tempdir().unwrap();
        let (s, path) = store_holding_free_pages(dir.path(), 400);

        let reclaimed = s.reclaim_space(None);

        assert_eq!(
            reclaimed,
            Reclamation {
                reclaimed_bytes: None,
                compaction_ran: true,
                on_disk_measured: false,
                compaction_error: None,
            }
        );
        assert_eq!(pragma_i64(&path, "freelist_count"), 0);
    }

    /// A file holding no free page is left exactly as it stands: not rewritten, zero bytes
    /// reported as the measurement whatever before-size was handed in, and the connection's
    /// `temp_store` untouched because no copy was staged.
    #[test]
    fn reclaim_space_leaves_a_file_holding_no_free_pages_unrewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_with(
            path.to_str().unwrap(),
            &[("run", vec![Event::new("RunStarted", b"{}".to_vec())])],
        );
        assert_eq!(pragma_i64(&path, "freelist_count"), 0);
        let measured_before = s.bytes_on_disk().expect("a file-backed store has a size");
        let bytes_before = std::fs::read(&path).unwrap();
        let wal = format!("{}-wal", path.to_str().unwrap());
        let wal_before = std::fs::read(&wal).unwrap();

        let measured = s.reclaim_space(Some(measured_before + 4_096));
        let unmeasured = s.reclaim_space(None);

        assert_eq!(
            measured,
            Reclamation {
                reclaimed_bytes: Some(0),
                compaction_ran: false,
                on_disk_measured: true,
                compaction_error: None,
            }
        );
        assert_eq!(
            unmeasured,
            Reclamation {
                reclaimed_bytes: None,
                compaction_ran: false,
                on_disk_measured: false,
                compaction_error: None,
            }
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes_before,
            "the main file is byte-for-byte the file the call found"
        );
        assert_eq!(
            std::fs::read(&wal).unwrap(),
            wal_before,
            "and nothing was written to its write-ahead log either"
        );
        assert_eq!(
            temp_store_of(&s),
            0,
            "no rewrite ran, so no copy was staged"
        );
    }

    /// The size a caller measures before its transaction is the main file PLUS its write-ahead
    /// log, the pair an operator's own `du` adds up; a store with no file behind it has none.
    #[test]
    fn bytes_on_disk_is_the_main_file_plus_its_write_ahead_log_and_none_without_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_with(
            path.to_str().unwrap(),
            &[("run", vec![Event::new("RunStarted", b"{}".to_vec())])],
        );
        let main = file_len(&path);
        let wal = file_len(std::path::Path::new(&format!(
            "{}-wal",
            path.to_str().unwrap()
        )));
        assert!(
            main > 1 && wal > 1,
            "both files must hold bytes, or a sum, a difference and a product could agree: \
             main {main}, wal {wal}"
        );

        assert_eq!(s.bytes_on_disk(), Some(main + wal));
        assert_eq!(Store::open(":memory:").unwrap().bytes_on_disk(), None);
    }

    /// Everything after a caller's commit is a REPORT, never an error return: the compacting step
    /// that fails is named in the `Reclamation` beside what the committed transaction deleted,
    /// which stays deleted.
    ///
    /// The failing step is INJECTED rather than provoked, because the real triggers (too little
    /// memory for the copy the rewrite stages, a writer holding the file past the busy timeout)
    /// are properties of the machine the test runs on. That the real step can fail at all is
    /// pinned separately by `the_real_compaction_step_reports_a_file_it_cannot_rewrite_as_an_error`.
    #[test]
    fn a_compaction_that_fails_after_the_commit_still_reports_what_was_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();
        let s = seeded_with_duplicated_key(path_str, 4);
        let shed = s.shed_derived("run", &|_| (String::new(), false)).unwrap();
        assert_eq!(
            shed,
            ShedDerived {
                converted: 1,
                shed: 4,
                unkeyed: 0,
            }
        );
        plant_free_pages(&path, 400);
        let free_before = pragma_i64(&path, "freelist_count");
        let before = s.bytes_on_disk();

        let reclaimed = s.reclaim_space_compacting_with(before, |_| {
            Err(Error::Backend("database or disk is full".into()))
        });

        assert_eq!(
            reclaimed,
            Reclamation {
                reclaimed_bytes: None,
                compaction_ran: true,
                on_disk_measured: true,
                compaction_error: Some(
                    Error::Backend("database or disk is full".into()).to_string()
                ),
            },
            "a reclamation whose step failed is unmeasured, not zero, and names the failure"
        );
        assert_eq!(
            pragma_i64(&path, "freelist_count"),
            free_before,
            "a step that failed reclaimed nothing: the free pages are still in the file"
        );
        assert_eq!(
            recordings_of_the_key(path_str),
            0,
            "and the committed migration stands whatever the reclamation reports"
        );
    }

    /// THE REMEDY THE REPORT PROMISES EXISTS. What triggers the rewrite is the space the file is
    /// holding free, never what an earlier step deleted, so the call after a failed reclamation
    /// reclaims what that failure left behind.
    ///
    /// The failure is injected on the first call and the REAL step runs on the second, over a
    /// file whose free pages are still sitting in it.
    #[test]
    fn a_rerun_reclaims_the_space_a_failed_reclamation_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let (s, path) = store_holding_free_pages(dir.path(), 3_000);
        let free_before = pragma_i64(&path, "freelist_count");
        let pages_before = pragma_i64(&path, "page_count");
        assert!(
            free_before > 100,
            "the fixture must leave real free pages, or a reclaimed file and an untouched one \
             look identical; the freelist holds {free_before} page(s)"
        );
        let before = s.bytes_on_disk().expect("a file-backed store has a size");

        let first = s.reclaim_space_compacting_with(Some(before), |_| {
            Err(Error::Backend("database or disk is full".into()))
        });
        assert_eq!(
            first.compaction_error,
            Some(Error::Backend("database or disk is full".into()).to_string())
        );
        assert_eq!(pragma_i64(&path, "freelist_count"), free_before);

        let second = s.reclaim_space(Some(before));

        let after = s.bytes_on_disk().expect("a file-backed store has a size");
        assert!(
            after < before,
            "the rerun must shrink the log: {before} before, {after} after"
        );
        assert_eq!(
            second,
            Reclamation {
                reclaimed_bytes: Some(before - after),
                compaction_ran: true,
                on_disk_measured: true,
                compaction_error: None,
            }
        );
        assert_eq!(pragma_i64(&path, "freelist_count"), 0);
        assert!(pragma_i64(&path, "page_count") < pages_before);
    }

    // --- Spec 107, criterion 16: THE MIGRATION'S ONE TRANSACTION AND ITS READ-ONLY COUNT ---

    /// One row of the events table, every column.
    type Row = (i64, String, String, String, Vec<u8>, String, i64, i64, i64);

    /// Every row the file at `path` holds, in position order, read through a connection of its
    /// own.
    fn rows_of(path: &std::path::Path) -> Vec<Row> {
        Connection::open(path)
            .unwrap()
            .prepare(&format!("SELECT {COLS} FROM events ORDER BY position"))
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// A derived event of `type_` under the replay key `key`, valid from `secs`.
    fn derived_at(type_: &str, key: &str, secs: u64) -> Event {
        keyed(type_, key).with_valid_from(std::time::UNIX_EPOCH + Duration::from_secs(secs))
    }

    /// The ledger entry of `identity` at `generation`, built by the crate's one test-side entry
    /// builder for a batch of `n` events extracted from `blob` under the flag `excluded`.
    fn entry_event(
        identity: &str,
        generation: &str,
        n: usize,
        blob: &str,
        excluded: bool,
    ) -> Event {
        let (prefix, file) =
            crate::retention::GenerationIngested::identity_parts(identity).unwrap();
        crate::eventstore::contract::entry_of_a_batch(prefix, file, generation, n, blob, excluded)
    }

    /// `row` as the migration leaves the row it rewrites into `entry`: its type, payload and
    /// metadata the entry's, every other column but the valid-time, which is `secs`, its own.
    fn rewritten(row: &Row, entry: &Event, secs: u64) -> Row {
        (
            row.0,
            row.1.clone(),
            entry.type_.clone(),
            row.3.clone(),
            entry.data.clone(),
            meta_json(&entry.meta),
            Duration::from_secs(secs).as_nanos() as i64,
            row.7,
            row.8,
        )
    }

    /// `row` with its valid-time moved to `secs` and nothing else changed.
    fn redated(row: &Row, secs: u64) -> Row {
        let mut row = row.clone();
        row.6 = Duration::from_secs(secs).as_nanos() as i64;
        row
    }

    /// The blob and flag the migration's caller answers for `identity` in these tests: a blob
    /// that names the identity, and the flag set for `gd/b.md` alone.
    fn named_entry(identity: &str) -> (String, bool) {
        (format!("blob-of-{identity}"), identity == "gd/b.md")
    }

    /// A store at `path` whose stream `p-run` holds, in position order: a run event (1); three
    /// generations of `gc/a.rs` (2 to 6); a latest generation of `gd/b.md` recorded twice (7 to
    /// 10); `gc/c.rs` with a ledger entry above its derived row (11, 12); `gc/d.rs` with a
    /// ledger entry below its derived rows (13 to 15); an unkeyed derived event (16) and one
    /// whose key does not parse (17); a generation of `gd/e.md` whose second recording repeats
    /// only its first key, an alias definition between the two (18 to 21); and a decision (22).
    /// The streams `p-run-x`, whose name starts with that stream's, and `q-run` hold one derived
    /// row each (23, 24).
    fn store_to_migrate(path: &std::path::Path) -> Store {
        use crate::contextgraph::{
            TYPE_CODE_ENTITY_EXTRACTED as CE, TYPE_DOC_CONCEPT_EXTRACTED as DC,
            TYPE_DOC_LINK_EXTRACTED as DL, TYPE_EDGE_INFERRED as EI,
        };
        let at = |event: Event, secs: u64| {
            event.with_valid_from(std::time::UNIX_EPOCH + Duration::from_secs(secs))
        };
        store_with(
            path.to_str().unwrap(),
            &[
                (
                    "p-run",
                    vec![
                        at(Event::new("RunStarted", b"{}".to_vec()), 1),
                        derived_at(CE, "gc/a.rs@h1#0", 50),
                        derived_at(CE, "gc/a.rs@h2#0", 20),
                        derived_at(EI, "gc/a.rs@h2#1", 21),
                        derived_at(CE, "gc/a.rs@h3#0", 60),
                        derived_at(EI, "gc/a.rs@h3#1", 61),
                        derived_at(DC, "gd/b.md@h1#0", 30),
                        derived_at(DL, "gd/b.md@h1#1", 31),
                        derived_at(DC, "gd/b.md@h1#0", 40),
                        derived_at(DL, "gd/b.md@h1#1", 41),
                        derived_at(CE, "gc/c.rs@h1#0", 70),
                        at(entry_event("gc/c.rs", "h2", 1, "held", false), 80),
                        at(entry_event("gc/d.rs", "h1", 1, "held", false), 90),
                        derived_at(CE, "gc/d.rs@h9#0", 10),
                        derived_at(CE, "gc/d.rs@h2#0", 85),
                        at(Event::new(CE, b"{}".to_vec()), 5),
                        derived_at(CE, "not a key", 6),
                        derived_at(DC, "gd/e.md@h1#0", 100),
                        derived_at(DL, "gd/e.md@h1#1", 101),
                        at(
                            Event::new(
                                crate::contextgraph::TYPE_ALIAS_DEFINED,
                                br#"{"alias":"x","canonical":"y"}"#.to_vec(),
                            ),
                            105,
                        ),
                        derived_at(DC, "gd/e.md@h1#0", 110),
                        at(Event::new("DecisionMade", b"{}".to_vec()), 120),
                    ],
                ),
                ("p-run-x", vec![derived_at(CE, "gc/a.rs@h0#0", 2)]),
                ("q-run", vec![derived_at(CE, "gc/a.rs@h0#0", 3)]),
            ],
        )
    }

    /// The count of `store_to_migrate`'s stream: every derived row of it, the two that name no
    /// identity, and the five identities holding one.
    fn counted_before() -> DerivedCount {
        DerivedCount {
            shed: 17,
            unkeyed: 2,
            identities: ["gc/a.rs", "gc/c.rs", "gc/d.rs", "gd/b.md", "gd/e.md"]
                .map(String::from)
                .into(),
        }
    }

    /// The read-only count answers every derived row of the one stream it is handed, the rows
    /// naming no identity among them, and the identities holding one - never a row of a stream
    /// whose name only starts with that stream's, never a ledger entry - and changes nothing.
    #[test]
    fn count_derived_counts_one_streams_derived_rows_its_unkeyed_rows_and_its_identities() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_to_migrate(&path);
        let before = rows_of(&path);

        assert_eq!(
            (
                s.count_derived("p-run").unwrap(),
                s.count_derived("p-run-x").unwrap(),
                s.count_derived("p-").unwrap(),
                rows_of(&path) == before,
            ),
            (
                counted_before(),
                DerivedCount {
                    shed: 1,
                    unkeyed: 0,
                    identities: ["gc/a.rs".to_string()].into(),
                },
                DerivedCount::default(),
                true,
            )
        );
    }

    /// THE MIGRATION'S ONE TRANSACTION: each identity whose latest recording is derived has the
    /// lowest row the selection keeps rewritten in place into its entry - every column a
    /// uniqueness rule covers and its recorded-time kept - every identity's earliest surviving
    /// recording takes the identity's earliest recorded valid-time, and every other derived row
    /// of the stream is deleted, keyed or not. Nothing else in the file changes.
    #[test]
    fn shed_derived_rewrites_each_latest_derived_batch_into_its_entry_and_deletes_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_to_migrate(&path);
        let before = rows_of(&path);
        // `before` is in position order from 1, so the row at position `p` is `before[p - 1]`.
        let at = |position: usize| &before[position - 1];

        let shed = s.shed_derived("p-run", &named_entry).unwrap();

        assert_eq!(
            (shed, rows_of(&path)),
            (
                ShedDerived {
                    converted: 4,
                    shed: 17,
                    unkeyed: 2,
                },
                vec![
                    at(1).clone(),
                    // Three generations: the entry stands at the first row of the latest batch,
                    // counts that generation's two keys and is dated at the earliest recording,
                    // a row of a superseded generation.
                    rewritten(
                        at(5),
                        &entry_event("gc/a.rs", "h3", 2, "blob-of-gc/a.rs", false),
                        20
                    ),
                    // Recorded twice: the entry stands at the first row of the second recording.
                    rewritten(
                        at(9),
                        &entry_event("gd/b.md", "h1", 2, "blob-of-gd/b.md", true),
                        30
                    ),
                    // An entry above derived rows: nothing is rewritten, the entry is re-dated.
                    redated(at(12), 70),
                    // An entry below derived rows: it is the earliest surviving recording, so
                    // it takes the earliest valid-time and the rewritten row keeps its own.
                    redated(at(13), 10),
                    rewritten(
                        at(15),
                        &entry_event("gc/d.rs", "h2", 1, "blob-of-gc/d.rs", false),
                        85
                    ),
                    // A generation whose kept rows span two recordings: the entry stands at the
                    // lowest kept row, the second row of the first recording.
                    rewritten(
                        at(19),
                        &entry_event("gd/e.md", "h1", 2, "blob-of-gd/e.md", false),
                        100
                    ),
                    at(20).clone(),
                    at(22).clone(),
                    at(23).clone(),
                    at(24).clone(),
                ]
            )
        );
    }

    /// An identity recorded as a derived row, then a ledger entry, then the same derived row
    /// again has a derived latest recording: the later row is rewritten, and the entry below it,
    /// the earliest surviving recording, keeps the earliest valid-time, its own.
    #[test]
    fn shed_derived_rewrites_a_derived_row_recorded_again_above_a_ledger_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let ce = crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED;
        let s = store_with(
            path.to_str().unwrap(),
            &[(
                "p-run",
                vec![
                    derived_at(ce, "gc/a.rs@h1#0", 30),
                    entry_event("gc/a.rs", "h1", 1, "held", false)
                        .with_valid_from(std::time::UNIX_EPOCH + Duration::from_secs(20)),
                    derived_at(ce, "gc/a.rs@h1#0", 40),
                ],
            )],
        );
        let before = rows_of(&path);

        let shed = s.shed_derived("p-run", &named_entry).unwrap();

        assert_eq!(
            (shed, rows_of(&path)),
            (
                ShedDerived {
                    converted: 1,
                    shed: 2,
                    unkeyed: 0,
                },
                vec![
                    before[1].clone(),
                    rewritten(
                        &before[2],
                        &entry_event("gc/a.rs", "h1", 1, "blob-of-gc/a.rs", false),
                        40
                    ),
                ]
            )
        );
    }

    /// A store at `path` whose stream `p-run` holds one identity recorded, in position order, as
    /// a ledger entry valid from `first`, a derived row valid from 40 and a second ledger entry
    /// valid from `second`, migrated: what the migration answered and the rows it left, beside
    /// the two entry rows as they stood before it.
    fn migrated_between_two_entries(first: u64, second: u64) -> (ShedDerived, Vec<Row>, [Row; 2]) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let entry_at = |secs: u64| {
            entry_event("gc/a.rs", "h1", 1, "held", false)
                .with_valid_from(std::time::UNIX_EPOCH + Duration::from_secs(secs))
        };
        let s = store_with(
            path.to_str().unwrap(),
            &[(
                "p-run",
                vec![
                    entry_at(first),
                    derived_at(
                        crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                        "gc/a.rs@h1#0",
                        40,
                    ),
                    entry_at(second),
                ],
            )],
        );
        let before = rows_of(&path);
        let shed = s.shed_derived("p-run", &named_entry).unwrap();
        (shed, rows_of(&path), [before[0].clone(), before[2].clone()])
    }

    /// Of two ledger entries of one identity with a derived row between them, the earliest
    /// surviving recording is the FIRST entry in position order and the date it takes is the
    /// EARLIEST any recording held, whichever entry held it: an earlier first entry keeps its
    /// own date, a later one is re-dated to the second entry's, and the second entry is left as
    /// it stands. The derived row is shed and nothing is converted.
    #[test]
    fn shed_derived_dates_the_first_of_two_entries_at_the_earliest_valid_time_either_holds() {
        for (first_valid, second_valid) in [(30, 50), (50, 30)] {
            let (shed, after, [first, second]) =
                migrated_between_two_entries(first_valid, second_valid);

            assert_eq!(
                (shed, after),
                (
                    ShedDerived {
                        converted: 0,
                        shed: 1,
                        unkeyed: 0,
                    },
                    vec![redated(&first, 30), second]
                ),
                "entries valid from {first_valid} then {second_valid}"
            );
        }
    }

    /// A migrated stream holds no derived row: the count answers nothing, and a second migration
    /// sheds nothing, converts nothing and leaves every row as it stands.
    #[test]
    fn a_migrated_stream_counts_no_derived_row_and_a_second_migration_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let s = store_to_migrate(&path);
        s.shed_derived("p-run", &named_entry).unwrap();
        let migrated = rows_of(&path);

        assert_eq!(
            (
                s.count_derived("p-run").unwrap(),
                s.shed_derived("p-run", &named_entry).unwrap(),
                rows_of(&path) == migrated,
            ),
            (DerivedCount::default(), ShedDerived::default(), true)
        );
    }
}
